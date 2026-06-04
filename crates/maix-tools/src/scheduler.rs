//! Scheduler — cron-like task scheduling with file change monitoring.
//!
//! Supports standard 5-field cron expressions (minute hour day-of-month month day-of-week),
//! durable persistence, one-shot and recurring tasks.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Local, Timelike};
use maix_core::MaixResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

// ---------------------------------------------------------------------------
// Cron expression parsing
// ---------------------------------------------------------------------------

/// A parsed 5-field cron expression: minute hour day-of-month month day-of-week.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronExpr {
    pub minute: Vec<u32>,
    pub hour: Vec<u32>,
    pub day_of_month: Vec<u32>,
    pub month: Vec<u32>,
    pub day_of_week: Vec<u32>,
}

impl CronExpr {
    /// Parse a 5-field cron expression.
    /// Supported: `*`, `*/N`, `N`, `N-M`, `N,M,O`
    pub fn parse(expr: &str) -> Result<Self, String> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!("cron expression must have 5 fields, got {}", fields.len()));
        }

        Ok(Self {
            minute: parse_cron_field(fields[0], 0, 59)?,
            hour: parse_cron_field(fields[1], 0, 23)?,
            day_of_month: parse_cron_field(fields[2], 1, 31)?,
            month: parse_cron_field(fields[3], 1, 12)?,
            day_of_week: parse_cron_field(fields[4], 0, 6)?,
        })
    }

    /// Check if this cron expression matches the given datetime.
    pub fn matches(&self, dt: &DateTime<Local>) -> bool {
        self.minute.contains(&dt.minute())
            && self.hour.contains(&dt.hour())
            && self.day_of_month.contains(&(dt.day()))
            && self.month.contains(&(dt.month()))
            && self.day_of_week.contains(&(dt.weekday().num_days_from_sunday()))
    }

    /// Calculate the next fire time after the given datetime.
    pub fn next_fire_after(&self, after: &DateTime<Local>) -> DateTime<Local> {
        let mut candidate = *after + chrono::Duration::minutes(1);
        // Round down to the start of the minute
        candidate = candidate.with_second(0).unwrap_or(candidate).with_nanosecond(0).unwrap_or(candidate);

        // Brute force search up to 2 years
        for _ in 0..(366 * 24 * 60 * 2) {
            if self.matches(&candidate) {
                return candidate;
            }
            candidate += chrono::Duration::minutes(1);
        }
        // Fallback: return 1 hour from now
        *after + chrono::Duration::hours(1)
    }
}

/// Parse a single cron field (e.g., "*/5", "1,15", "1-10", "*").
fn parse_cron_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let mut values = Vec::new();

    for part in field.split(',') {
        if part == "*" {
            values.extend(min..=max);
        } else if let Some(step_str) = part.strip_prefix("*/") {
            let step: u32 = step_str.parse().map_err(|_| format!("invalid step: {}", step_str))?;
            if step == 0 {
                return Err("step cannot be 0".into());
            }
            let mut v = min;
            while v <= max {
                values.push(v);
                v += step;
            }
        } else if part.contains('-') {
            // Handle "N-M" format
            if let Some(dash_pos) = part.find('-') {
                let start: u32 = part[..dash_pos].parse().map_err(|_| format!("invalid range start: {}", part))?;
                let end: u32 = part[dash_pos + 1..].parse().map_err(|_| format!("invalid range end: {}", part))?;
                if start > end {
                    return Err(format!("invalid range: {} > {}", start, end));
                }
                values.extend(start..=end);
            } else {
                return Err(format!("invalid field: {}", part));
            }
        } else if part.contains('-') {
            let dash_pos = part.find('-').unwrap();
            let start: u32 = part[..dash_pos].parse().map_err(|_| format!("invalid range start: {}", part))?;
            let end: u32 = part[dash_pos + 1..].parse().map_err(|_| format!("invalid range end: {}", part))?;
            if start > end {
                return Err(format!("invalid range: {} > {}", start, end));
            }
            values.extend(start..=end);
        } else {
            let v: u32 = part.parse().map_err(|_| format!("invalid value: {}", part))?;
            if v < min || v > max {
                return Err(format!("value {} out of range [{}, {}]", v, min, max));
            }
            values.push(v);
        }
    }

    values.sort();
    values.dedup();
    Ok(values)
}

