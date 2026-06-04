#![allow(dead_code)]
//! Real-time collaboration — multi-user sessions with role-based permissions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// User role in a collaboration session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRole {
    /// Full control: can manage users, kick, change settings.
    Host,
    /// Can send messages and execute commands.
    Editor,
    /// Read-only access, can observe.
    Viewer,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }

    pub fn can_send(&self) -> bool {
        matches!(self, Self::Host | Self::Editor)
    }

    pub fn can_execute(&self) -> bool {
        matches!(self, Self::Host | Self::Editor)
    }

    pub fn can_manage_users(&self) -> bool {
        matches!(self, Self::Host)
    }
}

/// A user in a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabUser {
    pub user_id: String,
    pub display_name: String,
    pub role: UserRole,
    pub joined_at: SystemTime,
    pub last_active: SystemTime,
}

/// A shared message in the collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabMessage {
    pub id: String,
    pub user_id: String,
    pub text: String,
    pub timestamp: SystemTime,
    pub is_agent: bool,
}

/// Session state shared among participants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedState {
    pub messages: Vec<CollabMessage>,
    pub context_summary: Option<String>,
    pub active_task: Option<String>,
}

/// Collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationSession {
    pub id: String,
    pub name: String,
    pub created_at: SystemTime,
    pub created_by: String,
    pub users: HashMap<String, CollabUser>,
    pub state: SharedState,
    pub max_users: usize,
}

impl CollaborationSession {
    pub fn new(id: &str, name: &str, host_id: &str, host_name: &str) -> Self {
        let now = SystemTime::now();
        let mut users = HashMap::new();
        users.insert(
            host_id.to_string(),
            CollabUser {
                user_id: host_id.to_string(),
                display_name: host_name.to_string(),
                role: UserRole::Host,
                joined_at: now,
                last_active: now,
            },
        );

        Self {
            id: id.to_string(),
            name: name.to_string(),
            created_at: now,
            created_by: host_id.to_string(),
            users,
            state: SharedState::default(),
            max_users: 10,
        }
    }

    pub fn add_user(&mut self, user_id: &str, display_name: &str, role: UserRole) -> Result<(), String> {
        if self.users.len() >= self.max_users {
            return Err(format!("session full (max {})", self.max_users));
        }
        if self.users.contains_key(user_id) {
            return Err(format!("user '{}' already in session", user_id));
        }
        let now = SystemTime::now();
        self.users.insert(
            user_id.to_string(),
            CollabUser {
                user_id: user_id.to_string(),
                display_name: display_name.to_string(),
                role,
                joined_at: now,
                last_active: now,
            },
        );
        Ok(())
    }

    pub fn remove_user(&mut self, user_id: &str) -> Result<(), String> {
        if user_id == self.created_by {
            return Err("cannot remove the host".to_string());
        }
        self.users.remove(user_id)
            .ok_or_else(|| format!("user '{}' not in session", user_id))?;
        Ok(())
    }

    pub fn change_role(&mut self, user_id: &str, new_role: UserRole) -> Result<(), String> {
        let user = self.users.get_mut(user_id)
            .ok_or_else(|| format!("user '{}' not in session", user_id))?;
        user.role = new_role;
        Ok(())
    }

    pub fn send_message(&mut self, user_id: &str, text: &str) -> Result<(), String> {
        let user = self.users.get(user_id)
            .ok_or_else(|| format!("user '{}' not in session", user_id))?;
        if !user.role.can_send() {
            return Err(format!("user '{}' ({}) cannot send messages", user_id, user.role.as_str()));
        }

        let msg = CollabMessage {
            id: format!("msg-{}", self.state.messages.len() + 1),
            user_id: user_id.to_string(),
            text: text.to_string(),
            timestamp: SystemTime::now(),
            is_agent: false,
        };
        self.state.messages.push(msg);
        Ok(())
    }

