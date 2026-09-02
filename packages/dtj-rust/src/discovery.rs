use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub struct Discovery {
    pub agent_path: Option<PathBuf>,
    pub socket_path: Option<PathBuf>,
}

/// Agent info for launching
pub struct AgentLaunchInfo {
    pub socket_path: PathBuf,
    pub data_dir: PathBuf,
    pub child: Child,
}

impl Discovery {
    /// Find agent path from config or environment
    fn find_agent_path(config: &crate::Config) -> Option<PathBuf> {
        if let Some(p) = &config.agent_path {
            return Some(p.clone());
        }
        if let Ok(p) = std::env::var("DTJ_AGENT_PATH") {
            return Some(p.into());
        }
        // Check common paths
        let common_paths = [
            "/usr/local/bin/dtj-agent",
            "/usr/bin/dtj-agent",
            "/opt/bin/dtj-agent",
            "./target/debug/dtj-agent",
            "./target/release/dtj-agent",
        ];
        for path in &common_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Wait for socket to appear, retrying for up to timeout
    fn wait_for_socket(
        socket_path: &PathBuf,
        timeout: Duration,
    ) -> Result<(), crate::error::Error> {
        let start = Instant::now();
        let interval = Duration::from_millis(100);
        while start.elapsed() < timeout {
            if socket_path.exists() {
                // Try to connect to verify it's listening
                if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
                    return Ok(());
                }
            }
            std::thread::sleep(interval);
        }
        Err(crate::error::Error::AgentNotFound)
    }

    /// Launch the agent process
    fn launch_agent(
        agent_path: &PathBuf,
        socket_path: &PathBuf,
        data_dir: &PathBuf,
    ) -> Result<Child, crate::error::Error> {
        Command::new(agent_path)
            .arg("--socket")
            .arg(socket_path)
            .arg("--data-dir")
            .arg(data_dir)
            .spawn()
            .map_err(|_| crate::error::Error::IoError)
    }

    /// Find existing agent or launch new one
    /// Returns socket_path and optionally a Child handle if we launched it
    pub fn find(config: &crate::Config) -> Result<Self, crate::error::Error> {
        // If socket_path explicitly provided, don't launch agent
        if let Some(ref socket_path) = config.socket_path {
            return Ok(Discovery {
                agent_path: None,
                socket_path: Some(socket_path.clone()),
            });
        }

        // Find agent binary path
        let agent_path = Self::find_agent_path(config).ok_or(crate::error::Error::AgentNotFound)?;

        // Create unique temp dir for this session
        let temp_dir = std::env::temp_dir().join(format!("dtj-agent-{}", uuid_simple()));
        std::fs::create_dir_all(&temp_dir).map_err(|_| crate::error::Error::IoError)?;

        let socket_path = temp_dir.join("agent.sock");
        let data_dir = config
            .data_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("./traces"));

        // Launch agent
        let mut child = Self::launch_agent(&agent_path, &socket_path, &data_dir)?;

        // Wait for socket with timeout
        if let Err(e) = Self::wait_for_socket(&socket_path, Duration::from_secs(5)) {
            // Kill the child on failure
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(e);
        }

        Ok(Discovery {
            agent_path: Some(agent_path),
            socket_path: Some(socket_path),
        })
    }

    /// Get the socket path, launching agent if needed
    /// Returns (socket_path, child_handle, temp_dir) if agent was launched
    pub fn discover_with_launch(
        config: &crate::Config,
    ) -> Result<(PathBuf, Option<Child>, Option<PathBuf>), crate::error::Error> {
        if config.enabled {
            let discovery = Self::find(config)?;
            if let Some(socket_path) = &discovery.socket_path {
                return Ok((socket_path.clone(), None, None));
            }
        }
        // Return a dummy path for disabled mode
        Ok((PathBuf::from("/dev/null"), None, None))
    }
}

/// Generate simple UUID for temp dir
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", now)
}