// ---------------------------------------------------------------------------
// Scheduled job types
// ---------------------------------------------------------------------------

/// A scheduled job definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub prompt: String,
    pub cron_expr: Option<String>,
    pub recurring: bool,
    pub durable: bool,
    pub next_fire: Option<DateTime<Local>>,
    pub created_at: DateTime<Local>,
    pub last_run: Option<DateTime<Local>>,
    pub run_count: u64,
}

/// Legacy schedule type for backward compatibility.
#[derive(Debug, Clone)]
pub enum Schedule {
    Interval(u64),
    Daily { hour: u32, minute: u32 },
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

/// Scheduler manages cron jobs, legacy jobs, and file change monitoring.
pub struct Scheduler {
    cron_jobs: Vec<ScheduledJob>,
    legacy_jobs: Vec<LegacyJob>,
    file_snapshots: HashMap<PathBuf, FileSnapshot>,
    watch_patterns: Vec<String>,
    durable_path: Option<PathBuf>,
}

/// Legacy job for backward compatibility.
#[derive(Debug, Clone)]
pub struct LegacyJob {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: Schedule,
    pub enabled: bool,
    pub last_run: Option<Instant>,
    pub run_count: u64,
}

/// Persisted cron tasks file.
#[derive(Serialize, Deserialize)]
struct DurableTasks {
    jobs: Vec<ScheduledJob>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        let mut s = Self {
            cron_jobs: Vec::new(),
            legacy_jobs: Vec::new(),
            file_snapshots: HashMap::new(),
            watch_patterns: Vec::new(),
            durable_path: None,
        };
        s.load_durable();
        s
    }

    /// Set the durable storage path.
    pub fn with_durable_path(mut self, path: PathBuf) -> Self {
        self.durable_path = Some(path);
        self.load_durable();
        self
    }

