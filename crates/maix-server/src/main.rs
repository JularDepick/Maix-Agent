//! # Maix-Server
//!
//! gRPC server for Maix-Agent — handles client connections, session management,
//! and orchestrates agent interactions.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │   CLI/TUI   │────▶│  gRPC Server │────▶│  Agent Core │
//! └─────────────┘     └─────────────┘     └─────────────┘
//!                           │
//!                     ┌─────┴─────┐
//!                     ▼           ▼
//!               Session Mgr  Transport
//! ```
//!
//! ## Modules
//!
//! - [`server`] — Core service implementation
//! - [`session_manager`] — Session lifecycle management
//! - [`chat_stream`] — Streaming chat response handling
//! - [`transport`] — Transport layer (TCP, Unix socket, Named Pipe)
//! - [`daemon`] — Daemonization and service management
//! - [`shutdown`] — Graceful shutdown handling

mod chat_stream;
mod collaboration;
mod daemon;
mod server;
pub mod service;
mod session_manager;
mod shutdown;
mod transport;

use clap::Parser;
use std::sync::Arc;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;

use maix_core::config::TransportMode;
use maix_core::proto::maix::core::v1::core_service_server::CoreServiceServer;
use server::{MaixCoreService, ServerCore};

#[derive(Parser, Debug)]
#[command(name = "maix", version)]
struct Cli {
    /// Run in foreground (skip daemonize)
    #[arg(long)]
    foreground: bool,

    /// Run as Windows Service (used by SCM, not for manual invocation)
    #[arg(long, hide = true)]
    service: bool,

    /// Unix socket path (overrides config)
    #[arg(long)]
    socket_path: Option<String>,

    /// Listen address for gRPC (overrides config, e.g. "0.0.0.0:26506")
    #[arg(long)]
    listen: Option<String>,

    /// Transport mode: auto, tcp, unix-socket, named-pipe
    #[arg(long, default_value = "auto")]
    transport: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    maix_core::init_console_utf8();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Windows Service mode: dispatch to SCM
    if cli.service {
        return service::windows::start_dispatcher().map_err(|e| e.into());
    }

    // Run the async server
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_main(cli))
}

async fn async_main(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if !cli.foreground {
        daemon::daemonize()?;
    }

    let config = maix_core::Config::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config: {e}, using defaults");
        maix_core::Config::minimal()
    });

    // Save listen addr before config is moved into ServerCore
    let config_listen = format!("{}:{}", config.server.listen_addr, config.server.listen_port);

    #[cfg_attr(not(unix), allow(unused_variables))]
    let transport_mode = match cli.transport.as_str() {
        "auto" => TransportMode::Auto,
        "unix-socket" => TransportMode::UnixSocket,
        "named-pipe" => TransportMode::NamedPipe,
        "tcp" => TransportMode::Tcp,
        _ => TransportMode::Auto,
    };

    let core = Arc::new(ServerCore::from_config(config).await?);

    // Background task: watch settings.json for changes and reload config
    {
        let core = core.clone();
        tokio::spawn(async move {
            let settings_path = maix_core::user_settings_path();
            let mut last_mtime = settings_path
                .metadata()
                .and_then(|m| m.modified())
                .ok();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let current_mtime = settings_path
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok();
                if current_mtime != last_mtime {
                    last_mtime = current_mtime;
                    tracing::info!("settings.json changed, reloading...");
                    core.reload_config().await;
                }
            }
        });
    }

    let core_service = CoreServiceServer::new(MaixCoreService(core.clone()));

    tracing::info!(
        "Maix-Agent gRPC server starting (v{})",
        env!("CARGO_PKG_VERSION")
    );

    #[cfg(unix)]
    {
        if transport_mode == TransportMode::UnixSocket || transport_mode == TransportMode::Auto {
            let sock_path = cli
                .socket_path
                .map(std::path::PathBuf::from)
                .unwrap_or_else(transport::default_socket_path);
            let incoming = transport::unix_transport::unix_listener_stream(&sock_path)?;
            tracing::info!("listening on unix socket: {}", sock_path.display());
            Server::builder()
                .add_service(core_service)
                .serve_with_incoming_shutdown(
                    incoming,
                    shutdown::shutdown_signal(core.cancel_root.clone()),
                )
                .await?;
            return Ok(());
        }
    }

    // Windows Named Pipe transport
    #[cfg(windows)]
    {
        if transport_mode == TransportMode::NamedPipe || transport_mode == TransportMode::Auto {
            let pipe_name = transport::default_pipe_name();
            match transport::named_pipe_transport::NamedPipeListener::bind(&pipe_name) {
                Ok(listener) => {
                    let incoming = transport::named_pipe_transport::NamedPipeListenerStream::new(listener);
                    tracing::info!("listening on named pipe: {}", pipe_name);
                    Server::builder()
                        .add_service(core_service)
                        .serve_with_incoming_shutdown(
                            incoming,
                            shutdown::shutdown_signal(core.cancel_root.clone()),
                        )
                        .await?;
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("Failed to bind named pipe '{}': {}, falling back to TCP", pipe_name, e);
                }
            }
        }
    }

    // TCP fallback: CLI --listen overrides config listen_addr:listen_port
    let addr_str = cli.listen.unwrap_or(config_listen);
    let addr: std::net::SocketAddr = addr_str
        .parse()
        .map_err(|e| format!("invalid listen address '{addr_str}': {e}"))?;
    let listener = transport::tcp_listener(addr).await?;
    tracing::info!("listening on tcp: {addr}");
    Server::builder()
        .add_service(core_service)
        .serve_with_incoming_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(listener),
            shutdown::shutdown_signal(core.cancel_root.clone()),
        )
        .await?;

    Ok(())
}
