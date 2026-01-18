//! SQLite session storage (feature-gated).

// TODO: Implement SQLite storage using sqlx.
// This is a placeholder for the feature-gated implementation.

use async_trait::async_trait;
use remote_agents_core::{
    ExecutionContext,
    traits::{Session, SessionFilter, SessionId, SessionStatus, SessionStorage, StorageError},
};

/// SQLite storage implementation.
pub struct SqliteStorage {
    // pool: sqlx::SqlitePool,
}

impl SqliteStorage {
    /// Create a new SQLite storage.
    ///
    /// # Errors
    /// Returns error if database connection fails.
    pub async fn new(_database_url: &str) -> Result<Self, StorageError> {
        // TODO: Implement
        Err(StorageError::Internal("SQLite storage not yet implemented".to_string()))
    }
}

#[async_trait]
impl SessionStorage for SqliteStorage {
    async fn create(&self, _ctx: &ExecutionContext) -> Result<SessionId, StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }

    async fn get(&self, _id: SessionId) -> Result<Option<Session>, StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }

    async fn update_status(&self, _id: SessionId, _status: SessionStatus) -> Result<(), StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }

    async fn set_agent_session_id(
        &self,
        _id: SessionId,
        _agent_session_id: String,
    ) -> Result<(), StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }

    async fn list(&self, _filter: SessionFilter) -> Result<Vec<Session>, StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }

    async fn append_output(&self, _id: SessionId, _data: &[u8]) -> Result<(), StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }

    async fn get_output(&self, _id: SessionId) -> Result<Vec<u8>, StorageError> {
        Err(StorageError::Internal("Not implemented".to_string()))
    }
}