    /// Load durable tasks from disk.
    fn load_durable(&mut self) {
        let path = match &self.durable_path {
            Some(p) => p.clone(),
            None => {
                // Default: ~/.maix/scheduled_tasks.json
                if let Some(home) = dirs_next() {
                    home.join(".maix").join("scheduled_tasks.json")
                } else {
                    return;
                }
            }
        };

        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(tasks) = serde_json::from_str::<DurableTasks>(&content) {
                for job in tasks.jobs {
                    if job.durable && !self.cron_jobs.iter().any(|j| j.id == job.id) {
                        self.cron_jobs.push(job);
                    }
                }
            }
        }
    }

    /// Save durable tasks to disk.
    fn save_durable(&self) {
        let path = match &self.durable_path {
            Some(p) => p.clone(),
            None => {
                if let Some(home) = dirs_next() {
                    home.join(".maix").join("scheduled_tasks.json")
                } else {
                    return;
                }
            }
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let durable_jobs: Vec<&ScheduledJob> = self.cron_jobs.iter().filter(|j| j.durable).collect();
        let tasks = DurableTasks {
            jobs: durable_jobs.into_iter().cloned().collect(),
        };

        if let Ok(content) = serde_json::to_string_pretty(&tasks) {
            if let Err(e) = std::fs::write(&path, content) {
                tracing::warn!("Failed to persist cron jobs: {e}");
            }
        }
    }

    /// Add a cron job.
    pub fn add_cron_job(&mut self, job: ScheduledJob) {
        self.cron_jobs.push(job);
        self.save_durable();
    }

    /// Remove a cron job by ID.
    pub fn remove_cron_job(&mut self, id: &str) -> bool {
        let len_before = self.cron_jobs.len();
        self.cron_jobs.retain(|j| j.id != id);
        let removed = self.cron_jobs.len() < len_before;
        if removed {
            self.save_durable();
        }
        removed
    }

    /// List all cron jobs.
    pub fn list_cron_jobs(&self) -> &[ScheduledJob] {
        &self.cron_jobs
    }

    /// Check which cron jobs are due to fire now.
    pub fn check_due_cron_jobs(&mut self) -> Vec<String> {
        let now = Local::now();
        let mut due = Vec::new();

        for job in &mut self.cron_jobs {
            let should_fire = match &job.next_fire {
                Some(fire_time) => now >= *fire_time,
                None => {
                    // Calculate next fire time
                    if let Some(cron_str) = &job.cron_expr {
                        if let Ok(cron) = CronExpr::parse(cron_str) {
                            job.next_fire = Some(cron.next_fire_after(&now));
                            false
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            };

            if should_fire {
                due.push(job.id.clone());
                job.last_run = Some(now);
                job.run_count += 1;

                if job.recurring {
                    // Calculate next fire time
                    if let Some(cron_str) = &job.cron_expr {
                        if let Ok(cron) = CronExpr::parse(cron_str) {
                            job.next_fire = Some(cron.next_fire_after(&now));
                        }
                    }
                } else {
                    // One-shot: mark for removal
                    job.next_fire = None;
                }
            }
        }

        // Remove completed one-shot jobs
        self.cron_jobs.retain(|j| j.recurring || j.next_fire.is_some());
        if !due.is_empty() {
            self.save_durable();
        }

        due
    }

    /// Add a one-shot wakeup at a specific time.
    pub fn schedule_wakeup(&mut self, delay_seconds: u64, prompt: String) -> String {
        let id = format!("wakeup-{}", uuid::Uuid::new_v4());
        let next_fire = Local::now() + chrono::Duration::seconds(delay_seconds as i64);

        let job = ScheduledJob {
            id: id.clone(),
            prompt,
            cron_expr: None,
            recurring: false,
            durable: false,
            next_fire: Some(next_fire),
            created_at: Local::now(),
            last_run: None,
            run_count: 0,
        };

        self.cron_jobs.push(job);
        id
    }

    // --- Legacy methods for backward compatibility ---

    pub fn add_job(&mut self, job: LegacyJob) {
        self.legacy_jobs.push(job);
    }

    pub fn remove_job(&mut self, id: &str) -> bool {
        let len_before = self.legacy_jobs.len();
        self.legacy_jobs.retain(|j| j.id != id);
        self.legacy_jobs.len() < len_before
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(job) = self.legacy_jobs.iter_mut().find(|j| j.id == id) {
            job.enabled = enabled;
            true
        } else {
            false
        }
    }

    pub fn list_jobs(&self) -> &[LegacyJob] {
        &self.legacy_jobs
    }

    pub fn check_due_jobs(&mut self) -> Vec<String> {
        let now = Instant::now();
        let mut due = Vec::new();

        for job in &mut self.legacy_jobs {
            if !job.enabled { continue; }
            if let Schedule::Interval(secs) = &job.schedule {
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
        }
        due
    }

    pub fn watch_files(&mut self, patterns: Vec<String>) {
        self.watch_patterns.extend(patterns);
    }

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
                if !ft.is_file() { continue; }

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
                if !ft.is_file() { continue; }

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
                    changed.push(path.clone());
                }

                self.file_snapshots.insert(path.clone(), FileSnapshot {
                    path, modified: current_modified, size: current_size,
                });
            }
        }

        // Trigger file-change legacy jobs
        for job in &mut self.legacy_jobs {
            if !job.enabled { continue; }
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
        let mut lines = Vec::new();

        // Cron jobs
        if !self.cron_jobs.is_empty() {
            lines.push(format!("Cron Jobs ({}):", self.cron_jobs.len()));
            for job in &self.cron_jobs {
                let recurring_str = if job.recurring { "recurring" } else { "one-shot" };
                let durable_str = if job.durable { ", durable" } else { "" };
                let next = job.next_fire
                    .map(|t| format!("next: {}", t.format("%Y-%m-%d %H:%M")))
                    .unwrap_or_else(|| "pending".to_string());
                let last = job.last_run
                    .map(|t| format!("last: {}", t.format("%H:%M")))
                    .unwrap_or_else(|| "never".to_string());
                lines.push(format!(
                    "  {} [{}{}] {} - {} ({}, runs: {})",
                    job.id, recurring_str, durable_str, job.prompt, next, last, job.run_count
                ));
            }
        }

        // Legacy jobs
        if !self.legacy_jobs.is_empty() {
            if !lines.is_empty() { lines.push(String::new()); }
            lines.push(format!("Legacy Jobs ({}):", self.legacy_jobs.len()));
            for job in &self.legacy_jobs {
                let status = if job.enabled { "enabled" } else { "disabled" };
                let last = job.last_run
                    .map(|t| format!("{:.0}s ago", t.elapsed().as_secs()))
                    .unwrap_or_else(|| "never".to_string());
                lines.push(format!(
                    "  {} [{}] {} - {} (last: {}, runs: {})",
                    job.id, status, job.name, job.schedule, last, job.run_count
                ));
            }
        }

        if lines.is_empty() {
            "No scheduled jobs.".to_string()
        } else {
            if !self.watch_patterns.is_empty() {
                lines.push(String::new());
                lines.push(format!("Watching: {}", self.watch_patterns.join(", ")));
            }
            lines.join("\n")
        }
    }
}

fn dirs_next() -> Option<PathBuf> {
    home::home_dir()
}

// ---------------------------------------------------------------------------
// CronCreate tool
// ---------------------------------------------------------------------------

pub struct CronCreateTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for CronCreateTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cron_create".into(),
            description: "Create a scheduled cron job. Supports standard 5-field cron expressions (minute hour day-of-month month day-of-week). One-shot or recurring, optional durable persistence.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Unique job ID" },
                    "cron": { "type": "string", "description": "5-field cron expression (e.g., \"*/5 * * * *\" for every 5 min, \"0 9 * * 1-5\" for weekdays at 9am)" },
                    "prompt": { "type": "string", "description": "Prompt to execute when the job fires" },
                    "recurring": { "type": "boolean", "description": "true for recurring, false for one-shot (default: true)" },
                    "durable": { "type": "boolean", "description": "true to persist across restarts (default: false)" }
                },
                "required": ["id", "cron", "prompt"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let id = args["id"].as_str().unwrap_or("job").to_string();
        let cron_str = args["cron"].as_str().unwrap_or("* * * * *");
        let prompt = args["prompt"].as_str().unwrap_or("").to_string();
        let recurring = args["recurring"].as_bool().unwrap_or(true);
        let durable = args["durable"].as_bool().unwrap_or(false);

        let cron = CronExpr::parse(cron_str)
            .map_err(|e| maix_core::MaixError::Tool(format!("invalid cron: {e}")))?;

        let now = Local::now();
        let next_fire = cron.next_fire_after(&now);

        let job = ScheduledJob {
            id: id.clone(),
            prompt,
            cron_expr: Some(cron_str.to_string()),
            recurring,
            durable,
            next_fire: Some(next_fire),
            created_at: now,
            last_run: None,
            run_count: 0,
        };

        let mut scheduler = self.0.lock().await;
        scheduler.add_cron_job(job);

        let recurring_label = if recurring { "recurring" } else { "one-shot" };
        let durable_label = if durable { ", durable" } else { "" };
        Ok(format!(
            "Created cron job '{}' [{}{}] next fire: {}",
            id, recurring_label, durable_label, next_fire.format("%Y-%m-%d %H:%M")
        ))
    }
}

