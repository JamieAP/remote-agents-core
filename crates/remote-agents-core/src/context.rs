//! Execution context for agent sessions.

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Generic execution context for agent sessions.
///
/// Unlike vibe-kanban's Task/Project model, this is fully generic
/// and allows apps to store arbitrary metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Working directory for the agent session.
    pub working_dir: PathBuf,

    /// Arbitrary metadata for app-specific needs.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl ExecutionContext {
    /// Create a new execution context with just a working directory.
    #[must_use]
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            metadata: HashMap::new(),
        }
    }

    /// Create a context with metadata.
    #[must_use]
    pub fn with_metadata(working_dir: PathBuf, metadata: HashMap<String, Value>) -> Self {
        Self {
            working_dir,
            metadata,
        }
    }

    /// Get a metadata value by key.
    #[must_use]
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// Set a metadata value.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }
}
