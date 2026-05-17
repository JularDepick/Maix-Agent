//! Windows Service implementation for Maix-Agent daemon.

#[cfg(windows)]
pub mod windows {
    use std::ffi::OsString;
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceStatus, ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    use windows_service::{define_windows_service, service_dispatcher};

    const SERVICE_NAME: &str = "MaixAgent";
    const SERVICE_DISPLAY_NAME: &str = "Maix-Agent AI Assistant";
    const SERVICE_DESCRIPTION: &str = "Maix-Agent gRPC daemon service for AI-assisted development";

    define_windows_service!(ffi_service_main, service_main);

    fn service_main(_arguments: Vec<OsString>) {
        if let Err(e) = run_service() {
            tracing::error!("Windows service error: {e}");
        }
    }

    fn run_service() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use windows_service::service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode,
        };
        use windows_service::service_control_handler::{
            self, ServiceControlHandlerResult,
        };

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

        let status_handle = service_control_handler::register(
            SERVICE_NAME,
            move |control_event| match control_event {
                ServiceControl::Stop | ServiceControl::Shutdown => {
                    let _ = shutdown_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            },
        )?;

        let next_status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::default(),
            process_id: None,
        };
        status_handle.set_service_status(next_status)?;

        // Run the actual server in a tokio runtime
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let config = maix_core::Config::load().unwrap_or_else(|e| {
                tracing::warn!("Failed to load config: {e}, using defaults");
                maix_core::Config::minimal()
            });

            let core = std::sync::Arc::new(
                crate::server::ServerCore::from_config(config).await
                    .map_err(|e| format!("init server: {e}"))?,
            );

            let core_service = maix_core::proto::maix::core::v1::core_service_server::CoreServiceServer::new(
                crate::server::MaixCoreService(core.clone()),
            );

            let addr: std::net::SocketAddr = {
                let cfg = core.config.read().await;
                format!("{}:{}", cfg.server.listen_addr, cfg.server.listen_port)
            }
            .parse()
            .map_err(|e| format!("invalid address: {e}"))?;
            let listener = crate::transport::tcp_listener(addr).await
                .map_err(|e| format!("tcp listener: {e}"))?;
            tracing::info!("Windows service listening on tcp: {addr}");

            tonic::transport::Server::builder()
                .add_service(core_service)
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    async {
                        // Wait for stop signal from Windows SCM
                        let _ = tokio::task::spawn_blocking(move || {
                            let _ = shutdown_rx.recv();
                        })
                        .await;
                        tracing::info!("Windows service stop signal received");
                        core.cancel_root.cancel();
                    },
                )
                .await
                .map_err(|e| format!("server error: {e}"))?;

            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        })?;

        let stopped_status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::default(),
            process_id: None,
        };
        status_handle.set_service_status(stopped_status)?;

        Ok(())
    }

    /// Install Maix-Agent as a Windows Service.
    pub fn install_service(maix_exe: &str) -> Result<(), String> {
        let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
        let manager = ServiceManager::local_computer(None::<&str>, manager_access)
            .map_err(|e| format!("open service manager: {e}"))?;

        let service_info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(SERVICE_DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: std::path::PathBuf::from(maix_exe),
            launch_arguments: vec![OsString::from("--service")],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };

        let service = manager
            .create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
            .map_err(|e| format!("create service: {e}"))?;

        service
            .set_description(SERVICE_DESCRIPTION)
            .map_err(|e| format!("set description: {e}"))?;

        Ok(())
    }

    /// Uninstall Maix-Agent Windows Service.
    pub fn uninstall_service() -> Result<(), String> {
        let manager_access = ServiceManagerAccess::CONNECT;
        let manager = ServiceManager::local_computer(None::<&str>, manager_access)
            .map_err(|e| format!("open service manager: {e}"))?;

        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::all())
            .map_err(|e| format!("open service: {e}"))?;

        service.delete().map_err(|e| format!("delete service: {e}"))?;
        Ok(())
    }

    /// Start the Maix-Agent Windows Service.
    pub fn start_service() -> Result<(), String> {
        use windows_service::service::ServiceAccess;

        let manager_access = ServiceManagerAccess::CONNECT;
        let manager = ServiceManager::local_computer(None::<&str>, manager_access)
            .map_err(|e| format!("open service manager: {e}"))?;

        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::all())
            .map_err(|e| format!("open service: {e}"))?;

        service
            .start::<&str>(&[])
            .map_err(|e| format!("start service: {e}"))?;
        Ok(())
    }

    /// Stop the Maix-Agent Windows Service.
    pub fn stop_service() -> Result<(), String> {
        use windows_service::service::ServiceAccess;

        let manager_access = ServiceManagerAccess::CONNECT;
        let manager = ServiceManager::local_computer(None::<&str>, manager_access)
            .map_err(|e| format!("open service manager: {e}"))?;

        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::all())
            .map_err(|e| format!("open service: {e}"))?;

        service.stop().map_err(|e| format!("stop service: {e}"))?;
        Ok(())
    }

    /// Check if the service is registered and get its status.
    pub fn service_status() -> Result<String, String> {
        use windows_service::service::ServiceAccess;

        let manager_access = ServiceManagerAccess::CONNECT;
        let manager = ServiceManager::local_computer(None::<&str>, manager_access)
            .map_err(|e| format!("open service manager: {e}"))?;

        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
            .map_err(|e| format!("open service: {e}"))?;

        let status = service
            .query_status()
            .map_err(|e| format!("query status: {e}"))?;

        Ok(format!("{:?}", status.current_state))
    }

    /// Start the Windows Service Dispatcher (called from main when --service flag is used).
    pub fn start_dispatcher() -> Result<(), String> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .map_err(|e| format!("start service dispatcher: {e}"))
    }
}

#[cfg(not(windows))]
pub mod windows {
    pub fn install_service(_exe: &str) -> Result<(), String> {
        Err("Windows Service is only available on Windows".into())
    }
    pub fn uninstall_service() -> Result<(), String> {
        Err("Windows Service is only available on Windows".into())
    }
    pub fn start_service() -> Result<(), String> {
        Err("Windows Service is only available on Windows".into())
    }
    pub fn stop_service() -> Result<(), String> {
        Err("Windows Service is only available on Windows".into())
    }
    pub fn service_status() -> Result<String, String> {
        Err("Windows Service is only available on Windows".into())
    }
    pub fn start_dispatcher() -> Result<(), String> {
        Err("Windows Service is only available on Windows".into())
    }
}
