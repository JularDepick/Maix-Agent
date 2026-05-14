//! Permission system — tool, skill, and path-level authorization (Phase 1.1).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Granular permission for a single resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    Tool { name: String },
    Skill { name: String },
    PathRead(PathBuf),
    PathWrite(PathBuf),
    Shell,
    Network,
}

impl Permission {
    pub fn tool(name: &str) -> Self { Self::Tool { name: name.into() } }
    pub fn skill(name: &str) -> Self { Self::Skill { name: name.into() } }
    pub fn path_read(path: PathBuf) -> Self { Self::PathRead(path) }
    pub fn path_write(path: PathBuf) -> Self { Self::PathWrite(path) }
}

/// A set of granted permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionSet {
    granted: HashSet<Permission>,
    /// When true, all tools are auto-granted (YOLO mode).
    auto_approve_all: bool,
    /// Restrict all file operations to this root.
    sandbox_root: Option<PathBuf>,
}

impl PermissionSet {
    pub fn new() -> Self { Self::default() }

    pub fn with_auto_approve(mut self, auto: bool) -> Self {
        self.auto_approve_all = auto;
        self
    }

    pub fn with_sandbox_root(mut self, root: PathBuf) -> Self {
        self.sandbox_root = Some(root);
        self
    }

    pub fn grant(&mut self, perm: Permission) { self.granted.insert(perm); }
    pub fn revoke(&mut self, perm: &Permission) { self.granted.remove(perm); }

    pub fn add_tool(&mut self, name: &str) { self.grant(Permission::tool(name)); }
    pub fn add_skill(&mut self, name: &str) { self.grant(Permission::skill(name)); }

