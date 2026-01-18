//! Command building utilities.

use std::path::PathBuf;

use remote_agents_pty::resolve_executable_path;
use thiserror::Error;

/// Command build error.
#[derive(Debug, Error)]
pub enum CommandBuildError {
    #[error("Base command cannot be parsed: {0}")]
    InvalidBase(String),
    #[error("Base command is empty after parsing")]
    EmptyCommand,
    #[error("Failed to quote command: {0}")]
    QuoteError(#[from] shlex::QuoteError),
    #[error("Invalid shell parameters: {0}")]
    InvalidShellParams(String),
}

/// Parsed command parts (program + args).
#[derive(Debug, Clone)]
pub struct CommandParts {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandParts {
    /// Create new command parts.
    #[must_use]
    pub fn new(program: String, args: Vec<String>) -> Self {
        Self { program, args }
    }

    /// Resolve the program to an absolute path.
    ///
    /// # Errors
    /// Returns error if executable not found.
    pub async fn into_resolved(self) -> Result<(PathBuf, Vec<String>), CommandBuildError> {
        let Self { program, args } = self;
        let executable = resolve_executable_path(&program)
            .await
            .ok_or_else(|| CommandBuildError::InvalidBase(format!("Executable not found: {program}")))?;
        Ok((executable, args))
    }
}

/// Builder for constructing commands.
#[derive(Debug, Clone)]
pub struct CommandBuilder {
    /// Base executable command.
    pub base: String,
    /// Optional parameters to append.
    pub params: Option<Vec<String>>,
}

impl CommandBuilder {
    /// Create a new command builder.
    #[must_use]
    pub fn new<S: Into<String>>(base: S) -> Self {
        Self {
            base: base.into(),
            params: None,
        }
    }

    /// Add parameters.
    #[must_use]
    pub fn params<I>(mut self, params: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.params = Some(params.into_iter().map(Into::into).collect());
        self
    }

    /// Override the base command.
    #[must_use]
    pub fn override_base<S: Into<String>>(mut self, base: S) -> Self {
        self.base = base.into();
        self
    }

    /// Extend parameters.
    #[must_use]
    pub fn extend_params<I>(mut self, more: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        let extra: Vec<String> = more.into_iter().map(Into::into).collect();
        match &mut self.params {
            Some(p) => p.extend(extra),
            None => self.params = Some(extra),
        }
        self
    }

    /// Build command for initial invocation.
    ///
    /// # Errors
    /// Returns error if command is invalid.
    pub fn build_initial(&self) -> Result<CommandParts, CommandBuildError> {
        self.build(&[])
    }

    /// Build command for follow-up invocation.
    ///
    /// # Errors
    /// Returns error if command is invalid.
    pub fn build_follow_up(&self, additional_args: &[String]) -> Result<CommandParts, CommandBuildError> {
        self.build(additional_args)
    }

    fn build(&self, additional_args: &[String]) -> Result<CommandParts, CommandBuildError> {
        let mut parts = vec![];
        let base_parts = split_command_line(&self.base)?;
        parts.extend(base_parts);
        if let Some(ref params) = self.params {
            parts.extend(params.clone());
        }
        parts.extend(additional_args.iter().cloned());

        if parts.is_empty() {
            return Err(CommandBuildError::EmptyCommand);
        }

        let program = parts.remove(0);
        Ok(CommandParts::new(program, parts))
    }
}

fn split_command_line(input: &str) -> Result<Vec<String>, CommandBuildError> {
    #[cfg(windows)]
    {
        let parts = winsplit::split(input);
        if parts.is_empty() {
            Err(CommandBuildError::EmptyCommand)
        } else {
            Ok(parts)
        }
    }

    #[cfg(not(windows))]
    {
        shlex::split(input).ok_or_else(|| CommandBuildError::InvalidBase(input.to_string()))
    }
}
