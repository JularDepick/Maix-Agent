//! Scheduler — cron-like task scheduling with file change monitoring.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// A scheduled job definition.
#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: Schedule,
    pub enabled: bool,
    pub last_run: Option<Instant>,
    pub run_count: u64,
}

/// Schedule type.
#[derive(Debug, Clone)]
pub enum Schedule {
    /// Run every N seconds.
    Interval(u64),
    /// Run at specific times (simplified: hour:minute pairs).
    Daily { hour: u32, minute: u32 },
    /// Run when files matching patterns change.
    FileChange { patterns: Vec<String> },
}

impl std::fmt::Display for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interval(secs) => write!(f, "every {}s", secs),
            Self::Daily { hour, minute } => write!(f, "daily {:02}:{:02}", hour, minute),
            Self::FileChange { patterns } => write!(f, "on change: {}", patterns.join(", ")),
        }
    }
}

/// File change detection state.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub modified: std::time::SystemTime,
    pub size: u64,
}

/// Scheduler manages scheduled jobs and file change monitoring.
pub struct Scheduler {
    jobs: Vec<ScheduledJob>,
    file_snapshots: HashMap<PathBuf, FileSnapshot>,
    watch_patterns: Vec<String>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            file_snapshots: HashMap::new(),
            watch_patterns: Vec::new(),
        }
    }

    /// Add a scheduled job.
    pub fn add_job(&mut self, job: ScheduledJob) {
        self.jobs.push(job);
    }

    /// Remove a job by ID.
    pub fn remove_job(&mut self, id: &str) -> bool {
        let len_before = self.jobs.len();
        self.jobs.retain(|j| j.id != id);
        self.jobs.len() < len_before
    }

    /// Enable or disable a job.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == id) {
            job.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// List all jobs.
    pub fn list_jobs(&self) -> &[ScheduledJob] {
        &self.jobs
    }

    /// Check which interval-based jobs are due to run.
    pub fn check_due_jobs(&mut self) -> Vec<String> {
        let now = Instant::now();
        let mut due = Vec::new();

        for job in &mut self.jobs {
            if !job.enabled {
                continue;
            }

            match &job.schedule {
                Schedule::Interval(secs) => {
                    let interval = Duration::from_secs(*secs);
                    let should_run = match job.last_run {
                        Some(last) => now.duration_since(last) >= interval,
                        None => true,
                    };

                    if should_run {
                        job.last_run = Some(now);
                        job.run_count += 1;
                        due.push(job.id.clone());
                    }
                }
                Schedule::Daily { .. } => {
                    // Daily scheduling would need wall-clock time comparison
                    // For now, skip in tick-based checking
                }
                Schedule::FileChange { .. } => {
                    // File change jobs are triggered by check_file_changes()
                }
            }
        }

        due
    }

    /// Add file patterns to watch.
    pub fn watch_files(&mut self, patterns: Vec<String>) {
        self.watch_patterns.extend(patterns);
    }

    /// Take a snapshot of files matching watch patterns.
    pub async fn snapshot_files(&mut self, root: &std::path::Path) -> MaixResult<()> {
        let mut stack = vec![root.to_path_buf()];
        let skip_dirs = [".git", "node_modules", "target", ".venv", "__pycache__"];

        while let Some(dir) = stack.pop() {
            let entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut entry_stream = entries;
            while let Some(entry) = entry_stream.next_entry().await.unwrap_or(None) {
                let ft = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if ft.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }

                if !ft.is_file() {
                    continue;
                }

                let path = entry.path();
                let meta = match entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                self.file_snapshots.insert(path.clone(), FileSnapshot {
                    path,
                    modified: meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    size: meta.len(),
                });
            }
        }

        Ok(())
    }

    /// Check for file changes since last snapshot. Returns changed file paths.
    pub async fn check_file_changes(&mut self, root: &std::path::Path) -> Vec<PathBuf> {
        let mut changed = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        let skip_dirs = [".git", "node_modules", "target", ".venv", "__pycache__"];

        while let Some(dir) = stack.pop() {
            let entries = match tokio::fs::read_dir(&dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut entry_stream = entries;
            while let Some(entry) = entry_stream.next_entry().await.unwrap_or(None) {
                let ft = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if ft.is_dir() {
                    if !skip_dirs.contains(&name_str.as_ref()) {
                        stack.push(entry.path());
                    }
                    continue;
                }

                if !ft.is_file() {
                    continue;
                }

                let path = entry.path();
                let meta = match entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let current_modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let current_size = meta.len();

                if let Some(snapshot) = self.file_snapshots.get(&path) {
                    if snapshot.modified != current_modified || snapshot.size != current_size {
                        changed.push(path.clone());
                    }
                } else {
                    // New file
                    changed.push(path.clone());
                }

                // Update snapshot
                self.file_snapshots.insert(path.clone(), FileSnapshot {
                    path,
                    modified: current_modified,
                    size: current_size,
                });
            }
        }

        // Trigger file-change jobs
        for job in &mut self.jobs {
            if !job.enabled {
                continue;
            }
            if let Schedule::FileChange { patterns } = &job.schedule {
                let matches = changed.iter().any(|path| {
                    let path_str = path.to_string_lossy();
                    patterns.iter().any(|p| path_str.contains(p.as_str()))
                });
                if matches {
                    job.last_run = Some(Instant::now());
                    job.run_count += 1;
                }
            }
        }

        changed
    }

    /// Format scheduler status.
    pub fn format_status(&self) -> String {
        if self.jobs.is_empty() {
            return "No scheduled jobs.".to_string();
        }

        let mut lines = vec![format!("Scheduled Jobs ({}):", self.jobs.len())];
        for job in &self.jobs {
            let status = if job.enabled { "enabled" } else { "disabled" };
            let last = job.last_run
                .map(|t| format!("{:.0}s ago", t.elapsed().as_secs()))
                .unwrap_or_else(|| "never".to_string());
            lines.push(format!(
                "  {} [{}] {} - {} (last: {}, runs: {})",
                job.id, status, job.name, job.schedule, last, job.run_count
            ));
        }

        if !self.watch_patterns.is_empty() {
            lines.push(String::new());
            lines.push(format!("Watching: {}", self.watch_patterns.join(", ")));
        }

        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// List scheduled jobs.
pub struct ScheduleListTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for ScheduleListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "schedule_list".into(),
            description: "List all scheduled jobs and their status.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let scheduler = self.0.lock().await;
        Ok(scheduler.format_status())
    }
}

