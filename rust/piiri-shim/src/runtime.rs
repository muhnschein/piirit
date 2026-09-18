//! The tokio runtime, kept off the Qt main thread.
//!
//! `Runtime::new` and dropping a `Runtime` both touch tokio's thread-local
//! context, which panics with `already borrowed: BorrowMutError` on the Qt
//! main thread of the Sailfish aarch64 build. `CoreRuntime` therefore
//! builds and drops the runtime on a thread of its own and hands back only
//! a `Handle`, whose `spawn` never enters the runtime context.

use std::future::Future;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::{Builder, Handle};
use tokio::task::JoinHandle;

/// Bounded so a wedged thread cannot hang the UI. A thread spawn plus a
/// runtime build is milliseconds.
const HANDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// Two, not one per core: everything here is one pipe to one process,
/// and a phone's cores are wanted for drawing the page.
const WORKER_THREADS: usize = 2;

/// Handle to a tokio runtime on its own thread. Clones share it; when the
/// last one drops, that thread shuts down and drops the runtime.
#[derive(Clone)]
pub struct CoreRuntime(Arc<RuntimeThread>);

struct RuntimeThread {
    handle: Handle,
    /// Dropping this sender is what tells the runtime thread to finish.
    _shutdown: mpsc::Sender<()>,
}

impl CoreRuntime {
    pub fn new() -> Result<Self, String> {
        let (handle_tx, handle_rx) = mpsc::channel::<Result<Handle, String>>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        std::thread::Builder::new()
            .name("piiri-tokio".to_string())
            .spawn(move || {
                // The one place a runtime may be built: this thread.
                let runtime = match Builder::new_multi_thread()
                    .worker_threads(WORKER_THREADS)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        let _ = handle_tx.send(Err(err.to_string()));
                        return;
                    }
                };
                if handle_tx.send(Ok(runtime.handle().clone())).is_err() {
                    return;
                }
                // Parks until every clone is gone, then drops the runtime
                // here rather than on the Qt thread.
                let _ = shutdown_rx.recv();
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

    /// Spawn onto the runtime. Goes through `Handle`, which only enqueues.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.handle.spawn(future)
    }
}
