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

    /// Returns the child's PID if still held.
    pub(crate) fn child_id(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Returns the temp_dir path if still held.
    pub(crate) fn temp_dir_path(&self) -> Option<&PathBuf> {
        self.temp_dir.as_ref()
    }

    /// Returns true if cleanup has been called (child and temp_dir are None).
    pub(crate) fn is_cleaned_up(&self) -> bool {
        self.child.is_none() && self.temp_dir.is_none()
    }
}

impl Drop for OwnedAgent {
    fn drop(&mut self) {
        self.cleanup();
    }
}
