use tokio_util::sync::CancellationToken;

/// Register signal handlers and return a future that resolves on shutdown.
pub async fn shutdown_signal(cancel: CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    let sigterm = {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
        async move {
            sigterm.recv().await;
            tracing::info!("received SIGTERM");
        }
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending();

    tokio::select! {
        res = ctrl_c => {
            if let Err(e) = res {
                tracing::error!("Ctrl+C handler error: {e}");
            }
            tracing::info!("received Ctrl+C");
        }
        _ = sigterm => {}
    }

    tracing::info!("shutting down gracefully...");
    cancel.cancel();
}
