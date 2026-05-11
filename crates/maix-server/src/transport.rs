#![allow(dead_code)]

use std::path::PathBuf;

/// Platform-aware transport binding.
/// On Unix: Unix domain socket at `~/.maix/maix.sock`.
/// On Windows: TCP fallback (Named Pipe support can be added via uds_windows).
/// Always available: TCP at `127.0.0.1:{port}`.

pub fn default_socket_path() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/tmp/maix.sock")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/tmp/maix.sock")
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"\\.\pipe\maix")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        home_dir().join(".maix").join("maix.sock")
    }
}

pub fn default_pipe_name() -> String {
    r"\\.\pipe\maix".into()
}

pub fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

use tokio::net::{TcpListener, ToSocketAddrs};

/// Create a TCP listener for gRPC.
pub async fn tcp_listener(
    addr: impl ToSocketAddrs,
) -> Result<TcpListener, std::io::Error> {
    TcpListener::bind(addr).await
}

#[cfg(unix)]
pub mod unix_transport {
    use std::path::Path;
    use tokio::net::UnixListener;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::transport::server::UdsConnectInfo;

    pub fn unix_listener_stream(
        path: &Path,
    ) -> Result<UnixListenerStream, std::io::Error> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let listener = UnixListener::bind(path)?;
        Ok(UnixListenerStream::new(listener))
    }

    pub type IncomingStream = UnixListenerStream;
    pub type ConnectInfo = UdsConnectInfo;
}

#[cfg(not(unix))]
pub mod unix_transport {
    use std::path::Path;
    use tokio_stream::wrappers::TcpListenerStream;

    pub fn unix_listener_stream(
        _path: &Path,
    ) -> Result<TcpListenerStream, std::io::Error> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix sockets not supported on this platform; use TCP",
        ))
    }

    pub type IncomingStream = TcpListenerStream;
}
