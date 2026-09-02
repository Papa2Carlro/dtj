use crate::protocol::{encode, decode};
use crate::error::Error;
#[test] fn frame_too_large() { assert_eq!(decode(&vec![0;1024*1024+1]), Err(Error::FrameTooLarge)); }
#[test] fn bad_length() { assert!(decode(&[0,0,0,0]).is_ok()); }

use std::os::unix::net::{UnixListener, UnixStream};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::net::Shutdown;
use crate::{Config, Session};
use std::path::PathBuf;

/// Vertical slice test: open_strict → emit(bool) → close over a mock Unix socket.
#[test]
fn mock_socket_vertical_slice() {
    // Create a temp directory with socket path
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent.sock");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    // Create UnixListener
    let listener = UnixListener::bind(&socket_path).unwrap();
    listener.set_nonblocking(true).ok();

    let opcode_log: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let opcode_log_clone = opcode_log.clone();
    let intern_count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let intern_count_clone = intern_count.clone();

    let server_done = Arc::new(Mutex::new(false));
    let server_done_clone = server_done.clone();

    // Spawn mock dtj-agent server
    let server = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut local_intern_count = 0;

            loop {
                // Read 4-byte length
                let mut len_buf = [0u8; 4];
                match stream.read_exact(&mut len_buf) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_micros(100));
                        continue;
                    }
                    Err(_) => break,
                }

                let len = u32::from_le_bytes(len_buf) as usize;
                if len == 0 || len > 1024 * 1024 {
                    break;
                }

                let mut frame = vec![0u8; len];
                if stream.read_exact(&mut frame).is_err() {
                    break;
                }

                let opcode = frame[0];
                let payload = &frame[1..];
                opcode_log_clone.lock().unwrap().push(opcode);

                match opcode {
                    0x01 => {
                        // Hello: verify protocol version
                        assert_eq!(payload.len(), 4, "Hello payload should be 4 bytes");
                        let version = u32::from_le_bytes(payload[..4].try_into().unwrap());
                        assert_eq!(version, 1u32, "Protocol version should be 1");
                        // Send HelloOk
                        let response = [4u8, 0x81, 1, 0, 0, 0];
                        stream.write_all(&response).unwrap();
                    }
                    0x02 => {
                        // OpenSession: verify correct opcode order
                        let log = opcode_log_clone.lock().unwrap();
                        assert_eq!(log[0], 0x01, "First opcode should be Hello");
                        assert_eq!(log.len(), 2, "Should have 2 opcodes (Hello + OpenSession)");
                        drop(log);
                        // Send OpenSessionOk (empty payload)
                        let response = [1u8, 0x82];
                        stream.write_all(&response).unwrap();
                    }
                    0x06 => {
                        // Intern: count and return InternOk
                        local_intern_count += 1;
                        intern_count_clone.lock().unwrap().unwrap();
                        *intern_count_clone.lock().unwrap() = local_intern_count;
                        let id = local_intern_count as u32;
                        let mut response = vec![5u8, 0x86];
                        response.extend_from_slice(&id.to_le_bytes());
                        stream.write_all(&response).unwrap();
                    }
                    0x03 => {
                        // AppendEvent: verify 4 Intern calls, verify bool type tag
                        let cnt = *intern_count_clone.lock().unwrap();
                        assert_eq!(cnt, 4, "Expected 4 Intern calls before AppendEvent");
                        // type_tag is at offset 32 in AppendEvent payload
                        // (8 monotonic_ns + 4 domain_id + 4 category_id + 4 event_name_id +
                        //  4 correlation_id + 1 severity + 2 field_count + 4 name_id + 1 type_tag +
                        //  3 reserved = 35, so type_tag is at index 32)
                        assert!(payload.len() > 32, "AppendEvent payload too short");
                        let type_tag = payload[32];
                        assert_eq!(type_tag, 0x01, "Expected BOOL type tag 0x01");
                        // Send AppendEventOk with event_sequence = 1
                        let response = [9u8, 0x83, 1, 0, 0, 0, 0, 0, 0, 0];
                        stream.write_all(&response).unwrap();
                    }
                    0x04 => {
                        // FinishSession: send FinishSessionOk
                        let response = [1u8, 0x84];
                        stream.write_all(&response).unwrap();
                        *server_done_clone.lock().unwrap() = true;
                        break;
                    }
                    _ => {
                        break;
                    }
                }
            }
            stream.shutdown(Shutdown::Both).ok();
        }
    });

    // Give server time to start
    thread::sleep(std::time::Duration::from_millis(50));

    // Create config with socket path
    let config = Config {
        socket_path: Some(PathBuf::from(&socket_path_str)),
        agent_path: None,
    };

    // Open session
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");
    assert!(!session.closed, "Session should not be closed after open");

    // Emit a bool event
    let emit_result = session.emit("test.domain", "test.category", "test_event", "enabled", true);
    assert!(emit_result.is_ok(), "emit should succeed");

    // Close session
    let close_result = session.close();
    assert!(close_result.is_ok(), "close should succeed");
    assert!(session.closed, "Session should be closed after close");

    // Second close should be idempotent
    let second_close = session.close();
    assert!(second_close.is_ok(), "second close should be idempotent and return Ok");

    // Verify opcode order
    let log = opcode_log.lock().unwrap();
    assert_eq!(log.len(), 7, "Expected 7 opcodes: Hello, OpenSession, 4xIntern, AppendEvent, FinishSession");
    assert_eq!(log[0], 0x01, "First opcode should be Hello");
    assert_eq!(log[1], 0x02, "Second opcode should be OpenSession");
    for i in 0..4 {
        assert_eq!(log[2 + i], 0x06, "Opcodes 2-5 should be Intern");
    }
    assert_eq!(log[6], 0x04, "Last opcode should be FinishSession");

    server.join().ok();
    assert!(*server_done.lock().unwrap(), "Server should have completed");
}

