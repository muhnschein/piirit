//! Transport errors: spawn and per-call.
//! Hand-written Display: no proc-macro crate for two enums.

use std::fmt;

use crate::protocol::ErrorObject;

/// Why spawn failed.
#[derive(Debug)]
pub enum SpawnError {
    /// Exec failed (missing binary, bad perms).
    Io(std::io::Error),
    /// Child lacks a requested stdio pipe; should be impossible.
    MissingPipe,
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to spawn deltachat-rpc-server: {err}"),
            Self::MissingPipe => {
                f.write_str("spawned child has no stdin/stdout pipe (this is a bug)")
            }
        }
    }
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::MissingPipe => None,
        }
    }
}

impl From<std::io::Error> for SpawnError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Why one call gave no result.
#[derive(Debug)]
pub enum RpcError {
    /// Server returned error object.
    Remote(ErrorObject),

    /// Result type mismatch.
    Decode {
        /// Method that failed to decode.
        method: String,
        /// Serde error.
        source: serde_json::Error,
    },

    /// Transport gone: stdio closed or server exited.
    TransportClosed,

    /// Server never answered.
    Timeout {
        /// Unanswered method.
        method: String,
        /// Seconds waited.
        seconds: u64,
    },

    /// Params not serializable.
    Encode {
        /// Method with bad params.
        method: String,
        /// Serde error.
        source: serde_json::Error,
    },
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remote(error) => write!(
                f,
                "deltachat-rpc-server returned an error ({}): {}",
                error.code, error.message
            ),
            Self::Decode { method, source } => {
                write!(f, "failed to decode result for method {method}: {source}")
            }
            Self::TransportClosed => f.write_str("deltachat-rpc-server transport closed"),
            Self::Timeout { method, seconds } => write!(
                f,
                "deltachat-rpc-server did not answer {method} within {seconds}s"
            ),
            Self::Encode { method, source } => {
                write!(
                    f,
                    "failed to serialize params for method {method}: {source}"
                )
            }
        }
    }
}

impl std::error::Error for RpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode { source, .. } | Self::Encode { source, .. } => Some(source),
            Self::Remote(_) | Self::TransportClosed | Self::Timeout { .. } => None,
        }
    }
}
