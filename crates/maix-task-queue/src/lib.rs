//! Task queue with arbitrary-position insertion (Phase 3).

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

pub type TaskId = String;
pub type AgentId = String;

#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub description: String,
    pub input: String,
    pub priority: u8,
    pub depends_on: Vec<TaskId>,
    pub deadline: Option<Instant>,
    pub retry: RetryPolicy,
    pub created_at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub retries: u32,
}

impl RetryPolicy {
    pub fn new(max_retries: u32) -> Self {
        Self { max_retries, retries: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Suspended,
    Blocked,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct TaskEntry {
    pub task: Task,
    pub status: TaskStatus,
    pub assigned: Option<AgentId>,
    pub started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
}

/// Where to insert a task in the queue.
#[derive(Debug, Clone)]
pub enum InsertAt {
    Head,
    Tail,
    After(TaskId),
    Before(TaskId),
    Index(usize),
}

/// The task queue: O(1) lookup by ID, supports insertion at any position.
pub struct TaskQueue {
    entries: VecDeque<TaskEntry>,
    id_index: HashMap<TaskId, usize>,
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskQueue {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            id_index: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get an entry by ID.
    pub fn get(&self, id: &str) -> Option<&TaskEntry> {
        self.id_index.get(id).and_then(|&idx| self.entries.get(idx))
    }

    /// Add to tail (default).
    pub fn enqueue(&mut self, task: Task) -> TaskId {
        let id = task.id.clone();
        let entry = TaskEntry {
            task,
            status: TaskStatus::Pending,
            assigned: None,
            started_at: None,
            finished_at: None,
        };
        let idx = self.entries.len();
        self.entries.push_back(entry);
        self.id_index.insert(id.clone(), idx);
        self.rebuild_index();
        id
    }

    /// Insert at a specific position.
    pub fn insert(&mut self, task: Task, at: InsertAt) -> Result<TaskId, String> {
        let id = task.id.clone();
        let entry = TaskEntry {
            task,
            status: TaskStatus::Pending,
            assigned: None,
            started_at: None,
            finished_at: None,
        };

        let idx = match &at {
            InsertAt::Head => 0,
            InsertAt::Tail => self.entries.len(),
            InsertAt::After(target_id) => {
                self.id_index
                    .get(target_id)
                    .map(|&i| i + 1)
                    .ok_or_else(|| format!("target task not found: {target_id}"))?
            }
            InsertAt::Before(target_id) => {
                self.id_index
                    .get(target_id)
                    .copied()
                    .ok_or_else(|| format!("target task not found: {target_id}"))?
            }
            InsertAt::Index(i) => (*i).min(self.entries.len()),
        };

        self.entries.insert(idx, entry);
        self.id_index.insert(id.clone(), idx);
        self.rebuild_index();
        Ok(id)
    }

    /// Move an existing task to a new position.
    pub fn reposition(&mut self, task_id: &str, to: InsertAt) -> Result<(), String> {
        let old_idx = self
            .id_index
            .get(task_id)
            .copied()
            .ok_or_else(|| format!("task not found: {task_id}"))?;

        let entry = self.entries.remove(old_idx).unwrap();
        let new_idx = match &to {
            InsertAt::Head => 0,
            InsertAt::Tail => self.entries.len(),
            InsertAt::After(tid) => self
                .id_index
                .get(tid)
                .map(|&i| if i >= old_idx { i } else { i + 1 })
                .unwrap_or(self.entries.len()),
            InsertAt::Before(tid) => self
                .id_index
                .get(tid)
                .copied()
                .unwrap_or(0),
            InsertAt::Index(i) => (*i).min(self.entries.len()),
        };

        self.entries.insert(new_idx, entry);
        self.rebuild_index();
        Ok(())
    }

    /// Change priority.
    pub fn reprioritize(&mut self, task_id: &str, priority: u8) -> Result<(), String> {
        if let Some(idx) = self.id_index.get(task_id) {
            self.entries[*idx].task.priority = priority;
            Ok(())
        } else {
            Err(format!("task not found: {task_id}"))
        }
    }

    /// Suspend a running or pending task.
    pub fn suspend(&mut self, task_id: &str) -> Result<(), String> {
        if let Some(idx) = self.id_index.get(task_id) {
            self.entries[*idx].status = TaskStatus::Suspended;
            Ok(())
        } else {
            Err(format!("task not found: {task_id}"))
        }
    }

    /// Resume a suspended task.
    pub fn resume(&mut self, task_id: &str, at: InsertAt) -> Result<(), String> {
        self.suspend(task_id)?; // Ensure it exists
        let idx = self.id_index[task_id];
        self.entries[idx].status = TaskStatus::Pending;
        self.reposition(task_id, at)?;
        Ok(())
    }

    /// Cancel and remove a task.
    pub fn cancel(&mut self, task_id: &str) -> Option<TaskEntry> {
        let idx = self.id_index.remove(task_id)?;
        let entry = self.entries.remove(idx)?;
        self.rebuild_index();
        Some(entry)
    }

    /// Pop the next ready task (highest priority, no dependencies).
    pub fn pop_next(&mut self) -> Option<TaskEntry> {
        let mut best_idx: Option<usize> = None;
        let mut best_prio: u8 = 0;

        for (i, entry) in self.entries.iter().enumerate() {
            if entry.status != TaskStatus::Pending {
                continue;
            }
            let blocked = entry.task.depends_on.iter().any(|dep_id| {
                self.id_index
                    .get(dep_id)
                    .and_then(|&idx| self.entries.get(idx))
                    .map(|e| !matches!(e.status, TaskStatus::Done))
                    .unwrap_or(true)
            });
            if blocked {
                continue;
            }
            if best_idx.is_none() || entry.task.priority > best_prio {
                best_idx = Some(i);
                best_prio = entry.task.priority;
            }
        }

        if let Some(idx) = best_idx {
            let mut entry = self.entries.remove(idx).unwrap();
            entry.status = TaskStatus::Running;
            entry.started_at = Some(Instant::now());
            let id = entry.task.id.clone();
            self.id_index.remove(&id);
            self.rebuild_index();
            Some(entry)
        } else {
            None
        }
    }

    /// Mark a task done/failed.
    pub fn complete(&mut self, task_id: &str, success: bool) -> Result<(), String> {
        let idx = self
            .id_index
            .get(task_id)
            .copied()
            .ok_or_else(|| format!("task not found: {task_id}"))?;
        self.entries[idx].status = if success {
            TaskStatus::Done
        } else {
            if self.entries[idx].task.retry.retries < self.entries[idx].task.retry.max_retries {
                self.entries[idx].task.retry.retries += 1;
                TaskStatus::Pending
            } else {
                TaskStatus::Failed
            }
        };
        self.entries[idx].finished_at = Some(Instant::now());
        Ok(())
    }

    /// List all entries with their status.
    pub fn list(&self) -> Vec<&TaskEntry> {
        self.entries.iter().collect()
    }

    // -----------------------------------------------------------------------
    // SQLite persistence (Phase 2.1)
    // -----------------------------------------------------------------------

    /// Save all tasks to the database.
    pub fn save_to_db(&self, db: &maix_db::Database) -> Result<(), String> {
        for entry in &self.entries {
            let depends_json =
                serde_json::to_string(&entry.task.depends_on).unwrap_or_else(|_| "[]".into());
            db.insert_task(
                &entry.task.id,
                &entry.task.description,
                &entry.task.input,
                entry.task.priority,
                &depends_json,
                entry.task.retry.max_retries,
            )
            .map_err(|e| format!("db insert: {e}"))?;
            db.update_task_status(
                &entry.task.id,
                &format!("{:?}", entry.status),
                entry.assigned.as_deref(),
            )
            .map_err(|e| format!("db update: {e}"))?;
        }
        Ok(())
    }

    /// Load tasks from the database into the queue.
    pub fn load_from_db(&mut self, db: &maix_db::Database) -> Result<usize, String> {
        let rows = db
            .list_tasks(None)
            .map_err(|e| format!("db list: {e}"))?;
        let mut count = 0;
        for row in rows {
            if self.id_index.contains_key(&row.id) {
                continue;
            }
            let depends_on: Vec<String> =
                serde_json::from_str(&row.depends_on_json).unwrap_or_default();
            let status = match row.status.as_str() {
                "Pending" => TaskStatus::Pending,
                "Running" => TaskStatus::Running,
                "Suspended" => TaskStatus::Suspended,
                "Blocked" => TaskStatus::Blocked,
                "Done" => TaskStatus::Done,
                "Failed" => TaskStatus::Failed,
                _ => TaskStatus::Pending,
            };
            let entry = TaskEntry {
                task: Task {
                    id: row.id.clone(),
                    description: row.description,
                    input: row.input,
                    priority: row.priority,
                    depends_on,
                    deadline: None,
                    retry: RetryPolicy {
                        max_retries: row.max_retries,
                        retries: row.retries,
                    },
                    created_at: std::time::Instant::now(),
                },
                status,
                assigned: row.assigned,
                started_at: None,
                finished_at: None,
            };
            self.entries.push_back(entry);
            self.id_index.insert(row.id, self.entries.len() - 1);
            count += 1;
        }
        self.rebuild_index();
        Ok(count)
    }

    fn rebuild_index(&mut self) {
        self.id_index.clear();
        for (i, entry) in self.entries.iter().enumerate() {
            self.id_index.insert(entry.task.id.clone(), i);
        }
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Serialize queue state to JSON for disk persistence.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        #[derive(serde::Serialize)]
        struct QueueSnapshot {
            entries: Vec<EntrySnapshot>,
        }
        #[derive(serde::Serialize)]
        struct EntrySnapshot {
            id: String,
            description: String,
            input: String,
            priority: u8,
            depends_on: Vec<String>,
            status: String,
            assigned: Option<String>,
            retries: u32,
            max_retries: u32,
        }

        let snapshot = QueueSnapshot {
            entries: self
                .entries
                .iter()
                .map(|e| EntrySnapshot {
                    id: e.task.id.clone(),
                    description: e.task.description.clone(),
                    input: e.task.input.clone(),
                    priority: e.task.priority,
                    depends_on: e.task.depends_on.clone(),
                    status: format!("{:?}", e.status),
                    assigned: e.assigned.clone(),
                    retries: e.task.retry.retries,
                    max_retries: e.task.retry.max_retries,
                })
                .collect(),
        };
        serde_json::to_string_pretty(&snapshot)
    }

    /// Load queue state from JSON, preserving existing entries.
    pub fn from_json(&mut self, json: &str) -> Result<usize, String> {
        #[derive(serde::Deserialize)]
        struct QueueSnapshot {
            entries: Vec<EntrySnapshot>,
        }
        #[derive(serde::Deserialize)]
        struct EntrySnapshot {
            id: String,
            description: String,
            input: String,
            priority: u8,
            depends_on: Vec<String>,
            status: String,
            assigned: Option<String>,
            retries: u32,
            max_retries: u32,
        }

        let snapshot: QueueSnapshot =
            serde_json::from_str(json).map_err(|e| format!("parse queue json: {e}"))?;

        let mut count = 0;
        for s in snapshot.entries {
            // Skip if already present
            if self.id_index.contains_key(&s.id) {
                continue;
            }
            let status = match s.status.as_str() {
                "Pending" => TaskStatus::Pending,
                "Running" => TaskStatus::Running,
                "Suspended" => TaskStatus::Suspended,
                "Blocked" => TaskStatus::Blocked,
                "Done" => TaskStatus::Done,
                "Failed" => TaskStatus::Failed,
                _ => TaskStatus::Pending,
            };
            let entry = TaskEntry {
                task: Task {
                    id: s.id.clone(),
                    description: s.description,
                    input: s.input,
                    priority: s.priority,
                    depends_on: s.depends_on,
                    deadline: None,
                    retry: RetryPolicy { max_retries: s.max_retries, retries: s.retries },
                    created_at: Instant::now(),
                },
                status,
                assigned: s.assigned,
                started_at: None,
                finished_at: None,
            };
            self.entries.push_back(entry);
            self.id_index.insert(s.id, self.entries.len() - 1);
            count += 1;
        }
        self.rebuild_index();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, desc: &str, priority: u8) -> Task {
        Task {
            id: id.into(),
            description: desc.into(),
            input: String::new(),
            priority,
            depends_on: vec![],
            deadline: None,
            retry: RetryPolicy::new(3),
            created_at: Instant::now(),
        }
    }

    #[test]
    fn test_enqueue_and_pop() {
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "task 1", 5));
        q.enqueue(make_task("t2", "task 2", 10));

        let next = q.pop_next().unwrap();
        assert_eq!(next.task.id, "t2"); // higher priority first
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn test_insert_at_head() {
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "first", 5));
        q.insert(make_task("t2", "head", 3), InsertAt::Head).unwrap();

        let next = q.pop_next().unwrap();
        assert_eq!(next.task.id, "t1"); // t1 has higher priority (5 > 3)
    }

    #[test]
    fn test_insert_after() {
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "first", 5));
        q.insert(make_task("t2", "after t1", 10), InsertAt::After("t1".into())).unwrap();

