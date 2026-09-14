//! Wire types for the JSON-RPC 2.0 messages exchanged with
//! `deltachat-rpc-server`. The framing is JSON Lines, not `Content-Length`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct RequestEnvelope<'a> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'a str,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponseEnvelope {
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<ErrorObject>,
}

/// A JSON-RPC 2.0 error object. Reproduced verbatim; the codes and messages
/// are the core's.
#[derive(Debug, Deserialize, Clone)]
pub struct ErrorObject {
    /// JSON-RPC error code.
    pub code: i64,
    /// Human-readable message, written by the core.
    pub message: String,
    /// Optional structured payload accompanying the error.
    #[serde(default)]
    pub data: Option<Value>,
}