/// Error frame test: mock server returns Error after Hello, open_strict returns Error::Protocol
#[test]
fn mock_error_after_hello() {
    // Create a temp directory with socket path
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent-error.sock");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    // Create UnixListener
    let listener = UnixListener::bind(&socket_path).unwrap();
    listener.set_nonblocking(true).ok();

    let server_done = Arc::new(Mutex::new(false));
    let server_done_clone = server_done.clone();

    let server = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            loop {
                // Read 4-byte length
                let mut len_buf = [0u8; 4];
                match stream.read_exact(&mut len_buf) {
                    Ok(_) => {}
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_micros(100));
                        continue;
                    }
                    Err(_) => break,
                }

                let len = u32::from_le_bytes(len_buf) as usize;
                if len == 0 || len > 1024 * 1024 {
                    break;
                }

                let mut frame = vec![0u8; len];
                if stream.read_exact(&mut frame).is_err() {
                    break;
                }

                let opcode = frame[0];

                if opcode == 0x01 {
                    // Hello: send Error frame instead of HelloOk
                    let error_msg = b"Unsupported version";
                    let payload_len = 1 + error_msg.len();
                    let mut error_frame = Vec::with_capacity(4 + payload_len);
                    error_frame.extend_from_slice(&(payload_len as u32).to_le_bytes());
                    error_frame.push(0xFF); // Error opcode
                    error_frame.extend_from_slice(error_msg);
                    stream.write_all(&error_frame).unwrap();
                    *server_done_clone.lock().unwrap() = true;
                    break;
                }
            }
            stream.shutdown(Shutdown::Both).ok();
        }
    });

    // Give server time to start
    thread::sleep(std::time::Duration::from_millis(50));

    // Try to open session - should get Error::Protocol
    let config = Config {
        socket_path: Some(PathBuf::from(&socket_path_str)),
        agent_path: None,
    };

    let result = Session::open_strict(&config);

    // Expect Protocol error since server sent Error frame
    assert!(result.is_err(), "open_strict should fail when server sends Error");
    assert_eq!(result.unwrap_err(), Error::Protocol, "Should return Error::Protocol");

    server.join().ok();
    assert!(*server_done.lock().unwrap(), "Server should have completed");
}
