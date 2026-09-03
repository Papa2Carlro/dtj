//! Discovery module for locating and launching the dtj-agent.
//!
//! Discovery order:
//! 1. `Config.agent_path`
//! 2. `DTJ_AGENT_PATH` environment variable
//! 3. `dtj-agent` in `PATH`
//! 4. macOS fallback paths

use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Discovery result containing paths needed to connect to agent
pub struct DiscoveryResult {
    /// Path to the agent binary (if discovered/validated)
    pub agent_path: Option<PathBuf>,
    /// Path to the Unix socket
    pub socket_path: PathBuf,
    /// Child process if we launched the agent
    pub child: Option<Child>,
    /// If we launched the agent, this is the temp dir to clean up
    pub temp_dir: Option<PathBuf>,
}

impl std::fmt::Debug for DiscoveryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryResult")
            .field("agent_path", &self.agent_path)
            .field("socket_path", &self.socket_path)
            .field("child", &self.child.as_ref().map(|_| "Child"))
            .field("temp_dir", &self.temp_dir)
            .finish()
    }
}

/// Errors that can occur during discovery
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryError {
    /// Agent binary not found in any expected location
    AgentNotFound,
    /// Agent binary exists but is not executable
    NotExecutable,
    /// Socket file did not appear within timeout
    SocketNotFound,
    /// Failed to connect to socket
    ConnectionFailed,
    /// IO error (permission, etc)
    IoError,
}

impl From<std::io::Error> for DiscoveryError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => DiscoveryError::AgentNotFound,
            std::io::ErrorKind::PermissionDenied => DiscoveryError::IoError,
            _ => DiscoveryError::IoError,
        }
    }
}

impl From<DiscoveryError> for crate::Error {
    fn from(e: DiscoveryError) -> Self {
        match e {
            DiscoveryError::AgentNotFound => crate::Error::AgentNotFound,
            DiscoveryError::NotExecutable => crate::Error::NotExecutable,
            DiscoveryError::SocketNotFound => crate::Error::SocketNotFound,
            DiscoveryError::ConnectionFailed => crate::Error::ConnectionFailed,
            DiscoveryError::IoError => crate::Error::IoError,
        }
    }
}

/// Validate that a path exists and is executable
fn validate_executable(path: &PathBuf) -> Result<(), DiscoveryError> {
    if !path.exists() {
        return Err(DiscoveryError::AgentNotFound);
    }
    // Check if executable by trying to run --version or similar
    // On Unix, we can check with access()
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode();
            if mode & 0o111 == 0 {
                return Err(DiscoveryError::NotExecutable);
            }
        }
    }
    Ok(())
}

/// Generate unique ID for temp directory
fn generate_unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    format!("dtj-{}-{:x}", pid, now)
}

/// Find dtj-agent in PATH
fn find_in_path() -> Option<PathBuf> {
    std::env::var("PATH").ok().and_then(|path_var| {
        std::env::split_paths(&path_var)
            .map(|p| p.join("dtj-agent"))
            .find(|p| p.exists())
    })
}

/// Discovery impl
impl DiscoveryResult {
    /// Discover agent and socket paths.
    ///
    /// If `socket_path` is set in config:
    /// - Return that socket path immediately (don't launch agent)
    ///
    /// Otherwise:
    /// - Find agent binary in order: config.agent_path → DTJ_AGENT_PATH → PATH → macOS fallbacks
    /// - Validate agent is executable
    /// - Create temp directory
    /// - Launch agent with --socket and --data-dir
    /// - Wait for socket file to appear and be a Unix socket (no connect probe)
    pub fn discover(config: &crate::Config) -> Result<Self, DiscoveryError> {
        // Case 1: explicit socket path - don't launch agent
        if let Some(ref socket_path) = config.socket_path {
            return Ok(DiscoveryResult {
                agent_path: None,
                socket_path: socket_path.clone(),
                child: None,
                temp_dir: None,
            });
        }

        // Case 2: find and launch agent
        let agent_path = Self::find_agent_path(config)?;
        validate_executable(&agent_path)?;

        // Create unique temp directory for this session
        let temp_dir = std::env::temp_dir().join(generate_unique_id());
        std::fs::create_dir_all(&temp_dir)?;

        let socket_path = temp_dir.join("agent.sock");
        let data_dir = config
            .data_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("./traces"));

