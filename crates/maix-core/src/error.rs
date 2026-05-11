/// Unified error type across all crates.
#[derive(Debug, thiserror::Error)]
pub enum MaixError {
    #[error("provider error: {0}")]
    Provider(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("memory error: {0}")]
    Memory(String),

    #[error("config error: {0}")]
    Config(#[from] Box<figment::Error>),

    #[error("task error: {0}")]
    Task(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(String),

    #[error("cancelled")]
    Cancelled,
}
