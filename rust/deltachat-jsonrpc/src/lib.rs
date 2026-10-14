//! Transport for `deltachat-rpc-server` JSON-RPC 2.0 over stdio.
//!
//! No Delta Chat logic: spawn, frame, correlate, drain events. See `docs/PROJECT.md`.

// Shipped code: stricter lints than tests. See piirit-shim/src/lib.rs.
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_must_use,
        clippy::panic,
        clippy::panic_in_result_fn,
        clippy::unwrap_in_result,
        clippy::get_unwrap
    )
)]

mod client;
mod error;
mod events;
mod protocol;

pub use client::RpcClient;
pub use error::{RpcError, SpawnError};
pub use events::{spawn_event_loop, CoreEvent, EventLoopHandle};
pub use protocol::ErrorObject;
