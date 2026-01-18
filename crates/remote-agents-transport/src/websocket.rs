//! WebSocket transport for web terminals.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::protocol::{ClientMessage, ServerMessage};

/// WebSocket handler state.
#[derive(Clone)]
pub struct WsState<S> {
    /// Application state.
    pub app_state: Arc<S>,
}

impl<S> WsState<S> {
    /// Create new WebSocket state.
    #[must_use]
    pub fn new(app_state: Arc<S>) -> Self {
        Self { app_state }
    }
}

/// WebSocket upgrade handler.
///
/// Use this as an Axum route handler.
pub async fn ws_handler<S>(
    ws: WebSocketUpgrade,
    State(state): State<WsState<S>>,
) -> impl IntoResponse
where
    S: Send + Sync + 'static,
{
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket<S>(socket: WebSocket, _state: WsState<S>)
where
    S: Send + Sync + 'static,
{
    let (mut sender, mut receiver) = socket.split();

    // Channel for sending messages to the client
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Spawn task to forward messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("Failed to serialize message: {e}");
                    continue;
                }
            };
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Binary(data)) => {
                match String::from_utf8(data.to_vec()) {
                    Ok(s) => s.into(),
                    Err(_) => continue,
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => {
                tracing::error!("WebSocket error: {e}");
                break;
            }
        };

        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Invalid client message: {e}");
                let _ = tx.send(ServerMessage::Error {
                    message: format!("Invalid message: {e}"),
                });
                continue;
            }
        };

        match client_msg {
            ClientMessage::Ping => {
                let _ = tx.send(ServerMessage::Pong);
            }
            ClientMessage::Input { data: _ } => {
                // TODO: Forward to PTY/session
            }
            ClientMessage::Resize { cols: _, rows: _ } => {
                // TODO: Resize PTY
            }
            ClientMessage::StartSession { working_dir: _, prompt: _ } => {
                // TODO: Start session
                let _ = tx.send(ServerMessage::SessionStarted {
                    session_id: "placeholder".to_string(),
                });
            }
            ClientMessage::ContinueSession { session_id: _, prompt: _ } => {
                // TODO: Continue session
            }
            ClientMessage::Interrupt => {
                // TODO: Interrupt session
            }
        }
    }

    send_task.abort();
}

/// Create WebSocket router.
///
/// # Example
/// ```ignore
/// let app = Router::new()
///     .merge(create_ws_router(app_state));
/// ```
#[must_use]
pub fn create_ws_router<S>(state: Arc<S>) -> axum::Router
where
    S: Send + Sync + 'static + Clone,
{
    axum::Router::new()
        .route("/ws", axum::routing::get(ws_handler::<S>))
        .with_state(WsState::new(state))
}
