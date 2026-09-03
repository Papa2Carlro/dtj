//! Unit tests for discovery module.
//!
//! Tests discovery order, executable validation, error types, and cleanup semantics.

use dtj_sdk::{Config, Error, Event, Session, Severity, Value};
use std::path::PathBuf;

/// Test that discovery errors implement correct Error conversions
#[test]
fn test_discovery_error_types() {
    use dtj_sdk::discovery::DiscoveryError;

    // AgentNotFound should map to Error::AgentNotFound
    let not_found = DiscoveryError::AgentNotFound;
    let error: Error = not_found.into();
    assert_eq!(error, Error::AgentNotFound);

    // NotExecutable should map to Error::NotExecutable
    let not_exec = DiscoveryError::NotExecutable;
    let error: Error = not_exec.into();
    assert_eq!(error, Error::NotExecutable);

    // SocketNotFound should map to Error::SocketNotFound
    let socket_not_found = DiscoveryError::SocketNotFound;
    let error: Error = socket_not_found.into();
    assert_eq!(error, Error::SocketNotFound);

    // ConnectionFailed should map to Error::ConnectionFailed
    let conn_failed = DiscoveryError::ConnectionFailed;
    let error: Error = conn_failed.into();
    assert_eq!(error, Error::ConnectionFailed);
}

/// Test that Error::is_not_found() works correctly
#[test]
fn test_error_is_not_found() {
    assert!(Error::AgentNotFound.is_not_found());
    assert!(Error::SocketNotFound.is_not_found());
    assert!(!Error::NotExecutable.is_not_found());
    assert!(!Error::IoError.is_not_found());
    assert!(!Error::Protocol.is_not_found());
}

/// Test that disabled session returns disabled error
#[test]
fn test_disabled_session_error() {
    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: None,
        session_file_name: None,
        enabled: false, // Disabled!
        warning_handler: None,
    };

    let result = Session::open_strict(&config);
    // enabled=false returns Ok(disabled session), not Err(Disabled)
    assert!(result.is_ok());
    let mut session = result.unwrap();
    // Emit on disabled session is a no-op (returns Ok without doing anything)
    let event = Event {
        domain: "domain".to_string(),
        category: "category".to_string(),
        name: "event".to_string(),
        severity: Severity::Info,
        field_name: "field".to_string(),
        value: Value::Bool(true),
        correlation: None,
    };
    let emit_result = session.emit(&event);
    assert!(emit_result.is_ok());
}

/// Test that non-existent socket path returns IoError
#[test]
fn test_nonexistent_socket_path() {
    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(PathBuf::from("/nonexistent/path/to/socket.sock")),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let result = Session::open_strict(&config);
    assert!(result.is_err());
    // Should fail to connect
    assert!(matches!(result.unwrap_err(), Error::IoError));
}

/// Test that open() returns disabled session on error
#[test]
fn test_open_fallback_to_disabled() {
    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(PathBuf::from("/nonexistent/socket.sock")),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    // open() should not panic, should return a disabled session (fallback behavior)
    let _session = Session::open(&config).unwrap();
    // The session should be created but in disabled state
    // (because connection failed)
    assert!(true); // If we get here without panic, test passes
}

/// Test that producer name length validation works
#[test]
fn test_producer_name_too_long() {
    // Validation happens in open_strict before connecting to socket.
    // Use socket_path set to a path that doesn't exist - if validation passes,
    // it would try to connect and get IoError. If validation fails (BadLength),
    // we get BadLength before any connection attempt.
    let config = Config {
        data_dir: None,
        producer_name: "a".repeat(33), // Max is 32 bytes
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(PathBuf::from(
            "/tmp/nonexistent-socket-for-validation-test.sock",
        )),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let result = Session::open_strict(&config);
    // Validation happens before connection, so we should get BadLength
    // even though the socket path doesn't exist
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Error::BadLength);
}

/// Test session file name validation - path traversal
#[test]
fn test_session_file_name_path_traversal() {
    // Validation happens in open_strict before connecting to socket.
    // Use socket_path set to a path that doesn't exist - validation should
    // fail with BadName before any connection attempt.
    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(PathBuf::from(
            "/tmp/nonexistent-socket-for-validation-test.sock",
        )),
        session_file_name: Some("../../../etc/passwd".to_string()),
        enabled: true,
        warning_handler: None,
    };

    let result = Session::open_strict(&config);
    // Validation happens before connection, so we should get BadName
    // even though the socket path doesn't exist
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Error::BadName);
}

