# Remote Agents Core

A framework for managing long-running Claude Code sessions with persistence, history, and multi-tenant support.

## Features

- **Session Persistence** - Store and resume agent sessions
- **Multi-tenant Support** - Manage multiple concurrent sessions
- **Web & TUI Interfaces** - Both xterm.js and ratatui transports
- **Pluggable Storage** - In-memory and SQLite implementations
- **Claude Code Protocol** - Full SDK control protocol support
- **Reconnection Support** - MsgStore with history replay

## Crate Structure

```
remote-agents-core/
├── crates/
│   ├── remote-agents-core/      # Core abstractions (MsgStore, traits)
│   ├── remote-agents-pty/       # PTY session management
│   ├── remote-agents-executor/  # Claude Code SDK protocol
│   ├── remote-agents-session/   # Session orchestration & storage
│   └── remote-agents-transport/ # WebSocket & TUI transports
└── examples/
    ├── web-server/              # Axum + xterm.js example
    └── tui-app/                 # ratatui terminal example
```

## Quick Start

### Web Server Example

```bash
cargo run -p web-server-example
```

Then open http://localhost:3000 in your browser.

### TUI Example

```bash
cargo run -p tui-app-example
```

## Core Types

### ExecutionContext

Generic context for agent sessions:

```rust
use remote_agents_core::ExecutionContext;

let ctx = ExecutionContext::new(PathBuf::from("/my/project"));
ctx.set_metadata("user_id", json!("user123"));
```

### MsgStore

Broadcast + history for reconnection support:

```rust
use remote_agents_core::MsgStore;

let store = MsgStore::new();
store.push_stdout("Hello, world!");

// New clients get history + live updates
let stream = store.history_plus_stream();
```

### SessionStorage Trait

Implement for custom storage backends:

```rust
#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn create(&self, ctx: &ExecutionContext) -> Result<SessionId, StorageError>;
    async fn get(&self, id: SessionId) -> Result<Option<Session>, StorageError>;
    async fn update_status(&self, id: SessionId, status: SessionStatus) -> Result<(), StorageError>;
    // ...
}
```

### ApprovalHandler Trait

Implement to handle tool approval requests:

```rust
#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    async fn request_approval(
        &self,
        tool_name: &str,
        tool_input: Value,
        tool_call_id: &str,
    ) -> Result<ApprovalResult, ApprovalError>;
}
```

## Features

### remote-agents-core
- `sse` - Enable SSE support (adds axum dependency)

### remote-agents-session
- `memory` (default) - In-memory storage
- `sqlite` - SQLite storage

### remote-agents-transport
- `websocket` (default) - WebSocket transport
- `tui` - TUI transport bridge

## License

MIT OR Apache-2.0
