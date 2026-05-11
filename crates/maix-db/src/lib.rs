//! Unified SQLite database layer for Maix-Agent (Phase 2.1).

use rusqlite::{params, Connection, Transaction};
use std::path::Path;

pub type DbResult<T> = Result<T, rusqlite::Error>;

const SCHEMA_VERSION: u32 = 1;

/// Central database holding all persistent state.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the database at `path`, running migrations automatically.
    pub fn open(path: &Path) -> DbResult<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn transaction(&mut self) -> DbResult<Transaction<'_>> {
        self.conn.transaction()
    }

    // -------------------------------------------------------------------
    // Schema migration
    // -------------------------------------------------------------------

    fn migrate(&mut self) -> DbResult<()> {
        let version: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);

        if version < 1 {
            self.conn.execute_batch(CREATE_SCHEMA_V1)?;
            self.conn.execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION};"))?;
        }

        if version > SCHEMA_VERSION {
            tracing::warn!("DB version {version} > code version {SCHEMA_VERSION}, proceeding");
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Memories
    // -------------------------------------------------------------------

    pub fn insert_memory(
        &self,
        id: &str,
        content: &str,
        kind: &str,
        importance: f32,
        session_id: Option<&str>,
        embedding: Option<&[f32]>,
    ) -> DbResult<()> {
        let emb_blob: Option<Vec<u8>> = embedding.map(|v| {
            v.iter()
                .flat_map(|f| f.to_le_bytes())
                .collect()
        });
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO memories (id, content, kind, importance, session_id, embedding, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(id) DO UPDATE SET content=excluded.content, importance=excluded.importance, updated_at=excluded.updated_at",
            params![id, content, kind, importance, session_id, emb_blob, now],
        )?;
        Ok(())
    }

    pub fn search_memories(
        &self,
        query: &str,
        kind: Option<&str>,
        limit: usize,
    ) -> DbResult<Vec<MemoryRow>> {
        let query_pattern = format!("%{query}%");
        if let Some(k) = kind {
            self.query_memories(
                "SELECT id, content, kind, importance, session_id, created_at
                 FROM memories WHERE kind=?1 AND content LIKE ?2
                 ORDER BY importance DESC, created_at DESC LIMIT ?3",
                params![k, query_pattern, limit as i64],
            )
        } else {
            self.query_memories(
                "SELECT id, content, kind, importance, session_id, created_at
                 FROM memories WHERE content LIKE ?1
                 ORDER BY importance DESC, created_at DESC LIMIT ?2",
                params![query_pattern, limit as i64],
            )
        }
    }

    fn query_memories(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> DbResult<Vec<MemoryRow>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, map_memory_row)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn list_memories(&self, kind: Option<&str>, limit: usize) -> DbResult<Vec<MemoryRow>> {
        if let Some(k) = kind {
            self.query_memories(
                "SELECT id, content, kind, importance, session_id, created_at
                 FROM memories WHERE kind=?1 ORDER BY created_at DESC LIMIT ?2",
                params![k, limit as i64],
            )
        } else {
            self.query_memories(
                "SELECT id, content, kind, importance, session_id, created_at
                 FROM memories ORDER BY created_at DESC LIMIT ?1",
                params![limit as i64],
            )
        }
    }

    pub fn delete_memory(&self, id: &str) -> DbResult<bool> {
        let n = self.conn.execute("DELETE FROM memories WHERE id=?1", params![id])?;
        Ok(n > 0)
    }

    pub fn memory_count(&self) -> DbResult<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))
            .map(|c| c as usize)
    }

    // -------------------------------------------------------------------
    // Tasks
    // -------------------------------------------------------------------

    pub fn insert_task(
        &self,
        id: &str,
        description: &str,
        input: &str,
        priority: u8,
        depends_on_json: &str,
        max_retries: u32,
    ) -> DbResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO tasks (id, description, input, priority, status, depends_on_json, max_retries, created_at)
             VALUES (?1, ?2, ?3, ?4, 'Pending', ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET description=excluded.description, priority=excluded.priority",
            params![id, description, input, priority, depends_on_json, max_retries, now],
        )?;
        Ok(())
    }

    pub fn update_task_status(
        &self,
        id: &str,
        status: &str,
        assigned: Option<&str>,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE tasks SET status=?2, assigned=?3, started_at=CASE WHEN ?2='Running' THEN ?4 ELSE started_at END, finished_at=CASE WHEN ?2 IN ('Done','Failed') THEN ?4 ELSE finished_at END WHERE id=?1",
            params![id, status, assigned, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_tasks(&self, status: Option<&str>) -> DbResult<Vec<TaskRow>> {
        if let Some(s) = status {
            self.query_tasks(
                "SELECT id, description, input, priority, status, depends_on_json, assigned, retries, max_retries, created_at, started_at, finished_at
                 FROM tasks WHERE status=?1 ORDER BY priority DESC",
                params![s],
            )
        } else {
            self.query_tasks(
                "SELECT id, description, input, priority, status, depends_on_json, assigned, retries, max_retries, created_at, started_at, finished_at
                 FROM tasks ORDER BY priority DESC",
                [],
            )
        }
    }

    fn query_tasks(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> DbResult<Vec<TaskRow>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, map_task_row)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn delete_task(&self, id: &str) -> DbResult<bool> {
        let n = self.conn.execute("DELETE FROM tasks WHERE id=?1", params![id])?;
        Ok(n > 0)
    }

    pub fn task_count(&self) -> DbResult<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, i64>(0))
            .map(|c| c as usize)
    }

    // -------------------------------------------------------------------
    // Sessions
    // -------------------------------------------------------------------

    pub fn create_session(&self, id: &str, name: &str) -> DbResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(id) DO UPDATE SET updated_at=?3",
            params![id, name, now],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> DbResult<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, updated_at, message_count FROM sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], map_session_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn delete_session(&self, id: &str) -> DbResult<bool> {
        let n = self.conn.execute("DELETE FROM sessions WHERE id=?1", params![id])?;
        Ok(n > 0)
    }

    // -------------------------------------------------------------------
    // Messages
    // -------------------------------------------------------------------

    /// Insert a message into a session and bump message_count.
    pub fn insert_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        tool_calls_json: Option<&str>,
        token_count: u64,
    ) -> DbResult<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO messages (session_id, role, content, tool_calls_json, token_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session_id, role, content, tool_calls_json, token_count as i64, now],
        )?;
        let msg_id = self.conn.last_insert_rowid();

        // Auto-update session message_count
        self.conn.execute(
            "UPDATE sessions SET message_count = message_count + 1, updated_at = ?2 WHERE id = ?1",
            params![session_id, now],
        )?;

        Ok(msg_id)
    }

    /// List messages for a session, ordered by creation time.
    pub fn list_messages(&self, session_id: &str, limit: Option<usize>) -> DbResult<Vec<MessageRow>> {
        let limit = limit.unwrap_or(500) as i64;
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, role, content, tool_calls_json, token_count, created_at
             FROM messages WHERE session_id = ?1 ORDER BY id ASC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![session_id, limit], map_message_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Delete all messages for a session.
    pub fn delete_messages_for_session(&self, session_id: &str) -> DbResult<usize> {
        let n = self.conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(n)
    }

    // -------------------------------------------------------------------
    // Identities
    // -------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_identity(
        &self,
        id: &str,
        name: &str,
        description: &str,
        system_prompt: &str,
        traits_json: &str,
        domains_json: &str,
        tone: &str,
    ) -> DbResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO identities (id, name, description, system_prompt, personality_traits_json, knowledge_domains_json, tone, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT(name) DO UPDATE SET description=excluded.description, system_prompt=excluded.system_prompt, updated_at=excluded.updated_at",
            params![id, name, description, system_prompt, traits_json, domains_json, tone, now],
        )?;
        Ok(())
    }

    pub fn list_identities(&self) -> DbResult<Vec<IdentityRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, system_prompt, personality_traits_json, knowledge_domains_json, tone, created_at
             FROM identities ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], map_identity_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    // -------------------------------------------------------------------
    // Skills
    // -------------------------------------------------------------------

    pub fn insert_skill(
        &self,
        id: &str,
        name: &str,
        version: &str,
        runtime: &str,
        manifest_json: &str,
    ) -> DbResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO skills (id, name, version, runtime, manifest_json, enabled, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
             ON CONFLICT(name) DO UPDATE SET version=excluded.version, manifest_json=excluded.manifest_json",
            params![id, name, version, runtime, manifest_json, now],
        )?;
        Ok(())
    }

    pub fn update_skill_enabled(&self, name: &str, enabled: bool) -> DbResult<()> {
        self.conn.execute(
            "UPDATE skills SET enabled=?2 WHERE name=?1",
            params![name, enabled as i32],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, name: &str) -> DbResult<bool> {
        let n = self.conn.execute("DELETE FROM skills WHERE name=?1", params![name])?;
        Ok(n > 0)
    }

    pub fn list_skills(&self) -> DbResult<Vec<SkillRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, version, runtime, manifest_json, enabled, installed_at FROM skills ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], map_skill_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    // -------------------------------------------------------------------
    // Architectures
    // -------------------------------------------------------------------

    pub fn insert_architecture(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        config_json: &str,
    ) -> DbResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO agent_architectures (id, name, description, config_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET config_json=excluded.config_json",
            params![id, name, description.unwrap_or(""), config_json, now],
        )?;
        Ok(())
    }

    pub fn list_architectures(&self) -> DbResult<Vec<ArchitectureRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, config_json, created_at FROM agent_architectures ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], map_architecture_row)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    // -------------------------------------------------------------------
    // JSON import (for migrating from Phase 1 JSON files)
    // -------------------------------------------------------------------

    /// Import old JSONL memory files into SQLite.
    pub fn import_memory_jsonl(
        &self,
        dir: &Path,
        kind_filter: Option<&str>,
    ) -> DbResult<usize> {
        let mut count = 0;
        if dir.exists() {
            for entry in std::fs::read_dir(dir).ok().into_iter().flatten() {
                let entry = entry.ok();
                let path = entry.as_ref().map(|e| e.path());
                if let Some(p) = path {
                    if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            for line in content.lines().filter(|l| !l.trim().is_empty()) {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                                    let kind = val["kind"].as_str().unwrap_or("semantic");
                                    if kind_filter.is_none_or(|k| k == kind) {
                                        let id = val["id"].as_str().unwrap_or("");
                                        let content = val["content"].as_str().unwrap_or("");
                                        let importance =
                                            val["importance"].as_f64().unwrap_or(0.5) as f32;
                                        let session_id =
                                            val["metadata"]["session_id"].as_str();
                                        if !id.is_empty() {
                                            let _ = self.insert_memory(
                                                id, content, kind, importance, session_id, None,
                                            );
                                            count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Raw connection access for advanced usage.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MemoryRow {
    pub id: String,
    pub content: String,
    pub kind: String,
    pub importance: f32,
    pub session_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: String,
    pub description: String,
    pub input: String,
    pub priority: u8,
    pub status: String,
    pub depends_on_json: String,
    pub assigned: Option<String>,
    pub retries: u32,
    pub max_retries: u32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
}

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls_json: Option<String>,
    pub token_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct IdentityRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub personality_traits_json: String,
    pub knowledge_domains_json: String,
    pub tone: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SkillRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub runtime: String,
    pub manifest_json: String,
    pub enabled: bool,
    pub installed_at: String,
}

#[derive(Debug, Clone)]
pub struct ArchitectureRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub config_json: String,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------------------

fn map_memory_row(row: &rusqlite::Row) -> rusqlite::Result<MemoryRow> {
    Ok(MemoryRow {
        id: row.get(0)?,
        content: row.get(1)?,
        kind: row.get(2)?,
        importance: row.get(3)?,
        session_id: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn map_task_row(row: &rusqlite::Row) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        id: row.get(0)?,
        description: row.get(1)?,
        input: row.get(2)?,
        priority: row.get(3)?,
        status: row.get(4)?,
        depends_on_json: row.get(5)?,
        assigned: row.get(6)?,
        retries: row.get(7)?,
        max_retries: row.get(8)?,
        created_at: row.get(9)?,
        started_at: row.get(10)?,
        finished_at: row.get(11)?,
    })
}

fn map_session_row(row: &rusqlite::Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        message_count: row.get(4)?,
    })
}

fn map_message_row(row: &rusqlite::Row) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        tool_calls_json: row.get(4)?,
        token_count: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn map_identity_row(row: &rusqlite::Row) -> rusqlite::Result<IdentityRow> {
    Ok(IdentityRow {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        system_prompt: row.get(3)?,
        personality_traits_json: row.get(4)?,
        knowledge_domains_json: row.get(5)?,
        tone: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_skill_row(row: &rusqlite::Row) -> rusqlite::Result<SkillRow> {
    Ok(SkillRow {
        id: row.get(0)?,
        name: row.get(1)?,
        version: row.get(2)?,
        runtime: row.get(3)?,
        manifest_json: row.get(4)?,
        enabled: row.get::<_, i32>(5)? != 0,
        installed_at: row.get(6)?,
    })
}

fn map_architecture_row(row: &rusqlite::Row) -> rusqlite::Result<ArchitectureRow> {
    Ok(ArchitectureRow {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        config_json: row.get(3)?,
        created_at: row.get(4)?,
    })
}

// ---------------------------------------------------------------------------
// SQL Schema V1
// ---------------------------------------------------------------------------

const CREATE_SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('episodic','semantic','working')),
    importance REAL DEFAULT 0.5,
    session_id TEXT,
    embedding BLOB,
    metadata_json TEXT DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind);
CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    input TEXT DEFAULT '',
    priority INTEGER DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'Pending'
        CHECK(status IN ('Pending','Running','Suspended','Blocked','Done','Failed')),
    depends_on_json TEXT DEFAULT '[]',
    assigned TEXT,
    retries INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority DESC);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT 'Untitled',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    message_count INTEGER DEFAULT 0,
    metadata_json TEXT DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    tool_calls_json TEXT,
    token_count INTEGER DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);

CREATE TABLE IF NOT EXISTS identities (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    personality_traits_json TEXT DEFAULT '[]',
    knowledge_domains_json TEXT DEFAULT '[]',
    tone TEXT DEFAULT 'professional',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    version TEXT NOT NULL,
    runtime TEXT NOT NULL DEFAULT 'prompt-only',
    manifest_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    installed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_architectures (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    config_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.memory_count().unwrap(), 0);
    }

    #[test]
    fn test_memory_crud() {
        let db = Database::open_in_memory().unwrap();
        db.insert_memory("m1", "hello world", "semantic", 0.5, None, None)
            .unwrap();
        assert_eq!(db.memory_count().unwrap(), 1);

        let rows = db.search_memories("hello", None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content, "hello world");

        assert!(db.delete_memory("m1").unwrap());
        assert_eq!(db.memory_count().unwrap(), 0);
    }

    #[test]
    fn test_memory_upsert() {
        let db = Database::open_in_memory().unwrap();
        db.insert_memory("m1", "v1", "semantic", 0.5, None, None)
            .unwrap();
        db.insert_memory("m1", "v2", "semantic", 0.8, None, None)
            .unwrap();
        let rows = db.list_memories(None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].content, "v2");
        assert!((rows[0].importance - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_task_crud() {
        let db = Database::open_in_memory().unwrap();
        db.insert_task("t1", "test task", "do stuff", 5, "[]", 3)
            .unwrap();
        assert_eq!(db.task_count().unwrap(), 1);

        db.update_task_status("t1", "Running", Some("agent-1"))
            .unwrap();
        let tasks = db.list_tasks(Some("Running")).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].assigned.as_deref(), Some("agent-1"));

        assert!(db.delete_task("t1").unwrap());
    }

    #[test]
    fn test_session_crud() {
        let db = Database::open_in_memory().unwrap();
        db.create_session("s1", "test session").unwrap();
        let sessions = db.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "test session");
        assert!(db.delete_session("s1").unwrap());
    }

    #[test]
    fn test_identity_crud() {
        let db = Database::open_in_memory().unwrap();
        db.insert_identity("i1", "test-identity", "desc", "you are a tester", "[]", "[]", "casual")
            .unwrap();
        let ids = db.list_identities().unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].tone, "casual");
    }

    #[test]
    fn test_skill_crud() {
        let db = Database::open_in_memory().unwrap();
        db.insert_skill("sk1", "test-skill", "1.0", "prompt-only", "{}")
            .unwrap();
        let skills = db.list_skills().unwrap();
        assert_eq!(skills.len(), 1);
        assert!(skills[0].enabled);

        db.update_skill_enabled("test-skill", false).unwrap();
        let skills = db.list_skills().unwrap();
        assert!(!skills[0].enabled);

        assert!(db.delete_skill("test-skill").unwrap());
    }

    #[test]
    fn test_architecture_crud() {
        let db = Database::open_in_memory().unwrap();
        db.insert_architecture("a1", "test-arch", Some("test desc"), "{}")
            .unwrap();
        let archs = db.list_architectures().unwrap();
        assert_eq!(archs.len(), 1);
    }

    #[test]
    fn test_message_crud() {
        let db = Database::open_in_memory().unwrap();
        db.create_session("s1", "test").unwrap();

        let msg_id = db.insert_message("s1", "user", "Hello", None, 10).unwrap();
        assert!(msg_id > 0);

        let msgs = db.list_messages("s1", None).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");

        // Check message_count updated
        let sessions = db.list_sessions().unwrap();
        assert_eq!(sessions[0].message_count, 1);

        let deleted = db.delete_messages_for_session("s1").unwrap();
        assert_eq!(deleted, 1);
    }

    #[test]
    fn test_migration_idempotent() {
        // Opening twice = migration runs twice, should be safe
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        // If we get here without error, idempotent migration works
    }
}
