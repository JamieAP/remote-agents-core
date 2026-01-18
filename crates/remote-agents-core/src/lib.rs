//! Core abstractions for remote agent session management.
//!
//! This crate provides the fundamental building blocks:
//! - `MsgStore` - Broadcast + history for reconnection support
//! - `LogMsg` - Typed log message enum
//! - `ExecutionContext` - Generic context for session execution
//! - Storage and Executor traits

pub mod context;
pub mod log_msg;
pub mod msg_store;
pub mod traits;

pub use context::ExecutionContext;
pub use log_msg::LogMsg;
pub use msg_store::MsgStore;
pub use traits::{Executor, SessionStorage};
