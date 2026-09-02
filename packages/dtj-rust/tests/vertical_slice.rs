//! Vertical slice tests for dtj-rust SDK
//!
//! Debug version with eprintln! tracing and timeouts on both ends.

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use dtj_sdk::{Config, Session};

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

/// Full handshake test: Hello→HelloOk, OpenSession→OpenSessionOk
#[test]
fn mock_full_handshake() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent.sock");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    eprintln!("[TEST] main: creating listener");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let _socket_path_str_clone = socket_path_str.clone();
    eprintln!("[TEST] main: spawning server thread");
    let server_handle = std::thread::spawn(move || {
        eprintln!("[SERVER] thread start");
        let (mut stream, _) = listener.accept().expect("server accept failed");
        eprintln!("[SERVER] accepted connection");

        // Set timeouts so we don't hang forever
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Read Hello frame
        eprintln!("[SERVER] reading Hello");
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => eprintln!("[SERVER] read length: {:?}", &len_buf),
            Err(e) => {
                eprintln!("[SERVER] read length failed: {}", e);
                return;
            }
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > 1024 {
            eprintln!("[SERVER] invalid length, exiting");
            return;
        }

        let mut frame = vec![0u8; len];
        match stream.read_exact(&mut frame) {
            Ok(()) => eprintln!("[SERVER] read frame: {:?}", &frame),
            Err(e) => {
                eprintln!("[SERVER] read frame failed: {}", e);
                return;
            }
        }

        let opcode = frame[0];
        eprintln!("[SERVER] opcode = 0x{:02x}", opcode);

        if opcode == 0x01 {
            eprintln!("[SERVER] sending HelloOk");
            // HelloOk: [len=5][opcode=0x81][version=1]
            if let Err(e) = send_frame(&mut stream, 0x81, &[1u8, 0, 0, 0]) {
                eprintln!("[SERVER] send HelloOk failed: {}", e);
                return;
            }
            eprintln!("[SERVER] HelloOk sent");
        }

        // Read OpenSession frame
        eprintln!("[SERVER] reading OpenSession");
        match stream.read_exact(&mut len_buf) {
            Ok(()) => eprintln!("[SERVER] read length: {:?}", &len_buf),
            Err(e) => {
                eprintln!("[SERVER] read OpenSession length failed: {}", e);
                return;
            }
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len == 0 || len > 4096 {
            eprintln!("[SERVER] invalid OpenSession length");
            return;
        }

        let mut frame = vec![0u8; len];
        match stream.read_exact(&mut frame) {
            Ok(()) => eprintln!("[SERVER] OpenSession frame: {:?}", &frame),
            Err(e) => {
                eprintln!("[SERVER] read OpenSession failed: {}", e);
                return;
            }
        }

        let opcode = frame[0];
        eprintln!("[SERVER] OpenSession opcode = 0x{:02x}", opcode);

        if opcode == 0x02 {
            eprintln!("[SERVER] sending OpenSessionOk");
            // OpenSessionOk: [len=1][opcode=0x82] (no payload)
            if let Err(e) = send_frame(&mut stream, 0x82, &[]) {
                eprintln!("[SERVER] send OpenSessionOk failed: {}", e);
                return;
            }
            eprintln!("[SERVER] OpenSessionOk sent");
        }

        // Give client time to read, then close
        std::thread::sleep(Duration::from_millis(100));
        let _ = stream.shutdown(Shutdown::Both);
        eprintln!("[SERVER] done");
    });

    eprintln!("[TEST] main: waiting 50ms for server to start");
    std::thread::sleep(Duration::from_millis(50));

    eprintln!("[TEST] main: connecting client to {}", socket_path_str);
    let config = Config {
        socket_path: Some(std::path::PathBuf::from(&socket_path_str)),
        agent_path: None,
        data_dir: None,
        enabled: true,
        warning_handler: None,
    };

    eprintln!("[TEST] main: calling open_strict");
    let result = Session::open_strict(&config);
    eprintln!("[TEST] main: open_strict returned: {:?}", result);

    eprintln!("[TEST] main: joining server thread");
    server_handle.join().unwrap();
    eprintln!("[TEST] main: server thread joined");

    // Verify result
    assert!(
        result.is_ok(),
        "open_strict should succeed, got {:?}",
        result
    );
}

/// Test that server returns Error frame after Hello → Error::Protocol
#[test]
fn mock_error_after_hello() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent-error.sock");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let listener = UnixListener::bind(&socket_path).unwrap();
    let _socket_path_str_clone = socket_path_str.clone();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server accept");
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Read Hello
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).ok();
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > 0 && len < 1024 {
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();

            // Send Error frame: [len=1][opcode=0xFF]
            let error_frame = [1u8, 0, 0, 0, 0xFF];
            stream.write_all(&error_frame).ok();
        }
        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    let config = Config {
        socket_path: Some(std::path::PathBuf::from(&socket_path_str)),
        agent_path: None,
        data_dir: None,
        enabled: true,
        warning_handler: None,
    };

    let result = Session::open_strict(&config);
    server_handle.join().unwrap();

    assert!(
        result.is_err(),
        "open_strict should fail when server sends Error"
    );
    assert!(matches!(result.unwrap_err(), dtj_sdk::Error::Protocol));
}

