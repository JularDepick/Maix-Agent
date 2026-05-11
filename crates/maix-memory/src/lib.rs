//! Memory system — working / episodic / semantic with file-based + SQLite storage (Phase 2).

pub mod compaction;
pub mod embedding;
pub mod retrieval;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Opaque memory entry ID.
pub type MemoryId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub content: String,
    pub kind: MemoryKind,
    pub importance: f32,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Episodic,
    Semantic,
    Working,
}

/// Trait that all memory stores must implement.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save(&mut self, entry: MemoryEntry) -> MaixResult<MemoryId>;
    async fn search(&self, query: &str, limit: usize) -> MaixResult<Vec<MemoryEntry>>;
    async fn forget(&mut self, id: &str) -> MaixResult<()>;
    async fn get_context_for_session(&self, session_id: &str, max_tokens: usize) -> MaixResult<String>;
    async fn list_all(&self) -> MaixResult<Vec<MemoryEntry>>;
}

/// File-based memory store (Phase 1).
/// Episodic memories stored per-session, semantic memories global.
pub struct FileMemoryStore {
    base_path: PathBuf,
    /// Episodic: keyed by session_id
    episodic: HashMap<String, Vec<MemoryEntry>>,
    /// Semantic: global KV store
    semantic: Vec<MemoryEntry>,
}

impl FileMemoryStore {
    pub fn new(base_path: PathBuf) -> MaixResult<Self> {
        std::fs::create_dir_all(base_path.join("episodic"))?;
        std::fs::create_dir_all(base_path.join("semantic"))?;
        let mut store = Self {
            base_path,
            episodic: HashMap::new(),
            semantic: Vec::new(),
        };
        store.load_all()?;
        Ok(store)
    }

    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    fn load_all(&mut self) -> MaixResult<()> {
        // Load semantic memories
        let sem_path = self.base_path.join("semantic").join("store.jsonl");
        if sem_path.exists() {
            let content = std::fs::read_to_string(&sem_path)?;
            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(entry) = serde_json::from_str::<MemoryEntry>(line) {
                    self.semantic.push(entry);
                }
            }
        }

        // Load episodic memories
        let epi_dir = self.base_path.join("episodic");
        if epi_dir.exists() {
            for entry in std::fs::read_dir(&epi_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|e| e == "jsonl")
                {
                    let session_id = entry
                        .path()
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let content = std::fs::read_to_string(entry.path())?;
                    let entries: Vec<MemoryEntry> = content
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .filter_map(|l| serde_json::from_str(l).ok())
                        .collect();
                    if !entries.is_empty() {
                        self.episodic.insert(session_id, entries);
                    }
                }
            }
        }
        Ok(())
    }

    fn persist_episodic(&self, session_id: &str) -> MaixResult<()> {
        let path = self.base_path.join("episodic").join(format!("{session_id}.jsonl"));
        if let Some(entries) = self.episodic.get(session_id) {
            let mut content = String::new();
            for e in entries {
                content.push_str(&serde_json::to_string(e)?);
                content.push('\n');
            }
            std::fs::write(&path, content)?;
        }
        Ok(())
    }

    fn persist_semantic(&self) -> MaixResult<()> {
        let path = self.base_path.join("semantic").join("store.jsonl");
        let mut content = String::new();
        for e in &self.semantic {
            content.push_str(&serde_json::to_string(e)?);
            content.push('\n');
        }
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Simple keyword-based search (v1 — replaced by vector search in Phase 2).
    fn keyword_score(entry: &MemoryEntry, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let content_lower = entry.content.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut score = 0.0_f32;
        for term in &query_terms {
            if content_lower.contains(term) {
                score += 1.0;
            }
        }
        // Boost by importance and recency
        score *= entry.importance;
        score
    }
}

