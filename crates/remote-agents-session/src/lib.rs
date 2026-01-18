//! Session orchestration and storage for remote agents.
//!
//! Provides:
//! - `SessionManager` - Orchestrate agent sessions
//! - Storage implementations (memory, SQLite)

pub mod manager;
pub mod storage;

pub use manager::SessionManager;
