//! Desktop notifications — cross-platform notification support.

/// Send a desktop notification. Non-blocking, best-effort.
pub fn send_notification(title: &str, body: &str) {
    let title = title.to_string();
    let body = body.to_string();
    std::thread::spawn(move || {
        send_notification_impl(&title, &body);
    });
}

fn send_notification_impl(title: &str, body: &str) {
    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to send a Windows toast notification
        let script = format!(
            r#"Add-Type -AssemblyName System.Windows.Forms
$notify = New-Object System.Windows.Forms.NotifyIcon
$notify.Icon = [System.Drawing.SystemIcons]::Information
$notify.Visible = $true
$notify.ShowBalloonTip(5000, '{}', '{}', [System.Windows.Forms.ToolTipIcon]::Info)"#,
            title.replace('\'', "''"),
            body.replace('\'', "''")
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "display notification \"{}\" with title \"{}\"",
                    body.replace('"', "\\\""),
                    title.replace('"', "\\\"")
                ),
            ])
            .output();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([title, body])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_notification_does_not_panic() {
        // Just verify it doesn't panic (actual notification may not show in CI)
        send_notification("Test", "Hello from Maix-Agent");
    }
}
