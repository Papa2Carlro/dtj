//! Integration tests for dtj-rust SDK using mock Unix socket server.
//!
//! Tests cover:
//! - Full protocol handshake (Hello, OpenSession, Intern, AppendEvent, FinishSession)
//! - Payload validation (version, metadata, type tags, value bodies)
//! - All dictionary kinds (domain, category, event_name, string)
//! - All value types (Bool, Int, UInt, F32, F64, String, Bytes)
//! - Dictionary caching (duplicate strings don't trigger new Intern)
//! - Error responses at each protocol step
//! - Session state management (closed, idempotent close)

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use dtj_sdk::{Config, Event, Session, Severity, Value};

/// Helper: send a properly framed response
fn send_frame(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    let len = 1u32 + payload.len() as u32;
    let mut frame = Vec::with_capacity(4 + 1 + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.push(opcode);
    frame.extend_from_slice(payload);
    stream.write_all(&frame)?;
    stream.flush()
}

// =============================================================================
// Test helpers
// =============================================================================

/// Helper: read a framed request
fn read_frame(stream: &mut UnixStream) -> Result<(u8, Vec<u8>), std::io::Error> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 65536 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid frame length",
        ));
    }
    let mut frame = vec![0u8; len];
    stream.read_exact(&mut frame)?;
    let opcode = frame[0];
    let payload = frame[1..].to_vec();
    Ok((opcode, payload))
}

/// Create a mock server that handles the full DTJ protocol sequence.
/// Returns (socket_path, temp_dir, server_handle).
/// temp_dir MUST be kept alive for the duration of the test.
fn setup_mock_server<F>(
    handler: F,
) -> (
    std::path::PathBuf,
    tempfile::TempDir,
    std::thread::JoinHandle<()>,
)
where
    F: FnOnce(&mut UnixStream) + Send + 'static,
{
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server accept");
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
        handler(&mut stream);
        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    (socket_path, temp_dir, handle)
}

/// Build a Config for testing
fn make_config(socket_path: &std::path::Path) -> Config {
    Config {
        data_dir: None,
        producer_name: "dtj-test".to_string(),
        producer_version: "0.1.0".to_string(),
        agent_path: None,
        socket_path: Some(std::path::PathBuf::from(socket_path)),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    }
}

// =============================================================================
// Test: Full handshake with payload validation
// =============================================================================

#[test]
fn test_full_handshake_validates_payload() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        // Hello: validate version
        let (opcode, payload) = read_frame(stream).unwrap();
        assert_eq!(opcode, 0x01);
        assert_eq!(payload.len(), 4);
        let version = u32::from_le_bytes(payload[..4].try_into().unwrap());
        assert_eq!(version, 1);
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        // OpenSession: validate metadata
        let (opcode, payload) = read_frame(stream).unwrap();
        assert_eq!(opcode, 0x02);

        let mut offset = 0;
        let file_name_len =
            u16::from_le_bytes(payload[offset..offset + 2].try_into().unwrap()) as usize;
        offset += 2;
        offset += file_name_len;
        offset += 16;
        offset += 8;
        offset += 8;

        let producer_name_len =
            u16::from_le_bytes(payload[offset..offset + 2].try_into().unwrap()) as usize;
        offset += 2;
        let producer_name =
            String::from_utf8(payload[offset..offset + producer_name_len].to_vec()).unwrap();
        offset += producer_name_len;
        assert_eq!(producer_name, "dtj-test");

        let producer_version_len =
            u16::from_le_bytes(payload[offset..offset + 2].try_into().unwrap()) as usize;
        let producer_version =
            String::from_utf8(payload[offset + 2..offset + 2 + producer_version_len].to_vec())
                .unwrap();
        assert_eq!(producer_version, "0.1.0");

        send_frame(stream, 0x82, &[]).ok();
    });

    let config = make_config(&socket_path);
    let result = Session::open_strict(&config);
    server_handle.join().unwrap();

    assert!(
        result.is_ok(),
        "open_strict should succeed, got {:?}",
        result
    );
}

// =============================================================================
// Test: Error after Hello
// =============================================================================

#[test]
fn test_error_after_hello() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0xFF, b"version mismatch").ok();
    });

    let config = make_config(&socket_path);
    let result = Session::open_strict(&config);
    server_handle.join().unwrap();

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), dtj_sdk::Error::Protocol));
}

// =============================================================================
// Test: Error after OpenSession
// =============================================================================

