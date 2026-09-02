pub mod client;
pub mod discovery;
pub mod error;
pub mod protocol;
pub mod types;
pub use client::{Client, Session};
pub use discovery::Discovery;
pub use error::Error;
pub use protocol::{decode, encode};
pub use types::Value;

use std::path::PathBuf;
use std::sync::Arc;

/// Callback type for warnings
pub type WarningHandler = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// SDK configuration
pub struct Config {
    /// Optional explicit socket path. If set, no agent process is spawned.
    pub socket_path: Option<PathBuf>,
    /// Optional explicit path to dtj-agent binary.
    pub agent_path: Option<PathBuf>,
    /// Data directory for agent (default: ./traces)
    pub data_dir: Option<PathBuf>,
    /// Enable/disable the SDK. When disabled, emit/close are no-ops.
    pub enabled: bool,
    /// Optional warning handler called once when SDK falls back to disabled mode.
    pub warning_handler: Option<WarningHandler>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            socket_path: None,
            agent_path: None,
            data_dir: None,
            enabled: true,
            warning_handler: None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("socket_path", &self.socket_path)
            .field("agent_path", &self.agent_path)
            .field("data_dir", &self.data_dir)
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// Event structure for the session.
#[derive(Debug, Clone)]
pub struct Event {
    pub domain: String,
    pub category: String,
    pub name: String,
    pub field_name: String,
    pub value: Value,
    pub correlation: Option<String>,
}
