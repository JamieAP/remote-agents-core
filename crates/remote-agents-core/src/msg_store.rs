//! Broadcast + history message store for reconnection support.

use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use futures::{StreamExt, future};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::LogMsg;

/// Default history size limit (100 MB).
const HISTORY_BYTES: usize = 100_000 * 1024;

#[derive(Clone)]
struct StoredMsg {
    msg: LogMsg,
    bytes: usize,
}

struct Inner {
    history: VecDeque<StoredMsg>,
    total_bytes: usize,
}

/// Message store with broadcast and history support.
///
/// Essential for reconnection: new clients receive history
/// then seamlessly switch to live updates.
pub struct MsgStore {
    inner: RwLock<Inner>,
    sender: broadcast::Sender<LogMsg>,
}

impl Default for MsgStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgStore {
    /// Create a new message store.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(10000);
        Self {
            inner: RwLock::new(Inner {
                history: VecDeque::with_capacity(32),
                total_bytes: 0,
            }),
            sender,
        }
    }

    /// Push a message to both live listeners and history.
    pub fn push(&self, msg: LogMsg) {
        let _ = self.sender.send(msg.clone()); // live listeners
        let bytes = msg.approx_bytes();

        let mut inner = self.inner.write().unwrap();
        while inner.total_bytes.saturating_add(bytes) > HISTORY_BYTES {
            if let Some(front) = inner.history.pop_front() {
                inner.total_bytes = inner.total_bytes.saturating_sub(front.bytes);
            } else {
                break;
            }
        }
        inner.history.push_back(StoredMsg { msg, bytes });
        inner.total_bytes = inner.total_bytes.saturating_add(bytes);
    }

    /// Push stdout message.
    pub fn push_stdout<S: Into<String>>(&self, s: S) {
        self.push(LogMsg::Stdout(s.into()));
    }

    /// Push stderr message.
    pub fn push_stderr<S: Into<String>>(&self, s: S) {
        self.push(LogMsg::Stderr(s.into()));
    }

    /// Push a JSON patch.
    pub fn push_patch(&self, patch: json_patch::Patch) {
        self.push(LogMsg::JsonPatch(patch));
    }

    /// Push session ID notification.
    pub fn push_session_id(&self, session_id: String) {
        self.push(LogMsg::SessionId(session_id));
    }

    /// Push finished notification.
    pub fn push_finished(&self) {
        self.push(LogMsg::Finished);
    }

    /// Get a receiver for live updates.
    #[must_use]
    pub fn get_receiver(&self) -> broadcast::Receiver<LogMsg> {
        self.sender.subscribe()
    }

    /// Get a snapshot of the history.
    #[must_use]
    pub fn get_history(&self) -> Vec<LogMsg> {
        self.inner
            .read()
            .unwrap()
            .history
            .iter()
            .map(|s| s.msg.clone())
            .collect()
    }

    /// Stream that yields history first, then live updates.
    #[must_use]
    pub fn history_plus_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>> {
        let (history, rx) = (self.get_history(), self.get_receiver());

        let hist = futures::stream::iter(history.into_iter().map(Ok::<_, std::io::Error>));
        let live = BroadcastStream::new(rx)
            .filter_map(|res: Result<LogMsg, _>| async move { res.ok().map(Ok::<_, std::io::Error>) });

        Box::pin(hist.chain(live))
    }

    /// Stream of stdout chunks (until Finished).
    #[must_use]
    pub fn stdout_chunked_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<String, std::io::Error>> {
        self.history_plus_stream()
            .take_while(|res| future::ready(!matches!(res, Ok(LogMsg::Finished))))
            .filter_map(|res| async move {
                match res {
                    Ok(LogMsg::Stdout(s)) => Some(Ok(s)),
                    _ => None,
                }
            })
            .boxed()
    }

    /// Stream of stderr chunks (until Finished).
    #[must_use]
    pub fn stderr_chunked_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<String, std::io::Error>> {
        self.history_plus_stream()
            .take_while(|res| future::ready(!matches!(res, Ok(LogMsg::Finished))))
            .filter_map(|res| async move {
                match res {
                    Ok(LogMsg::Stderr(s)) => Some(Ok(s)),
                    _ => None,
                }
            })
            .boxed()
    }

    /// SSE stream (requires `sse` feature).
    #[cfg(feature = "sse")]
    #[must_use]
    pub fn sse_stream(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<axum::response::sse::Event, std::io::Error>>
    {
        use futures::TryStreamExt;
        self.history_plus_stream()
            .map_ok(|m| m.to_sse_event())
            .boxed()
    }

    /// Forward a stream of log messages into this store.
    pub fn spawn_forwarder<S, E>(self: Arc<Self>, stream: S) -> tokio::task::JoinHandle<()>
    where
        S: futures::Stream<Item = Result<LogMsg, E>> + Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        tokio::spawn(async move {
            tokio::pin!(stream);

            while let Some(next) = stream.next().await {
                match next {
                    Ok(msg) => self.push(msg),
                    Err(e) => self.push(LogMsg::Stderr(format!("stream error: {e}"))),
                }
            }
        })
    }
}
