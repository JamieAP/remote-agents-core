//! Claude Code executor with SDK protocol support.
//!
//! Provides:
//! - Claude Code SDK protocol types
//! - Command building utilities
//! - Approval handler trait

pub mod approvals;
pub mod claude;
pub mod command;

pub use approvals::{ApprovalHandler, ApprovalResult, ApprovalStatus};
pub use command::{CommandBuilder, CommandParts};