#[test]
fn test_error_after_open_session() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();
        read_frame(stream).ok();
        send_frame(stream, 0xFF, b"session rejected").ok();
    });

    let config = make_config(&socket_path);
    let result = Session::open_strict(&config);
    server_handle.join().unwrap();

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), dtj_sdk::Error::Protocol));
}

// =============================================================================
// Test: All dictionary kinds
// =============================================================================

#[test]
fn test_all_dictionary_kinds() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        let mut id_counter = 0u32;

        // Domain (kind=1)
        let (opcode, payload) = read_frame(stream).unwrap();
        assert_eq!(opcode, 0x06);
        assert_eq!(payload[0], 1);
        id_counter += 1;
        send_frame(stream, 0x86, &id_counter.to_le_bytes()).ok();

        // Category (kind=2)
        let (opcode, payload) = read_frame(stream).unwrap();
        assert_eq!(opcode, 0x06);
        assert_eq!(payload[0], 2);
        id_counter += 1;
        send_frame(stream, 0x86, &id_counter.to_le_bytes()).ok();

        // EventName (kind=3)
        let (opcode, payload) = read_frame(stream).unwrap();
        assert_eq!(opcode, 0x06);
        assert_eq!(payload[0], 3);
        id_counter += 1;
        send_frame(stream, 0x86, &id_counter.to_le_bytes()).ok();

        // String field_name (kind=4)
        let (opcode, payload) = read_frame(stream).unwrap();
        assert_eq!(opcode, 0x06);
        assert_eq!(payload[0], 4);
        id_counter += 1;
        send_frame(stream, 0x86, &id_counter.to_le_bytes()).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x84, &[]).ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "my.domain".to_string(),
        category: "my.category".to_string(),
        name: "my_event".to_string(),
        severity: Severity::Info,
        field_name: "my_field".to_string(),
        value: Value::Bool(true),
        correlation: None,
    };
    session.emit(&event).expect("emit should succeed");
    session.close().expect("close should succeed");
    server_handle.join().unwrap();
}

// =============================================================================
// Test: All severity levels
// =============================================================================

#[test]
fn test_all_severity_levels() {
    let severities = [
        (Severity::Debug, 0u8),
        (Severity::Info, 1u8),
        (Severity::Warn, 2u8),
        (Severity::Error, 3u8),
        (Severity::Fatal, 4u8),
    ];

    for (severity, expected_tag) in severities {
        let (socket_path, _temp_dir, server_handle) = setup_mock_server(move |stream| {
            read_frame(stream).ok();
            send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

            read_frame(stream).ok();
            send_frame(stream, 0x82, &[]).ok();

            // 4 Intern requests
            for _ in 0..4 {
                read_frame(stream).ok();
                send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
            }

            // AppendEvent: severity at byte 24
            let (_, payload) = read_frame(stream).unwrap();
            assert_eq!(
                payload[24], expected_tag,
                "Severity mismatch for {:?}",
                severity
            );

            send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

            read_frame(stream).ok();
            send_frame(stream, 0x84, &[]).ok();
        });

        let config = make_config(&socket_path);
        let mut session = Session::open_strict(&config).expect("open_strict should succeed");

        let event = Event {
            domain: "t".to_string(),
            category: "t".to_string(),
            name: "t".to_string(),
            severity,
            field_name: "f".to_string(),
            value: Value::Bool(true),
            correlation: None,
        };
        session.emit(&event).expect("emit should succeed");
        session.close().expect("close should succeed");

        server_handle.join().unwrap();
    }
}

// =============================================================================
// Test: Correlation ID
// =============================================================================

#[test]
fn test_correlation_id() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        // 5 Intern: domain, category, event_name, field_name, correlation
        for _ in 0..5 {
            read_frame(stream).ok();
            send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
        }

        // AppendEvent: correlation_id at bytes 20-23
        // Note: mock returns 1 for all Intern calls, so correlation_id = 1 (not 42)
        let (_, payload) = read_frame(stream).unwrap();
        let correlation_id = u32::from_le_bytes(payload[20..24].try_into().unwrap());
        assert_eq!(correlation_id, 1, "Correlation ID should be 1");

        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x84, &[]).ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::Bool(true),
        correlation: Some("corr-42".to_string()),
    };
    session.emit(&event).expect("emit should succeed");
    session.close().expect("close should succeed");
    server_handle.join().unwrap();
}

// =============================================================================
// Test: All value types with type tags and body validation
// =============================================================================

