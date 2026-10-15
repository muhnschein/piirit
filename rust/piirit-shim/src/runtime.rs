//! Tokio runtime off Qt main thread.
//!
//! `Runtime::new` and drop touch tokio's thread-local context, which on Qt main thread
//! (Sailfish aarch64) panics `already borrowed: BorrowMutError`. So built and dropped on own
//! thread; only a `Handle` returned (its `spawn` never enters runtime context).

use std::future::Future;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::{Builder, Handle};
use tokio::task::JoinHandle;

/// Bounded: wedged thread cannot hang UI.
const HANDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// Two: one pipe to one process; keep cores for drawing.
const WORKER_THREADS: usize = 2;

/// Handle to runtime thread. Last clone dropped shuts it down.
#[derive(Clone)]
pub struct CoreRuntime(Arc<RuntimeThread>);

struct RuntimeThread {
    handle: Handle,
    /// Dropping sender stops runtime thread.
    _shutdown: mpsc::Sender<()>,
}

impl CoreRuntime {
    pub fn new() -> Result<Self, String> {
        let (handle_tx, handle_rx) = mpsc::channel::<Result<Handle, String>>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        std::thread::Builder::new()
            .name("piirit-tokio".to_string())
            .spawn(move || {
                // Only place runtime built.
                let runtime = match Builder::new_multi_thread()
                    .worker_threads(WORKER_THREADS)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        // Caller gone: ignore.
                        handle_tx.send(Err(err.to_string())).ok();
                        return;
                    }
                };
                if handle_tx.send(Ok(runtime.handle().clone())).is_err() {
                    return;
                }
                // Park until clones gone; drop runtime here, not on Qt thread.
                shutdown_rx.recv().ok();
            })
            .map_err(|err| format!("could not start runtime thread: {err}"))?;

        match handle_rx.recv_timeout(HANDLE_TIMEOUT) {
            Ok(Ok(handle)) => Ok(Self(Arc::new(RuntimeThread {
                handle,
                _shutdown: shutdown_tx,
            }))),
            Ok(Err(err)) => Err(err),
            Err(err) => Err(format!("runtime thread did not start: {err}")),
        }
    }

    /// Spawn via `Handle` (enqueue only).
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.handle.spawn(future)
    }
}
