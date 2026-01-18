//! Core traits for storage and execution.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::ExecutionContext;

/// Session identifier.
pub type SessionId = Uuid;

/// Session status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session is queued but not yet started.
    Pending,
    /// Session is currently running.
    Running,
    /// Session completed successfully.
    Completed,
    /// Session failed.
    Failed,
    /// Session was cancelled.
    Cancelled,
}

/// Session filter for queries.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    /// Filter by status.
    pub status: Option<SessionStatus>,
    /// Filter by working directory.
    pub working_dir: Option<PathBuf>,
    /// Limit results.
    pub limit: Option<usize>,
}

/// Persisted session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Execution context.
    pub context: ExecutionContext,
    /// Current status.
    pub status: SessionStatus,
    /// Agent session ID (for follow-up).
    pub agent_session_id: Option<String>,
    /// Creation timestamp (Unix epoch seconds).
    pub created_at: i64,
    /// Last update timestamp.
    pub updated_at: i64,
}

/// Storage error.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Session not found: {0}")]
    NotFound(SessionId),
    #[error("Storage error: {0}")]
    Internal(String),
}

/// Trait for session storage backends.
#[async_trait]
pub trait SessionStorage: Send + Sync {
    /// Create a new session.
    async fn create(&self, ctx: &ExecutionContext) -> Result<SessionId, StorageError>;

    /// Get a session by ID.
    async fn get(&self, id: SessionId) -> Result<Option<Session>, StorageError>;

    /// Update session status.
    async fn update_status(&self, id: SessionId, status: SessionStatus) -> Result<(), StorageError>;

    /// Set agent session ID (for follow-up support).
    async fn set_agent_session_id(
        &self,
        id: SessionId,
        agent_session_id: String,
    ) -> Result<(), StorageError>;

    /// List sessions with optional filter.
    async fn list(&self, filter: SessionFilter) -> Result<Vec<Session>, StorageError>;

    /// Append output data to session.
    async fn append_output(&self, id: SessionId, data: &[u8]) -> Result<(), StorageError>;

    /// Get session output.
    async fn get_output(&self, id: SessionId) -> Result<Vec<u8>, StorageError>;
}

/// Spawned process handle.
pub struct SpawnedProcess {
    /// Child process handle.
    pub child: command_group::AsyncGroupChild,
    /// Receiver for graceful interrupt requests.
    pub interrupt_rx: Option<tokio::sync::oneshot::Receiver<()>>,
}

/// Executor error.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("Spawn failed: {0}")]
    SpawnFailed(String),
    #[error("Executable not found: {0}")]
    ExecutableNotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Command build error: {0}")]
    CommandBuild(String),
}

/// Trait for agent executors.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Spawn a new agent session.
    async fn spawn(
        &self,
        ctx: &ExecutionContext,
        prompt: &str,
    ) -> Result<SpawnedProcess, ExecutorError>;

    /// Spawn a follow-up session (forking from existing).
    async fn spawn_follow_up(
        &self,
        ctx: &ExecutionContext,
        prompt: &str,
        session_id: &str,
    ) -> Result<SpawnedProcess, ExecutorError>;
}