        // Create data_dir if needed
        std::fs::create_dir_all(&data_dir)?;

        // Launch agent
        let mut child = Command::new(&agent_path)
            .arg("--socket")
            .arg(socket_path.as_os_str())
            .arg("--data-dir")
            .arg(data_dir.as_os_str())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => DiscoveryError::AgentNotFound,
                std::io::ErrorKind::PermissionDenied => DiscoveryError::NotExecutable,
                _ => DiscoveryError::IoError,
            })?;

        // Wait for socket to appear (up to 5 seconds)
        let start = Instant::now();
        let interval = Duration::from_millis(100);

        while start.elapsed() < Duration::from_secs(5) {
            // Check if socket file exists and is a Unix socket (not just any file)
            if let Ok(metadata) = std::fs::metadata(&socket_path) {
                use std::os::unix::fs::MetadataExt;
                // S_IFSOCK = 0140000
                if metadata.mode() & 0o170000 == 0o140000 {
                    return Ok(DiscoveryResult {
                        agent_path: Some(agent_path),
                        socket_path,
                        child: Some(child),
                        temp_dir: Some(temp_dir),
                    });
                }
            }
            std::thread::sleep(interval);
        }

        // Socket didn't appear - kill the child
        let _ = child.kill();
        let _ = child.wait();

        // Clean up temp dir
        let _ = std::fs::remove_dir_all(&temp_dir);

        Err(DiscoveryError::SocketNotFound)
    }

    /// Find agent path following discovery order:
    /// 1. Config.agent_path
    /// 2. DTJ_AGENT_PATH env
    /// 3. `dtj-agent` in PATH
    /// 4. macOS fallbacks
    fn find_agent_path(config: &crate::Config) -> Result<PathBuf, DiscoveryError> {
        // 1. Config.agent_path - if set, use it (even if doesn't exist yet, we validate later)
        if let Some(ref path) = config.agent_path {
            return Ok(path.clone());
        }

        // 2. DTJ_AGENT_PATH environment variable
        if let Ok(path) = std::env::var("DTJ_AGENT_PATH") {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
        }

        // 3. Look in PATH
        if let Some(path) = find_in_path() {
            return Ok(path);
        }

        // 4. macOS fallbacks
        let macos_fallbacks = [
            "/opt/homebrew/bin/dtj-agent",
            "/usr/local/bin/dtj-agent",
            "~/.cargo/bin/dtj-agent",
        ];

        for path_str in &macos_fallbacks {
            let path = if path_str.starts_with("~/") {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(&path_str[2..]))
                    .unwrap_or_else(|| PathBuf::from(*path_str))
            } else {
                PathBuf::from(*path_str)
            };
            if path.exists() {
                return Ok(path);
            }
        }

        Err(DiscoveryError::AgentNotFound)
    }
}

/// Discovery holds the result of agent discovery.
#[derive(Debug)]
pub struct Discovery {
    pub agent_path: Option<PathBuf>,
    pub socket_path: Option<PathBuf>,
    /// Child process if SDK auto-launched the agent; None for explicit socket_path
    pub child: Option<Child>,
    /// Temp directory if SDK auto-launched the agent; None for explicit socket_path
    pub temp_dir: Option<PathBuf>,
}

impl Discovery {
    /// Find and optionally launch dtj-agent.
    ///
    /// If `Config.socket_path` is set:
    /// - Returns immediately with that socket path and child=None, temp_dir=None
    ///   (SDK doesn't own external socket)
    ///
    /// Otherwise:
    /// - Finds agent binary via discovery order
    /// - Launches agent, creating temp directory for socket
    /// - Returns socket_path and child/temp_dir for lifecycle cleanup
    pub fn find(config: &crate::Config) -> Result<Self, crate::Error> {
        if config.socket_path.is_some() {
            return Ok(Discovery {
                agent_path: None,
                socket_path: config.socket_path.clone(),
                child: None,
                temp_dir: None,
            });
        }

        let result = DiscoveryResult::discover(config)?;
        Ok(Discovery {
            agent_path: result.agent_path,
            socket_path: Some(result.socket_path),
            child: result.child,
            temp_dir: result.temp_dir,
        })
    }
}
