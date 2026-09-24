use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::RpcClient;
use crate::error::RpcError;

/// One event as delivered by `get_next_event_batch`.
///
/// `event` stays a raw [`serde_json::Value`]: hand-typing the core's several
/// dozen event variants would be the protocol reimplementation
/// `docs/PROJECT.md` rules out. Callers match on `event["kind"]`.
#[derive(Debug, Clone, Deserialize)]
pub struct CoreEvent {
    /// Which account (the core calls it a "context") the event belongs to.
    #[serde(rename = "contextId")]
    pub context_id: u32,
    /// The event object itself, tagged by its `kind` field.
    pub event: serde_json::Value,
}

/// How long to wait after an answer that was not a batch of events before
/// asking again, and the ceiling that wait doubles towards. The loop never
/// gives up on anything but the transport closing: ending the stream is
/// read by the shim as the server having died, and a server that is
/// answering -- even wrongly -- has not.
const ERROR_BACKOFF_MIN: Duration = Duration::from_millis(50);
const ERROR_BACKOFF_MAX: Duration = Duration::from_secs(5);

/// The wait after `failures` consecutive bad answers.
fn error_backoff(failures: u32) -> Duration {
    ERROR_BACKOFF_MIN
        .saturating_mul(
            1_u32
                .checked_shl(failures.saturating_sub(1))
                .unwrap_or(u32::MAX),
        )
        .min(ERROR_BACKOFF_MAX)
}

/// Handle to the polling task. Cancels on drop, as well as through
/// [`EventLoopHandle::stop`].
pub struct EventLoopHandle {
    task: JoinHandle<()>,
}

impl EventLoopHandle {
    /// Cancel the polling task.
    pub fn stop(self) {
        self.task.abort();
    }
}

impl Drop for EventLoopHandle {
    fn drop(&mut self) {
        // Dropping a `JoinHandle` on its own detaches the task, which would
        // leave it polling a server nobody is listening to.
        self.task.abort();
    }
}

/// Poll `get_next_event_batch` in a loop, pushing each event to the returned
/// channel until the transport closes or the handle is stopped.
///
/// The core has no unsolicited notifications: the call blocks server-side
/// until an event is available, so this is an ordinary RPC call repeated.
///
/// The stream ending means one thing only: the transport closed. Callers
/// rely on that -- the shim restarts the server when the stream ends.
pub fn spawn_event_loop(
    client: Arc<RpcClient>,
) -> (mpsc::UnboundedReceiver<CoreEvent>, EventLoopHandle) {
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut failures = 0_u32;
        loop {
            match client
                .call_polling::<Vec<CoreEvent>>("get_next_event_batch")
                .await
            {
                Ok(batch) => {
                    failures = 0;
                    for event in batch {
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                }
                // The server is gone; nothing to poll.
                Err(RpcError::TransportClosed) => return,
                // Anything else is one bad answer, not a dead core: an
                // event shape we could not read, or a call the core
                // refused. Giving up after a handful of these would end the
                // stream, which the shim reads as the server having died --
                // and it would kill a running server to start another. So:
                // wait, longer each time, and ask again.
                Err(_) => {
                    failures = failures.saturating_add(1);
                    tokio::time::sleep(error_backoff(failures)).await;
                }
            }
        }
    });
    (rx, EventLoopHandle { task })
}

#[cfg(test)]
mod tests {
    use super::{error_backoff, ERROR_BACKOFF_MAX, ERROR_BACKOFF_MIN};

    #[test]
    fn the_backoff_doubles_from_the_first_failure_and_stops_at_the_ceiling() {
        assert_eq!(error_backoff(1), ERROR_BACKOFF_MIN);
        assert_eq!(error_backoff(2), ERROR_BACKOFF_MIN * 2);
        assert_eq!(error_backoff(4), ERROR_BACKOFF_MIN * 8);
        assert_eq!(error_backoff(20), ERROR_BACKOFF_MAX);
        assert_eq!(error_backoff(u32::MAX), ERROR_BACKOFF_MAX);
    }
}
