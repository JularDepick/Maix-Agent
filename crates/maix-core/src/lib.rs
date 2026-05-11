//! Shared types, error, and configuration for Maix-Agent.

pub mod architecture;
pub mod auto_launch;
pub mod client;
pub mod config;
pub mod conversions;
pub mod credentials;
pub mod error;
pub mod identity;
pub mod model_router;
pub mod permissions;
pub mod traits;
pub mod types;
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
pub use config::{Config, TransportMode};
pub use conversions::{json_to_prost_struct, prost_struct_to_json};
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
