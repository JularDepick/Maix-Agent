//! Multi-turn tool chaining — orchestrates multiple tool calls with variable passing.

use std::collections::HashMap;

/// A single step in a tool chain.
#[derive(Debug, Clone)]
pub struct ChainStep {
    pub tool_name: String,
    pub params: serde_json::Value,
    pub depends_on: Option<usize>,
    pub output_var: String,
}

/// A chain of tool calls to execute in sequence.
#[derive(Debug, Clone)]
pub struct ToolChain {
    pub steps: Vec<ChainStep>,
    pub current: usize,
}

impl ToolChain {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            current: 0,
        }
    }

    pub fn add_step(
        &mut self,
        tool_name: &str,
        params: serde_json::Value,
        output_var: &str,
        depends_on: Option<usize>,
    ) {
        self.steps.push(ChainStep {
            tool_name: tool_name.to_string(),
            params,
            depends_on,
            output_var: output_var.to_string(),
        });
    }

    pub fn is_complete(&self) -> bool {
        self.current >= self.steps.len()
    }

    pub fn remaining(&self) -> usize {
        self.steps.len().saturating_sub(self.current)
    }
}

impl Default for ToolChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Executes tool chains with variable resolution.
pub struct ChainExecutor {
    variables: HashMap<String, serde_json::Value>,
}

impl ChainExecutor {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    pub fn set_variable(&mut self, name: &str, value: serde_json::Value) {
        self.variables.insert(name.to_string(), value);
    }

    pub fn get_variable(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables.get(name)
    }

    pub fn variables(&self) -> &HashMap<String, serde_json::Value> {
        &self.variables
    }

    /// Resolve variable references in params (e.g., "$var_name").
    pub fn resolve_params(&self, params: &serde_json::Value) -> serde_json::Value {
        match params {
            serde_json::Value::String(s) => {
                if let Some(var_name) = s.strip_prefix('$') {
                    self.variables
                        .get(var_name)
                        .cloned()
                        .unwrap_or_else(|| params.clone())
                } else {
                    params.clone()
                }
            }
            serde_json::Value::Object(map) => {
                let resolved: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), self.resolve_params(v)))
                    .collect();
                serde_json::Value::Object(resolved)
            }
            serde_json::Value::Array(arr) => {
                let resolved: Vec<serde_json::Value> =
                    arr.iter().map(|v| self.resolve_params(v)).collect();
                serde_json::Value::Array(resolved)
            }
            _ => params.clone(),
        }
    }

    /// Execute a single step, returning the result.
    pub fn execute_step_result(
        &mut self,
        step: &ChainStep,
        result: serde_json::Value,
    ) {
        self.variables
            .insert(step.output_var.clone(), result);
    }

    /// Build a plan for executing a chain (respects dependencies).
    pub fn execution_plan(chain: &ToolChain) -> Vec<usize> {
        let mut plan = Vec::new();
        let mut completed = std::collections::HashSet::new();

        for _ in 0..chain.steps.len() {
            for (i, step) in chain.steps.iter().enumerate() {
                if plan.contains(&i) {
                    continue;
                }
                let deps_met = step
                    .depends_on
                    .map(|dep| completed.contains(&dep))
                    .unwrap_or(true);
                if deps_met {
                    plan.push(i);
                    completed.insert(i);
                    break;
                }
            }
        }
        plan
    }
}

impl Default for ChainExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_chain_new() {
        let chain = ToolChain::new();
        assert!(chain.is_complete());
        assert_eq!(chain.remaining(), 0);
    }

    #[test]
    fn test_tool_chain_add_step() {
        let mut chain = ToolChain::new();
        chain.add_step(
            "fs_read",
            serde_json::json!({"path": "main.rs"}),
            "file_content",
            None,
        );
        assert_eq!(chain.steps.len(), 1);
        assert!(!chain.is_complete());
        assert_eq!(chain.remaining(), 1);
    }

    #[test]
    fn test_chain_executor_variable() {
        let mut exec = ChainExecutor::new();
        exec.set_variable("x", serde_json::json!(42));
        assert_eq!(exec.get_variable("x"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn test_resolve_simple_var() {
        let mut exec = ChainExecutor::new();
        exec.set_variable("file", serde_json::json!("main.rs"));
        let params = serde_json::json!({"path": "$file"});
        let resolved = exec.resolve_params(&params);
        assert_eq!(resolved["path"], "main.rs");
    }

    #[test]
    fn test_resolve_no_var() {
        let exec = ChainExecutor::new();
        let params = serde_json::json!({"path": "main.rs"});
        let resolved = exec.resolve_params(&params);
        assert_eq!(resolved["path"], "main.rs");
    }

    #[test]
    fn test_resolve_nested_var() {
        let mut exec = ChainExecutor::new();
        exec.set_variable("content", serde_json::json!("hello"));
        let params = serde_json::json!({
            "file": {"path": "$content", "mode": "read"}
        });
        let resolved = exec.resolve_params(&params);
        assert_eq!(resolved["file"]["path"], "hello");
    }

    #[test]
    fn test_resolve_array_var() {
        let mut exec = ChainExecutor::new();
        exec.set_variable("x", serde_json::json!("val"));
        let params = serde_json::json!(["$x", "literal"]);
        let resolved = exec.resolve_params(&params);
        assert_eq!(resolved[0], "val");
        assert_eq!(resolved[1], "literal");
    }

    #[test]
    fn test_resolve_missing_var() {
        let exec = ChainExecutor::new();
        let params = serde_json::json!({"path": "$missing"});
        let resolved = exec.resolve_params(&params);
        assert_eq!(resolved["path"], "$missing"); // unchanged
    }

    #[test]
    fn test_execute_step_result() {
        let mut exec = ChainExecutor::new();
        let step = ChainStep {
            tool_name: "fs_read".into(),
            params: serde_json::json!({}),
            depends_on: None,
            output_var: "result".into(),
        };
        exec.execute_step_result(&step, serde_json::json!("file content"));
        assert_eq!(exec.get_variable("result"), Some(&serde_json::json!("file content")));
    }

    #[test]
    fn test_execution_plan_no_deps() {
        let mut chain = ToolChain::new();
        chain.add_step("a", serde_json::json!({}), "out_a", None);
        chain.add_step("b", serde_json::json!({}), "out_b", None);
        let plan = ChainExecutor::execution_plan(&chain);
        assert_eq!(plan, vec![0, 1]);
    }

    #[test]
    fn test_execution_plan_with_deps() {
        let mut chain = ToolChain::new();
        chain.add_step("a", serde_json::json!({}), "out_a", None);
        chain.add_step("b", serde_json::json!({}), "out_b", Some(0));
        let plan = ChainExecutor::execution_plan(&chain);
        assert_eq!(plan, vec![0, 1]);
    }

    #[test]
    fn test_chain_is_complete() {
        let mut chain = ToolChain::new();
        chain.add_step("a", serde_json::json!({}), "x", None);
        chain.current = 1;
        assert!(chain.is_complete());
    }
}
