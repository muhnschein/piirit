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

/// Lock ignoring poison: plain collections survive panic.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Running `deltachat-rpc-server` over stdio.
///
/// Transport only (correlation, framing, process); payloads uninterpreted.
pub struct RpcClient {
    stdin_tx: mpsc::UnboundedSender<String>,
    pending: PendingMap,
    next_id: AtomicU64,
    child: Mutex<Option<Child>>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
    writer_task: Mutex<Option<JoinHandle<()>>>,
    /// Last stderr lines, capped.
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

const STDERR_TAIL_CAPACITY: usize = 200;

/// Call timeout, generous (slow phone, real work). Bounded: unanswered request (task panic,
/// process still up) would hang caller forever.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Write request lines to stdin until closed or failed.
async fn write_requests(mut stdin: ChildStdin, mut requests: mpsc::UnboundedReceiver<String>) {
    while let Some(line) = requests.recv().await {
        if write_line(&mut stdin, &line).await.is_err() {
            break;
        }
    }
}

/// Write one request line, flushed.
async fn write_line(stdin: &mut ChildStdin, line: &str) -> std::io::Result<()> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

/// Match stdout answers to waiters; wake them at end.
///
/// EOF/read error: server gone. Bad UTF-8 line skipped (its call times out).
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
        // Remove first: no lock while sending.
        let waiting = lock(pending).remove(&id);
        if let Some(sender) = waiting {
            // Closed receiver: caller timed out.
            sender.send(outcome).ok();
        }
    }
    // Transport gone: wake waiters.
    for (_, sender) in lock(pending).drain() {
        sender.send(Err(RpcError::TransportClosed)).ok();
    }
}

/// Answer on one stdout line, None if not a response.
fn parse_response(line: &str) -> Option<(u64, Result<serde_json::Value, RpcError>)> {
    if line.trim().is_empty() {
        return None;
    }
    let envelope = serde_json::from_str::<ResponseEnvelope>(line).ok()?;
    // Id-less answer unexpected (events are polled).
    let id = envelope.id?;
    let outcome = match (envelope.result, envelope.error) {
        (_, Some(err)) => Err(RpcError::Remote(err)),
        (Some(result), None) => Ok(result),
        (None, None) => Ok(serde_json::Value::Null),
    };
    Some((id, outcome))
}

/// Last stderr lines, for death message.
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
    /// Spawn `program` with `args`, wire up stdio.
    ///
    /// # Errors
    ///
    /// Exec failed or stdio pipe missing.
    pub async fn spawn<S, I, A>(program: S, args: I) -> Result<Self, SpawnError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        Self::spawn_with_env(program, args, std::iter::empty::<(&OsStr, &OsStr)>()).await
    }

    /// [`RpcClient::spawn`] plus child env. Set `DC_ACCOUNTS_PATH`, else server uses cwd's
    /// `./accounts`.
    ///
    /// # Errors
    ///
    /// Same as [`RpcClient::spawn`].
    // Async with no await: `Command::spawn` needs active reactor. Clippy 1.99 also flags
    // `unused_async_trait_impl`; `unknown_lints` for pinned 1.94.1, which lacks it.
    #[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
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

    /// Call RPC method: serialize params, deserialize result as `R`.
    ///
    /// Method names from core API (`--openrpc`); not validated.
    ///
    /// # Errors
    ///
    /// [`RpcError::Encode`]: params unserializable. [`RpcError::Remote`]: server error object.
    /// [`RpcError::Decode`]: result doesn't fit `R`. [`RpcError::TransportClosed`]: server gone.
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

    /// Call method, no params.
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

    /// Blocks server-side until output. No timeout.
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
            // Nobody reads it now; drop sender.
            lock(&self.pending).remove(&id);
            return Err(RpcError::Timeout {
                method: method.to_string(),
                seconds: limit.as_secs(),
            });
        };
        answer.unwrap_or(Err(RpcError::TransportClosed))
    }

    /// Last `STDERR_TAIL_CAPACITY` stderr lines, oldest first.
    pub fn stderr_tail(&self) -> Vec<String> {
        lock(&self.stderr_tail).clone()
    }

    /// Server pid while running (`/proc/<pid>` profiling).
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        lock(&self.child).as_ref().and_then(Child::id)
    }

    /// Kill child, wait for tasks. Safe if child already exited.
    ///
    /// # Errors
    ///
    /// I/O error from waiting on child.
    pub async fn shutdown(&self) -> std::io::Result<()> {
        let child = lock(&self.child).take();
        if let Some(mut child) = child {
            // Fails only if child exited: fine.
            child.start_kill().ok();
            child.wait().await?;
        }
        let reader_task = lock(&self.reader_task).take();
        if let Some(task) = reader_task {
            // Panicked or cancelled: stopped.
            task.await.ok();
        }
        let writer_task = lock(&self.writer_task).take();
        if let Some(task) = writer_task {
            task.abort();
        }
        Ok(())
    }
}
