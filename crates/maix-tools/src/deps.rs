//! Dependency analysis — analyze project dependencies, detect cycles, impact analysis.

use async_trait::async_trait;
use maix_core::MaixResult;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use crate::{Tool, ToolCtx, ToolDef, RiskLevel};

/// A dependency edge: `from` depends on `to`.
#[derive(Debug, Clone)]
pub struct DepEdge {
    pub from: String,
    pub to: String,
}

/// A dependency graph with nodes and edges.
#[derive(Debug, Clone)]
pub struct DepGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<DepEdge>,
}

impl DepGraph {
    /// Build adjacency list (from → list of to).
    fn adjacency(&self) -> HashMap<String, Vec<String>> {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &self.edges {
            adj.entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }
        adj
    }

    /// Build reverse adjacency list (to → list of from).
    fn reverse_adjacency(&self) -> HashMap<String, Vec<String>> {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &self.edges {
            adj.entry(edge.to.clone())
                .or_default()
                .push(edge.from.clone());
        }
        adj
    }
}

/// Analyzes project dependencies.
pub struct DepAnalyzer;

impl DepAnalyzer {
    /// Analyze Rust workspace dependencies from Cargo.toml files.
    pub async fn analyze_rust(root: &Path) -> MaixResult<DepGraph> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Read workspace Cargo.toml
        let cargo_path = root.join("Cargo.toml");
        let content = tokio::fs::read_to_string(&cargo_path).await.map_err(|e| {
            maix_core::MaixError::Tool(format!("Failed to read Cargo.toml: {e}"))
        })?;

        let cargo: toml::Value = content.parse().map_err(|e| {
            maix_core::MaixError::Tool(format!("Failed to parse Cargo.toml: {e}"))
        })?;