// ---------------------------------------------------------------------------
// CronDelete tool
// ---------------------------------------------------------------------------

pub struct CronDeleteTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for CronDeleteTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cron_delete".into(),
            description: "Delete a cron job by ID.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Job ID to delete" }
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
        if scheduler.remove_cron_job(id) {
            Ok(format!("Deleted cron job '{}'", id))
        } else {
            Err(maix_core::MaixError::Tool(format!("cron job '{}' not found", id)))
        }
    }
}

// ---------------------------------------------------------------------------
// CronList tool
// ---------------------------------------------------------------------------

pub struct CronListTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for CronListTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "cron_list".into(),
            description: "List all cron jobs and legacy scheduled jobs.".into(),
            parameters: json!({"type": "object", "properties": {}}),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, _args: serde_json::Value) -> MaixResult<String> {
        let scheduler = self.0.lock().await;
        Ok(scheduler.format_status())
    }
}

// ---------------------------------------------------------------------------
// ScheduleWakeup tool
// ---------------------------------------------------------------------------

pub struct ScheduleWakeupTool(pub Arc<Mutex<Scheduler>>);

#[async_trait]
impl Tool for ScheduleWakeupTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "schedule_wakeup".into(),
            description: "Schedule a one-shot wakeup after a delay. The prompt will be executed when the delay expires.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "delay_seconds": { "type": "integer", "description": "Seconds to wait before firing (60-3600)" },
                    "prompt": { "type": "string", "description": "Prompt to execute on wakeup" }
                },
                "required": ["delay_seconds", "prompt"]
            }),
            risk_level: RiskLevel::Write,
        }
    }

    async fn execute(&self, _ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let delay = args["delay_seconds"].as_u64()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'delay_seconds'".into()))?;
        let prompt = args["prompt"].as_str().unwrap_or("").to_string();

        if !(60..=3600).contains(&delay) {
            return Err(maix_core::MaixError::Tool("delay_seconds must be between 60 and 3600".into()));
        }

        let mut scheduler = self.0.lock().await;
        let id = scheduler.schedule_wakeup(delay, prompt);

        Ok(format!("Scheduled wakeup '{}' in {} seconds", id, delay))
    }
}

