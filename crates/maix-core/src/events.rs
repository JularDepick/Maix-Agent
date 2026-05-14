//! Event-driven architecture — pub/sub event bus with filtering and logging.
//!
//! Provides a lightweight event system for decoupled communication between
//! components (agent, tools, TUI, server).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Event severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventSeverity {
    Debug,
    Info,
    Warn,
    Error,
}

impl EventSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// Event categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    File,
    Tool,
    Agent,
    Session,
    System,
    User,
    Network,
}

impl EventCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Tool => "tool",
            Self::Agent => "agent",
            Self::Session => "session",
            Self::System => "system",
            Self::User => "user",
            Self::Network => "network",
        }
    }
}

/// An event on the bus.
#[derive(Debug, Clone)]
pub struct Event {
    pub id: u64,
    pub category: EventCategory,
    pub severity: EventSeverity,
    pub name: String,
    pub message: String,
    pub data: HashMap<String, String>,
    pub timestamp: std::time::SystemTime,
}

/// Filter condition for event matching.
#[derive(Debug, Clone)]
pub enum FilterCondition {
    Equals(String),
    Contains(String),
    StartsWith(String),
    Regex(String),
}

impl FilterCondition {
    pub fn matches(&self, value: &str) -> bool {
        match self {
            Self::Equals(expected) => value == expected,
            Self::Contains(sub) => value.contains(sub.as_str()),
            Self::StartsWith(prefix) => value.starts_with(prefix.as_str()),
            Self::Regex(pattern) => {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(value))
                    .unwrap_or(false)
            }
        }
    }
}

/// Event filter with conditions on category, severity, and name.
#[derive(Debug, Clone)]
pub struct EventFilter {
    pub category: Option<EventCategory>,
    pub severity: Option<EventSeverity>,
    pub name_condition: Option<FilterCondition>,
    pub message_condition: Option<FilterCondition>,
}

impl EventFilter {
    pub fn matches(&self, event: &Event) -> bool {
        if let Some(cat) = self.category {
            if event.category != cat {
                return false;
            }
        }
        if let Some(sev) = self.severity {
            if event.severity != sev {
                return false;
            }
        }
        if let Some(ref cond) = self.name_condition {
            if !cond.matches(&event.name) {
                return false;
            }
        }
        if let Some(ref cond) = self.message_condition {
            if !cond.matches(&event.message) {
                return false;
            }
        }
        true
    }
}

/// Subscription ID.
pub type SubscriptionId = u64;

/// An event handler callback.
pub type EventHandler = Box<dyn Fn(&Event) + Send + Sync>;

struct Subscription {
    filter: EventFilter,
    handler: EventHandler,
}

/// Event bus — central pub/sub system.
pub struct EventBus {
    subscriptions: RwLock<HashMap<SubscriptionId, Subscription>>,
    log: RwLock<Vec<Event>>,
    next_id: RwLock<u64>,
    next_sub_id: RwLock<SubscriptionId>,
    max_log_size: usize,
}

