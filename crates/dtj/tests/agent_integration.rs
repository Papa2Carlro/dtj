//! Integration test for dtj-agent binary.
//! Spawns the agent, performs Hello → OpenSession → Intern* → AppendEvent → FinishSession,
//! then validates the produced .dtj file with SessionReader.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use dtj::{SessionReader, Severity};

const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME: usize = 1_048_576;

fn write_frame(stream: &mut UnixStream, opcode: u8, body: &[u8]) -> std::io::Result<()> {
    let mut frame = Vec::with_capacity(4 + 1 + body.len());
    frame.extend_from_slice(&(1 + body.len() as u32).to_le_bytes());
    frame.push(opcode);
    frame.extend_from_slice(body);
    stream.write_all(&frame)?;
    stream.flush()
}

fn read_frame(stream: &mut UnixStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn expect_response(stream: &mut UnixStream, expected_opcode: u8) -> Vec<u8> {
    let frame = read_frame(stream).unwrap().expect("no frame");
    if frame[0] != expected_opcode {
        if frame[0] == 0xFF {
            let error_msg = String::from_utf8_lossy(&frame[1..]);
            eprintln!("Agent returned Error: {}", error_msg);
        }
        assert_eq!(frame[0], expected_opcode, "unexpected opcode");
    }
    frame[1..].to_vec()
}

fn intern(stream: &mut UnixStream, kind: u8, name: &str) -> u32 {
    let mut body = Vec::new();
    body.push(kind);
    let name_bytes = name.as_bytes();
    assert!(name_bytes.len() <= 1024);
    body.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    body.extend_from_slice(name_bytes);
    write_frame(stream, 0x06, &body).unwrap();
    let resp = expect_response(stream, 0x86);
    u32::from_le_bytes(resp[..4].try_into().unwrap())
}

fn connect_with_retry(sock_str: &str, deadline: Duration) -> UnixStream {
    let start = std::time::Instant::now();
    loop {
        if let Ok(stream) = UnixStream::connect(sock_str) {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            return stream;
        }
        if start.elapsed() > deadline {
            panic!("timeout connecting to agent");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct ChildGuard {
    child: Option<std::process::Child>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn agent_full_cycle() {
    let sock_dir = tempfile::tempdir().unwrap();
    let sock_path = sock_dir.path().join("agent.sock");
    let sock_str = sock_path.to_str().unwrap();

    let data_dir = tempfile::tempdir().unwrap();
    let data_dir_str = data_dir.path().to_str().unwrap();

    let agent_bin = std::env::var("CARGO_BIN_EXE_dtj-agent")
        .expect("CARGO_BIN_EXE_dtj-agent not set; run via `cargo test`");
    let child = Command::new(agent_bin)
        .arg("--socket")
        .arg(sock_str)
        .arg("--data-dir")
        .arg(data_dir_str)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn dtj-agent");

    let mut _guard = ChildGuard { child: Some(child) };

    // Connect with retry
    let mut stream = connect_with_retry(sock_str, Duration::from_secs(2));

    // Hello
    write_frame(&mut stream, 0x01, &PROTOCOL_VERSION.to_le_bytes()).unwrap();
    let resp = expect_response(&mut stream, 0x81);
    assert_eq!(
        u32::from_le_bytes(resp[..4].try_into().unwrap()),
        PROTOCOL_VERSION
    );

    // Build a minimal FileHeader (128 bytes)
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(b"DTJ1");
    header[4..6].copy_from_slice(&1u16.to_le_bytes());
    header[6..8].copy_from_slice(&128u16.to_le_bytes());
    header[8..12].copy_from_slice(&0x01020304u32.to_le_bytes());
    header[16..32].copy_from_slice(b"test-session-id\0");
    header[32..40].copy_from_slice(&1722470400000i64.to_le_bytes());
    header[40..48].copy_from_slice(&0u64.to_le_bytes());
    header[48..48 + 9].copy_from_slice(b"test-prod");
    header[80..80 + 5].copy_from_slice(b"1.0.0");

    // OpenSession: header + file name (NUL terminated)
    let mut open_body = Vec::new();
    open_body.extend_from_slice(&header);
    open_body.extend_from_slice(b"session.dtj\0");
    write_frame(&mut stream, 0x02, &open_body).unwrap();
    expect_response(&mut stream, 0x82);

    // Intern dictionary entries
    let domain_id = intern(&mut stream, 1, "wire");
    let category_id = intern(&mut stream, 2, "gesture");
    let event_name_id = intern(&mut stream, 3, "KnotHit");
    let correlation_id = intern(&mut stream, 4, "gesture-7f3a");
    let duration_ms_id = intern(&mut stream, 4, "durationMs");
    let pos_id = intern(&mut stream, 4, "pos");
    let ok_id = intern(&mut stream, 4, "ok");

    // AppendEvent 1 (durationMs f64)
    {
        let mut body = Vec::new();
        body.extend_from_slice(&1_250_000_000u64.to_le_bytes());
        body.extend_from_slice(&domain_id.to_le_bytes());
        body.extend_from_slice(&category_id.to_le_bytes());
        body.extend_from_slice(&event_name_id.to_le_bytes());
        body.extend_from_slice(&correlation_id.to_le_bytes());
        body.push(Severity::Info as u8);
        body.extend_from_slice(&1u16.to_le_bytes()); // field_count = 1
        body.extend_from_slice(&duration_ms_id.to_le_bytes());
        body.push(0x07); // F64
        body.extend_from_slice(&[0, 0, 0]); // reserved
        body.extend_from_slice(&12.5f64.to_le_bytes());
        write_frame(&mut stream, 0x03, &body).unwrap();
        let resp = expect_response(&mut stream, 0x83);
        let seq1 = u64::from_le_bytes(resp[..8].try_into().unwrap());
        assert_eq!(seq1, 1);
    }

    // AppendEvent 2 (pos vec2)
    {
        let mut body = Vec::new();
        body.extend_from_slice(&1_250_000_000u64.to_le_bytes());
        body.extend_from_slice(&domain_id.to_le_bytes());
        body.extend_from_slice(&category_id.to_le_bytes());
        body.extend_from_slice(&event_name_id.to_le_bytes());
        body.extend_from_slice(&correlation_id.to_le_bytes());
        body.push(Severity::Info as u8);
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&pos_id.to_le_bytes());
        body.push(0x09); // VEC2_F32
        body.extend_from_slice(&[0, 0, 0]);
        body.extend_from_slice(&1.0f32.to_le_bytes());
        body.extend_from_slice(&2.5f32.to_le_bytes());
        write_frame(&mut stream, 0x03, &body).unwrap();
        let resp = expect_response(&mut stream, 0x83);
        let seq2 = u64::from_le_bytes(resp[..8].try_into().unwrap());
        assert_eq!(seq2, 2);
    }

    // AppendEvent 3 (ok bool)
    {
        let mut body = Vec::new();
        body.extend_from_slice(&1_250_000_000u64.to_le_bytes());
        body.extend_from_slice(&domain_id.to_le_bytes());
        body.extend_from_slice(&category_id.to_le_bytes());
        body.extend_from_slice(&event_name_id.to_le_bytes());
        body.extend_from_slice(&correlation_id.to_le_bytes());
        body.push(Severity::Info as u8);
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&ok_id.to_le_bytes());
        body.push(0x01); // BOOL
        body.extend_from_slice(&[0, 0, 0]);
        body.push(1);
        write_frame(&mut stream, 0x03, &body).unwrap();
        let resp = expect_response(&mut stream, 0x83);
        let seq3 = u64::from_le_bytes(resp[..8].try_into().unwrap());
        assert_eq!(seq3, 3);
    }

    // FinishSession
    write_frame(&mut stream, 0x04, &[]).unwrap();
    expect_response(&mut stream, 0x84);

    drop(stream);
    let mut child = _guard.child.take().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());

    // Validate produced file with SessionReader
    let out_path = std::path::Path::new(data_dir.path()).join("session.dtj");
    let reader = SessionReader::open(&out_path).expect("SessionReader open");
    assert_eq!(reader.events().len(), 3);
    let ev0 = &reader.events()[0];
    assert_eq!(ev0.severity, Severity::Info);
    assert_eq!(ev0.payload.fields.len(), 1);
    println!("Integration test passed");
}

#[test]
fn agent_unsupported_version() {
    let sock_dir = tempfile::tempdir().unwrap();
    let sock_path = sock_dir.path().join("agent.sock");
    let sock_str = sock_path.to_str().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let data_dir_str = data_dir.path().to_str().unwrap();

    let agent_bin = std::env::var("CARGO_BIN_EXE_dtj-agent").unwrap();
    let child = Command::new(agent_bin)
        .arg("--socket")
        .arg(sock_str)
        .arg("--data-dir")
        .arg(data_dir_str)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut _guard = ChildGuard { child: Some(child) };

    let mut stream = connect_with_retry(sock_str, Duration::from_secs(2));
    write_frame(&mut stream, 0x01, &999u32.to_le_bytes()).unwrap();
    let frame = read_frame(&mut stream).unwrap().unwrap();
    assert_eq!(frame[0], 0xFF); // Error
    drop(stream);
    let mut child = _guard.child.take().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn agent_malformed_frame() {
    let sock_dir = tempfile::tempdir().unwrap();
    let sock_path = sock_dir.path().join("agent.sock");
    let sock_str = sock_path.to_str().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let data_dir_str = data_dir.path().to_str().unwrap();

    let agent_bin = std::env::var("CARGO_BIN_EXE_dtj-agent").unwrap();
    let child = Command::new(agent_bin)
        .arg("--socket")
        .arg(sock_str)
        .arg("--data-dir")
        .arg(data_dir_str)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut _guard = ChildGuard { child: Some(child) };

    let mut stream = connect_with_retry(sock_str, Duration::from_secs(2));
    // Send a truncated frame: length 5 (includes opcode) but only 2 bytes sent
    let mut bad = Vec::new();
    bad.extend_from_slice(&5u32.to_le_bytes()); // length 5 (includes opcode)
    bad.push(0x01); // opcode Hello
                    // only 1 byte payload (need 4)
    stream.write_all(&bad).unwrap();
    stream.flush().unwrap();
    drop(stream);
    let mut child = _guard.child.take().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn agent_bad_severity() {
    let sock_dir = tempfile::tempdir().unwrap();
    let sock_path = sock_dir.path().join("agent.sock");
    let sock_str = sock_path.to_str().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let data_dir_str = data_dir.path().to_str().unwrap();

    let agent_bin = std::env::var("CARGO_BIN_EXE_dtj-agent").unwrap();
    let child = Command::new(agent_bin)
        .arg("--socket")
        .arg(sock_str)
        .arg("--data-dir")
        .arg(data_dir_str)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut _guard = ChildGuard { child: Some(child) };

    let mut stream = connect_with_retry(sock_str, Duration::from_secs(2));
    // Hello
    write_frame(&mut stream, 0x01, &PROTOCOL_VERSION.to_le_bytes()).unwrap();
    expect_response(&mut stream, 0x81);
    // OpenSession minimal
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(b"DTJ1");
    header[4..6].copy_from_slice(&1u16.to_le_bytes());
    header[6..8].copy_from_slice(&128u16.to_le_bytes());
    header[8..12].copy_from_slice(&0x01020304u32.to_le_bytes());
    header[16..32].copy_from_slice(b"test-session-id\0");
    header[32..40].copy_from_slice(&1722470400000i64.to_le_bytes());
    header[40..48].copy_from_slice(&0u64.to_le_bytes());
    header[48..48 + 9].copy_from_slice(b"test-prod");
    header[80..80 + 5].copy_from_slice(b"1.0.0");
    let mut open_body = Vec::new();
    open_body.extend_from_slice(&header);
    open_body.extend_from_slice(b"session.dtj\0");
    write_frame(&mut stream, 0x02, &open_body).unwrap();
    expect_response(&mut stream, 0x82);
    // Intern a domain
    let domain_id = intern(&mut stream, 1, "wire");
    // AppendEvent with invalid severity (255)
    let mut body = Vec::new();
    body.extend_from_slice(&1_250_000_000u64.to_le_bytes());
    body.extend_from_slice(&domain_id.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes()); // category_id dummy
    body.extend_from_slice(&1u32.to_le_bytes()); // event_name_id dummy
    body.extend_from_slice(&0u32.to_le_bytes()); // correlation_id
    body.push(255); // invalid severity
    body.extend_from_slice(&1u16.to_le_bytes()); // field_count
    body.extend_from_slice(&1u32.to_le_bytes()); // name_id dummy
    body.push(0x01); // BOOL
    body.extend_from_slice(&[0, 0, 0]);
    body.push(1);
    write_frame(&mut stream, 0x03, &body).unwrap();
    let frame = read_frame(&mut stream).unwrap().unwrap();
    assert_eq!(frame[0], 0xFF); // Error
    drop(stream);
    let mut child = _guard.child.take().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn agent_path_traversal() {
    let sock_dir = tempfile::tempdir().unwrap();
    let sock_path = sock_dir.path().join("agent.sock");
    let sock_str = sock_path.to_str().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let data_dir_str = data_dir.path().to_str().unwrap();

    let agent_bin = std::env::var("CARGO_BIN_EXE_dtj-agent").unwrap();
    let child = Command::new(agent_bin)
        .arg("--socket")
        .arg(sock_str)
        .arg("--data-dir")
        .arg(data_dir_str)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut _guard = ChildGuard { child: Some(child) };

    let mut stream = connect_with_retry(sock_str, Duration::from_secs(2));
    // Hello
    write_frame(&mut stream, 0x01, &PROTOCOL_VERSION.to_le_bytes()).unwrap();
    expect_response(&mut stream, 0x81);
    // OpenSession with traversal attempt
    let mut header = vec![0u8; 128];
    header[0..4].copy_from_slice(b"DTJ1");
    header[4..6].copy_from_slice(&1u16.to_le_bytes());
    header[6..8].copy_from_slice(&128u16.to_le_bytes());
    header[8..12].copy_from_slice(&0x01020304u32.to_le_bytes());
    header[16..32].copy_from_slice(b"test-session-id\0");
    header[32..40].copy_from_slice(&1722470400000i64.to_le_bytes());
    header[40..48].copy_from_slice(&0u64.to_le_bytes());
    header[48..48 + 9].copy_from_slice(b"test-prod");
    header[80..80 + 5].copy_from_slice(b"1.0.0");
    let mut open_body = Vec::new();
    open_body.extend_from_slice(&header);
    open_body.extend_from_slice(b"../evil.dtj\0");
    write_frame(&mut stream, 0x02, &open_body).unwrap();
    let frame = read_frame(&mut stream).unwrap().unwrap();
    assert_eq!(frame[0], 0xFF); // Error
    drop(stream);
    let mut child = _guard.child.take().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    // Ensure no file created outside data_dir
    assert!(!std::path::Path::new("../evil.dtj").exists());
}
