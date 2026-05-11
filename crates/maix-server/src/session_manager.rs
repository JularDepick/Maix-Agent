use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use maix_agent::Agent;

/// Per-session metadata exposed via ListSessions.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: u64,
}

/// A session handle wraps an owned Agent behind a Mutex.
/// The Option allows the Chat handler to `take()` the Agent for exclusive use.
#[derive(Clone)]
pub struct SessionHandle {
    pub agent: Arc<Mutex<Option<Agent>>>,
    pub meta: SessionMeta,
    pub cancel: CancellationToken,
}

/// Session state held by the server.
pub struct SessionStore {
    handles: RwLock<HashMap<String, SessionHandle>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            handles: RwLock::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, id: String, handle: SessionHandle) {
        self.handles.write().await.insert(id, handle);
    }

    pub async fn get(&self, id: &str) -> Option<SessionHandle> {
        self.handles.read().await.get(id).cloned()
    }

    pub async fn remove(&self, id: &str) -> Option<SessionHandle> {
        let mut lock = self.handles.write().await;
        let removed = lock.remove(id);
        if let Some(ref h) = removed {
            h.cancel.cancel();
        }
        removed
    }

    pub async fn list_meta(&self) -> Vec<SessionMeta> {
        self.handles
            .read()
            .await
            .values()
            .map(|h| h.meta.clone())
            .collect()
    }

    pub async fn count(&self) -> usize {
        self.handles.read().await.len()
    }

    pub async fn increment_message_count(&self, id: &str) {
        if let Some(handle) = self.handles.write().await.get_mut(id) {
            handle.meta.message_count += 1;
        }
    }
}
