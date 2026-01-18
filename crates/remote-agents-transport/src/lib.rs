//! Transport layer for web and TUI interfaces.
//!
//! Provides:
//! - Wire protocol (JSON + base64)
//! - WebSocket transport (feature: websocket)
//! - TUI transport bridge (feature: tui)

pub mod protocol;

#[cfg(feature = "websocket")]
pub mod websocket;

#[cfg(feature = "tui")]
pub mod tui;

pub use protocol::{ClientMessage, ServerMessage};
