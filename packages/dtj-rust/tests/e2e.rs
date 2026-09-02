//! End-to-end test with real dtj-agent
//!
//! Run with:
//! DTJ_RUN_AGENT_E2E=1 DTJ_AGENT_PATH="$(pwd)/crates/dtj/target/debug/dtj-agent" \
//!     cargo test --manifest-path packages/dtj-rust/Cargo.toml --test e2e -- --nocapture

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

fn launch_agent(agent_path: &PathBuf, socket_path: &PathBuf, data_dir: &PathBuf) -> Child {
    Command::new(agent_path)
        .arg("--socket")
        .arg(socket_path.as_os_str())
        .arg("--data-dir")
        .arg(data_dir.as_os_str())
        .spawn()
        .expect("launch dtj-agent")
}

fn wait_for_socket(socket_path: &PathBuf, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if socket_path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn e2e() {
    if std::env::var("DTJ_RUN_AGENT_E2E").unwrap_or_default() != "1" {
        eprintln!("Skipping e2e test - set DTJ_RUN_AGENT_E2E=1 to run");
        return;
    }

    // Find agent binary
    let agent_path = std::env::var("DTJ_AGENT_PATH")
        .unwrap_or_else(|_| String::from("./crates/dtj/target/debug/dtj-agent"));
    let agent_path = PathBuf::from(agent_path);
    if !agent_path.exists() {
        eprintln!("dtj-agent not found at {:?}", agent_path);
        return;
    }

    // Create temp dir for this test
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let socket_path = temp_dir.path().join("agent.sock");
    let data_dir = temp_dir.path().join("traces");
    fs::create_dir_all(&data_dir).expect("create data dir");

    // Launch dtj-agent
    let mut child = launch_agent(&agent_path, &socket_path, &data_dir);

    // Wait for agent to be ready (socket appears)
    if !wait_for_socket(&socket_path, 5) {
        child.kill().ok();
        panic!("dtj-agent failed to start - socket not created");
    }

    // Create SDK config pointing to our socket
    let mut config = dtj_sdk::Config::new();
    config.socket_path = Some(socket_path.clone());
    config.enabled = true;

    // Open session and emit events
    let mut session = dtj_sdk::Session::open_strict(&config).expect("open_strict should succeed");

    // Emit bool event
    session
        .emit(
            "test.domain",
            "test.category",
            "test_event",
            "field_bool",
            dtj_sdk::Value::Bool(true),
        )
        .expect("emit bool");

    // Emit int event
    session
        .emit(
            "test.domain",
            "test.category",
            "test_event",
            "field_int",
            dtj_sdk::Value::Int(42),
        )
        .expect("emit int");

    // Emit string event
    session
        .emit(
            "test.domain",
            "test.category",
            "test_event",
            "field_string",
            dtj_sdk::Value::String("hello world".to_string()),
        )
        .expect("emit string");

    // Emit bytes event
    session
        .emit(
            "test.domain",
            "test.category",
            "test_event",
            "field_bytes",
            dtj_sdk::Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        )
        .expect("emit bytes");

    session.close().expect("close");

    // Kill agent
    child.kill().ok();
    child.wait().ok();

    // Give filesystem time to sync
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Find the .dtj file
    let dtj_files: Vec<_> = fs::read_dir(&data_dir)
        .expect("read data dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "dtj"))
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

    eprintln!(
        "SUCCESS: Created .dtj file at {:?} with size {} bytes",
        dtj_path,
        metadata.len()
    );

    // Cleanup temp_dir
    drop(temp_dir);
}
