//! Example web server with xterm.js terminal support.
//!
//! Run with: cargo run -p web-server-example
//!
//! Then open http://localhost:3000 in your browser.

use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::{Html, IntoResponse},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use futures::{SinkExt, StreamExt};
use remote_agents_pty::PtyService;
use tokio::sync::{mpsc, RwLock};
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

/// Application state shared across handlers.
#[derive(Clone)]
struct AppState {
    pty_service: PtyService,
    working_dir: PathBuf,
    sessions: Arc<RwLock<HashMap<Uuid, Uuid>>>, // ws_id -> pty_session_id
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let state = AppState {
        pty_service: PtyService::new(),
        working_dir,
        sessions: Arc::new(RwLock::new(HashMap::new())),
    };

    // Build router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let ws_id = Uuid::new_v4();

    // Channel for sending messages to the WebSocket
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMsg>();

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
            if ws_sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Create PTY session with default size
    let pty_result = state
        .pty_service
        .create_session(state.working_dir.clone(), 80, 24)
        .await;

    let (session_id, mut pty_output) = match pty_result {
        Ok((id, output)) => (id, output),
        Err(e) => {
            let _ = tx.send(ServerMsg::Error {
                message: format!("Failed to create PTY: {e}"),
            });
            send_task.abort();
            return;
        }
    };

    // Track the session
    state.sessions.write().await.insert(ws_id, session_id);

    let _ = tx.send(ServerMsg::SessionStarted {
        session_id: session_id.to_string(),
    });

    // Spawn task to forward PTY output to WebSocket
    let tx_clone = tx.clone();
    let output_task = tokio::spawn(async move {
        while let Some(data) = pty_output.recv().await {
            let _ = tx_clone.send(ServerMsg::Output {
                data: BASE64.encode(&data),
            });
        }
    });

    // Handle incoming WebSocket messages
    let pty_service = state.pty_service.clone();
    while let Some(msg) = ws_receiver.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Binary(data)) => match String::from_utf8(data.to_vec()) {
                Ok(s) => s.into(),
                Err(_) => continue,
            },
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => {
                tracing::error!("WebSocket error: {e}");
                break;
            }
        };

        let client_msg: ClientMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Invalid client message: {e}");
                continue;
            }
        };

        match client_msg {
            ClientMsg::Input { data } => {
                if let Ok(bytes) = BASE64.decode(&data) {
                    if let Err(e) = pty_service.write(session_id, &bytes).await {
                        tracing::error!("Failed to write to PTY: {e}");
                    }
                }
            }
            ClientMsg::Resize { cols, rows } => {
                if let Err(e) = pty_service.resize(session_id, cols, rows).await {
                    tracing::error!("Failed to resize PTY: {e}");
                }
            }
            ClientMsg::Ping => {
                let _ = tx.send(ServerMsg::Pong);
            }
        }
    }

    // Cleanup
    output_task.abort();
    send_task.abort();
    let _ = state.pty_service.close_session(session_id).await;
    state.sessions.write().await.remove(&ws_id);

    tracing::info!("WebSocket {ws_id} disconnected, PTY session {session_id} closed");
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
    Ping,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg {
    Output { data: String },
    SessionStarted { session_id: String },
    Error { message: String },
    Pong,
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Remote Agents - Terminal</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.css" />
    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.js"></script>
    <style>
        body {
            margin: 0;
            padding: 20px;
            background: #1e1e1e;
            font-family: system-ui, sans-serif;
        }
        h1 { color: #fff; margin-bottom: 10px; }
        #terminal-container {
            width: 100%;
            height: calc(100vh - 100px);
        }
        .status {
            color: #888;
            font-size: 14px;
            margin-bottom: 10px;
        }
        .connected { color: #4a4; }
        .disconnected { color: #a44; }
    </style>
</head>
<body>
    <h1>Remote Agents Terminal</h1>
    <div class="status" id="status">Connecting...</div>
    <div id="terminal-container"></div>

    <script>
        const term = new Terminal({
            cursorBlink: true,
            fontSize: 14,
            fontFamily: 'Menlo, Monaco, "Courier New", monospace',
            theme: {
                background: '#1e1e1e',
                foreground: '#d4d4d4',
            }
        });

        const fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);
        term.open(document.getElementById('terminal-container'));
        fitAddon.fit();

        const status = document.getElementById('status');
        let ws;

        function connect() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            ws = new WebSocket(`${protocol}//${window.location.host}/ws`);

            ws.onopen = () => {
                status.textContent = 'Connected';
                status.className = 'status connected';

                // Send initial resize
                const { cols, rows } = term;
                ws.send(JSON.stringify({ type: 'resize', cols, rows }));
            };

            ws.onclose = () => {
                status.textContent = 'Disconnected - reconnecting...';
                status.className = 'status disconnected';
                setTimeout(connect, 2000);
            };

            ws.onerror = (err) => {
                console.error('WebSocket error:', err);
            };

            ws.onmessage = (event) => {
                try {
                    const msg = JSON.parse(event.data);
                    if (msg.type === 'output' && msg.data) {
                        const decoded = atob(msg.data);
                        term.write(decoded);
                    } else if (msg.type === 'session_started') {
                        console.log('Session started:', msg.session_id);
                    } else if (msg.type === 'error') {
                        term.writeln(`\r\n[Error: ${msg.message}]\r\n`);
                    }
                } catch (e) {
                    console.error('Failed to parse message:', e);
                }
            };
        }

        // Handle terminal input
        term.onData((data) => {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'input',
                    data: btoa(data)
                }));
            }
        });

        // Handle resize
        window.addEventListener('resize', () => {
            fitAddon.fit();
            if (ws && ws.readyState === WebSocket.OPEN) {
                const { cols, rows } = term;
                ws.send(JSON.stringify({ type: 'resize', cols, rows }));
            }
        });

        // Start connection
        connect();
    </script>
</body>
</html>
"#;