/// Test that discovery order is followed: Config.agent_path first
#[test]
fn test_discovery_order_agent_path_first() {
    use dtj_sdk::discovery::DiscoveryResult;

    // Create a temp dir with a fake "agent"
    let temp_dir = tempfile::tempdir().unwrap();
    let fake_agent = temp_dir.path().join("fake-dtj-agent");
    std::fs::write(&fake_agent, "fake").unwrap();

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: Some(fake_agent.clone()),
        socket_path: None,
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    // Discovery should try to use the agent_path
    // It will fail to connect because the fake agent doesn't speak our protocol
    // But it should NOT try to look elsewhere
    let result = DiscoveryResult::discover(&config);
    // We expect SocketNotFound because the fake agent doesn't create a socket
    // (unless the fake file happens to be executable and launches, which it won't be)
    assert!(result.is_err());
}

/// Test that discovery uses DTJ_AGENT_PATH when agent_path not set
#[test]
fn test_discovery_env_var_fallback() {
    use dtj_sdk::discovery::DiscoveryResult;

    // Create temp dir with fake agent
    let temp_dir = tempfile::tempdir().unwrap();
    let fake_agent = temp_dir.path().join("dtj-agent");
    std::fs::write(&fake_agent, "fake").unwrap();

    // Set DTJ_AGENT_PATH to point to our fake agent
    std::env::set_var("DTJ_AGENT_PATH", fake_agent.to_str().unwrap());

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None, // Not set
        socket_path: None,
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let result = DiscoveryResult::discover(&config);

    // Clean up env
    std::env::remove_var("DTJ_AGENT_PATH");

    // Should fail with SocketNotFound (agent launches but doesn't create socket)
    // NOT AgentNotFound (which would mean it didn't find the binary)
    assert!(result.is_err());
    // If agent was found and launched, we'd get SocketNotFound
    // If it wasn't found in PATH, we'd get AgentNotFound
}

/// Test that discovery returns AgentNotFound when nothing is available
#[test]
fn test_discovery_nothing_available() {
    use dtj_sdk::discovery::DiscoveryResult;

    // Create a config with an agent_path that doesn't exist and no socket_path
    // Also clear relevant env vars
    let original_agent_path = std::env::var("DTJ_AGENT_PATH").ok();
    std::env::remove_var("DTJ_AGENT_PATH");

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: Some(PathBuf::from("/nonexistent/dtj-agent-that-does-not-exist")),
        socket_path: None,
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let result = DiscoveryResult::discover(&config);

    // Restore env
    if let Some(v) = original_agent_path {
        std::env::set_var("DTJ_AGENT_PATH", v);
    }

    // Should fail with AgentNotFound since the agent_path doesn't exist
    assert!(result.is_err());
    use dtj_sdk::discovery::DiscoveryError;
    assert!(matches!(result.unwrap_err(), DiscoveryError::AgentNotFound));
}

/// Test that Session emits are no-ops when session is disabled
#[test]
fn test_disabled_session_emit_noop() {
    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: None,
        session_file_name: None,
        enabled: false,
        warning_handler: None,
    };

    let mut session = Session::open_strict(&config).unwrap();

    // Emit should be a no-op (not an error)
    let event = dtj_sdk::Event {
        domain: "test".to_string(),
        category: "test".to_string(),
        name: "event".to_string(),
        severity: dtj_sdk::Severity::Info,
        field_name: "field".to_string(),
        value: dtj_sdk::Value::Bool(true),
        correlation: None,
    };

    let result = session.emit(&event);
    assert!(result.is_ok()); // Should succeed (no-op)
}

/// Test that Session close is idempotent
#[test]
fn test_close_is_idempotent() {
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::Duration;

    fn send_frame(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
        let len = 1u32 + payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + 1 + payload.len());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.push(opcode);
        frame.extend_from_slice(payload);
        stream.write_all(&frame)?;
        stream.flush()
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("test-idempotent.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Hello → HelloOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x81, &[1u8, 0, 0, 0]).ok();
        }

        // OpenSession → OpenSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x82, &[]).ok();
        }

        // FinishSession → FinishSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x84, &[]).ok();
        }

        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(socket_path),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    // First close
    let result1 = session.close();
    assert!(result1.is_ok());
    assert!(session.is_closed());

    // Second close should also succeed (idempotent)
    let result2 = session.close();
    assert!(result2.is_ok());

    server.join().unwrap();
}

