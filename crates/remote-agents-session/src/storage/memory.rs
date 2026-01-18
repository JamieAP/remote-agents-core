//! In-memory session storage.

use std::{
    collections::HashMap,
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use remote_agents_core::{
    ExecutionContext,
    traits::{Session, SessionFilter, SessionId, SessionStatus, SessionStorage, StorageError},
};
use uuid::Uuid;

/// In-memory storage implementation.
///
/// Useful for development and single-process deployments.
/// Data is lost on restart.
pub struct MemoryStorage {
    sessions: RwLock<HashMap<SessionId, Session>>,
    outputs: RwLock<HashMap<SessionId, Vec<u8>>>,
}

impl MemoryStorage {
    /// Create a new in-memory storage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            outputs: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[async_trait]
impl SessionStorage for MemoryStorage {
    async fn create(&self, ctx: &ExecutionContext) -> Result<SessionId, StorageError> {
        let id = Uuid::new_v4();
        let timestamp = now();

        let session = Session {
            id,
            context: ctx.clone(),
            status: SessionStatus::Pending,
            agent_session_id: None,
            created_at: timestamp,
            updated_at: timestamp,
        };

        self.sessions
            .write()
            .map_err(|e| StorageError::Internal(e.to_string()))?
            .insert(id, session);

        self.outputs
            .write()
            .map_err(|e| StorageError::Internal(e.to_string()))?
            .insert(id, Vec::new());

        Ok(id)
    }

    async fn get(&self, id: SessionId) -> Result<Option<Session>, StorageError> {
        Ok(self
            .sessions
            .read()
            .map_err(|e| StorageError::Internal(e.to_string()))?
            .get(&id)
            .cloned())
    }

    async fn update_status(&self, id: SessionId, status: SessionStatus) -> Result<(), StorageError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| StorageError::Internal(e.to_string()))?;

        let session = sessions.get_mut(&id).ok_or(StorageError::NotFound(id))?;

        session.status = status;
        session.updated_at = now();

        Ok(())
    }

    async fn set_agent_session_id(
        &self,
        id: SessionId,
        agent_session_id: String,
    ) -> Result<(), StorageError> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| StorageError::Internal(e.to_string()))?;

        let session = sessions.get_mut(&id).ok_or(StorageError::NotFound(id))?;

        session.agent_session_id = Some(agent_session_id);
        session.updated_at = now();

        Ok(())
    }

    async fn list(&self, filter: SessionFilter) -> Result<Vec<Session>, StorageError> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| StorageError::Internal(e.to_string()))?;

        let mut result: Vec<Session> = sessions
            .values()
            .filter(|s| {
                if let Some(status) = filter.status {
                    if s.status != status {
                        return false;
                    }
                }
                if let Some(ref working_dir) = filter.working_dir {
                    if s.context.working_dir != *working_dir {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by created_at descending
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }

        Ok(result)
    }

    async fn append_output(&self, id: SessionId, data: &[u8]) -> Result<(), StorageError> {
        let mut outputs = self
            .outputs
            .write()
            .map_err(|e| StorageError::Internal(e.to_string()))?;

        let output = outputs.get_mut(&id).ok_or(StorageError::NotFound(id))?;

        output.extend_from_slice(data);

        Ok(())
    }

    async fn get_output(&self, id: SessionId) -> Result<Vec<u8>, StorageError> {
        let outputs = self
            .outputs
            .read()
            .map_err(|e| StorageError::Internal(e.to_string()))?;

        outputs
            .get(&id)
            .cloned()
            .ok_or(StorageError::NotFound(id))
    }
}
