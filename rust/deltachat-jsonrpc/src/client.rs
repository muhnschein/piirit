use std::collections::HashMap;
use std::ffi::OsStr;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{RpcError, SpawnError};
use crate::protocol::{RequestEnvelope, ResponseEnvelope};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, RpcError>>>>>;

/// Lock a mutex, recovering the guard if a previous holder panicked.
///
/// These mutexes guard plain collections, which a panic leaves structurally
/// intact. Killing the transport over a fault that happened elsewhere is
/// the worse outcome.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A running `deltachat-rpc-server` subprocess, addressable via JSON-RPC 2.0
/// over its stdio.
///
/// Transport only: correlation, framing, process lifecycle. Method names and
/// payloads pass through uninterpreted.
pub struct RpcClient {
    stdin_tx: mpsc::UnboundedSender<String>,
    pending: PendingMap,
    next_id: AtomicU64,
    child: Mutex<Option<Child>>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
    writer_task: Mutex<Option<JoinHandle<()>>>,
    /// Lines the server wrote to stderr, most recent last, capped to a
    /// small ring so a misbehaving server can't grow this unbounded.
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

const STDERR_TAIL_CAPACITY: usize = 200;

/// How long an ordinary call waits for its answer. Generous: the core does
/// real work behind some of these, and a slow phone is not a broken one.
/// Without any bound at all, a request the server accepts and never answers
/// -- a panic in the task handling it leaves the process up and its stdout
/// open -- hangs the caller for the life of the app, with no error and
/// nothing to cancel.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Feed the server's stdin from the request channel, one line per request,
/// until the channel closes or a write fails.
async fn write_requests(mut stdin: ChildStdin, mut requests: mpsc::UnboundedReceiver<String>) {
    while let Some(line) = requests.recv().await {
        if write_line(&mut stdin, &line).await.is_err() {
            break;
        }
    }
}

/// One request, newline-terminated and flushed, so the server reads it now.
async fn write_line(stdin: &mut ChildStdin, line: &str) -> std::io::Result<()> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

/// Match the server's stdout to the calls waiting on it, and wake anyone
/// still waiting once the stream ends.
///
/// Ends on end-of-stdout or a failed read: the server is gone either way.
/// A line that is not UTF-8 is neither -- it is one answer nobody can
/// read, and the call it answered times out -- so the stream carries on
/// past it rather than reporting a live server as closed.
async fn read_responses(stdout: ChildStdout, pending: &PendingMap) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Err(err) if err.kind() == std::io::ErrorKind::InvalidData => continue,
            Ok(None) | Err(_) => break,
        };
        let Some((id, outcome)) = parse_response(&line) else {
            continue;
        };
        // Out of the map first, so the lock is not held while the answer
        // is handed over.
        let waiting = lock(pending).remove(&id);
        if let Some(sender) = waiting {
            // A closed receiver is a caller that stopped waiting (its
            // timeout ran out); there is no one left to tell.
            sender.send(outcome).ok();
        }
    }
    // Transport is gone: wake up anyone still waiting rather than leaving
    // them hanging forever.
    for (_, sender) in lock(pending).drain() {
        sender.send(Err(RpcError::TransportClosed)).ok();
    }
}

/// One line of stdout as the answer it carries, or nothing for a line that
/// is not a JSON-RPC response -- skipped rather than tearing down a live
/// transport.
fn parse_response(line: &str) -> Option<(u64, Result<serde_json::Value, RpcError>)> {
    if line.trim().is_empty() {
        return None;
    }
    let envelope = serde_json::from_str::<ResponseEnvelope>(line).ok()?;
    // The core delivers events by polling, not by notification, so an
    // answer without an id should not happen.
    let id = envelope.id?;
    let outcome = match (envelope.result, envelope.error) {
        (_, Some(err)) => Err(RpcError::Remote(err)),
        (Some(result), None) => Ok(result),
        (None, None) => Ok(serde_json::Value::Null),
    };
    Some((id, outcome))
}

/// Keep the last few lines the server wrote to stderr, for the message
/// shown when it dies.
async fn keep_stderr_tail(stderr: ChildStderr, tail: &Mutex<Vec<String>>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut kept = lock(tail);
        kept.push(line);
        let excess = kept.len().saturating_sub(STDERR_TAIL_CAPACITY);
        kept.drain(..excess);
    }
}

