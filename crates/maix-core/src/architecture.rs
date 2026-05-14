//! Programmable agent architecture DSL (Phase 2.4).
//!
//! Defines custom multi-agent topologies beyond the 3 preset modes.
//! Format: TOML-based, storable in DB.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A complete multi-agent architecture definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Architecture {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub nodes: Vec<ArchNode>,
    pub flows: Vec<ArchFlow>,
    #[serde(default)]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchNode {
    pub id: String,
    pub role: String,
    pub system_prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub personality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchFlow {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub merge: Option<MergeStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeStrategy {
    Append,
    Summarize,
    Vote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyType {
    Sequential,
    Parallel,
    Router,
    Debate,
    Custom,
}

impl Architecture {
    pub fn detect_topology(&self) -> TopologyType {
        let has_router = self.nodes.iter().any(|n| n.role.contains("router"));
        let has_vote = self
            .flows
            .iter()
            .any(|f| matches!(f.merge, Some(MergeStrategy::Vote)));
        let has_condition = self.flows.iter().any(|f| f.condition.is_some());

        if has_vote {
            return TopologyType::Debate;
        }
        if has_router || has_condition {
            return TopologyType::Router;
        }

        // Check if linear (each node has <=1 outgoing flow)
        let mut out_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for flow in &self.flows {
            *out_counts.entry(&flow.from).or_default() += 1;
        }
        let is_linear = out_counts.values().all(|&c| c <= 1) && !self.flows.is_empty();

        if is_linear {
            TopologyType::Sequential
        } else if self.nodes.len() > 1 && self.flows.len() > 1 {
            TopologyType::Parallel
        } else {
            TopologyType::Custom
        }
    }

    pub fn entry_node(&self) -> Option<&ArchNode> {
        let entry_id = self
            .entry
            .as_deref()
            .or_else(|| self.nodes.first().map(|n| n.id.as_str()))?;
        self.nodes.iter().find(|n| n.id == entry_id)
    }

    pub fn successors(&self, node_id: &str) -> Vec<&ArchFlow> {
        self.flows.iter().filter(|f| f.from == node_id).collect()
    }

    pub fn node(&self, id: &str) -> Option<&ArchNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Validate architecture for basic correctness.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let node_ids: Vec<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();

        if self.nodes.is_empty() {
            errors.push("at least one node required".into());
        }

        for flow in &self.flows {
            if !node_ids.contains(&flow.from.as_str()) {
                errors.push(format!("flow from unknown node: {}", flow.from));
            }
            if !node_ids.contains(&flow.to.as_str()) {
                errors.push(format!("flow to unknown node: {}", flow.to));
            }
        }

        // Check for duplicate IDs
        let mut seen = HashMap::new();
        for n in &self.nodes {
            if seen.contains_key(&n.id) {
                errors.push(format!("duplicate node id: {}", n.id));
            }
            seen.insert(&n.id, true);
        }

        if let Some(ref entry) = self.entry {
            if !node_ids.contains(&entry.as_str()) {
                errors.push(format!("entry node not found: {entry}"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Parse from TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        toml::from_str::<Architecture>(toml_str).map_err(|e| format!("parse: {e}"))
    }

    /// Export as TOML string.
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| format!("serialize: {e}"))
    }
}

/// Predefined architecture templates.
impl Architecture {
    pub fn sequential(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: Some("Sequential chain of agents".into()),
            nodes: vec![
                ArchNode {
                    id: "analyzer".into(),
                    role: "analyzer".into(),
                    system_prompt: "Analyze the input and identify key points.".into(),
                    model: None,
                    tools: vec![],
                    max_iterations: Some(1),
                    personality: Some("analytical".into()),
                },
                ArchNode {
                    id: "executor".into(),
                    role: "executor".into(),
                    system_prompt: "Execute the plan based on analysis.".into(),
                    model: None,
                    tools: vec!["fs_read".into(), "fs_write".into()],
                    max_iterations: Some(3),
                    personality: Some("practical".into()),
                },
            ],
            flows: vec![ArchFlow {
                from: "analyzer".into(),
                to: "executor".into(),
                condition: None,
                merge: Some(MergeStrategy::Append),
            }],
            entry: Some("analyzer".into()),
        }
    }

    pub fn debate(name: &str, topic_agents: usize) -> Self {
        let mut nodes = vec![
            ArchNode {
                id: "moderator".into(),
                role: "moderator".into(),
                system_prompt: "Moderate the debate and synthesize conclusions.".into(),
                model: None,
                tools: vec![],
                max_iterations: Some(1),
                personality: Some("neutral".into()),
            }
        ];

        for i in 0..topic_agents.max(2) {
            nodes.push(ArchNode {
                id: format!("debater_{i}"),
                role: "debater".into(),
                system_prompt: format!("Argue from perspective #{i}. Be critical and thorough."),
                model: None,
                tools: vec![],
                max_iterations: Some(2),
                personality: Some("critical".into()),
            });
        }

        let mut flows: Vec<ArchFlow> = vec![];
        for i in 0..topic_agents.max(2) {
            flows.push(ArchFlow {
                from: format!("debater_{i}"),
                to: "moderator".into(),
                condition: None,
                merge: Some(MergeStrategy::Vote),
            });
        }

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: Some("Multi-perspective debate architecture".into()),
            nodes,
            flows,
            entry: Some("moderator".into()),
        }
    }

    pub fn router(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            description: Some("Router-based task distribution".into()),
            nodes: vec![
                ArchNode {
                    id: "router".into(),
                    role: "router".into(),
                    system_prompt: "Classify the task and route to the appropriate specialist.".into(),
                    model: None,
                    tools: vec![],
                    max_iterations: Some(1),
                    personality: Some("decisive".into()),
                },
                ArchNode {
                    id: "coder".into(),
                    role: "specialist".into(),
                    system_prompt: "You are a coding specialist. Write clean, correct code.".into(),
                    model: Some("model-a".into()),
                    tools: vec!["fs_read".into(), "fs_write".into(), "shell_exec".into()],
                    max_iterations: Some(5),
                    personality: Some("precise".into()),
                },
                ArchNode {
                    id: "reasoner".into(),
                    role: "specialist".into(),
                    system_prompt: "You are a reasoning specialist. Think deeply about complex problems.".into(),
                    model: Some("model-b".into()),
                    tools: vec!["fs_read".into()],
                    max_iterations: Some(3),
                    personality: Some("analytical".into()),
                },
            ],
            flows: vec![
                ArchFlow {
                    from: "router".into(),
                    to: "coder".into(),
                    condition: Some("task_contains_code".into()),
                    merge: Some(MergeStrategy::Append),
                },
                ArchFlow {
                    from: "router".into(),
                    to: "reasoner".into(),
                    condition: Some("task_requires_reasoning".into()),
                    merge: Some(MergeStrategy::Append),
                },
            ],
            entry: Some("router".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_to_toml_roundtrip() {
        let arch = Architecture::sequential("test-seq");
        let toml_str = arch.to_toml().unwrap();
        let arch2 = Architecture::from_toml(&toml_str).unwrap();
        assert_eq!(arch.name, arch2.name);
        assert_eq!(arch.nodes.len(), arch2.nodes.len());
    }

    #[test]
    fn test_validate_empty_nodes() {
        let arch = Architecture {
            id: "x".into(),
            name: "empty".into(),
            description: None,
            nodes: vec![],
            flows: vec![],
            entry: None,
        };
        assert!(arch.validate().is_err());
    }

    #[test]
    fn test_validate_bad_flow() {
        let arch = Architecture {
            id: "x".into(),
            name: "bad".into(),
            description: None,
            nodes: vec![ArchNode {
                id: "a".into(),
                role: "test".into(),
                system_prompt: "test".into(),
                model: None,
                tools: vec![],
                max_iterations: None,
                personality: None,
            }],
            flows: vec![ArchFlow {
                from: "a".into(),
                to: "b".into(),
                condition: None,
                merge: None,
            }],
            entry: None,
        };
        assert!(arch.validate().is_err());
    }

    #[test]
    fn test_detect_topology() {
        let seq = Architecture::sequential("s");
        assert_eq!(seq.detect_topology(), TopologyType::Sequential);

        let debate = Architecture::debate("d", 2);
        assert_eq!(debate.detect_topology(), TopologyType::Debate);

        let router = Architecture::router("r");
        assert_eq!(router.detect_topology(), TopologyType::Router);
    }
}
