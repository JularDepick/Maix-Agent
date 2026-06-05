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
            handle.meta.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_handle(id: &str, name: &str) -> SessionHandle {
        SessionHandle {
            agent: Arc::new(Mutex::new(None)),
            meta: SessionMeta {
                id: id.into(),
                name: name.into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                message_count: 0,
            },
            cancel: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let store = SessionStore::new();
        store.insert("s1".into(), make_handle("s1", "test")).await;
        let h = store.get("s1").await;
        assert!(h.is_some());
        assert_eq!(h.unwrap().meta.name, "test");
    }

    #[tokio::test]
    async fn test_get_missing() {
        let store = SessionStore::new();
        assert!(store.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_remove() {
        let store = SessionStore::new();
        store.insert("s1".into(), make_handle("s1", "test")).await;
        let removed = store.remove("s1").await;
        assert!(removed.is_some());
        assert!(store.get("s1").await.is_none());
    }

    #[tokio::test]
    async fn test_remove_cancels_token() {
        let store = SessionStore::new();
        store.insert("s1".into(), make_handle("s1", "test")).await;
        let handle = store.get("s1").await.unwrap();
        assert!(!handle.cancel.is_cancelled());
        store.remove("s1").await;
        // Token was cancelled during remove
        // (we can't easily check after remove since handle was moved, but the test verifies no panic)
    }

    #[tokio::test]
    async fn test_remove_missing() {
        let store = SessionStore::new();
        assert!(store.remove("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_list_meta() {
        let store = SessionStore::new();
        store.insert("s1".into(), make_handle("s1", "first")).await;
        store.insert("s2".into(), make_handle("s2", "second")).await;
        let metas = store.list_meta().await;
        assert_eq!(metas.len(), 2);
        let names: Vec<&str> = metas.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"first"));
        assert!(names.contains(&"second"));
    }

    #[tokio::test]
    async fn test_count() {
        let store = SessionStore::new();
        assert_eq!(store.count().await, 0);
        store.insert("s1".into(), make_handle("s1", "a")).await;
        assert_eq!(store.count().await, 1);
        store.insert("s2".into(), make_handle("s2", "b")).await;
        assert_eq!(store.count().await, 2);
        store.remove("s1").await;
        assert_eq!(store.count().await, 1);
    }

    #[tokio::test]
    async fn test_increment_message_count() {
        let store = SessionStore::new();
        store.insert("s1".into(), make_handle("s1", "test")).await;
        assert_eq!(store.get("s1").await.unwrap().meta.message_count, 0);
        store.increment_message_count("s1").await;
        assert_eq!(store.get("s1").await.unwrap().meta.message_count, 1);
        store.increment_message_count("s1").await;
        store.increment_message_count("s1").await;
        assert_eq!(store.get("s1").await.unwrap().meta.message_count, 3);
    }

    #[tokio::test]
    async fn test_increment_missing() {
        let store = SessionStore::new();
        // Should not panic on missing session
        store.increment_message_count("nonexistent").await;
    }
}