        let next = q.pop_next().unwrap();
        assert_eq!(next.task.id, "t2"); // highest priority overall
    }

    #[test]
    fn test_dependency_blocking() {
        let mut q = TaskQueue::new();
        let mut t1 = make_task("t1", "first", 5);
        t1.depends_on = vec!["t2".into()];
        q.enqueue(t1);
        q.enqueue(make_task("t2", "second", 1));

        let next = q.pop_next().unwrap();
        assert_eq!(next.task.id, "t2"); // t1 blocked by t2 dependency
    }

    #[test]
    fn test_suspend_and_resume() {
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "task", 5));

        q.suspend("t1").unwrap();
        assert!(q.pop_next().is_none()); // suspended, can't dequeue

        q.resume("t1", InsertAt::Head).unwrap();
        let next = q.pop_next().unwrap();
        assert_eq!(next.task.id, "t1");
    }

    #[test]
    fn test_reprioritize() {
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "low", 1));
        q.enqueue(make_task("t2", "also low", 2));
        q.reprioritize("t1", 100).unwrap();

        let next = q.pop_next().unwrap();
        assert_eq!(next.task.id, "t1");
    }

    #[test]
    fn test_persistence_roundtrip() {
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "task one", 5));
        q.enqueue(make_task("t2", "task two", 3));

        let json = q.to_json().unwrap();
        assert!(json.contains("task one"));

        let mut q2 = TaskQueue::new();
        let count = q2.from_json(&json).unwrap();
        assert_eq!(count, 2);
        assert_eq!(q2.len(), 2);

        let next = q2.pop_next().unwrap();
        assert_eq!(next.task.id, "t1"); // higher priority
    }

    #[test]
    fn test_db_persistence_roundtrip() {
        let db = maix_db::Database::open_in_memory().unwrap();
        let mut q = TaskQueue::new();
        q.enqueue(make_task("t1", "db task one", 10));
        q.enqueue(make_task("t2", "db task two", 5));

        q.save_to_db(&db).unwrap();

        let mut q2 = TaskQueue::new();
        let count = q2.load_from_db(&db).unwrap();
        assert_eq!(count, 2);
        assert_eq!(q2.len(), 2);

        let next = q2.pop_next().unwrap();
        assert_eq!(next.task.id, "t1");
    }
}
