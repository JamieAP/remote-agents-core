//! Approval handling for tool invocations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Approval status for a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Tool invocation is approved.
    Approved,
    /// Tool invocation is denied.
    Denied { reason: Option<String> },
    /// Approval request timed out.
    TimedOut,
    /// Approval is still pending.
    Pending,
}

/// Result of an approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "behavior", rename_all = "camelCase")]
pub enum ApprovalResult {
    /// Allow the tool invocation.
    Allow {
        #[serde(rename = "updatedInput")]
        updated_input: Value,
    },
    /// Deny the tool invocation.
    Deny {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt: Option<bool>,
    },
}

/// Approval error.
#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("Approval service unavailable")]
    ServiceUnavailable,
    #[error("Approval request failed: {0}")]
    RequestFailed(String),
    #[error("Approval request timed out")]
    TimedOut,
}

/// Trait for handling tool approval requests.
///
/// Implement this trait to integrate with your approval UI/system.
/// The framework provides the protocol; your app implements the UX.
#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    /// Request approval for a tool invocation.
    ///
    /// # Arguments
    /// * `tool_name` - Name of the tool being invoked
    /// * `tool_input` - Input to the tool
    /// * `tool_call_id` - Unique identifier for this tool call
    ///
    /// # Returns
    /// Approval result indicating whether to allow or deny.
    async fn request_approval(
        &self,
        tool_name: &str,
        tool_input: Value,
        tool_call_id: &str,
    ) -> Result<ApprovalResult, ApprovalError>;
}

/// No-op approval handler that auto-approves everything.
#[derive(Debug, Default, Clone)]
pub struct AutoApproveHandler;

#[async_trait]
impl ApprovalHandler for AutoApproveHandler {
    async fn request_approval(
        &self,
        _tool_name: &str,
        tool_input: Value,
        _tool_call_id: &str,
    ) -> Result<ApprovalResult, ApprovalError> {
        Ok(ApprovalResult::Allow {
            updated_input: tool_input,
        })
    }
}