#[async_trait]
impl MemoryStore for FileMemoryStore {
    async fn save(&mut self, entry: MemoryEntry) -> MaixResult<MemoryId> {
        let id = entry.id.clone();
        match entry.kind {
            MemoryKind::Episodic | MemoryKind::Working => {
                let session_id = entry
                    .metadata
                    .get("session_id")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());
                self.episodic
                    .entry(session_id.clone())
                    .or_default()
                    .push(entry);
                self.persist_episodic(&session_id)?;
            }
            MemoryKind::Semantic => {
                // Upsert by id
                if let Some(existing) = self.semantic.iter_mut().find(|e| e.id == id) {
                    *existing = entry;
                } else {
                    self.semantic.push(entry);
                }
                self.persist_semantic()?;
            }
        }
        Ok(id)
    }

    async fn search(&self, query: &str, limit: usize) -> MaixResult<Vec<MemoryEntry>> {
        let mut scored: Vec<(f32, &MemoryEntry)> = Vec::new();
        let query_blank = query.trim().is_empty();

        for entries in self.episodic.values() {
            for entry in entries {
                let score = if query_blank { entry.importance } else { Self::keyword_score(entry, query) };
                if score > 0.0 || query_blank {
                    scored.push((score, entry));
                }
            }
        }
        for entry in &self.semantic {
            let score = if query_blank { entry.importance } else { Self::keyword_score(entry, query) };
            if score > 0.0 || query_blank {
                scored.push((score, entry));
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let results: Vec<MemoryEntry> = scored
            .into_iter()
            .take(limit)
            .map(|(_, entry)| entry.clone())
            .collect();
        Ok(results)
    }

    async fn forget(&mut self, id: &str) -> MaixResult<()> {
        // Search episodic
        let mut found_session = None;
        for (sid, entries) in self.episodic.iter_mut() {
            if let Some(pos) = entries.iter().position(|e| e.id == id) {
                entries.remove(pos);
                found_session = Some(sid.clone());
                break;
            }
        }
        if let Some(sid) = found_session {
            self.persist_episodic(&sid)?;
            return Ok(());
        }

        // Search semantic
        if let Some(pos) = self.semantic.iter().position(|e| e.id == id) {
            self.semantic.remove(pos);
            self.persist_semantic()?;
            return Ok(());
        }
        Err(maix_core::MaixError::Memory("memory id not found".into()))
    }

    async fn get_context_for_session(&self, session_id: &str, max_tokens: usize) -> MaixResult<String> {
        let mut parts: Vec<String> = Vec::new();
        let mut char_count = 0;
        let char_limit = max_tokens * 3; // rough estimate: 1 token ~ 3 chars

        // Relevant semantic memories first
        for entry in self.semantic.iter().filter(|e| e.importance > 0.5) {
            let text = format!("[Memory: {}] {}", entry.id, entry.content);
            if char_count + text.len() > char_limit {
                break;
            }
            char_count += text.len();
            parts.push(text);
        }

        // Recent episodic memories for this session
        if let Some(entries) = self.episodic.get(session_id) {
            for entry in entries.iter().rev().take(10) {
                let text = format!("[History] {}", entry.content);
                if char_count + text.len() > char_limit {
                    break;
                }
                char_count += text.len();
                parts.push(text);
            }
        }

        Ok(parts.join("\n"))
    }

    async fn list_all(&self) -> MaixResult<Vec<MemoryEntry>> {
        let mut all: Vec<MemoryEntry> = self.semantic.clone();
        for entries in self.episodic.values() {
            all.extend(entries.clone());
        }
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all)
    }
}

/// SQLite-backed memory store (Phase 2.1).
pub struct SqliteMemoryStore {
    db: Arc<Mutex<maix_db::Database>>,
}

