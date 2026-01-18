//! Claude Code control protocol handler.

use std::sync::Arc;

use futures::FutureExt;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout},
    sync::{Mutex, oneshot},
};

use super::client::ClaudeClient;
use super::types::{
    CLIMessage, ControlRequestType, ControlResponseMessage, ControlResponseType,
    Message, PermissionMode, SDKControlRequest, SDKControlRequestType,
};

/// Protocol error.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Handles bidirectional control protocol communication.
#[derive(Clone)]
pub struct ProtocolPeer {
    stdin: Arc<Mutex<ChildStdin>>,
}

impl ProtocolPeer {
    /// Spawn a new protocol peer.
    ///
    /// This starts a background task to read from stdout and handle control messages.
    #[must_use]
    pub fn spawn(
        stdin: ChildStdin,
        stdout: ChildStdout,
        client: Arc<ClaudeClient>,
        interrupt_rx: oneshot::Receiver<()>,
    ) -> Self {
        let peer = Self {
            stdin: Arc::new(Mutex::new(stdin)),
        };

        let reader_peer = peer.clone();
        tokio::spawn(async move {
            if let Err(e) = reader_peer.read_loop(stdout, client, interrupt_rx).await {
                tracing::error!("Protocol reader loop error: {}", e);
            }
        });

        peer
    }

    async fn read_loop(
        &self,
        stdout: ChildStdout,
        client: Arc<ClaudeClient>,
        interrupt_rx: oneshot::Receiver<()>,
    ) -> Result<(), ProtocolError> {
        let mut reader = BufReader::new(stdout);
        let mut buffer = String::new();
        let mut interrupt_rx = interrupt_rx.fuse();

        loop {
            buffer.clear();
            tokio::select! {
                line_result = reader.read_line(&mut buffer) => {
                    match line_result {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let line = buffer.trim();
                            if line.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<CLIMessage>(line) {
                                Ok(CLIMessage::ControlRequest { request_id, request }) => {
                                    self.handle_control_request(&client, request_id, request).await;
                                }
                                Ok(CLIMessage::ControlResponse { .. }) => {}
                                Ok(CLIMessage::Result(_)) => {
                                    client.on_non_control(line).await;
                                    break;
                                }
                                _ => {
                                    client.on_non_control(line).await;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error reading stdout: {}", e);
                            break;
                        }
                    }
                }
                _ = &mut interrupt_rx => {
                    if let Err(e) = self.interrupt().await {
                        tracing::debug!("Failed to send interrupt to Claude: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_control_request(
        &self,
        client: &Arc<ClaudeClient>,
        request_id: String,
        request: ControlRequestType,
    ) {
        match request {
            ControlRequestType::CanUseTool {
                tool_name,
                input,
                tool_use_id,
                ..
            } => {
                match client.on_can_use_tool(tool_name, input, tool_use_id).await {
                    Ok(result) => {
                        if let Err(e) = self
                            .send_hook_response(request_id, serde_json::to_value(result).unwrap())
                            .await
                        {
                            tracing::error!("Failed to send permission result: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error in on_can_use_tool: {e}");
                        if let Err(e2) = self.send_error(request_id, e.to_string()).await {
                            tracing::error!("Failed to send error response: {e2}");
                        }
                    }
                }
            }
            ControlRequestType::HookCallback {
                callback_id,
                input,
                tool_use_id,
            } => {
                match client.on_hook_callback(callback_id, input, tool_use_id).await {
                    Ok(hook_output) => {
                        if let Err(e) = self.send_hook_response(request_id, hook_output).await {
                            tracing::error!("Failed to send hook callback result: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error in on_hook_callback: {e}");
                        if let Err(e2) = self.send_error(request_id, e.to_string()).await {
                            tracing::error!("Failed to send error response: {e2}");
                        }
                    }
                }
            }
        }
    }

    /// Send a hook response.
    ///
    /// # Errors
    /// Returns error if write fails.
    pub async fn send_hook_response(
        &self,
        request_id: String,
        hook_output: serde_json::Value,
    ) -> Result<(), ProtocolError> {
        self.send_json(&ControlResponseMessage::new(ControlResponseType::Success {
            request_id,
            response: Some(hook_output),
        }))
        .await
    }

    async fn send_error(&self, request_id: String, error: String) -> Result<(), ProtocolError> {
        self.send_json(&ControlResponseMessage::new(ControlResponseType::Error {
            request_id,
            error: Some(error),
        }))
        .await
    }

    async fn send_json<T: serde::Serialize>(&self, message: &T) -> Result<(), ProtocolError> {
        let json = serde_json::to_string(message)?;
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Send a user message.
    ///
    /// # Errors
    /// Returns error if write fails.
    pub async fn send_user_message(&self, content: String) -> Result<(), ProtocolError> {
        let message = Message::new_user(content);
        self.send_json(&message).await
    }

    /// Initialize the protocol.
    ///
    /// # Errors
    /// Returns error if write fails.
    pub async fn initialize(&self, hooks: Option<serde_json::Value>) -> Result<(), ProtocolError> {
        self.send_json(&SDKControlRequest::new(SDKControlRequestType::Initialize { hooks }))
            .await
    }

    /// Send interrupt request.
    ///
    /// # Errors
    /// Returns error if write fails.
    pub async fn interrupt(&self) -> Result<(), ProtocolError> {
        self.send_json(&SDKControlRequest::new(SDKControlRequestType::Interrupt {}))
            .await
    }

    /// Set permission mode.
    ///
    /// # Errors
    /// Returns error if write fails.
    pub async fn set_permission_mode(&self, mode: PermissionMode) -> Result<(), ProtocolError> {
        self.send_json(&SDKControlRequest::new(SDKControlRequestType::SetPermissionMode { mode }))
            .await
    }
}
