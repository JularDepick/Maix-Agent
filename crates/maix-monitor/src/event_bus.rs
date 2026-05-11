//! Global event bus — tokio broadcast-based pub/sub for system events.

use tokio::sync::broadcast;

/// System event with typed payload.
#[derive(Debug, Clone)]
pub struct SystemEvent {
    pub event_type: String,
    pub agent_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub payload: serde_json::Value,
}

/// Global event bus for cross-module communication.
pub struct EventBus {
    tx: broadcast::Sender<SystemEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemEvent> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: SystemEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}
