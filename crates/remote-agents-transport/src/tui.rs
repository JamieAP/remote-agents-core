//! TUI transport bridge for ratatui applications.

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::protocol::{ClientMessage, ServerMessage};

/// TUI bridge for connecting terminal UI to session.
pub struct TuiBridge {
    /// Sender for client messages.
    pub client_tx: mpsc::UnboundedSender<ClientMessage>,
    /// Receiver for server messages.
    pub server_rx: mpsc::UnboundedReceiver<ServerMessage>,
}

impl TuiBridge {
    /// Create a new TUI bridge.
    ///
    /// Returns the bridge and a channel pair for the session side.
    #[must_use]
    pub fn new() -> (Self, TuiSession) {
        let (client_tx, client_rx) = mpsc::unbounded_channel();
        let (server_tx, server_rx) = mpsc::unbounded_channel();

        let bridge = Self {
            client_tx,
            server_rx,
        };

        let session = TuiSession {
            client_rx,
            server_tx,
        };

        (bridge, session)
    }

    /// Send input to the session.
    ///
    /// # Errors
    /// Returns error if channel is closed.
    pub fn send_input(&self, data: &[u8]) -> Result<(), SendError> {
        self.client_tx
            .send(ClientMessage::input(data))
            .map_err(|_| SendError::ChannelClosed)
    }

    /// Send resize event.
    ///
    /// # Errors
    /// Returns error if channel is closed.
    pub fn send_resize(&self, cols: u16, rows: u16) -> Result<(), SendError> {
        self.client_tx
            .send(ClientMessage::Resize { cols, rows })
            .map_err(|_| SendError::ChannelClosed)
    }

    /// Convert a crossterm key event to input data.
    #[must_use]
    pub fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
        match key.code {
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+A through Ctrl+Z
                    if c.is_ascii_lowercase() {
                        let ctrl_char = (c as u8) - b'a' + 1;
                        return Some(vec![ctrl_char]);
                    }
                }
                let mut buf = [0; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
            KeyCode::Enter => Some(vec![b'\r']),
            KeyCode::Backspace => Some(vec![0x7f]),
            KeyCode::Tab => Some(vec![b'\t']),
            KeyCode::Esc => Some(vec![0x1b]),
            KeyCode::Up => Some(b"\x1b[A".to_vec()),
            KeyCode::Down => Some(b"\x1b[B".to_vec()),
            KeyCode::Right => Some(b"\x1b[C".to_vec()),
            KeyCode::Left => Some(b"\x1b[D".to_vec()),
            KeyCode::Home => Some(b"\x1b[H".to_vec()),
            KeyCode::End => Some(b"\x1b[F".to_vec()),
            KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
            KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
            KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
            KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
            KeyCode::F(n) => {
                let seq = match n {
                    1 => b"\x1bOP".to_vec(),
                    2 => b"\x1bOQ".to_vec(),
                    3 => b"\x1bOR".to_vec(),
                    4 => b"\x1bOS".to_vec(),
                    5 => b"\x1b[15~".to_vec(),
                    6 => b"\x1b[17~".to_vec(),
                    7 => b"\x1b[18~".to_vec(),
                    8 => b"\x1b[19~".to_vec(),
                    9 => b"\x1b[20~".to_vec(),
                    10 => b"\x1b[21~".to_vec(),
                    11 => b"\x1b[23~".to_vec(),
                    12 => b"\x1b[24~".to_vec(),
                    _ => return None,
                };
                Some(seq)
            }
            _ => None,
        }
    }

    /// Handle a crossterm event.
    ///
    /// Returns true if the event was handled.
    pub fn handle_event(&self, event: &Event) -> bool {
        match event {
            Event::Key(key) => {
                if let Some(bytes) = Self::key_to_bytes(key) {
                    let _ = self.send_input(&bytes);
                    return true;
                }
            }
            Event::Resize(cols, rows) => {
                let _ = self.send_resize(*cols, *rows);
                return true;
            }
            _ => {}
        }
        false
    }

    /// Receive a server message (non-blocking).
    pub fn try_recv(&mut self) -> Option<ServerMessage> {
        self.server_rx.try_recv().ok()
    }
}

impl Default for TuiBridge {
    fn default() -> Self {
        Self::new().0
    }
}

/// Session side of the TUI bridge.
pub struct TuiSession {
    /// Receiver for client messages.
    pub client_rx: mpsc::UnboundedReceiver<ClientMessage>,
    /// Sender for server messages.
    pub server_tx: mpsc::UnboundedSender<ServerMessage>,
}

impl TuiSession {
    /// Send output to the TUI.
    ///
    /// # Errors
    /// Returns error if channel is closed.
    pub fn send_output(&self, data: &[u8]) -> Result<(), SendError> {
        self.server_tx
            .send(ServerMessage::output(data))
            .map_err(|_| SendError::ChannelClosed)
    }

    /// Receive a client message.
    pub async fn recv(&mut self) -> Option<ClientMessage> {
        self.client_rx.recv().await
    }
}

/// Send error.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("Channel closed")]
    ChannelClosed,
}

/// Shared state for TUI applications.
pub struct TuiState {
    bridge: Arc<TuiBridge>,
    output_buffer: Vec<u8>,
}

impl TuiState {
    /// Create new TUI state.
    #[must_use]
    pub fn new(bridge: TuiBridge) -> Self {
        Self {
            bridge: Arc::new(bridge),
            output_buffer: Vec::new(),
        }
    }

    /// Get the output buffer.
    #[must_use]
    pub fn output(&self) -> &[u8] {
        &self.output_buffer
    }

    /// Clear the output buffer.
    pub fn clear_output(&mut self) {
        self.output_buffer.clear();
    }
}