/// Add a scheduled job.
pub struct ScheduleAddTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for ScheduleAddTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "schedule_add".into(),
            description: "Add a scheduled job with an interval or daily schedule.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique job ID" },
                    "name": { "type": "string", "description": "Job name" },
                    "prompt": { "type": "string", "description": "Prompt to execute" },
                    "interval_secs": { "type": "integer", "description": "Run every N seconds (for interval schedule)" },
                    "hour": { "type": "integer", "description": "Hour to run (for daily schedule)" },
                    "minute": { "type": "integer", "description": "Minute to run (for daily schedule)" }
                },
                "required": ["id", "name", "prompt"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let id = args["id"].as_str().unwrap_or("job").to_string();
        let name = args["name"].as_str().unwrap_or("Job").to_string();
        let prompt = args["prompt"].as_str().unwrap_or("").to_string();

        let schedule = if let Some(secs) = args["interval_secs"].as_u64() {
            Schedule::Interval(secs)
        } else if let (Some(hour), Some(minute)) = (args["hour"].as_u64(), args["minute"].as_u64()) {
            Schedule::Daily { hour: hour as u32, minute: minute as u32 }
        } else {
            Schedule::Interval(3600) // Default: hourly
        };

        let job = ScheduledJob {
            id: id.clone(),
            name,
            prompt,
            schedule,
            enabled: true,
            last_run: None,
            run_count: 0,
        };

        let mut scheduler = self.0.lock().await;
        scheduler.add_job(job);
        Ok(format!("Added scheduled job '{}'", id))
    }
}

