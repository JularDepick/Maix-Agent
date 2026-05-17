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

/// Windows Named Pipe transport for gRPC.
#[cfg(windows)]
pub mod named_pipe_transport {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    /// A Named Pipe listener that yields `NamedPipeConnection` instances.
    pub struct NamedPipeListener {
        pipe_name: String,
    }

    impl NamedPipeListener {
        pub fn bind(pipe_name: &str) -> Result<Self, std::io::Error> {
            // Create the first pipe instance to ensure the name is valid
            let _server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(pipe_name)?;
            Ok(Self {
                pipe_name: pipe_name.to_string(),
            })
        }
    }

    /// A connected Named Pipe stream.
    pub struct NamedPipeConnection {
        inner: NamedPipeServer,
    }

    impl tonic::transport::server::Connected for NamedPipeConnection {
        type ConnectInfo = ();

        fn connect_info(&self) -> Self::ConnectInfo {}
    }

    impl AsyncRead for NamedPipeConnection {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for NamedPipeConnection {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Pin::new(&mut self.inner).poll_write(cx, buf)
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.inner).poll_flush(cx)
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.inner).poll_shutdown(cx)
        }
    }

    /// Stream adapter for tonic's `serve_with_incoming`.
    pub struct NamedPipeListenerStream {
        listener: NamedPipeListener,
        pending: Option<Pin<Box<dyn std::future::Future<Output = Result<NamedPipeConnection, std::io::Error>> + Send>>>,
    }

    impl NamedPipeListenerStream {
        pub fn new(listener: NamedPipeListener) -> Self {
            Self { listener, pending: None }
        }
    }

    impl futures::Stream for NamedPipeListenerStream {
        type Item = Result<NamedPipeConnection, std::io::Error>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            loop {
                if let Some(ref mut fut) = self.pending {
                    match fut.as_mut().poll(cx) {
                        Poll::Ready(result) => {
                            self.pending = None;
                            return Poll::Ready(Some(result));
                        }
                        Poll::Pending => return Poll::Pending,
                    }
                }
                let pipe_name = self.listener.pipe_name.clone();
                self.pending = Some(Box::pin(async move {
                    let server = ServerOptions::new().create(&pipe_name)?;
                    server.connect().await?;
                    Ok(NamedPipeConnection { inner: server })
                }));
            }
        }
    }
}