    /// Check if a tool is allowed.
    pub fn can_use_tool(&self, name: &str) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::tool(name))
    }

    /// Check if a skill is allowed.
    pub fn can_use_skill(&self, name: &str) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::skill(name))
    }

    /// Check if a file read path is within the sandbox.
    pub fn can_read_path(&self, path: &Path) -> bool {
        if self.auto_approve_all { return true; }
        if self.granted.contains(&Permission::path_read(path.to_path_buf())) {
            return true;
        }
        self.is_within_sandbox(path)
    }

    /// Check if a file write path is within the sandbox.
    pub fn can_write_path(&self, path: &Path) -> bool {
        if self.auto_approve_all { return true; }
        if self.granted.contains(&Permission::path_write(path.to_path_buf())) {
            return true;
        }
        self.is_within_sandbox(path)
    }

    /// Check if shell execution is allowed.
    pub fn can_use_shell(&self) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::Shell)
    }

    /// Check if network access is allowed.
    pub fn can_use_network(&self) -> bool {
        self.auto_approve_all || self.granted.contains(&Permission::Network)
    }

    fn is_within_sandbox(&self, path: &Path) -> bool {
        if let Some(ref root) = self.sandbox_root {
            match path.canonicalize() {
                Ok(canon) => canon.starts_with(root),
                Err(_) => false,
            }
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Pattern-matching permission checker (Claude Code parity)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allowed,
    Denied(String),
    AskUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Tool name pattern: "shell_exec", "fs_write", "*" (all tools)
    pub tool_pattern: String,
    /// Optional argument pattern: "(cargo test *)", "(path:src/**)"
    pub arg_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionConfig {
    pub allow: Vec<PermissionRule>,
    pub deny: Vec<PermissionRule>,
}

pub struct PermissionChecker {
    allow_rules: Vec<PermissionRule>,
    deny_rules: Vec<PermissionRule>,
}

impl PermissionChecker {
    pub fn new(config: PermissionConfig) -> Self {
        Self {
            allow_rules: config.allow,
            deny_rules: config.deny,
        }
    }

    pub fn empty() -> Self {
        Self {
            allow_rules: Vec::new(),
            deny_rules: Vec::new(),
        }
    }

    /// Check if a tool call is allowed, denied, or needs user approval.
    pub fn check(
        &self,
        tool_name: &str,
        args_json: &str,
        risk_fallback: PermissionDecision,
    ) -> PermissionDecision {
        // 1. Deny rules first
        for rule in &self.deny_rules {
            if self.matches_rule(rule, tool_name, args_json) {
                return PermissionDecision::Denied(format!(
                    "Denied by rule: {}",
                    self.rule_desc(rule)
                ));
            }
        }

        // 2. Allow rules
        for rule in &self.allow_rules {
            if self.matches_rule(rule, tool_name, args_json) {
                return PermissionDecision::Allowed;
            }
        }

        // 3. Fall back to RiskLevel default
        risk_fallback
    }

    fn matches_rule(&self, rule: &PermissionRule, tool_name: &str, args_json: &str) -> bool {
        if !simple_glob_match(&rule.tool_pattern, tool_name) {
            return false;
        }
        if let Some(ref arg_pattern) = rule.arg_pattern {
            return self.matches_arg_pattern(arg_pattern, args_json);
        }
        true
    }

    fn matches_arg_pattern(&self, pattern: &str, args_json: &str) -> bool {
        let pattern = pattern.trim().trim_start_matches('(').trim_end_matches(')');
        if let Some(colon_pos) = pattern.find(':') {
            let key = &pattern[..colon_pos];
            let value_pattern = &pattern[colon_pos + 1..];
            if let Ok(args) = serde_json::from_str::<serde_json::Value>(args_json) {
                if let Some(value) = args.get(key).and_then(|v| v.as_str()) {
                    return simple_glob_match(value_pattern, value);
                }
            }
            return false;
        }
        simple_glob_match(pattern, &args_json.to_lowercase())
    }

    fn rule_desc(&self, rule: &PermissionRule) -> String {
        let mut desc = rule.tool_pattern.clone();
        if let Some(ref arg) = rule.arg_pattern {
            desc.push_str(arg);
        }
        desc
    }
}

/// Simple glob matching: *, **, ?
pub fn simple_glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_recurse(&p, &t, 0, 0)
}

fn glob_recurse(p: &[char], t: &[char], pi: usize, ti: usize) -> bool {
    // Base: both exhausted
    if pi == p.len() && ti == t.len() {
        return true;
    }
    // Pattern exhausted but text remains
    if pi == p.len() {
        return false;
    }

    // ** matches any characters including /
    if pi + 1 < p.len() && p[pi] == '*' && p[pi + 1] == '*' {
        let mut next = pi + 2;
        if next < p.len() && p[next] == '/' {
            next += 1;
        }
        // Try matching 0, 1, 2, ... characters
        for skip in 0..=(t.len() - ti) {
            if glob_recurse(p, t, next, ti + skip) {
                return true;
            }
        }
        return false;
    }

    // * matches any characters (including /)
    if p[pi] == '*' {
        let next = pi + 1;
        for skip in 0..=(t.len() - ti) {
            if glob_recurse(p, t, next, ti + skip) {
                return true;
            }
        }
        return false;
    }

    // ? matches single char (not /)
    if p[pi] == '?' && ti < t.len() && t[ti] != '/' {
        return glob_recurse(p, t, pi + 1, ti + 1);
    }

    // Literal match
    if ti < t.len() && p[pi] == t[ti] {
        return glob_recurse(p, t, pi + 1, ti + 1);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_permission() {
        let mut ps = PermissionSet::new();
        ps.add_tool("fs_read");
        assert!(ps.can_use_tool("fs_read"));
        assert!(!ps.can_use_tool("shell_exec"));
    }

    #[test]
    fn test_auto_approve() {
        let ps = PermissionSet::new().with_auto_approve(true);
        assert!(ps.can_use_tool("any_tool"));
        assert!(ps.can_use_shell());
        assert!(ps.can_use_network());
    }

    #[test]
    fn test_skill_permission() {
        let mut ps = PermissionSet::new();
        ps.add_skill("code-review");
        assert!(ps.can_use_skill("code-review"));
        assert!(!ps.can_use_skill("shell-tool"));
    }

    // --- PermissionChecker (pattern matching) tests ---

    #[test]
    fn test_glob_match() {
        assert!(simple_glob_match("*", "anything"));
        assert!(simple_glob_match("shell_exec", "shell_exec"));
        assert!(!simple_glob_match("shell_exec", "fs_read"));
        assert!(simple_glob_match("fs_*", "fs_read"));
        assert!(simple_glob_match("git_*", "git_status"));
    }

    #[test]
    fn test_checker_deny_overrides_allow() {
        let config = PermissionConfig {
            allow: vec![PermissionRule {
                tool_pattern: "shell_exec".into(),
                arg_pattern: None,
            }],
            deny: vec![PermissionRule {
                tool_pattern: "shell_exec".into(),
                arg_pattern: Some("(command:rm -rf *)".into()),
            }],
        };
        let checker = PermissionChecker::new(config);

        assert_eq!(
            checker.check("shell_exec", r#"{"command":"rm -rf /"}"#, PermissionDecision::AskUser),
            PermissionDecision::Denied("Denied by rule: shell_exec(command:rm -rf *)".into())
        );
        assert_eq!(
            checker.check("shell_exec", r#"{"command":"cargo test"}"#, PermissionDecision::AskUser),
            PermissionDecision::Allowed
        );
    }

    #[test]
    fn test_checker_allow_auto_approve() {
        let config = PermissionConfig {
            allow: vec![
                PermissionRule { tool_pattern: "fs_read".into(), arg_pattern: None },
                PermissionRule { tool_pattern: "grep".into(), arg_pattern: None },
            ],
            deny: vec![],
        };
        let checker = PermissionChecker::new(config);

        assert_eq!(checker.check("fs_read", "{}", PermissionDecision::AskUser), PermissionDecision::Allowed);
        assert_eq!(checker.check("grep", "{}", PermissionDecision::AskUser), PermissionDecision::Allowed);
        assert_eq!(checker.check("shell_exec", "{}", PermissionDecision::AskUser), PermissionDecision::AskUser);
    }

    #[test]
    fn test_checker_path_pattern() {
        let config = PermissionConfig {
            allow: vec![PermissionRule {
                tool_pattern: "fs_write".into(),
                arg_pattern: Some("(file_path:src/**)".into()),
            }],
            deny: vec![],
        };
        let checker = PermissionChecker::new(config);

        assert_eq!(
            checker.check("fs_write", r#"{"file_path":"src/main.rs"}"#, PermissionDecision::AskUser),
            PermissionDecision::Allowed
        );
        assert_eq!(
            checker.check("fs_write", r#"{"file_path":"/etc/passwd"}"#, PermissionDecision::AskUser),
            PermissionDecision::AskUser
        );
    }

    #[test]
    fn test_checker_wildcard_tool() {
        let config = PermissionConfig {
            allow: vec![PermissionRule { tool_pattern: "git_*".into(), arg_pattern: None }],
            deny: vec![],
        };
        let checker = PermissionChecker::new(config);

        assert_eq!(checker.check("git_status", "{}", PermissionDecision::AskUser), PermissionDecision::Allowed);
        assert_eq!(checker.check("git_diff", "{}", PermissionDecision::AskUser), PermissionDecision::Allowed);
        assert_eq!(checker.check("fs_read", "{}", PermissionDecision::AskUser), PermissionDecision::AskUser);
    }
}
