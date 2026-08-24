use std::process::Command;

use dtj::SessionReader;
use tempfile::tempdir;

#[test]
fn stress_cli_writes_and_verifies_requested_event_count() {
    let directory = tempdir().unwrap();
    let output_path = directory.path().join("stress.dtj");
    let binary = env!("CARGO_BIN_EXE_dtj-stress");

    let result = Command::new(binary)
        .args([output_path.to_str().unwrap(), "3"])
        .output()
        .expect("spawn dtj-stress");

    assert!(
        result.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("ok events=3"));

    let session = SessionReader::open(&output_path).unwrap();
    assert_eq!(session.events().len(), 3);
    assert!(!session.had_torn_tail());
}
