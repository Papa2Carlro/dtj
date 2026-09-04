//! CLI adapter contract: `dtj read-session` projects SessionReader to JSON.

use std::process::Command;

use tempfile::tempdir;

#[test]
fn cli_read_session_golden_ok() {
    let mut fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture.push("tests/fixtures/minimal_session.dtj");
    let bin = env!("CARGO_BIN_EXE_dtj");
    let output = Command::new(bin)
        .args(["read-session", fixture.to_str().unwrap()])
        .output()
        .expect("spawn dtj");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"ok\":true"));
    assert!(stdout.contains("\"producer_name\":\"dtj-ref\""));
    assert!(stdout.contains("\"torn_tail\":false"));
    assert!(stdout.contains("\"type\":\"f64\""));
}

#[test]
fn cli_long_version_prints_binary_name_and_pkg_version() {
    let bin = env!("CARGO_BIN_EXE_dtj");
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .expect("spawn dtj --version");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, format!("dtj {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(output.stderr, b"");
}

#[test]
fn cli_short_version_matches_long_version() {
    let bin = env!("CARGO_BIN_EXE_dtj");
    let output = Command::new(bin).arg("-V").output().expect("spawn dtj -V");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, format!("dtj {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(output.stderr, b"");
}

#[test]
fn agent_long_version_prints_binary_name_and_pkg_version() {
    let bin = env!("CARGO_BIN_EXE_dtj-agent");
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .expect("spawn dtj-agent --version");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, format!("dtj-agent {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(output.stderr, b"");
}

#[test]
fn agent_short_version_works_without_runtime_args() {
    let bin = env!("CARGO_BIN_EXE_dtj-agent");
    // Pass only -V — no --socket, --data-dir, --config.
    let output = Command::new(bin)
        .arg("-V")
        .output()
        .expect("spawn dtj-agent -V");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, format!("dtj-agent {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(output.stderr, b"");
}

#[test]
fn cli_read_session_missing_file_structured_error() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("nope.dtj");
    let bin = env!("CARGO_BIN_EXE_dtj");
    let output = Command::new(bin)
        .args(["read-session", missing.to_str().unwrap()])
        .output()
        .expect("spawn dtj");
    assert!(output.status.success(), "structured errors still exit 0");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"ok\":false"));
    assert!(stdout.contains("\"kind\":\"Io\""));
}