#[test]
fn test_all_value_types() {
    let test_cases = vec![
        (Value::Bool(true), 0x01u8, vec![1u8]),
        (Value::Int(-42i64), 0x03u8, (-42i64).to_le_bytes().to_vec()),
        (Value::UInt(42u64), 0x05u8, 42u64.to_le_bytes().to_vec()),
        (Value::F32(3.14f32), 0x06u8, 3.14f32.to_le_bytes().to_vec()),
        (
            Value::F64(2.718281828f64),
            0x07u8,
            2.718281828f64.to_le_bytes().to_vec(),
        ),
    ];

    for (value, expected_type_tag, expected_body) in test_cases {
        let value_for_closure = value.clone();
        let value_for_event = value.clone();
        let (socket_path, _temp_dir, server_handle) = setup_mock_server(move |stream| {
            read_frame(stream).ok();
            send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

            read_frame(stream).ok();
            send_frame(stream, 0x82, &[]).ok();

            // 4 Intern requests
            for _ in 0..4 {
                read_frame(stream).ok();
                send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
            }

            // AppendEvent: type_tag at offset 31, value_body at offset 35
            let (_, payload) = read_frame(stream).unwrap();
            assert_eq!(
                payload[31], expected_type_tag,
                "Type tag mismatch for {:?}",
                value_for_closure
            );
            assert_eq!(
                &payload[35..35 + expected_body.len()],
                expected_body.as_slice(),
                "Body mismatch for {:?}",
                value_for_closure
            );

            send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

            read_frame(stream).ok();
            send_frame(stream, 0x84, &[]).ok();
        });

        let config = make_config(&socket_path);
        let mut session = Session::open_strict(&config).expect("open_strict should succeed");

        let event = Event {
            domain: "t".to_string(),
            category: "t".to_string(),
            name: "t".to_string(),
            severity: Severity::Info,
            field_name: "f".to_string(),
            value: value_for_event,
            correlation: None,
        };
        session.emit(&event).expect("emit should succeed");
        session.close().expect("close should succeed");

        server_handle.join().unwrap();
    }
}

// =============================================================================
// Test: String interning
// =============================================================================

#[test]
fn test_string_value_interning() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        // 5 Intern: domain, category, event_name, field_name, string_value
        for i in 0..5 {
            read_frame(stream).ok();
            send_frame(stream, 0x86, &((i + 1) as u32).to_le_bytes()).ok();
        }

        // AppendEvent: type tag should be 0x0B (TYPE_INTERNED), value at offset 35 should be ID 5
        let (_, payload) = read_frame(stream).unwrap();
        assert_eq!(payload[31], 0x0B, "String should use TYPE_INTERNED");
        let string_id = u32::from_le_bytes(payload[35..39].try_into().unwrap());
        assert_eq!(string_id, 5, "String value should be interned with ID 5");

        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x84, &[]).ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::String("hello interned world".to_string()),
        correlation: None,
    };
    session.emit(&event).expect("emit should succeed");
    session.close().expect("close should succeed");
    server_handle.join().unwrap();
}

// =============================================================================
// Test: Bytes value
// =============================================================================

#[test]
fn test_bytes_value() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        for _ in 0..4 {
            read_frame(stream).ok();
            send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
        }

        let (_, payload) = read_frame(stream).unwrap();
        assert_eq!(payload[31], 0x0C, "Bytes should use TYPE_BYTES");

        let len = u32::from_le_bytes(payload[35..39].try_into().unwrap());
        assert_eq!(len, 4);
        assert_eq!(&payload[39..43], &[0xDE, 0xAD, 0xBE, 0xEF]);

        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x84, &[]).ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
        correlation: None,
    };
    session.emit(&event).expect("emit should succeed");
    session.close().expect("close should succeed");
    server_handle.join().unwrap();
}

// =============================================================================
// Test: Dictionary caching - duplicate strings don't trigger new Intern
// =============================================================================

