//! Server service management commands.

use crate::cli::ServerAction;

const SERVICE_NAME: &str = "MaixAgent";
const SERVICE_DISPLAY: &str = "Maix-Agent AI Assistant";

pub async fn cmd_server(action: ServerAction) {
    match action {
        ServerAction::Install => {
            #[cfg(windows)]
            {
                let maix_exe = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("maix.exe")))
                    .filter(|p| p.exists());

                let exe_path = match maix_exe {
                    Some(p) => p,
                    None => {
                        eprintln!("Error: maix.exe not found next to maix-cli.exe");
                        eprintln!("Ensure maix-server is built and in the same directory.");
                        std::process::exit(1);
                    }
                };

                let output = std::process::Command::new("sc.exe")
                    .args([
                        "create",
                        SERVICE_NAME,
                        "binPath=",
                        &format!("{} --service", exe_path.display()),
                        "start=",
                        "auto",
                        "DisplayName=",
                        SERVICE_DISPLAY,
                    ])
                    .output();

                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' installed successfully.", SERVICE_NAME);
                        println!("  Binary: {}", exe_path.display());
                        println!("  Start:  auto (boot)");
                        println!("  Use 'maix server start' to start now.");
                    }
                    Ok(o) => {
                        eprintln!("sc.exe failed: {}", String::from_utf8_lossy(&o.stderr));
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let unit_content = format!(
                    "[Unit]\n\
                     Description=Maix-Agent AI Assistant\n\
                     After=network.target\n\n\
                     [Service]\n\
                     Type=simple\n\
                     ExecStart=/usr/local/bin/maix --foreground\n\
                     Restart=on-failure\n\
                     RestartSec=5\n\n\
                     [Install]\n\
                     WantedBy=multi-user.target\n"
                );
                let unit_path = "/etc/systemd/system/maix-agent.service";
                match std::fs::write(unit_path, unit_content) {
                    Ok(_) => {
                        println!("Service unit file written to {unit_path}");
                        println!("Run: sudo systemctl daemon-reload && sudo systemctl enable maix-agent");
                    }
                    Err(e) => {
                        eprintln!("Error writing unit file: {e}");
                        eprintln!("Try running with sudo.");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Uninstall => {
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("sc.exe")
                    .args(["stop", SERVICE_NAME])
                    .output();

                let output = std::process::Command::new("sc.exe")
                    .args(["delete", SERVICE_NAME])
                    .output();

                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' uninstalled.", SERVICE_NAME);
                    }
                    Ok(o) => {
                        eprintln!("sc.exe failed: {}", String::from_utf8_lossy(&o.stderr));
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = std::process::Command::new("systemctl")
                    .args(["disable", "maix-agent"])
                    .status();
                match std::fs::remove_file("/etc/systemd/system/maix-agent.service") {
                    Ok(_) => {
                        println!("Service removed. Run: sudo systemctl daemon-reload");
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Start => {
            #[cfg(windows)]
            {
                let output = std::process::Command::new("sc.exe")
                    .args(["start", SERVICE_NAME])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' started.", SERVICE_NAME);
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stderr.contains("1056") || stdout.contains("1056") {
                            println!("Service '{}' is already running.", SERVICE_NAME);
                        } else {
                            eprintln!("sc.exe failed: {stderr}{stdout}");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                match std::process::Command::new("systemctl")
                    .args(["start", "maix-agent"])
                    .status()
                {
                    Ok(s) if s.success() => println!("Service started."),
                    Ok(_) => {
                        eprintln!("Failed to start service.");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Stop => {
            #[cfg(windows)]
            {
                let output = std::process::Command::new("sc.exe")
                    .args(["stop", SERVICE_NAME])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        println!("Service '{}' stopped.", SERVICE_NAME);
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stderr.contains("1062") || stdout.contains("1062") {
                            println!("Service '{}' is not running.", SERVICE_NAME);
                        } else {
                            eprintln!("sc.exe failed: {stderr}{stdout}");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                match std::process::Command::new("systemctl")
                    .args(["stop", "maix-agent"])
                    .status()
                {
                    Ok(s) if s.success() => println!("Service stopped."),
                    Ok(_) => {
                        eprintln!("Failed to stop service.");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
        ServerAction::Status => {
            #[cfg(windows)]
            {
                let output = std::process::Command::new("sc.exe")
                    .args(["query", SERVICE_NAME])
                    .output();
                match output {
                    Ok(o) => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stdout.contains("does not exist") || stdout.contains("1060") {
                            println!("Service '{}' is not installed.", SERVICE_NAME);
                        } else {
                            println!("{}", stdout.trim());
                        }
                    }
                    Err(e) => {
                        eprintln!("Error running sc.exe: {e}");
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                match std::process::Command::new("systemctl")
                    .args(["status", "maix-agent"])
                    .output()
                {
                    Ok(o) => {
                        println!("{}", String::from_utf8_lossy(&o.stdout));
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
