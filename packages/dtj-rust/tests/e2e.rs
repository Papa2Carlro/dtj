//! End-to-end test with real dtj-agent
//!
//! Run with:
//! DTJ_RUN_AGENT_E2E=1 DTJ_AGENT_PATH="$(pwd)/crates/dtj/target/debug/dtj-agent" \
//!     cargo test --manifest-path packages/dtj-rust/Cargo.toml --test e2e -- --nocapture

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

/// RAII guard for dtj-agent process - kills on drop
struct AgentGuard {
    child: Child,
    #[allow(dead_code)]
    socket_path: PathBuf,
}

impl Drop for AgentGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn launch_agent(agent_path: &Path, socket_path: &Path, data_dir: &Path) -> std::io::Result<Child> {
    Command::new(agent_path)
        .arg("--socket")
        .arg(socket_path.as_os_str())
        .arg("--data-dir")
        .arg(data_dir.as_os_str())
        .spawn()
}

fn wait_for_socket(socket_path: &Path, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if socket_path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Run `dtj read-session` to verify session file
fn verify_session_file(data_dir: &Path) -> std::io::Result<()> {
    let dtj_bin = std::env::var("DTJ_BIN").unwrap_or_else(|_| String::from("dtj"));

    let dtj_files: Vec<_> = fs::read_dir(data_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "dtj"))
        .collect();

    if dtj_files.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No .dtj file found",
        ));
    }

    let dtj_path = &dtj_files[0].path();

    let output = Command::new(&dtj_bin)
        .arg("read-session")
        .arg(dtj_path)
        .output()?;

    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "dtj read-session failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    eprintln!(
        "[E2E] dtj read-session output:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(())
}

#[test]
fn e2e() {
    // Only run when DTJ_RUN_AGENT_E2E=1
    if std::env::var("DTJ_RUN_AGENT_E2E").unwrap_or_default() != "1" {
        eprintln!("Skipping e2e test - set DTJ_RUN_AGENT_E2E=1 to run");
        return;
    }

    // Find agent binary - use env var or default path
    let agent_path = std::env::var("DTJ_AGENT_PATH")
        .unwrap_or_else(|_| String::from("./crates/dtj/target/debug/dtj-agent"));
    let agent_path = PathBuf::from(agent_path);
    if !agent_path.exists() {
        panic!("dtj-agent not found at {:?}", agent_path);
    }

    // Create temp dir for this test
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("agent.sock");
    let data_dir = temp_dir.path().join("traces");
    fs::create_dir_all(&data_dir).expect("create data dir");

    // Launch dtj-agent with RAII guard
    let child = launch_agent(&agent_path, &socket_path, &data_dir).expect("launch dtj-agent");

    if !wait_for_socket(&socket_path, 5) {
        panic!("dtj-agent failed to start - socket not created");
    }

    let _agent_guard = AgentGuard {
        child,
        socket_path: socket_path.clone(),
    };

    // Create SDK config pointing to our socket
    let mut config = dtj_sdk::Config::new();
    config.socket_path = Some(socket_path.clone());
    config.enabled = true;

    // Open session and emit events
    let mut session = dtj_sdk::Session::open_strict(&config).expect("open_strict should succeed");

    // Emit bool event
    let event1 = dtj_sdk::Event {
        domain: "test.domain".to_string(),
        category: "test.category".to_string(),
        name: "test_event".to_string(),
        severity: dtj_sdk::Severity::Info,
        field_name: "field_bool".to_string(),
        value: dtj_sdk::Value::Bool(true),
        correlation: None,
    };
    session.emit(&event1).expect("emit bool");

    // Emit int event
    let event2 = dtj_sdk::Event {
        domain: "test.domain".to_string(),
        category: "test.category".to_string(),
        name: "test_event".to_string(),
        severity: dtj_sdk::Severity::Info,
        field_name: "field_int".to_string(),
        value: dtj_sdk::Value::Int(42),
        correlation: None,
    };
    session.emit(&event2).expect("emit int");

    // Emit string event
    let event3 = dtj_sdk::Event {
        domain: "test.domain".to_string(),
        category: "test.category".to_string(),
        name: "test_event".to_string(),
        severity: dtj_sdk::Severity::Info,
        field_name: "field_string".to_string(),
        value: dtj_sdk::Value::String("hello world".to_string()),
        correlation: None,
    };
    session.emit(&event3).expect("emit string");

    // Emit bytes event
    let event4 = dtj_sdk::Event {
        domain: "test.domain".to_string(),
        category: "test.category".to_string(),
        name: "test_event".to_string(),
        severity: dtj_sdk::Severity::Info,
        field_name: "field_bytes".to_string(),
        value: dtj_sdk::Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        correlation: None,
    };
    session.emit(&event4).expect("emit bytes");

    session.close().expect("close");

    // AgentGuard will automatically kill the agent on drop

    // Give filesystem time to sync
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Find the .dtj file
    let dtj_files: Vec<_> = fs::read_dir(&data_dir)
        .expect("read data dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "dtj"))
        .collect();

    // Verify exactly one .dtj file exists
    assert!(
        !dtj_files.is_empty(),
        "Expected at least one .dtj file in {:?}",
        data_dir
    );
    assert_eq!(dtj_files.len(), 1, "Expected exactly one .dtj file");

    let dtj_path = dtj_files[0].path().clone();

    // Wait for file to be fully written
    let mut retries = 5;
    let metadata = loop {
        if let Ok(m) = fs::metadata(&dtj_path) {
            break m;
        }
        if retries == 0 {
            panic!("Timeout waiting for .dtj file to be written");
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        retries -= 1;
    };

    assert!(
        metadata.len() > 0,
        "Expected .dtj file to have non-zero size"
    );

    // Verify session file with dtj read-session
    verify_session_file(&data_dir).expect("dtj read-session verification should succeed");

    eprintln!(
        "SUCCESS: Created .dtj file at {:?} with size {} bytes",
        dtj_path,
        metadata.len()
    );
}