        // Get workspace members
        let members: Vec<String> = cargo
            .get("workspace")
            .and_then(|w| w.get("members"))
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Expand glob patterns in members
        let mut expanded_members = Vec::new();
        for member in &members {
            if member.contains('*') {
                // Expand glob
                let pattern = root.join(member).to_string_lossy().to_string();
                if let Ok(paths) = glob::glob(&pattern) {
                    for path in paths.flatten() {
                        if path.join("Cargo.toml").exists() {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                expanded_members.push(name.to_string());
                            }
                        }
                    }
                }
            } else {
                expanded_members.push(member.clone());
            }
        }

        // Parse each member's Cargo.toml for dependencies
        for member in &expanded_members {
            nodes.push(member.clone());

            let member_cargo_path = root.join(member).join("Cargo.toml");
            let member_content = match tokio::fs::read_to_string(&member_cargo_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let member_cargo: toml::Value = match member_content.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Get the crate name (may differ from directory name)
            let crate_name = member_cargo
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or(member)
                .to_string();

            // Collect dependencies
            for dep_section in &["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(deps) = member_cargo.get(dep_section).and_then(|d| d.as_table()) {
                    for (dep_name, dep_value) in deps {
                        // Check if it's a workspace dependency
                        let is_path_dep = dep_value
                            .as_table()
                            .and_then(|t| t.get("path"))
                            .is_some();

                        let is_workspace_dep = dep_value
                            .as_table()
                            .and_then(|t| t.get("workspace"))
                            .and_then(|w| w.as_bool())
                            .unwrap_or(false);

                        // Check if it's an internal crate
                        if is_path_dep || is_workspace_dep || expanded_members.contains(dep_name) {
                            // Normalize the dep name
                            let dep_node = dep_name.replace('-', "_");
                            if !nodes.contains(&dep_node) && expanded_members.contains(&dep_name.replace('_', "-")) {
                                // already added
                            }
                            edges.push(DepEdge {
                                from: crate_name.clone(),
                                to: dep_name.replace('-', "_"),
                            });
                        }
                    }
                }
            }
        }

        // Deduplicate nodes
        nodes.sort();
        nodes.dedup();

        Ok(DepGraph { nodes, edges })
    }

    /// Detect cycles in a dependency graph using DFS.
    pub fn detect_cycles(graph: &DepGraph) -> Vec<Vec<String>> {
        let adj = graph.adjacency();
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();
        let mut cycles = Vec::new();

        for node in &graph.nodes {
            if !visited.contains(node.as_str()) {
                Self::dfs_cycle(
                    node,
                    &adj,
                    &mut visited,
                    &mut in_stack,
                    &mut Vec::new(),
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_cycle(
        node: &str,
        adj: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        in_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        in_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = adj.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor.as_str()) {
                    Self::dfs_cycle(neighbor, adj, visited, in_stack, path, cycles);
                } else if in_stack.contains(neighbor.as_str()) {
                    // Found a cycle
                    if let Some(start) = path.iter().position(|n| n == neighbor) {
                        let cycle: Vec<String> = path[start..].to_vec();
                        cycles.push(cycle);
                    }
                }
            }
        }

        path.pop();
        in_stack.remove(node);
    }

    /// Impact analysis: find all nodes that depend on the given node (transitively).
    pub fn impact_analysis(graph: &DepGraph, changed_node: &str) -> Vec<String> {
        let rev_adj = graph.reverse_adjacency();
        let mut affected = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(changed_node.to_string());
        visited.insert(changed_node.to_string());

        while let Some(current) = queue.pop_front() {
            if let Some(dependents) = rev_adj.get(&current) {
                for dep in dependents {
                    if !visited.contains(dep.as_str()) {
                        visited.insert(dep.clone());
                        affected.push(dep.clone());
                        queue.push_back(dep.clone());
                    }
                }
            }
        }

        affected
    }

    /// Find unused dependencies: crates that are listed but never imported.
    pub async fn find_unused(root: &Path) -> MaixResult<Vec<String>> {
        let graph = Self::analyze_rust(root).await?;
        let mut unused = Vec::new();

        // For each node, check if any other node depends on it
        let depended_on: HashSet<&str> = graph.edges.iter().map(|e| e.to.as_str()).collect();

        for node in &graph.nodes {
            // Skip if it's the root workspace member that others depend on
            if !depended_on.contains(node.as_str()) {
                // Check if this node has any edges (i.e., it depends on others)
                let has_outgoing = graph.edges.iter().any(|e| e.from == *node);
                if !has_outgoing {
                    unused.push(node.clone());
                }
            }
        }

        Ok(unused)
    }

    /// Format graph as a readable text representation.
    pub fn format_graph(graph: &DepGraph) -> String {
        if graph.nodes.is_empty() {
            return "No dependencies found.".into();
        }

        let adj = graph.adjacency();
        let mut lines = vec![format!(
            "Dependency graph: {} nodes, {} edges",
            graph.nodes.len(),
            graph.edges.len()
        )];

        for node in &graph.nodes {
            if let Some(deps) = adj.get(node) {
                if deps.is_empty() {
                    lines.push(format!("  {} (no dependencies)", node));
                } else {
                    lines.push(format!("  {} -> {}", node, deps.join(", ")));
                }
            } else {
                lines.push(format!("  {} (no dependencies)", node));
            }
        }

        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// Analyze project dependencies and show the dependency graph.
pub struct DepGraphTool;

#[async_trait]
impl Tool for DepGraphTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "dep_graph".into(),
            description: "Analyze and display the project dependency graph. Detects internal crate dependencies in Rust workspaces.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project root path (default: working dir)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let root = args["path"]
            .as_str()
            .map(|p| ctx.working_dir.join(p))
            .unwrap_or_else(|| ctx.working_dir.clone());

        let graph = DepAnalyzer::analyze_rust(&root).await?;
        Ok(DepAnalyzer::format_graph(&graph))
    }
}

/// Detect circular dependencies in the project.
pub struct DepCyclesTool;