    pub fn send_agent_message(&mut self, text: &str) {
        let msg = CollabMessage {
            id: format!("msg-{}", self.state.messages.len() + 1),
            user_id: "agent".to_string(),
            text: text.to_string(),
            timestamp: SystemTime::now(),
            is_agent: true,
        };
        self.state.messages.push(msg);
    }

    pub fn user_count(&self) -> usize {
        self.users.len()
    }

    pub fn format_session(&self) -> String {
        let mut lines = vec![
            format!("Session: {} ({})", self.name, self.id),
            format!("Users ({}/{}):", self.users.len(), self.max_users),
        ];
        for user in self.users.values() {
            lines.push(format!("  {} [{}]", user.display_name, user.role.as_str()));
        }
        if !self.state.messages.is_empty() {
            lines.push(format!("Messages: {}", self.state.messages.len()));
        }
        lines.join("\n")
    }
}

/// Permission check result.
#[derive(Debug, Clone)]
pub struct PermissionCheck {
    pub allowed: bool,
    pub reason: String,
}

/// Permission manager for collaboration sessions.
pub struct PermissionManager {
    /// Default role for new users.
    pub default_role: UserRole,
    /// Whether viewers can see other users' messages.
    pub viewers_see_all: bool,
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self {
            default_role: UserRole::Editor,
            viewers_see_all: true,
        }
    }
}

impl PermissionManager {
    pub fn check_send(&self, session: &CollaborationSession, user_id: &str) -> PermissionCheck {
        match session.users.get(user_id) {
            Some(user) if user.role.can_send() => PermissionCheck {
                allowed: true,
                reason: "ok".to_string(),
            },
            Some(user) => PermissionCheck {
                allowed: false,
                reason: format!("role '{}' cannot send messages", user.role.as_str()),
            },
            None => PermissionCheck {
                allowed: false,
                reason: format!("user '{}' not in session", user_id),
            },
        }
    }

    pub fn check_execute(&self, session: &CollaborationSession, user_id: &str) -> PermissionCheck {
        match session.users.get(user_id) {
            Some(user) if user.role.can_execute() => PermissionCheck {
                allowed: true,
                reason: "ok".to_string(),
            },
            Some(user) => PermissionCheck {
                allowed: false,
                reason: format!("role '{}' cannot execute commands", user.role.as_str()),
            },
            None => PermissionCheck {
                allowed: false,
                reason: format!("user '{}' not in session", user_id),
            },
        }
    }

    pub fn check_manage(&self, session: &CollaborationSession, user_id: &str) -> PermissionCheck {
        match session.users.get(user_id) {
            Some(user) if user.role.can_manage_users() => PermissionCheck {
                allowed: true,
                reason: "ok".to_string(),
            },
            Some(user) => PermissionCheck {
                allowed: false,
                reason: format!("role '{}' cannot manage users", user.role.as_str()),
            },
            None => PermissionCheck {
                allowed: false,
                reason: format!("user '{}' not in session", user_id),
            },
        }
    }
}

/// Session manager — holds all active collaboration sessions.
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<RwLock<CollaborationSession>>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_session(&self, name: &str, host_id: &str, host_name: &str) -> String {
        let id = format!("collab-{}", uuid_count());
        let session = CollaborationSession::new(&id, name, host_id, host_name);
        let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        sessions.insert(id.clone(), Arc::new(RwLock::new(session)));
        id
    }

    pub fn get_session(&self, id: &str) -> Option<Arc<RwLock<CollaborationSession>>> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.get(id).cloned()
    }

    pub fn remove_session(&self, id: &str) -> bool {
        let mut sessions = self.sessions.write().unwrap_or_else(|e| e.into_inner());
        sessions.remove(id).is_some()
    }

    pub fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.keys().cloned().collect()
    }

    pub fn session_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap_or_else(|e| e.into_inner());
        sessions.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

