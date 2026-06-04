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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_new() {
        let bus = EventBus::new(64);
        assert_eq!(bus.receiver_count(), 0);
    }

    #[test]
    fn test_event_bus_default() {
        let bus = EventBus::default();
        assert_eq!(bus.receiver_count(), 0);
    }

    #[test]
    fn test_event_bus_subscribe_increments_count() {
        let bus = EventBus::new(64);
        let _rx1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);
        let _rx2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);
    }

    #[test]
    fn test_event_bus_publish_no_receivers() {
        let bus = EventBus::new(64);
        let event = SystemEvent {
            event_type: "test".into(),
            agent_id: None,
            timestamp: chrono::Utc::now(),
            payload: serde_json::json!({}),
        };
        let count = bus.publish(event);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_event_bus_publish_with_receivers() {
        let bus = EventBus::new(64);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let event = SystemEvent {
            event_type: "test".into(),
            agent_id: Some("agent-1".into()),
            timestamp: chrono::Utc::now(),
            payload: serde_json::json!({"key": "value"}),
        };
        let count = bus.publish(event);
        assert_eq!(count, 2);

        let received = rx1.try_recv().unwrap();
        assert_eq!(received.event_type, "test");
        assert_eq!(received.agent_id, Some("agent-1".into()));

        let received2 = rx2.try_recv().unwrap();
        assert_eq!(received2.event_type, "test");
    }

    #[test]
    fn test_event_bus_receiver_count_after_drop() {
        let bus = EventBus::new(64);
        let rx1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);
        drop(rx1);
        assert_eq!(bus.receiver_count(), 0);
    }
}
