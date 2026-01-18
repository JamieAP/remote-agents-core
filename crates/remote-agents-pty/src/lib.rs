//! Cross-platform PTY session management.
//!
//! Provides:
//! - `PtyService` - Manage PTY sessions
//! - Shell detection utilities for Unix and Windows

pub mod service;
pub mod shell;

pub use service::{PtyError, PtyService};
pub use shell::{get_interactive_shell, get_shell_command, resolve_executable_path};
