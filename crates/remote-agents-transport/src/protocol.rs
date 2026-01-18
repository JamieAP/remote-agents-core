//! Wire protocol for client-server communication.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

/// Message from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Terminal input data (base64 encoded).
    Input { data: String },
    /// Resize terminal.
    Resize { cols: u16, rows: u16 },
    /// Start a new session.
    StartSession { working_dir: String, prompt: String },
    /// Continue existing session.
    ContinueSession { session_id: String, prompt: String },
    /// Interrupt current session.
    Interrupt,
    /// Ping for keepalive.
    Ping,
}

impl ClientMessage {
    /// Create an input message from raw bytes.
    #[must_use]
    pub fn input(data: &[u8]) -> Self {
        Self::Input {
            data: BASE64.encode(data),
        }
    }

    /// Decode input data from base64.
    #[must_use]
    pub fn decode_input(&self) -> Option<Vec<u8>> {
        if let Self::Input { data } = self {
            BASE64.decode(data).ok()
        } else {
            None
        }
    }
}

/// Message from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Terminal output data (base64 encoded).
    Output { data: String },
    /// Session started.
    SessionStarted { session_id: String },
    /// Session ended.
    SessionEnded { session_id: String, success: bool },
    /// Error message.
    Error { message: String },
    /// Pong response.
    Pong,
}

impl ServerMessage {
    /// Create an output message from raw bytes.
    #[must_use]
    pub fn output(data: &[u8]) -> Self {
        Self::Output {
            data: BASE64.encode(data),
        }
    }

    /// Decode output data from base64.
    #[must_use]
    pub fn decode_output(&self) -> Option<Vec<u8>> {
        if let Self::Output { data } = self {
            BASE64.decode(data).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_roundtrip() {
        let original = b"Hello, World!";
        let msg = ClientMessage::input(original);
        let decoded = msg.decode_input().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_output_roundtrip() {
        let original = b"Response data";
        let msg = ServerMessage::output(original);
        let decoded = msg.decode_output().unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_message_serialization() {
        let msg = ClientMessage::Resize { cols: 80, rows: 24 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("resize"));

        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        if let ClientMessage::Resize { cols, rows } = parsed {
            assert_eq!(cols, 80);
            assert_eq!(rows, 24);
        } else {
            panic!("Wrong message type");
        }
    }
}
