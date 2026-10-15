use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::RpcClient;
use crate::error::RpcError;

/// One event from `get_next_event_batch`. Raw [`serde_json::Value`] (typing dozens of variants
/// reimplements protocol, against `docs/PROJECT.md`): match on `event["kind"]`.
#[derive(Debug, Clone, Deserialize)]
pub struct CoreEvent {
    /// Account (core's "context").
    #[serde(rename = "contextId")]
    pub context_id: u32,
    /// Event, tagged by `kind`.
    pub event: serde_json::Value,
}

/// Backoff after non-batch answer, doubling to ceiling.
const ERROR_BACKOFF_MIN: Duration = Duration::from_millis(50);
const ERROR_BACKOFF_MAX: Duration = Duration::from_secs(5);

/// Wait after `failures` bad answers.
fn error_backoff(failures: u32) -> Duration {
    ERROR_BACKOFF_MIN
        .saturating_mul(
            1_u32
                .checked_shl(failures.saturating_sub(1))
                .unwrap_or(u32::MAX),
        )
        .min(ERROR_BACKOFF_MAX)
}

/// Polling handle. Cancels on drop or [`EventLoopHandle::stop`].
pub struct EventLoopHandle {
    task: JoinHandle<()>,
}

impl EventLoopHandle {
    /// Cancel task.
    pub fn stop(self) {
        self.task.abort();
    }
}

impl Drop for EventLoopHandle {
    fn drop(&mut self) {
        // Dropping `JoinHandle` alone detaches: task would poll dead listener.
        self.task.abort();
    }
}

/// Poll `get_next_event_batch` (blocks server-side; core has no unsolicited notifications) into
/// channel till stop or transport close. Only close ends stream: shim restarts server on it.
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
                // Server gone.
                Err(RpcError::TransportClosed) => return,
                // Else bad answer (unreadable event, refused call); core alive. Ending stream would
                // make shim replace a running server: back off, retry.
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
