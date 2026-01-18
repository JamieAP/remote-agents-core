//! Session manager for orchestrating agent sessions.

use std::sync::Arc;

use remote_agents_core::{
    ExecutionContext, MsgStore,
    traits::{Executor, ExecutorError, SessionId, SessionStatus, SessionStorage, StorageError},
};
use tokio::sync::RwLock;

/// Session manager error.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Executor error: {0}")]
    Executor(#[from] ExecutorError),
    #[error("Session not found: {0}")]
    NotFound(SessionId),
    #[error("Session already running")]
    AlreadyRunning,
}

/// Active session state.
struct ActiveSession {
    msg_store: Arc<MsgStore>,
    interrupt_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

/// Session manager for orchestrating agent sessions.
pub struct SessionManager<S, E>
where
    S: SessionStorage,
    E: Executor,
{
    storage: S,
    executor: E,
    active_sessions: RwLock<std::collections::HashMap<SessionId, ActiveSession>>,
}

impl<S, E> SessionManager<S, E>
where
    S: SessionStorage,
    E: Executor,
{
    /// Create a new session manager.
    #[must_use]
    pub fn new(storage: S, executor: E) -> Self {
        Self {
            storage,
            executor,
            active_sessions: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Start a new session.
    ///
    /// # Errors
    /// Returns error if session creation or spawn fails.
    pub async fn start_session(
        &self,
        ctx: ExecutionContext,
        prompt: &str,
    ) -> Result<SessionId, ManagerError> {
        let session_id = self.storage.create(&ctx).await?;
        self.storage
            .update_status(session_id, SessionStatus::Running)
            .await?;

        let msg_store = Arc::new(MsgStore::new());
        let process = self.executor.spawn(&ctx, prompt).await?;

        let active = ActiveSession {
            msg_store: Arc::clone(&msg_store),
            interrupt_tx: None, // TODO: Wire up interrupt
        };

        self.active_sessions.write().await.insert(session_id, active);

        // TODO: Spawn output forwarding task

        drop(process); // Placeholder - should be managed

        Ok(session_id)
    }

    /// Start a follow-up session.
    ///
    /// # Errors
    /// Returns error if session not found or spawn fails.
    pub async fn start_follow_up(
        &self,
        original_session_id: SessionId,
        prompt: &str,
    ) -> Result<SessionId, ManagerError> {
        let session = self
            .storage
            .get(original_session_id)
            .await?
            .ok_or(ManagerError::NotFound(original_session_id))?;

        let agent_session_id = session
            .agent_session_id
            .ok_or(ManagerError::NotFound(original_session_id))?;

        let new_session_id = self.storage.create(&session.context).await?;
        self.storage
            .update_status(new_session_id, SessionStatus::Running)
            .await?;

        let msg_store = Arc::new(MsgStore::new());
        let process = self
            .executor
            .spawn_follow_up(&session.context, prompt, &agent_session_id)
            .await?;

        let active = ActiveSession {
            msg_store: Arc::clone(&msg_store),
            interrupt_tx: None,
        };

        self.active_sessions
            .write()
            .await
            .insert(new_session_id, active);

        drop(process); // Placeholder

        Ok(new_session_id)
    }

    /// Get the message store for a session.
    pub async fn get_msg_store(&self, session_id: SessionId) -> Option<Arc<MsgStore>> {
        self.active_sessions
            .read()
            .await
            .get(&session_id)
            .map(|s| Arc::clone(&s.msg_store))
    }

    /// Interrupt a running session.
    ///
    /// # Errors
    /// Returns error if session not found.
    pub async fn interrupt_session(&self, session_id: SessionId) -> Result<(), ManagerError> {
        let mut sessions = self.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            if let Some(tx) = session.interrupt_tx.take() {
                let _ = tx.send(());
            }
        }
        Ok(())
    }
}