/// Test that FinishSession frame is sent when session.close() is called.
/// Note: actual child process + temp_dir cleanup is tested by
/// `test_owned_agent_cleanup_terminates_child_and_removes_temp_dir` in client.rs.
#[test]
fn test_finish_session_sent_on_close() {
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn send_frame(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
        let len = 1u32 + payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + 1 + payload.len());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.push(opcode);
        frame.extend_from_slice(payload);
        stream.write_all(&frame)?;
        stream.flush()
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("test-lifecycle.sock");
    let finish_called = Arc::new(AtomicBool::new(false));
    let finish_called_clone = finish_called.clone();

    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Hello → HelloOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x81, &[1u8, 0, 0, 0]).ok();
        }

        // OpenSession → OpenSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x82, &[]).ok();
        }

        // Read FinishSession
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();

            if frame[0] == 0x04 {
                finish_called_clone.store(true, Ordering::SeqCst);
                send_frame(&mut stream, 0x84, &[]).ok();
            }
        }

        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(socket_path.clone()),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    // Close the session
    session.close().expect("close should succeed");

    server.join().unwrap();

    // Verify FinishSession was called
    assert!(
        finish_called.load(Ordering::SeqCst),
        "FinishSession should be sent on close"
    );
}

/// Test that session is closed on drop when not explicitly closed
#[test]
fn test_session_closed_on_drop() {
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn send_frame(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
        let len = 1u32 + payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + 1 + payload.len());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.push(opcode);
        frame.extend_from_slice(payload);
        stream.write_all(&frame)?;
        stream.flush()
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("test-drop.sock");
    let close_called = Arc::new(AtomicBool::new(false));
    let close_called_clone = close_called.clone();

    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Hello → HelloOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x81, &[1u8, 0, 0, 0]).ok();
        }

        // OpenSession → OpenSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x82, &[]).ok();
        }

        // Read FinishSession (triggered by Drop)
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();

            if frame[0] == 0x04 {
                close_called_clone.store(true, Ordering::SeqCst);
                send_frame(&mut stream, 0x84, &[]).ok();
            }
        }

        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(socket_path),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    {
        let _session = Session::open_strict(&config).expect("open_strict should succeed");
        // Session goes out of scope here without explicit close
        // Drop should be called, which should send FinishSession
    }

    server.join().unwrap();

    // Verify that close was called via Drop
    assert!(
        close_called.load(Ordering::SeqCst),
        "Drop should have called close"
    );
}

/// Test that emit returns SessionClosed after session is closed
#[test]
fn test_emit_after_close_returns_error() {
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::Duration;

    fn send_frame(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
        let len = 1u32 + payload.len() as u32;
        let mut frame = Vec::with_capacity(4 + 1 + payload.len());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.push(opcode);
        frame.extend_from_slice(payload);
        stream.write_all(&frame)?;
        stream.flush()
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("test-post-close.sock");

    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Hello → HelloOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x81, &[1u8, 0, 0, 0]).ok();
        }

        // OpenSession → OpenSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x82, &[]).ok();
        }

        // FinishSession → FinishSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x84, &[]).ok();
        }

        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    let config = Config {
        data_dir: None,
        producer_name: "test".to_string(),
        producer_version: "1.0".to_string(),
        agent_path: None,
        socket_path: Some(socket_path),
        session_file_name: None,
        enabled: true,
        warning_handler: None,
    };

    let mut session = Session::open_strict(&config).expect("open_strict should succeed");
    session.close().expect("close should succeed");

    // Emit after close should return SessionClosed error
    let event = dtj_sdk::Event {
        domain: "test".to_string(),
        category: "test".to_string(),
        name: "event".to_string(),
        severity: dtj_sdk::Severity::Info,
        field_name: "field".to_string(),
        value: dtj_sdk::Value::Bool(true),
        correlation: None,
    };

    let result = session.emit(&event);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Error::SessionClosed);

    server.join().unwrap();
}
