//! Wire types. JSON Lines framing, not `Content-Length`.

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

/// JSON-RPC error. Verbatim from core.
#[derive(Debug, Deserialize, Clone)]
pub struct ErrorObject {
    /// Error code.
    pub code: i64,
    /// Message, from core.
    pub message: String,
    /// Optional payload.
    #[serde(default)]
    pub data: Option<Value>,
}