impl RpcClient {
    /// Spawn `program` (typically the path to a bundled
    /// `deltachat-rpc-server` binary) with `args` and set up the
    /// request/response machinery over its stdio.
    ///
    /// # Errors
    ///
    /// Fails if the process cannot be executed, or if the spawned child
    /// does not expose the stdio pipes we asked for.
    pub async fn spawn<S, I, A>(program: S, args: I) -> Result<Self, SpawnError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        Self::spawn_with_env(program, args, std::iter::empty::<(&OsStr, &OsStr)>()).await
    }

    /// Like [`RpcClient::spawn`], with extra environment variables on the
    /// child. `DC_ACCOUNTS_PATH` is the one that matters: without it the
    /// server stores account state under `./accounts`, relative to its
    /// working directory.
    ///
    /// # Errors
    ///
    /// Same as [`RpcClient::spawn`].
    // Stays `async` though nothing awaits: `Command::spawn` needs an active
    // reactor, and an `async fn` can only run inside one.
    #[allow(clippy::unused_async)]
    pub async fn spawn_with_env<S, I, A, E, K, V>(
        program: S,
        args: I,
        envs: E,
    ) -> Result<Self, SpawnError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut child = Command::new(program)
            .args(args)
            .envs(envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().ok_or(SpawnError::MissingPipe)?;
        let stdout = child.stdout.take().ok_or(SpawnError::MissingPipe)?;
        let stderr = child.stderr.take().ok_or(SpawnError::MissingPipe)?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));

        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<String>();
        let writer_task = tokio::spawn(write_requests(stdin, stdin_rx));

        let reader_pending = pending.clone();
        let reader_task =
            tokio::spawn(async move { read_responses(stdout, &reader_pending).await });

        let stderr_capture = stderr_tail.clone();
        tokio::spawn(async move { keep_stderr_tail(stderr, &stderr_capture).await });

        Ok(Self {
            stdin_tx,
            pending,
            next_id: AtomicU64::new(1),
            child: Mutex::new(Some(child)),
            reader_task: Mutex::new(Some(reader_task)),
            writer_task: Mutex::new(Some(writer_task)),
            stderr_tail,
        })
    }

    /// Call an RPC method, serializing `params` and deserializing the
    /// result as `R`.
    ///
    /// Method names and shapes come from the core's API (`--openrpc`); this
    /// crate neither hardcodes nor validates them.
    ///
    /// # Errors
    ///
    /// Returns [`RpcError::Encode`] if `params` cannot be serialized,
    /// [`RpcError::Remote`] if the server answered with an error object,
    /// [`RpcError::Decode`] if the result does not fit `R`, and
    /// [`RpcError::TransportClosed`] if the server went away.
    pub async fn call<P, R>(&self, method: &str, params: P) -> Result<R, RpcError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let value = self.call_raw(method, &params, Some(CALL_TIMEOUT)).await?;
        serde_json::from_value(value).map_err(|source| RpcError::Decode {
            method: method.to_string(),
            source,
        })
    }

    /// Call a method that takes no params.
    ///
    /// # Errors
    ///
    /// Same as [`RpcClient::call`].
    pub async fn call_unit<R>(&self, method: &str) -> Result<R, RpcError>
    where
        R: DeserializeOwned,
    {
        self.call(method, ()).await
    }

    /// Call a method that blocks server-side until it has something to say,
    /// such as `get_next_event_batch`. No timeout: waiting is the point.
    ///
    /// # Errors
    ///
    /// Same as [`RpcClient::call`], less [`RpcError::Timeout`].
    pub async fn call_polling<R>(&self, method: &str) -> Result<R, RpcError>
    where
        R: DeserializeOwned,
    {
        let value = self.call_raw(method, &(), None).await?;
        serde_json::from_value(value).map_err(|source| RpcError::Decode {
            method: method.to_string(),
            source,
        })
    }

    async fn call_raw(
        &self,
        method: &str,
        params: &impl Serialize,
        timeout: Option<Duration>,
    ) -> Result<serde_json::Value, RpcError> {
        let params = serde_json::to_value(params).map_err(|source| RpcError::Encode {
            method: method.to_string(),
            source,
        })?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);

        let request = RequestEnvelope {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let line = serde_json::to_string(&request).map_err(|source| RpcError::Encode {
            method: method.to_string(),
            source,
        })?;

        if self.stdin_tx.send(line).is_err() {
            lock(&self.pending).remove(&id);
            return Err(RpcError::TransportClosed);
        }

        let Some(limit) = timeout else {
            return rx.await.unwrap_or(Err(RpcError::TransportClosed));
        };
        let Ok(answer) = tokio::time::timeout(limit, rx).await else {
            // Nothing will read this now, and the reader would only find a
            // sender nobody is waiting on.
            lock(&self.pending).remove(&id);
            return Err(RpcError::Timeout {
                method: method.to_string(),
                seconds: limit.as_secs(),
            });
        };
        answer.unwrap_or(Err(RpcError::TransportClosed))
    }

    /// The last `STDERR_TAIL_CAPACITY` lines of the server's stderr, oldest
    /// first. Diagnostics for a failed call or an unexpected exit.
    pub fn stderr_tail(&self) -> Vec<String> {
        lock(&self.stderr_tail).clone()
    }

    /// The server's process id, while it is running. What a memory
    /// profile of the core reads `/proc/<pid>` by.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        lock(&self.child).as_ref().and_then(Child::id)
    }

    /// Terminate the child process and wait for the reader/writer tasks to
    /// finish. Safe to call even if the process has already exited.
    ///
    /// # Errors
    ///
    /// Propagates the I/O error from waiting on the child, if any.
    pub async fn shutdown(&self) -> std::io::Result<()> {
        let child = lock(&self.child).take();
        if let Some(mut child) = child {
            // Fails only for a child that has already exited, which is
            // what this is for; the wait below says how it went.
            child.start_kill().ok();
            child.wait().await?;
        }
        let reader_task = lock(&self.reader_task).take();
        if let Some(task) = reader_task {
            // Its only result is whether it panicked or was cancelled, and
            // either way it has stopped, which is all shutdown waits for.
            task.await.ok();
        }
        let writer_task = lock(&self.writer_task).take();
        if let Some(task) = writer_task {
            task.abort();
        }
        Ok(())
    }
}
