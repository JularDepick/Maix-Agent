//! Client auto-launcher — ensures maix.exe is running before clients connect.

use std::path::PathBuf;

const POLL_INTERVAL_MS: u64 = 200;
const STARTUP_TIMEOUT_MS: u64 = 10_000;

fn exe_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn find_maix_exe() -> Option<PathBuf> {
    // 1. Next to the current exe (same directory as maix-tui.exe / maix-cli.exe)
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join(exe_name("maix"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 2. target/release/ and target/debug/ relative to CARGO_MANIFEST_DIR (dev layout)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let workspace_root = PathBuf::from(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        if let Some(root) = workspace_root {
            // Try release first, then debug
            for profile in &["release", "debug"] {
                let candidate = root.join("target").join(profile).join(exe_name("maix"));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    // 3. On PATH
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(exe_name("maix"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

async fn spawn_and_wait(exe_path: &std::path::Path, server_addr: &str) -> bool {
    let mut cmd = std::process::Command::new(exe_path);
    cmd.arg("--foreground");
    cmd.arg("--listen").arg(server_addr);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    match cmd.spawn() {
        Ok(child) => {
            tracing::info!("Launched maix daemon (pid {})", child.id());
            poll_health(server_addr).await
        }
        Err(e) => {
            tracing::error!("Failed to spawn maix: {e}");
            false
        }
    }
}

async fn poll_health(server_addr: &str) -> bool {
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_millis(STARTUP_TIMEOUT_MS);

    loop {
        if let Ok(true) = try_health_check(server_addr).await { return true }

        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

async fn try_health_check(server_addr: &str) -> Result<bool, std::io::Error> {
    let stream = tokio::net::TcpStream::connect(server_addr).await?;
    drop(stream);
    Ok(true)
}

/// Ensure the maix.exe daemon is running.
///
/// Returns `Ok(())` if the server is reachable (already running or successfully
/// launched). Returns `Err(message)` if the server could not be started.
pub async fn ensure_server_running(server_addr: &str) -> Result<(), String> {
    if try_health_check(server_addr).await.unwrap_or(false) {
        tracing::debug!("maix server already running at {server_addr}");
        return Ok(());
    }

    match find_maix_exe() {
        Some(exe) => {
            tracing::info!("Starting maix daemon: {}", exe.display());
            if spawn_and_wait(&exe, server_addr).await {
                Ok(())
            } else {
                Err(format!(
                    "maix daemon started but not reachable at {server_addr} within {}s. \
                     Check if port is available and config is valid.",
                    STARTUP_TIMEOUT_MS / 1000
                ))
            }
        }
        None => {
            let search_info = describe_search();
            Err(format!(
                "Could not find maix executable.\n\
                 Searched:\n{search_info}\n\
                 Fix: run 'cargo build --release' or add maix to PATH."
            ))
        }
    }
}

fn describe_search() -> String {
    let mut lines = Vec::new();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            lines.push(format!("  1. {} (next to current exe)", dir.join(exe_name("maix")).display()));
        }
    }

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        if let Some(root) = PathBuf::from(&manifest_dir).parent().and_then(|p| p.parent()) {
            lines.push(format!("  2. {} (release)", root.join("target").join("release").join(exe_name("maix")).display()));
            lines.push(format!("  3. {} (debug)", root.join("target").join("debug").join(exe_name("maix")).display()));
        }
    }

    lines.push("  4. $PATH".into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exe_name() {
        #[cfg(target_os = "windows")]
        assert_eq!(exe_name("maix"), "maix.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(exe_name("maix"), "maix");
    }

    #[test]
    fn test_exe_name_custom() {
        #[cfg(target_os = "windows")]
        assert_eq!(exe_name("myapp"), "myapp.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(exe_name("myapp"), "myapp");
    }
}