impl EventBus {
    pub fn new(max_log_size: usize) -> Self {
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            log: RwLock::new(Vec::new()),
            next_id: RwLock::new(1),
            next_sub_id: RwLock::new(1),
            max_log_size,
        }
    }

    /// Publish an event to all matching subscribers.
    pub fn publish(
        &self,
        category: EventCategory,
        severity: EventSeverity,
        name: &str,
        message: &str,
        data: HashMap<String, String>,
    ) -> u64 {
        let id = {
            let mut next = self.next_id.write().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        let event = Event {
            id,
            category,
            severity,
            name: name.to_string(),
            message: message.to_string(),
            data,
            timestamp: std::time::SystemTime::now(),
        };

        // Notify subscribers
        let subs = self.subscriptions.read().unwrap();
        for sub in subs.values() {
            if sub.filter.matches(&event) {
                (sub.handler)(&event);
            }
        }

        // Log event
        {
            let mut log = self.log.write().unwrap();
            if log.len() >= self.max_log_size {
                log.remove(0);
            }
            log.push(event);
        }

        id
    }

    /// Subscribe to events matching a filter. Returns a subscription ID.
    pub fn subscribe(&self, filter: EventFilter, handler: EventHandler) -> SubscriptionId {
        let mut subs = self.subscriptions.write().unwrap();
        let mut next = self.next_sub_id.write().unwrap();
        let id = *next;
        *next += 1;
        subs.insert(id, Subscription { filter, handler });
        id
    }

    /// Unsubscribe a handler.
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut subs = self.subscriptions.write().unwrap();
        subs.remove(&id).is_some()
    }

    /// Get recent events from the log.
    pub fn recent(&self, count: usize) -> Vec<Event> {
        let log = self.log.read().unwrap();
        log.iter().rev().take(count).cloned().collect()
    }

    /// Get events matching a filter from the log.
    pub fn query(&self, filter: &EventFilter, limit: usize) -> Vec<Event> {
        let log = self.log.read().unwrap();
        log.iter()
            .rev()
            .filter(|e| filter.matches(e))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Count events in log.
    pub fn log_size(&self) -> usize {
        self.log.read().unwrap().len()
    }

    /// Format recent events for display.
    pub fn format_recent(&self, count: usize) -> String {
        let events = self.recent(count);
        if events.is_empty() {
            return "No events.".to_string();
        }
        let mut lines = Vec::new();
        for event in &events {
            lines.push(format!(
                "[{}] {} {}: {}",
                event.severity.as_str(),
                event.category.as_str(),
                event.name,
                event.message
            ));
        }
        lines.join("\n")
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Global event bus instance.
static GLOBAL_BUS: std::sync::OnceLock<Arc<EventBus>> = std::sync::OnceLock::new();

/// Get or initialize the global event bus.
pub fn global_bus() -> &'static Arc<EventBus> {
    GLOBAL_BUS.get_or_init(|| Arc::new(EventBus::new(1000)))
}

/// Convenience: publish to global bus.
pub fn emit(
    category: EventCategory,
    severity: EventSeverity,
    name: &str,
    message: &str,
) -> u64 {
    global_bus().publish(category, severity, name, message, HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_publish_and_log() {
        let bus = EventBus::new(100);
        bus.publish(
            EventCategory::Tool,
            EventSeverity::Info,
            "tool_called",
            "fs_read called",
            HashMap::new(),
        );
        assert_eq!(bus.log_size(), 1);
        let recent = bus.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "tool_called");
    }

    #[test]
    fn test_subscribe_and_notify() {
        let bus = EventBus::new(100);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let filter = EventFilter {
            category: Some(EventCategory::Tool),
            severity: None,
            name_condition: None,
            message_condition: None,
        };
        bus.subscribe(filter, Box::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }));

        bus.publish(EventCategory::Tool, EventSeverity::Info, "test", "msg", HashMap::new());
        bus.publish(EventCategory::File, EventSeverity::Info, "test", "msg", HashMap::new());
        bus.publish(EventCategory::Tool, EventSeverity::Info, "test", "msg", HashMap::new());

        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_unsubscribe() {
        let bus = EventBus::new(100);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let filter = EventFilter {
            category: None,
            severity: None,
            name_condition: None,
            message_condition: None,
        };
        let sub_id = bus.subscribe(filter, Box::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }));

        bus.publish(EventCategory::System, EventSeverity::Info, "test", "msg", HashMap::new());
        assert_eq!(counter.load(Ordering::Relaxed), 1);

        assert!(bus.unsubscribe(sub_id));
        bus.publish(EventCategory::System, EventSeverity::Info, "test", "msg", HashMap::new());
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_filter_by_severity() {
        let bus = EventBus::new(100);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let filter = EventFilter {
            category: None,
            severity: Some(EventSeverity::Error),
            name_condition: None,
            message_condition: None,
        };
        bus.subscribe(filter, Box::new(move |_| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }));

        bus.publish(EventCategory::System, EventSeverity::Info, "test", "msg", HashMap::new());
        bus.publish(EventCategory::System, EventSeverity::Error, "test", "msg", HashMap::new());
        bus.publish(EventCategory::System, EventSeverity::Warn, "test", "msg", HashMap::new());

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_filter_by_name_condition() {
        let filter = EventFilter {
            category: None,
            severity: None,
            name_condition: Some(FilterCondition::Contains("read".to_string())),
            message_condition: None,
        };

        let event_ok = Event {
            id: 1,
            category: EventCategory::Tool,
            severity: EventSeverity::Info,
            name: "fs_read".to_string(),
            message: "read file".to_string(),
            data: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };
        let event_no = Event {
            id: 2,
            category: EventCategory::Tool,
            severity: EventSeverity::Info,
            name: "fs_write".to_string(),
            message: "write file".to_string(),
            data: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        };

        assert!(filter.matches(&event_ok));
        assert!(!filter.matches(&event_no));
    }

    #[test]
    fn test_query_log() {
        let bus = EventBus::new(100);
        bus.publish(EventCategory::Tool, EventSeverity::Info, "a", "1", HashMap::new());
        bus.publish(EventCategory::File, EventSeverity::Warn, "b", "2", HashMap::new());
        bus.publish(EventCategory::Tool, EventSeverity::Error, "c", "3", HashMap::new());

        let filter = EventFilter {
            category: Some(EventCategory::Tool),
            severity: None,
            name_condition: None,
            message_condition: None,
        };
        let results = bus.query(&filter, 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_max_log_size() {
        let bus = EventBus::new(3);
        for i in 0..5 {
            bus.publish(
                EventCategory::System,
                EventSeverity::Info,
                "test",
                &format!("msg {}", i),
                HashMap::new(),
            );
        }
        assert_eq!(bus.log_size(), 3);
    }

    #[test]
    fn test_format_recent() {
        let bus = EventBus::new(100);
        bus.publish(EventCategory::Tool, EventSeverity::Info, "call", "fs_read", HashMap::new());
        let s = bus.format_recent(10);
        assert!(s.contains("tool"));
        assert!(s.contains("call"));
    }

    #[test]
    fn test_format_recent_empty() {
        let bus = EventBus::new(100);
        assert_eq!(bus.format_recent(10), "No events.");
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(EventSeverity::Debug.as_str(), "debug");
        assert_eq!(EventSeverity::Error.as_str(), "error");
    }

    #[test]
    fn test_category_as_str() {
        assert_eq!(EventCategory::File.as_str(), "file");
        assert_eq!(EventCategory::Network.as_str(), "network");
    }

    #[test]
    fn test_filter_condition_matches() {
        assert!(FilterCondition::Equals("test".to_string()).matches("test"));
        assert!(!FilterCondition::Equals("test".to_string()).matches("other"));

        assert!(FilterCondition::Contains("hello".to_string()).matches("say hello world"));
        assert!(!FilterCondition::Contains("hello".to_string()).matches("goodbye"));

        assert!(FilterCondition::StartsWith("fn ".to_string()).matches("fn main()"));
        assert!(!FilterCondition::StartsWith("fn ".to_string()).matches("let x = 1"));

        assert!(FilterCondition::Regex(r"\d+".to_string()).matches("test 123"));
        assert!(!FilterCondition::Regex(r"^\d+$".to_string()).matches("test 123"));
    }
}
