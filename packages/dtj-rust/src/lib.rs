pub mod client;
pub mod discovery;
pub mod error;
pub(crate) mod owned_agent;
pub mod protocol;
pub mod types;
pub use crate::types::Value;
pub use client::Session;
pub use discovery::Discovery;
pub use error::Error;

use std::path::PathBuf;
use std::sync::Arc;

/// Callback type for warnings
pub type WarningHandler = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// SDK configuration
#[derive(Clone)]
pub struct Config {
    /// Data directory for trace files (default: ./traces)
    pub data_dir: Option<PathBuf>,
    /// Producer name (max 32 bytes)
    pub producer_name: String,
    /// Producer version (max 16 bytes)
    pub producer_version: String,
    /// Optional explicit path to dtj-agent binary.
    pub agent_path: Option<PathBuf>,
    /// Optional explicit socket path. If set, no agent process is spawned.
    pub socket_path: Option<PathBuf>,
    /// Session file name (default: session-<unix-ms>.dtj)
    pub session_file_name: Option<String>,
    /// Enable/disable the SDK. When disabled, emit/close are no-ops.
    pub enabled: bool,
    /// Optional warning handler called once when SDK falls back to disabled mode.
    pub warning_handler: Option<WarningHandler>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            data_dir: None,
            producer_name: "dtj-rust".to_string(),
            producer_version: "0.1.0".to_string(),
            agent_path: None,
            socket_path: None,
            session_file_name: None,
            enabled: true,
            warning_handler: None,
        }
    }

    /// Validate config fields before opening a session.
    /// Returns `Error::BadLength` if producer_name > 32 bytes.
    /// Returns `Error::BadName` if session_file_name contains path traversal.
    pub fn validate(&self) -> Result<(), crate::Error> {
        // producer_name max 32 bytes
        if self.producer_name.len() > 32 {
            return Err(crate::Error::BadLength);
        }

        // session_file_name must not contain path traversal
        if let Some(ref name) = self.session_file_name {
            if name.contains("..") || name.starts_with('/') {
                return Err(crate::Error::BadName);
            }
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Event severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
    Fatal = 4,
}

impl Severity {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Event structure for the session.
#[derive(Debug, Clone)]
pub struct Event {
    pub domain: String,
    pub category: String,
    pub name: String,
    pub severity: Severity,
    pub field_name: String,
    pub value: Value,
    pub correlation: Option<String>,
}

#[cfg(test)]
mod protocol_tests;