impl SqliteMemoryStore {
    pub fn new(db_path: &Path) -> MaixResult<Self> {
        let db = maix_db::Database::open(db_path)
            .map_err(|e| maix_core::MaixError::Memory(format!("open db: {e}")))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    pub fn new_in_memory() -> MaixResult<Self> {
        let db = maix_db::Database::open_in_memory()
            .map_err(|e| maix_core::MaixError::Memory(format!("open db: {e}")))?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    pub fn import_jsonl(&self, dir: &Path) -> MaixResult<usize> {
        let db = self.db.lock().unwrap();
        Ok(db.import_memory_jsonl(dir, None).unwrap_or(0))
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn save(&mut self, entry: MemoryEntry) -> MaixResult<MemoryId> {
        let kind = kind_to_str(&entry.kind);
        let session_id = entry.metadata.get("session_id").map(|s| s.as_str());
        let id = entry.id.clone();
        let db = self.db.lock().unwrap();
        db.insert_memory(&id, &entry.content, kind, entry.importance, session_id, None)
            .map_err(|e| maix_core::MaixError::Memory(format!("insert memory: {e}")))?;
        Ok(id)
    }

    async fn search(&self, query: &str, limit: usize) -> MaixResult<Vec<MemoryEntry>> {
        let db = self.db.lock().unwrap();
        let rows = db
            .search_memories(query, None, limit)
            .map_err(|e| maix_core::MaixError::Memory(format!("search memories: {e}")))?;
        Ok(rows.into_iter().map(|r| MemoryEntry {
            id: r.id,
            content: r.content,
            kind: str_to_kind(&r.kind),
            importance: r.importance,
            created_at: r.created_at.parse().unwrap_or_else(|_| Utc::now()),
            metadata: HashMap::new(),
        }).collect())
    }

    async fn forget(&mut self, id: &str) -> MaixResult<()> {
        let db = self.db.lock().unwrap();
        let deleted = db.delete_memory(id)
            .map_err(|e| maix_core::MaixError::Memory(format!("delete memory: {e}")))?;
        if deleted {
            Ok(())
        } else {
            Err(maix_core::MaixError::Memory("memory id not found".into()))
        }
    }

    async fn get_context_for_session(&self, session_id: &str, max_tokens: usize) -> MaixResult<String> {
        let db = self.db.lock().unwrap();
        let rows = db
            .list_memories(None, 50)
            .map_err(|e| maix_core::MaixError::Memory(format!("list memories: {e}")))?;
        let mut parts: Vec<String> = Vec::new();
        let mut char_count = 0;
        let char_limit = max_tokens * 3;
        for row in rows {
            let text = format!("[Memory: {}] {}", row.id, row.content);
            if char_count + text.len() > char_limit {
                break;
            }
            char_count += text.len();
            parts.push(text);
        }
        let _ = session_id;
        Ok(parts.join("\n"))
    }

    async fn list_all(&self) -> MaixResult<Vec<MemoryEntry>> {
        let db = self.db.lock().unwrap();
        let rows = db
            .list_memories(None, 1000)
            .map_err(|e| maix_core::MaixError::Memory(format!("list memories: {e}")))?;
        Ok(rows.into_iter().map(|r| MemoryEntry {
            id: r.id,
            content: r.content,
            kind: str_to_kind(&r.kind),
            importance: r.importance,
            created_at: r.created_at.parse().unwrap_or_else(|_| Utc::now()),
            metadata: HashMap::new(),
        }).collect())
    }
}

fn kind_to_str(kind: &MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Episodic => "episodic",
        MemoryKind::Semantic => "semantic",
        MemoryKind::Working => "working",
    }
}

fn str_to_kind(s: &str) -> MemoryKind {
    match s {
        "episodic" => MemoryKind::Episodic,
        "working" => MemoryKind::Working,
        _ => MemoryKind::Semantic,
    }
}

/// Wraps a shared `Arc<RwLock<Box<dyn MemoryStore>>>` so multiple Agents
/// can share a single memory store through interior mutability.
pub struct SharedMemoryProxy {
    inner: Arc<tokio::sync::RwLock<Box<dyn MemoryStore>>>,
}

impl SharedMemoryProxy {
    pub fn new(inner: Arc<tokio::sync::RwLock<Box<dyn MemoryStore>>>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl MemoryStore for SharedMemoryProxy {
    async fn save(&mut self, entry: MemoryEntry) -> MaixResult<MemoryId> {
        self.inner.write().await.save(entry).await
    }
    async fn search(&self, query: &str, limit: usize) -> MaixResult<Vec<MemoryEntry>> {
        self.inner.read().await.search(query, limit).await
    }
    async fn forget(&mut self, id: &str) -> MaixResult<()> {
        self.inner.write().await.forget(id).await
    }
    async fn get_context_for_session(&self, session_id: &str, max_tokens: usize) -> MaixResult<String> {
        self.inner.read().await.get_context_for_session(session_id, max_tokens).await
    }
    async fn list_all(&self) -> MaixResult<Vec<MemoryEntry>> {
        self.inner.read().await.list_all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_entry(id: &str, content: &str, kind: MemoryKind) -> MemoryEntry {
        MemoryEntry {
            id: id.into(),
            content: content.into(),
            kind,
            importance: 1.0,
            created_at: Utc::now(),
            metadata: {
                let mut m = HashMap::new();
                m.insert("session_id".into(), "test-session".into());
                m
            },
        }
    }

    #[tokio::test]
    async fn test_save_and_search_episodic() {
        let tmp = TempDir::new().unwrap();
        let mut store = FileMemoryStore::new(tmp.path().to_path_buf()).unwrap();

        let entry = make_entry("ep1", "Rust is the primary language for this project", MemoryKind::Episodic);
        store.save(entry).await.unwrap();

        let results = store.search("Rust project", 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].content.contains("Rust"));
    }

    #[tokio::test]
    async fn test_save_and_search_semantic() {
        let tmp = TempDir::new().unwrap();
        let mut store = FileMemoryStore::new(tmp.path().to_path_buf()).unwrap();

        let mut entry = make_entry("sem1", "User prefers Rust over Python", MemoryKind::Semantic);
        entry.metadata.remove("session_id");
        store.save(entry).await.unwrap();

        let results = store.search("Rust Python", 5).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_forget() {
        let tmp = TempDir::new().unwrap();
        let mut store = FileMemoryStore::new(tmp.path().to_path_buf()).unwrap();

        let entry = make_entry("ep1", "test content", MemoryKind::Episodic);
        store.save(entry).await.unwrap();
        store.forget("ep1").await.unwrap();

        let results = store.search("test content", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_persistence() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        {
            let mut store = FileMemoryStore::new(path.clone()).unwrap();
            let entry = make_entry("ep1", "persistent data", MemoryKind::Episodic);
            store.save(entry).await.unwrap();
        }

        // Reopen and check
        let store = FileMemoryStore::new(path).unwrap();
        let results = store.search("persistent", 5).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "ep1");
    }

    mod sqlite_tests {
        use super::*;

        #[tokio::test]
        async fn test_sqlite_save_and_search() {
            let mut store = SqliteMemoryStore::new_in_memory().unwrap();
            let entry = make_entry("s1", "Rust is a systems programming language", MemoryKind::Semantic);
            store.save(entry).await.unwrap();

            let results = store.search("Rust", 5).await.unwrap();
            assert!(!results.is_empty());
            assert!(results[0].content.contains("Rust"));
        }

        #[tokio::test]
        async fn test_sqlite_forget() {
            let mut store = SqliteMemoryStore::new_in_memory().unwrap();
            let entry = make_entry("s1", "temporary note", MemoryKind::Working);
            store.save(entry).await.unwrap();
            store.forget("s1").await.unwrap();

            let results = store.search("temporary", 5).await.unwrap();
            assert!(results.is_empty());
        }
    }
}
