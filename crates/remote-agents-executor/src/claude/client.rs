//! Claude Code agent client.

use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;

use crate::approvals::{ApprovalHandler, ApprovalResult};
use super::types::PermissionResult;

/// Claude agent client with control protocol support.
pub struct ClaudeClient {
    log_writer: LogWriter,
    approval_handler: Option<Arc<dyn ApprovalHandler>>,
    auto_approve: bool,
}

impl ClaudeClient {
    /// Create a new client with optional approval handler.
    #[must_use]
    pub fn new(
        log_writer: LogWriter,
        approval_handler: Option<Arc<dyn ApprovalHandler>>,
    ) -> Arc<Self> {
        let auto_approve = approval_handler.is_none();
        Arc::new(Self {
            log_writer,
            approval_handler,
            auto_approve,
        })
    }

    /// Handle can_use_tool request.
    pub(crate) async fn on_can_use_tool(
        &self,
        tool_name: String,
        input: Value,
        tool_use_id: Option<String>,
    ) -> Result<PermissionResult, ClientError> {
        if self.auto_approve {
            return Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            });
        }

        if let Some(tool_use_id) = tool_use_id {
            let handler = self
                .approval_handler
                .as_ref()
                .ok_or(ClientError::ApprovalUnavailable)?;

            let result = handler
                .request_approval(&tool_name, input.clone(), &tool_use_id)
                .await
                .map_err(|e| ClientError::ApprovalFailed(e.to_string()))?;

            match result {
                ApprovalResult::Allow { updated_input } => Ok(PermissionResult::Allow {
                    updated_input,
                    updated_permissions: None,
                }),
                ApprovalResult::Deny { message, interrupt } => Ok(PermissionResult::Deny {
                    message,
                    interrupt,
                }),
            }
        } else {
            // Auto-approve if no tool_use_id
            tracing::warn!(
                "No tool_use_id for tool '{}', auto-approving",
                tool_name
            );
            Ok(PermissionResult::Allow {
                updated_input: input,
                updated_permissions: None,
            })
        }
    }

    /// Handle hook callback.
    pub(crate) async fn on_hook_callback(
        &self,
        callback_id: String,
        _input: Value,
        _tool_use_id: Option<String>,
    ) -> Result<Value, ClientError> {
        if self.auto_approve {
            return Ok(serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "permissionDecisionReason": "Auto-approved"
                }
            }));
        }

        // Forward to can_use_tool by asking
        Ok(serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "ask",
                "permissionDecisionReason": format!("Forwarding {} to approval handler", callback_id)
            }
        }))
    }

    /// Handle non-control message.
    pub(crate) async fn on_non_control(&self, line: &str) {
        if let Err(e) = self.log_writer.log_raw(line).await {
            tracing::error!("Failed to log message: {e}");
        }
    }
}

/// Client error.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Approval handler unavailable")]
    ApprovalUnavailable,
    #[error("Approval failed: {0}")]
    ApprovalFailed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Log writer for agent output.
#[derive(Clone)]
pub struct LogWriter {
    writer: Arc<Mutex<BufWriter<Box<dyn AsyncWrite + Send + Unpin>>>>,
}

impl LogWriter {
    /// Create a new log writer.
    #[must_use]
    pub fn new(writer: impl AsyncWrite + Send + Unpin + 'static) -> Self {
        Self {
            writer: Arc::new(Mutex::new(BufWriter::new(Box::new(writer)))),
        }
    }

    /// Log a raw line.
    ///
    /// # Errors
    /// Returns error if write fails.
    pub async fn log_raw(&self, raw: &str) -> Result<(), std::io::Error> {
        let mut guard = self.writer.lock().await;
        guard.write_all(raw.as_bytes()).await?;
        guard.write_all(b"\n").await?;
        guard.flush().await?;
        Ok(())
    }
}