/// Remove a scheduled job.
pub struct ScheduleRemoveTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for ScheduleRemoveTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "schedule_remove".into(),
            description: "Remove a scheduled job by ID.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Job ID to remove" }
                },
                "required": ["id"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let id = args["id"].as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'id'".into()))?;

        let mut scheduler = self.0.lock().await;
        if scheduler.remove_job(id) {
            Ok(format!("Removed job '{}'", id))
        } else {
            Err(maix_core::MaixError::Tool(format!("job '{}' not found", id)))
        }
    }
}

/// Check for file changes in the working directory.
pub struct FileWatchTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for FileWatchTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "file_watch".into(),
            description: "Check for file changes in the working directory since last snapshot.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let mut scheduler = self.0.lock().await;
        let changed = scheduler.check_file_changes(&ctx.working_dir).await;

        if changed.is_empty() {
            return Ok("No file changes detected.".to_string());
        }

        let mut lines = vec![format!("{} file(s) changed:", changed.len())];
        for path in &changed {
            let rel = path.strip_prefix(&ctx.working_dir).unwrap_or(path);
            lines.push(format!("  {}", rel.display()));
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_list_jobs() {
        let mut scheduler = Scheduler::new();
        assert!(scheduler.list_jobs().is_empty());

        scheduler.add_job(ScheduledJob {
            id: "test".into(),
            name: "Test Job".into(),
            prompt: "run tests".into(),
            schedule: Schedule::Interval(60),
            enabled: true,
            last_run: None,
            run_count: 0,
        });

        assert_eq!(scheduler.list_jobs().len(), 1);
    }

    #[test]
    fn test_remove_job() {
        let mut scheduler = Scheduler::new();
        scheduler.add_job(ScheduledJob {
            id: "test".into(),
            name: "Test".into(),
            prompt: "test".into(),
            schedule: Schedule::Interval(60),
            enabled: true,
            last_run: None,
            run_count: 0,
        });

        assert!(scheduler.remove_job("test"));
        assert!(scheduler.list_jobs().is_empty());
        assert!(!scheduler.remove_job("nonexistent"));
    }

    #[test]
    fn test_enable_disable() {
        let mut scheduler = Scheduler::new();
        scheduler.add_job(ScheduledJob {
            id: "test".into(),
            name: "Test".into(),
            prompt: "test".into(),
            schedule: Schedule::Interval(60),
            enabled: true,
            last_run: None,
            run_count: 0,
        });

        assert!(scheduler.set_enabled("test", false));
        assert!(!scheduler.list_jobs()[0].enabled);
        assert!(scheduler.set_enabled("test", true));
        assert!(scheduler.list_jobs()[0].enabled);
    }

    #[test]
    fn test_check_due_jobs() {
        let mut scheduler = Scheduler::new();
        scheduler.add_job(ScheduledJob {
            id: "test".into(),
            name: "Test".into(),
            prompt: "test".into(),
            schedule: Schedule::Interval(0), // Always due
            enabled: true,
            last_run: None,
            run_count: 0,
        });

        let due = scheduler.check_due_jobs();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0], "test");
    }

    #[test]
    fn test_disabled_job_not_due() {
        let mut scheduler = Scheduler::new();
        scheduler.add_job(ScheduledJob {
            id: "test".into(),
            name: "Test".into(),
            prompt: "test".into(),
            schedule: Schedule::Interval(0),
            enabled: false,
            last_run: None,
            run_count: 0,
        });

        let due = scheduler.check_due_jobs();
        assert!(due.is_empty());
    }

    #[test]
    fn test_schedule_display() {
        assert_eq!(Schedule::Interval(60).to_string(), "every 60s");
        assert_eq!(Schedule::Daily { hour: 9, minute: 0 }.to_string(), "daily 09:00");
    }

    #[test]
    fn test_format_status_empty() {
        let scheduler = Scheduler::new();
        assert_eq!(scheduler.format_status(), "No scheduled jobs.");
    }
}
