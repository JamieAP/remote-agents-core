//! Example web server with xterm.js terminal support.
//!
//! Run with: cargo run --example web-server
//!
//! Then open http://localhost:3000 in your browser.

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Router,
    response::Html,
    routing::get,
};
use remote_agents_session::storage::MemoryStorage;
use remote_agents_transport::websocket::create_ws_router;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Application state shared across handlers.
#[derive(Clone)]
struct AppState {
    #[allow(dead_code)]
    storage: Arc<MemoryStorage>,
    #[allow(dead_code)]
    working_dir: PathBuf,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Create storage
    let storage = Arc::new(MemoryStorage::new());
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let state = AppState {
        storage,
        working_dir,
    };

    // Build router
    let app = Router::new()
        .route("/", get(index_handler))
        .merge(create_ws_router(Arc::new(state.clone())))
        .layer(CorsLayer::permissive());

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
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
                        term.writeln(`\r\n[Session ${msg.session_id} started]\r\n`);
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

        // Display welcome message
        term.writeln('Remote Agents Core - Web Terminal Example');
        term.writeln('==========================================');
        term.writeln('');
        term.writeln('This is a minimal example demonstrating the transport layer.');
        term.writeln('Connect to a Claude Code session by implementing the session manager.');
        term.writeln('');
    </script>
</body>
</html>
"#;