/// Full vertical slice: emit a bool event and close
#[test]
fn mock_socket_vertical_slice() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent-slice.sock");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let listener = UnixListener::bind(&socket_path).unwrap();
    let _socket_path_str_clone = socket_path_str.clone();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server accept");
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

        // Intern domain → InternOk (id=1)
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x86, &[1u8, 0, 0, 0]).ok();
        }

        // Intern category → InternOk (id=2)
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x86, &[2u8, 0, 0, 0]).ok();
        }

        // Intern event_name → InternOk (id=3)
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x86, &[3u8, 0, 0, 0]).ok();
        }

        // Intern field_name → InternOk (id=4)
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x86, &[4u8, 0, 0, 0]).ok();
        }

        // AppendEvent → AppendEventOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            stream.read_exact(&mut frame).ok();
            // AppendEventOk: [len=9][opcode=0x83][seq=1]
            send_frame(&mut stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();
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
        socket_path: Some(std::path::PathBuf::from(&socket_path_str)),
        agent_path: None,
        data_dir: None,
        enabled: true,
        warning_handler: None,
    };

    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    // Emit a bool event
    let emit_result = session.emit(
        "domain",
        "category",
        "event",
        "field",
        dtj_sdk::Value::Bool(true),
    );
    assert!(emit_result.is_ok(), "emit should succeed");

    // Close session
    let close_result = session.close();
    assert!(close_result.is_ok(), "close should succeed");
    assert!(session.is_closed(), "Session should be closed after close");

    // Second close should be idempotent
    let second_close = session.close();
    assert!(second_close.is_ok(), "second close should be idempotent");

    server_handle.join().unwrap();
}

/// Test that dictionary caching works - repeated strings should not send duplicate Intern
#[test]
fn mock_dictionary_caching() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("dtj-agent-cache.sock");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let listener = UnixListener::bind(&socket_path).unwrap();

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server accept");
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

        // Count Intern requests received
        let mut intern_count = 0;

        // Hello → HelloOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let mut frame = vec![0u8; u32::from_le_bytes(len_buf) as usize];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x81, &[1u8, 0, 0, 0]).ok();
        }

        // OpenSession → OpenSessionOk
        {
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).ok();
            let mut frame = vec![0u8; u32::from_le_bytes(len_buf) as usize];
            stream.read_exact(&mut frame).ok();
            send_frame(&mut stream, 0x82, &[]).ok();
        }

        // Read frames until connection closes
        // First emit: 4 Intern (domain, category, event, field) + 1 AppendEvent
        // Second emit: 0 Intern (all cached) + 1 AppendEvent
        // Then close
        let mut id_counter = 0u32;
        loop {
            let mut len_buf = [0u8; 4];
            if stream.read_exact(&mut len_buf).is_err() {
                break; // Connection closed
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut frame = vec![0u8; len];
            if stream.read_exact(&mut frame).is_err() {
                break;
            }

            match frame[0] {
                0x06 => {
                    // Intern
                    intern_count += 1;
                    id_counter += 1;
                    send_frame(&mut stream, 0x86, &id_counter.to_le_bytes()).ok();
                }
                0x03 => {
                    // AppendEvent
                    send_frame(&mut stream, 0x83, &[1u8, 0, 0, 0, 0, 0, 0, 0]).ok();
                }
                0x04 => {
                    // FinishSession
                    send_frame(&mut stream, 0x84, &[]).ok();
                }
                _ => {}
            }
        }

        eprintln!("[SERVER] Received {} Intern requests", intern_count);
        // First emit sends 4 Intern requests (domain, category, event, field_name)
        // Second emit sends 0 Intern requests (all cached)
        // So total should be 4
        assert_eq!(
            intern_count, 4,
            "Expected 4 Intern requests (all from first emit, second emit cached)"
        );

        let _ = stream.shutdown(Shutdown::Both);
    });

    std::thread::sleep(Duration::from_millis(50));

    let config = Config {
        socket_path: Some(std::path::PathBuf::from(&socket_path_str)),
        agent_path: None,
        data_dir: None,
        enabled: true,
        warning_handler: None,
    };

    let mut session = Session::open_strict(&config).expect("open_strict should succeed");

    // First emit with all unique strings
    session
        .emit(
            "domain",
            "category",
            "event",
            "field",
            dtj_sdk::Value::Bool(true),
        )
        .ok();

    // Second emit with SAME domain/category/event/field but different value
    // The string interning should be cached (no new Intern requests)
    session
        .emit(
            "domain",
            "category",
            "event",
            "field",
            dtj_sdk::Value::Bool(false),
        )
        .ok();

    session.close().ok();
    server_handle.join().unwrap();
}
