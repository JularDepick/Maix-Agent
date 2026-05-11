//! Multi-agent message router.
//!
//! Routes messages between agents in multi-agent architectures:
//! Sequential, Parallel, Router, and Debate topologies.

use crate::orchestrator::AgentRole;
use std::collections::HashMap;

/// Routing strategy for multi-agent message dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Send to all agents in parallel.
    Broadcast,
    /// Send to a specific agent by role name.
    Direct,
    /// Route based on message content classification.
    Classified,
    /// Round-robin across available agents.
    RoundRobin,
}

/// Message router for multi-agent communication.
pub struct MessageRouter {
    strategy: RoutingStrategy,
    route_map: HashMap<String, Vec<String>>,
}

impl MessageRouter {
    pub fn new(strategy: RoutingStrategy) -> Self {
        Self {
            strategy,
            route_map: HashMap::new(),
        }
    }

    pub fn with_route(mut self, from_role: &str, to_roles: Vec<&str>) -> Self {
        self.route_map.insert(
            from_role.to_string(),
            to_roles.iter().map(|s| s.to_string()).collect(),
        );
        self
    }

    pub fn resolve_targets(&self, from: &AgentRole) -> Vec<String> {
        match self.strategy {
            RoutingStrategy::Direct => {
                self.route_map
                    .get(&from.name)
                    .cloned()
                    .unwrap_or_default()
            }
            RoutingStrategy::Broadcast => vec![],
            RoutingStrategy::Classified => vec![],
            RoutingStrategy::RoundRobin => vec![],
        }
    }
}
