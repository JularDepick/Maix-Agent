#![allow(dead_code)]
//! Desktop notification system for task completion and errors.

/// Notification configuration.
#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub on_task_complete: bool,
    pub on_error: bool,
    pub sound: bool,
    pub timeout_ms: u32,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            on_task_complete: true,
            on_error: true,
            sound: false,
            timeout_ms: 5000,
        }
    }
}

/// Notification severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    Info,
    Success,
    Warning,
    Error,
}

/// Desktop notifier.
pub struct Notifier {
    config: NotificationConfig,
    history: Vec<NotificationRecord>,
}

#[derive(Debug, Clone)]
pub struct NotificationRecord {
    pub title: String,
    pub body: String,
    pub kind: NotifyKind,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Notifier {
    pub fn new(config: NotificationConfig) -> Self {
        Self {
            config,
            history: Vec::new(),
        }
    }

    pub fn notify(&mut self, title: &str, body: &str, kind: NotifyKind) {
        if !self.config.enabled {
            return;
        }

        self.history.push(NotificationRecord {
            title: title.to_string(),
            body: body.to_string(),
            kind,
            timestamp: chrono::Utc::now(),
        });

        // Platform-specific notification
        self.send_platform_notification(title, body, kind);
    }

    #[cfg(target_os = "windows")]
    fn send_platform_notification(&self, title: &str, body: &str, _kind: NotifyKind) {
        // Use PowerShell to send Windows toast notification
        let script = format!(
            r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
$template = @"
<toast>
    <visual>
        <binding template="ToastGeneric">
            <text>{}</text>
            <text>{}</text>
        </binding>
    </visual>
</toast>
"@
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml($template)
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("Maix-Agent").Show($toast)"#,
            title.replace('"', "&quot;"),
            body.replace('"', "&quot;").replace('\n', "&#10;")
        );
        let _ = std::process::Command::new("powershell")
            .args(["-Command", &script])
            .output();
    }

    #[cfg(target_os = "macos")]
    fn send_platform_notification(&self, title: &str, body: &str, _kind: NotifyKind) {
        let script = format!(
            r#"display notification "{}" with title "{}""#,
            body.replace('"', "\\\""),
            title.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }

    #[cfg(target_os = "linux")]
    fn send_platform_notification(&self, title: &str, body: &str, _kind: NotifyKind) {
        let _ = std::process::Command::new("notify-send")
            .args([title, body])
            .output();
    }

    pub fn task_complete(&mut self, summary: &str) {
        if self.config.on_task_complete {
            self.notify("Task Complete", summary, NotifyKind::Success);
        }
    }

    pub fn error(&mut self, message: &str) {
        if self.config.on_error {
            self.notify("Error", message, NotifyKind::Error);
        }
    }

    pub fn warning(&mut self, message: &str) {
        self.notify("Warning", message, NotifyKind::Warning);
    }

    pub fn history(&self) -> &[NotificationRecord] {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Play a system sound for important events.
    pub fn play_sound(&self, kind: NotifyKind) {
        if !self.config.enabled || !self.config.sound {
            return;
        }
        self.play_platform_sound(kind);
    }

    #[cfg(target_os = "windows")]
    fn play_platform_sound(&self, kind: NotifyKind) {
        let sound = match kind {
            NotifyKind::Info => "SystemAsterisk",
            NotifyKind::Success => "SystemExclamation",
            NotifyKind::Warning => "SystemExclamation",
            NotifyKind::Error => "SystemHand",
        };
        let script = format!(r#"Add-Type -AssemblyName System.Media; [System.Media.SystemSounds]::{}.Play()"#, sound);
        let _ = std::process::Command::new("powershell")
            .args(["-Command", &script])
            .output();
    }

    #[cfg(target_os = "macos")]
    fn play_platform_sound(&self, _kind: NotifyKind) {
        let _ = std::process::Command::new("afplay")
            .args(["/System/Library/Sounds/Glass.aiff"])
            .output();
    }

    #[cfg(target_os = "linux")]
    fn play_platform_sound(&self, _kind: NotifyKind) {
        let _ = std::process::Command::new("paplay")
            .args(["/usr/share/sounds/freedesktop/stereo/complete.oga"])
            .output();
    }

    pub fn set_sound(&mut self, enabled: bool) {
        self.config.sound = enabled;
    }

    pub fn sound_enabled(&self) -> bool {
        self.config.sound
    }
}

fn kind_str(kind: NotifyKind) -> &'static str {
    match kind {
        NotifyKind::Info => "INFO",
        NotifyKind::Success => "OK",
        NotifyKind::Warning => "WARN",
        NotifyKind::Error => "ERR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notifier_default_config() {
        let config = NotificationConfig::default();
        assert!(config.enabled);
        assert!(config.on_task_complete);
        assert!(config.on_error);
    }

    #[test]
    fn test_notifier_disabled() {
        let config = NotificationConfig {
            enabled: false,
            ..Default::default()
        };
        let mut n = Notifier::new(config);
        n.notify("test", "body", NotifyKind::Info);
        assert!(n.history().is_empty());
    }

    #[test]
    fn test_notifier_records_history() {
        let mut n = Notifier::new(NotificationConfig::default());
        n.notify("Test", "Hello", NotifyKind::Info);
        assert_eq!(n.history().len(), 1);
        assert_eq!(n.history()[0].title, "Test");
    }

    #[test]
    fn test_task_complete() {
        let mut n = Notifier::new(NotificationConfig::default());
        n.task_complete("All tests passed");
        assert_eq!(n.history().len(), 1);
        assert_eq!(n.history()[0].kind, NotifyKind::Success);
    }

    #[test]
    fn test_error_notification() {
        let mut n = Notifier::new(NotificationConfig::default());
        n.error("Something went wrong");
        assert_eq!(n.history().len(), 1);
        assert_eq!(n.history()[0].kind, NotifyKind::Error);
    }

    #[test]
    fn test_warning_notification() {
        let mut n = Notifier::new(NotificationConfig::default());
        n.warning("Low disk space");
        assert_eq!(n.history().len(), 1);
        assert_eq!(n.history()[0].kind, NotifyKind::Warning);
    }

    #[test]
    fn test_clear_history() {
        let mut n = Notifier::new(NotificationConfig::default());
        n.notify("a", "b", NotifyKind::Info);
        n.notify("c", "d", NotifyKind::Info);
        assert_eq!(n.history().len(), 2);
        n.clear_history();
        assert!(n.history().is_empty());
    }

    #[test]
    fn test_toggle_enabled() {
        let mut n = Notifier::new(NotificationConfig::default());
        assert!(n.is_enabled());
        n.set_enabled(false);
        assert!(!n.is_enabled());
        n.notify("test", "body", NotifyKind::Info);
        assert!(n.history().is_empty());
    }

    #[test]
    fn test_kind_str() {
        assert_eq!(kind_str(NotifyKind::Info), "INFO");
        assert_eq!(kind_str(NotifyKind::Success), "OK");
        assert_eq!(kind_str(NotifyKind::Warning), "WARN");
        assert_eq!(kind_str(NotifyKind::Error), "ERR");
    }
}
