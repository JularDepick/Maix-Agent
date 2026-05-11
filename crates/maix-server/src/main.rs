mod chat_stream;
mod client_launcher;
mod daemon;
mod server;
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
    #[arg(long)]
    foreground: bool,

    #[arg(long)]
    socket_path: Option<String>,

    #[arg(long, default_value = "127.0.0.1:26506")]
    tcp_addr: String,

    #[arg(long, default_value = "auto")]
    transport: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    maix_core::init_console_utf8();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    if !cli.foreground {
        daemon::daemonize()?;
    }

    let config = maix_core::Config::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config: {e}, using defaults");
        maix_core::Config::minimal()
    });

    #[cfg_attr(not(unix), allow(unused_variables))]
    let transport_mode = match cli.transport.as_str() {
        "auto" => TransportMode::Auto,
        "unix-socket" => TransportMode::UnixSocket,
        "named-pipe" => TransportMode::NamedPipe,
        "tcp" => TransportMode::Tcp,
        _ => TransportMode::Auto,
    };

    let core = Arc::new(ServerCore::from_config(config).await?);

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

    // TCP fallback
    let addr: std::net::SocketAddr = cli
        .tcp_addr
        .parse()
        .map_err(|e| format!("invalid tcp address: {e}"))?;
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
