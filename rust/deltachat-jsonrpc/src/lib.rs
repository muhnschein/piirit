//! Transport for a `deltachat-rpc-server` subprocess over its JSON-RPC 2.0
//! stdio interface.
//!
//! Knows nothing of Delta Chat's protocol, accounts, chats or messages: it
//! spawns the server, frames and correlates requests, and drains the event
//! stream. See `docs/PROJECT.md` for why that boundary is strict.

// Shipped code is held to more than tests are; piirit-shim/src/lib.rs
// says the rest.
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