// ---------------------------------------------------------------------------
// Legacy tools (backward compatible)
// ---------------------------------------------------------------------------

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
            Schedule::Interval(3600)
        };

        let job = LegacyJob {
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
    fn test_cron_parse_wildcard() {
        let cron = CronExpr::parse("* * * * *").unwrap();
        assert_eq!(cron.minute.len(), 60);
        assert_eq!(cron.hour.len(), 24);
    }

    #[test]
    fn test_cron_parse_step() {
        let cron = CronExpr::parse("*/5 * * * *").unwrap();
        assert_eq!(cron.minute, vec![0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55]);
    }

    #[test]
    fn test_cron_parse_range() {
        let cron = CronExpr::parse("0 9 * * 1-5").unwrap();
        assert_eq!(cron.minute, vec![0]);
        assert_eq!(cron.hour, vec![9]);
        assert_eq!(cron.day_of_week, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_cron_parse_list() {
        let cron = CronExpr::parse("0,30 * * * *").unwrap();
        assert_eq!(cron.minute, vec![0, 30]);
    }

    #[test]
    fn test_cron_parse_invalid() {
        assert!(CronExpr::parse("* * *").is_err());
        assert!(CronExpr::parse("60 * * * *").is_err());
    }

    #[test]
    fn test_cron_parse_specific_time() {
        let cron = CronExpr::parse("3 9 * * *").unwrap();
        assert_eq!(cron.minute, vec![3]);
        assert_eq!(cron.hour, vec![9]);
    }

    #[test]
    fn test_add_and_list_jobs() {
        let mut scheduler = Scheduler::new();
        assert!(scheduler.list_jobs().is_empty());

        scheduler.add_job(LegacyJob {
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
        scheduler.add_job(LegacyJob {
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
        scheduler.add_job(LegacyJob {
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
        scheduler.add_job(LegacyJob {
            id: "test".into(),
            name: "Test".into(),
            prompt: "test".into(),
            schedule: Schedule::Interval(0),
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
        scheduler.add_job(LegacyJob {
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

    #[test]
    fn test_cron_job_add_remove() {
        let mut scheduler = Scheduler::new();
        let now = Local::now();

        let job = ScheduledJob {
            id: "test-cron".into(),
            prompt: "test".into(),
            cron_expr: Some("*/5 * * * *".into()),
            recurring: true,
            durable: false,
            next_fire: Some(now + chrono::Duration::minutes(5)),
            created_at: now,
            last_run: None,
            run_count: 0,
        };

        scheduler.add_cron_job(job);
        assert_eq!(scheduler.list_cron_jobs().len(), 1);

        assert!(scheduler.remove_cron_job("test-cron"));
        assert!(scheduler.list_cron_jobs().is_empty());
    }
}