#[async_trait]
impl Tool for DepCyclesTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "dep_cycles".into(),
            description: "Detect circular dependencies in the project.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project root path (default: working dir)" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let root = args["path"]
            .as_str()
            .map(|p| ctx.working_dir.join(p))
            .unwrap_or_else(|| ctx.working_dir.clone());

        let graph = DepAnalyzer::analyze_rust(&root).await?;
        let cycles = DepAnalyzer::detect_cycles(&graph);

        if cycles.is_empty() {
            return Ok("No circular dependencies detected.".into());
        }

        let mut lines = vec![format!("Found {} circular dependency cycle(s):", cycles.len())];
        for (i, cycle) in cycles.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, cycle.join(" -> ")));
        }
        Ok(lines.join("\n"))
    }
}

/// Analyze the impact of changing a specific crate/module.
pub struct DepImpactTool;

#[async_trait]
impl Tool for DepImpactTool {
    fn def(&self) -> ToolDef {
        ToolDef {
            name: "dep_impact".into(),
            description: "Analyze the impact of changing a crate. Shows all crates that depend on it.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "crate_name": { "type": "string", "description": "Name of the crate to analyze" },
                    "path": { "type": "string", "description": "Project root path (default: working dir)" }
                },
                "required": ["crate_name"]
            }),
            risk_level: RiskLevel::ReadOnly,
        }
    }

    async fn execute(&self, ctx: &ToolCtx, args: serde_json::Value) -> MaixResult<String> {
        let crate_name = args["crate_name"]
            .as_str()
            .ok_or_else(|| maix_core::MaixError::Tool("missing 'crate_name'".into()))?;
        let root = args["path"]
            .as_str()
            .map(|p| ctx.working_dir.join(p))
            .unwrap_or_else(|| ctx.working_dir.clone());

        let graph = DepAnalyzer::analyze_rust(&root).await?;
        let affected = DepAnalyzer::impact_analysis(&graph, crate_name);

        if affected.is_empty() {
            return Ok(format!("No crates depend on '{}'", crate_name));
        }

        let mut lines = vec![format!(
            "Changing '{}' affects {} crate(s):",
            crate_name,
            affected.len()
        )];
        for (i, dep) in affected.iter().enumerate() {
            lines.push(format!("  {}. {}", i + 1, dep));
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_detection() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![
                DepEdge { from: "a".into(), to: "b".into() },
                DepEdge { from: "b".into(), to: "c".into() },
                DepEdge { from: "c".into(), to: "a".into() },
            ],
        };
        let cycles = DepAnalyzer::detect_cycles(&graph);
        assert!(!cycles.is_empty());
    }

    #[test]
    fn test_no_cycles() {
        let graph = DepGraph {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![
                DepEdge { from: "a".into(), to: "b".into() },
                DepEdge { from: "b".into(), to: "c".into() },
            ],
        };
        let cycles = DepAnalyzer::detect_cycles(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_impact_analysis() {
        let graph = DepGraph {
            nodes: vec!["core".into(), "tools".into(), "agent".into(), "server".into()],
            edges: vec![
                DepEdge { from: "tools".into(), to: "core".into() },
                DepEdge { from: "agent".into(), to: "core".into() },
                DepEdge { from: "agent".into(), to: "tools".into() },
                DepEdge { from: "server".into(), to: "agent".into() },
            ],
        };
        let affected = DepAnalyzer::impact_analysis(&graph, "core");
        assert_eq!(affected.len(), 3); // tools, agent, server
        assert!(affected.contains(&"tools".to_string()));
        assert!(affected.contains(&"agent".to_string()));
        assert!(affected.contains(&"server".to_string()));
    }

    #[test]
    fn test_impact_analysis_leaf() {
        let graph = DepGraph {
            nodes: vec!["core".into(), "tools".into()],
            edges: vec![
                DepEdge { from: "tools".into(), to: "core".into() },
            ],
        };
        let affected = DepAnalyzer::impact_analysis(&graph, "tools");
        assert!(affected.is_empty());
    }
}
