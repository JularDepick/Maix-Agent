//! Daemon lifecycle management.
//!
//! Handles platform-specific daemonization (Unix fork + setsid, Windows service-like).
//! On Unix, uses the `daemonize` crate for proper double-fork + PID file.

use std::path::PathBuf;

#[allow(dead_code)]
pub fn pid_file_path() -> PathBuf {
    crate::transport::home_dir()
        .join(".maix")
        .join("maix-server.pid")
}

#[cfg(unix)]
pub fn daemonize() -> Result<(), Box<dyn std::error::Error>> {
    use daemonize::Daemonize;
    Daemonize::new()
        .pid_file(pid_file_path())
        .working_directory(std::env::current_dir()?)
        .start()?;
    Ok(())
}

#[cfg(not(unix))]
pub fn daemonize() -> Result<(), Box<dyn std::error::Error>> {
    // On Windows, daemon mode is not supported — use --foreground instead.
    // The client auto-launcher spawns with --foreground on all platforms.
    tracing::info!("daemon mode not supported on this platform; running in foreground");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_file_path() {
        let path = pid_file_path();
        assert!(path.ends_with("maix-server.pid"));
        assert!(path.to_string_lossy().contains(".maix"));
    }

    #[test]
    fn test_daemonize_non_unix_succeeds() {
        // On non-Unix, daemonize() should succeed (no-op)
        let result = daemonize();
        assert!(result.is_ok());
    }
}
