//! OwnedAgent holds lifecycle ownership of a spawned agent process and its temp directory.
//! It provides idempotent cleanup: kill/wait child, remove temp_dir.
//! All stream operations are handled separately by the caller.

use std::path::PathBuf;
use std::process::Child;

/// OwnedAgent holds lifecycle ownership of a spawned agent process and its temp directory.
/// It provides idempotent cleanup: kill/wait child, remove temp_dir.
/// All stream operations are handled separately by the caller.
#[derive(Debug)]
pub(crate) struct OwnedAgent {
    child: Option<Child>,
    temp_dir: Option<PathBuf>,
}

impl OwnedAgent {
    /// Create a new OwnedAgent from a spawned child and temp directory.
    pub(crate) fn new(child: Child, temp_dir: PathBuf) -> Self {
        Self {
            child: Some(child),
            temp_dir: Some(temp_dir),
        }
    }

    /// Idempotent cleanup: kill child (if alive), wait it, remove temp_dir.
    /// Safe to call multiple times.
    pub(crate) fn cleanup(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
        if let Some(ref temp_dir) = self.temp_dir {
            let _ = std::fs::remove_dir_all(temp_dir);
        }
        self.temp_dir = None;
    }
}

impl Drop for OwnedAgent {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
impl OwnedAgent {
    fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    fn temp_dir_path(&self) -> Option<&PathBuf> {
        self.temp_dir.as_ref()
    }
}

#[cfg(test)]
mod owned_agent_tests {
    use std::path::PathBuf;

    /// Verify that auto-launched agent process and temp directory are cleaned up.
    #[test]
    fn test_auto_launched_agent_is_cleaned_up() {
        // Build the agent if not already built
        let agent_path = std::env::var("DTJ_AGENT_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./crates/dtj/target/debug/dtj-agent"));

        if !agent_path.exists() {
            eprintln!("skipped: dtj-agent not built at {:?}", agent_path);
            return;
        }

        let config = crate::Config {
            data_dir: None,
            producer_name: "cleanup-test".to_string(),
            producer_version: "0.1.0".to_string(),
            agent_path: Some(agent_path.clone()),
            socket_path: None, // Trigger auto-launch
            session_file_name: None,
            enabled: true,
            warning_handler: None,
        };

        // Open session with auto-launch
        let mut session = crate::Session::open(&config).expect("session open");
        assert!(
            session.owned.is_some(),
            "should have owned agent after auto-launch"
        );

        // Capture PID and temp_dir before taking ownership
        let child_pid = session.owned.as_ref().unwrap().child_id();
        let temp_path = session.owned.as_ref().unwrap().temp_dir_path().cloned();

        let pid = child_pid.expect("should have PID");
        let temp_path = temp_path.expect("should have temp_dir");

        println!("AUTO_LAUNCH_PID={}", pid);
        println!("AUTO_LAUNCH_TEMP_DIR={}", temp_path.display());

        // Verify temp_dir exists before cleanup
        assert!(temp_path.exists(), "temp_dir should exist before cleanup");

        // Call session.close() - this triggers cleanup via self.owned.take()
        session.close().ok();
        drop(session);

        // Verify: process no longer exists
        let pid_exists = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        println!(
            "AUTO_LAUNCH_PID_CLEANUP={}",
            if !pid_exists { "PASS" } else { "FAIL" }
        );
        assert!(
            !pid_exists,
            "agent process {} should be killed after cleanup",
            pid
        );

        // Verify: temp directory deleted
        let temp_gone = !temp_path.exists();
        println!(
            "AUTO_LAUNCH_TEMP_DIR_CLEANUP={}",
            if temp_gone { "PASS" } else { "FAIL" }
        );
        assert!(
            !temp_path.exists(),
            "temp dir {:?} should be deleted after cleanup",
            temp_path
        );
    }
}