#[test]
fn test_dictionary_caching() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        let mut intern_count = 0;

        loop {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            if stream.read_exact(&mut frame).is_err() {
                break;
            }

            match frame[0] {
                0x06 => {
                    intern_count += 1;
                    send_frame(stream, 0x86, &(intern_count as u32).to_le_bytes()).ok();
                }
                0x03 => {
                    send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();
                }
                0x04 => {
                    send_frame(stream, 0x84, &[]).ok();
                }
                0xFF => break,
                _ => {}
            }
        }

        // First event: 4 Interns (domain, category, event, field_name)
        // Second event: 0 Interns (all cached)
        assert_eq!(intern_count, 4, "Expected 4 Intern requests only");
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event1 = Event {
        domain: "domain".to_string(),
        category: "category".to_string(),
        name: "event".to_string(),
        severity: Severity::Info,
        field_name: "field".to_string(),
        value: Value::Bool(true),
        correlation: None,
    };
    session.emit(&event1).ok();

    let event2 = Event {
        domain: "domain".to_string(),
        category: "category".to_string(),
        name: "event".to_string(),
        severity: Severity::Warn,
        field_name: "field".to_string(),
        value: Value::Bool(false),
        correlation: None,
    };
    session.emit(&event2).ok();

    session.close().ok();
    server_handle.join().unwrap();
}

// =============================================================================
// Test: Error after Intern
// =============================================================================

#[test]
fn test_error_after_intern() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0xFF, b"dictionary full").ok();
    });

    let config = make_config(&socket_path);
    let result = Session::open_strict(&config);

    server_handle.join().unwrap();

    if result.is_ok() {
        let mut session = result.unwrap();
        let emit_result = session.emit(&Event {
            domain: "t".to_string(),
            category: "t".to_string(),
            name: "t".to_string(),
            severity: Severity::Info,
            field_name: "f".to_string(),
            value: Value::Bool(true),
            correlation: None,
        });
        assert!(emit_result.is_err());
    }
}

// =============================================================================
// Test: Error after AppendEvent
// =============================================================================

#[test]
fn test_error_after_append_event() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        for _ in 0..4 {
            read_frame(stream).ok();
            send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
        }

        read_frame(stream).ok();
        send_frame(stream, 0xFF, b"event rejected").ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::Bool(true),
        correlation: None,
    };
    let emit_result = session.emit(&event);

    server_handle.join().unwrap();

    assert!(emit_result.is_err());
}

// =============================================================================
// Test: Error after FinishSession
// =============================================================================

#[test]
fn test_error_after_finish_session() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        for _ in 0..4 {
            read_frame(stream).ok();
            send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
        }

        read_frame(stream).ok();
        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0xFF, b"close failed").ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::Bool(true),
        correlation: None,
    };
    session.emit(&event).ok();
    let close_result = session.close();

    server_handle.join().unwrap();

    assert!(close_result.is_err());
}

// =============================================================================
// Test: SessionClosed after session is closed
// =============================================================================

#[test]
fn test_session_closed_returns_error() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();

        for _ in 0..4 {
            read_frame(stream).ok();
            send_frame(stream, 0x86, &[1u8, 0, 0, 0]).ok();
        }

        read_frame(stream).ok();
        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();

        read_frame(stream).ok();
        send_frame(stream, 0x84, &[]).ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    let event = Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::Bool(true),
        correlation: None,
    };
    session.emit(&event).ok();
    session.close().ok();

    let second_emit = session.emit(&Event {
        domain: "t".to_string(),
        category: "t".to_string(),
        name: "t".to_string(),
        severity: Severity::Info,
        field_name: "f".to_string(),
        value: Value::Bool(true),
        correlation: None,
    });
    assert!(matches!(second_emit, Err(dtj_sdk::Error::SessionClosed)));

    let second_close = session.close();
    assert!(second_close.is_ok());

    server_handle.join().unwrap();
}

// =============================================================================
// Test: Session::is_closed()
// =============================================================================

#[test]
fn test_session_is_closed() {
    let (socket_path, _temp_dir, server_handle) = setup_mock_server(|stream| {
        read_frame(stream).ok();
        send_frame(stream, 0x81, &[1u8, 0, 0, 0]).ok();
        read_frame(stream).ok();
        send_frame(stream, 0x82, &[]).ok();
        read_frame(stream).ok();
        send_frame(stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();
        read_frame(stream).ok();
        send_frame(stream, 0x84, &[]).ok();
    });

    let config = make_config(&socket_path);
    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    assert!(!session.is_closed());

    session
        .emit(&Event {
            domain: "t".to_string(),
            category: "t".to_string(),
            name: "t".to_string(),
            severity: Severity::Info,
            field_name: "f".to_string(),
            value: Value::Bool(true),
            correlation: None,
        })
        .ok();

    assert!(!session.is_closed());

    session.close().ok();

    assert!(session.is_closed());

    server_handle.join().unwrap();
}
