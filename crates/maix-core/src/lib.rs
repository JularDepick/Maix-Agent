//! # Maix-Core
//!
//! Core types, error handling, and configuration for the Maix-Agent system.
//!
//! This crate provides the foundational types and utilities used across all
//! Maix-Agent components:
//!
//! - **Error types** ([`error::MaixError`]) — unified error handling
//! - **Configuration** ([`config`]) — TOML-based configuration with validation
//! - **Client** ([`client::MaixClient`]) — gRPC client for server communication
//! - **Types** ([`types`]) — shared data structures (TokenUsage, CostTracker, etc.)
//! - **Credentials** ([`credentials`]) — API key management (env vars + file)
//! - **Health** ([`health`]) — system health checks and diagnostics
//! - **Logging** ([`logging`]) — debug logging configuration
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use maix_core::client::MaixClient;
//! use maix_core::config::Config;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = Config::load()?;
//! let client = MaixClient::connect("http://127.0.0.1:9527").await?;
//! # Ok(())
//! # }
//! ```

pub mod architecture;
pub mod auto_launch;
pub mod client;
pub mod config;
pub mod config_validator;
pub mod conversions;
pub mod credentials;
pub mod error;
pub mod events;
pub mod health;
pub mod i18n;
pub mod identity;
pub mod logging;
pub mod model_router;
pub mod notify;
pub mod perf;
pub mod permissions;
pub mod tokenizer;
pub mod traits;
pub mod types;
pub mod update;
pub mod util;

pub mod proto {
    pub mod maix {
        pub mod common {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/maix.common.v1.rs"));
            }
        }
        pub mod core {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/maix.core.v1.rs"));
            }
        }
    }
}

pub use architecture::{Architecture, TopologyType};
pub use auto_launch::ensure_server_running;
pub use client::{start_chat, ChatHandle, MaixClient};
pub use config::{AgentConfig, Config, MemoryConfig, ToolsConfig, TransportMode, UserSettings, system_config_path, user_settings_path};
pub use conversions::{json_to_prost_struct, prost_struct_to_json, prost_value_to_json};
pub use credentials::{resolve_api_base, resolve_api_key};
pub use error::MaixError;
pub use identity::{Identity, IdentityManager};
pub use model_router::{detect_category, ModelRoute, ModelRouter, TaskCategory};
pub use permissions::{Permission, PermissionSet};
pub use traits::{
    ChatChunkData, ChatOutput, ChatStreamTrait, LLMProviderTrait, MemoryProvider, SkillProvider,
    ToolProvider,
};
pub use types::*;
pub use util::{contains_sensitive, init_console_utf8, mask_key, sanitize_for_log};

/// Convenient result type.
pub type MaixResult<T> = Result<T, MaixError>;