fn uuid_count() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let session = CollaborationSession::new("s1", "Test Session", "host1", "Alice");
        assert_eq!(session.id, "s1");
        assert_eq!(session.user_count(), 1);
        assert!(session.users.contains_key("host1"));
    }

    #[test]
    fn test_add_user() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("user1", "Bob", UserRole::Editor).unwrap();
        assert_eq!(session.user_count(), 2);
        assert_eq!(session.users["user1"].role, UserRole::Editor);
    }

    #[test]
    fn test_add_duplicate_user() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("user1", "Bob", UserRole::Editor).unwrap();
        assert!(session.add_user("user1", "Bob2", UserRole::Viewer).is_err());
    }

    #[test]
    fn test_remove_user() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("user1", "Bob", UserRole::Editor).unwrap();
        session.remove_user("user1").unwrap();
        assert_eq!(session.user_count(), 1);
    }

    #[test]
    fn test_cannot_remove_host() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        assert!(session.remove_user("host1").is_err());
    }

    #[test]
    fn test_change_role() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("user1", "Bob", UserRole::Editor).unwrap();
        session.change_role("user1", UserRole::Viewer).unwrap();
        assert_eq!(session.users["user1"].role, UserRole::Viewer);
    }

    #[test]
    fn test_send_message() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("user1", "Bob", UserRole::Editor).unwrap();
        session.send_message("user1", "Hello!").unwrap();
        assert_eq!(session.state.messages.len(), 1);
        assert_eq!(session.state.messages[0].text, "Hello!");
    }

    #[test]
    fn test_viewer_cannot_send() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("user1", "Bob", UserRole::Viewer).unwrap();
        assert!(session.send_message("user1", "Hello!").is_err());
    }

    #[test]
    fn test_agent_message() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.send_agent_message("I can help with that.");
        assert_eq!(session.state.messages.len(), 1);
        assert!(session.state.messages[0].is_agent);
    }

    #[test]
    fn test_user_role_permissions() {
        assert!(UserRole::Host.can_send());
        assert!(UserRole::Editor.can_send());
        assert!(!UserRole::Viewer.can_send());
        assert!(UserRole::Host.can_manage_users());
        assert!(!UserRole::Editor.can_manage_users());
    }

    #[test]
    fn test_permission_manager() {
        let pm = PermissionManager::default();
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.add_user("editor1", "Bob", UserRole::Editor).unwrap();
        session.add_user("viewer1", "Charlie", UserRole::Viewer).unwrap();

        assert!(pm.check_send(&session, "host1").allowed);
        assert!(pm.check_send(&session, "editor1").allowed);
        assert!(!pm.check_send(&session, "viewer1").allowed);
        assert!(!pm.check_send(&session, "nobody").allowed);

        assert!(pm.check_manage(&session, "host1").allowed);
        assert!(!pm.check_manage(&session, "editor1").allowed);
    }

    #[test]
    fn test_session_manager() {
        let mgr = SessionManager::new();
        let id = mgr.create_session("Test Session", "host1", "Alice");
        assert!(id.starts_with("collab-"));
        assert_eq!(mgr.session_count(), 1);

        let session = mgr.get_session(&id).unwrap();
        let s = session.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(s.name, "Test Session");
        drop(s);

        mgr.remove_session(&id);
        assert_eq!(mgr.session_count(), 0);
    }

    #[test]
    fn test_session_manager_list() {
        let mgr = SessionManager::new();
        mgr.create_session("A", "h1", "Alice");
        mgr.create_session("B", "h2", "Bob");
        let list = mgr.list_sessions();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_format_session() {
        let session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        let s = session.format_session();
        assert!(s.contains("Test"));
        assert!(s.contains("Alice"));
    }

    #[test]
    fn test_max_users() {
        let mut session = CollaborationSession::new("s1", "Test", "host1", "Alice");
        session.max_users = 2;
        session.add_user("u1", "Bob", UserRole::Editor).unwrap();
        assert!(session.add_user("u2", "Charlie", UserRole::Viewer).is_err());
    }
}
