//! dtj-agent — local Unix domain socket server that owns the DTJ v1 SessionWriter.
//!
//! This binary is the single ingress point for writing `.dtj` files.  Clients
//! communicate via a tiny versioned binary protocol (see
//! `docs/dtj-agent-protocol-v1.md`).  The agent never exposes the DTJ byte
//! format to clients; it only forwards high‑level operations to the existing
//! `dtj::SessionWriter` implementation.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dtj::{AppendEvent, FileHeader, SessionWriter, Severity, TypedPayload, Value, MAX_EVENTS_PER_CHUNK};

/// Protocol version supported by this agent.
const PROTOCOL_VERSION: u32 = 1;

/// Maximum frame size we accept (1 MiB) to avoid unbounded allocations.
const MAX_FRAME_SIZE: usize = 1_048_576;

/// Configuration structure for TOML parsing.
#[derive(Debug, serde::Deserialize)]
struct Config {
    storage: Option<StorageConfig>,
}

#[derive(Debug, serde::Deserialize)]
struct StorageConfig {
    data_dir: Option<String>,
    /// If true, flush every event to disk immediately (slower but durable).
    flush_every_event: Option<bool>,
    /// Max events per chunk before flushing (default 65535).
    max_events_per_chunk: Option<u32>,
    /// Max file size in MB before rotation (default unlimited).
    max_file_size_mb: Option<u64>,
    /// Session TTL in hours - auto-delete sessions older than this.
    session_ttl_hours: Option<u64>,
    /// Max number of session files to keep (oldest deleted first).
    max_sessions: Option<u32>,
    /// Socket file permissions (octal, default 0600).
    socket_permissions: Option<u32>,
    /// Prefix for session file names (default "session").
    session_prefix: Option<String>,
    /// Flush buffer when it exceeds this many bytes.
    max_pending_bytes: Option<u64>,
}

/// Command opcodes (client → server).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Cmd {
    Hello = 0x01,
    OpenSession = 0x02,
    AppendEvent = 0x03,
    FinishSession = 0x04,
    Ping = 0x05,
    Intern = 0x06,
    Flush = 0x07,
}

/// Response opcodes (server → client).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Resp {
    HelloOk = 0x81,
    OpenSessionOk = 0x82,
    AppendEventOk = 0x83,
    FinishSessionOk = 0x84,
    Pong = 0x85,
    InternOk = 0x86,
    FlushOk = 0x87,
    Error = 0xFF,
}

/// Simple length‑prefixed frame: 4‑byte LE length + payload.
fn read_frame(stream: &mut UnixStream) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_frame(stream: &mut UnixStream, opcode: u8, body: &[u8]) -> io::Result<()> {
    let mut frame = Vec::with_capacity(4 + 1 + body.len());
    frame.extend_from_slice(&(1 + body.len() as u32).to_le_bytes()); // length includes opcode
    frame.push(opcode);
    frame.extend_from_slice(body);
    stream.write_all(&frame)?;
    stream.flush()
}

/// Minimal session state kept by the agent (single client, single session).
struct AgentState {
    writer: Option<SessionWriter>,
    hello_done: bool,
    session_opened: bool,
    /// If true, flush every event to disk immediately (slower but durable).
    flush_every_event: bool,
    /// Max events per chunk before flushing.
    max_events_per_chunk: u32,
    /// Max file size in MB before rotation (0 = unlimited).
    max_file_size_mb: u64,
    /// Session TTL in hours (0 = never delete).
    session_ttl_hours: u64,
    /// Max number of session files (0 = unlimited).
    max_sessions: u32,
    /// Socket file permissions.
    socket_permissions: u32,
    /// Prefix for session file names.
    session_prefix: String,
    /// Flush buffer when it exceeds this many bytes (0 = use events count only).
    max_pending_bytes: u64,
}

impl AgentState {
    fn new(
        flush_every_event: bool,
        max_events_per_chunk: u32,
        max_file_size_mb: u64,
        session_ttl_hours: u64,
        max_sessions: u32,
        socket_permissions: u32,
        session_prefix: String,
        max_pending_bytes: u64,
    ) -> Self {
        Self {
            writer: None,
            hello_done: false,
            session_opened: false,
            flush_every_event,
            max_events_per_chunk,
            max_file_size_mb,
            session_ttl_hours,
            max_sessions,
            socket_permissions,
            session_prefix,
            max_pending_bytes,
        }
    }
}

/// Handle a single client connection.
fn handle_client(mut stream: UnixStream, data_dir: &Path, flush_every_event: bool, max_events_per_chunk: u32, max_file_size_mb: u64, session_ttl_hours: u64, max_sessions: u32, socket_permissions: u32, session_prefix: String, max_pending_bytes: u64) -> io::Result<()> {
    let state = Arc::new(Mutex::new(AgentState::new(flush_every_event, max_events_per_chunk, max_file_size_mb, session_ttl_hours, max_sessions, socket_permissions, session_prefix, max_pending_bytes)));

    while let Some(frame) = read_frame(&mut stream)? {
        if frame.is_empty() {
            write_frame(&mut stream, Resp::Error as u8, b"empty frame")?;
            continue;
        }
        let opcode = frame[0];
        let body = &frame[1..];

        let mut st = state.lock().unwrap();

        match opcode {
            // Hello { protocol_version: u32 LE }
            c if c == Cmd::Hello as u8 => {
                if body.len() < 4 {
                    write_frame(&mut stream, Resp::Error as u8, b"Hello expects 4 bytes")?;
                    continue;
                }
                let ver = u32::from_le_bytes(body[..4].try_into().unwrap());
                if ver != PROTOCOL_VERSION {
                    let mut err = Vec::new();
                    err.extend_from_slice(b"unsupported protocol version ");
                    err.extend_from_slice(&ver.to_le_bytes());
                    write_frame(&mut stream, Resp::Error as u8, &err)?;
                    continue;
                }
                st.hello_done = true;
                write_frame(
                    &mut stream,
                    Resp::HelloOk as u8,
                    &PROTOCOL_VERSION.to_le_bytes(),
                )?;
            }

            // Intern { dict_kind: u8, name_len: u16 LE, name bytes }
            c if c == Cmd::Intern as u8 => {
                if !st.hello_done || !st.session_opened {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"Intern requires Hello and OpenSession",
                    )?;
                    continue;
                }
                if body.len() < 3 {
                    write_frame(&mut stream, Resp::Error as u8, b"Intern payload too short")?;
                    continue;
                }
                let dict_kind = body[0];
                let name_len = u16::from_le_bytes([body[1], body[2]]) as usize;
                if body.len() < 3 + name_len {
                    write_frame(&mut stream, Resp::Error as u8, b"Intern name truncated")?;
                    continue;
                }
                let name_bytes = &body[3..3 + name_len];
                let name = match std::str::from_utf8(name_bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        write_frame(
                            &mut stream,
                            Resp::Error as u8,
                            b"Intern name not valid UTF-8",
                        )?;
                        continue;
                    }
                };
                if name.is_empty() || name_len > 1024 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"Intern name empty or too long",
                    )?;
                    continue;
                }
                let writer = match st.writer.as_mut() {
                    Some(w) => w,
                    None => {
                        write_frame(&mut stream, Resp::Error as u8, b"no open session")?;
                        continue;
                    }
                };
                let id = match dict_kind {
                    1 => writer.intern_domain(name),
                    2 => writer.intern_category(name),
                    3 => writer.intern_event_name(name),
                    4 => writer.intern_string(name),
                    _ => {
                        write_frame(&mut stream, Resp::Error as u8, b"unknown dictionary kind")?;
                        continue;
                    }
                };
                match id {
                    Ok(id) => {
                        let mut resp = Vec::new();
                        resp.extend_from_slice(&id.to_le_bytes());
                        write_frame(&mut stream, Resp::InternOk as u8, &resp)?;
                    }
                    Err(e) => {
                        let msg = format!("intern failed: {:?}", e);
                        write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                    }
                }
            }

            // OpenSession { metadata only; agent constructs FileHeader }
            c if c == Cmd::OpenSession as u8 => {
                if !st.hello_done {
                    write_frame(&mut stream, Resp::Error as u8, b"Hello required first")?;
                    continue;
                }
                // Parse metadata body:
                // file_name_len (u16 LE), file_name (UTF-8)
                // session_id (16 bytes)
                // start_utc_unix_ms (i64 LE), mono_origin_ns (u64 LE)
                // producer_name_len (u16 LE), producer_name (UTF-8)
                // producer_version_len (u16 LE), producer_version (UTF-8)
                let mut off = 0;
                if body.len() < 2 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: missing file_name_len",
                    )?;
                    continue;
                }
                let file_name_len = u16::from_le_bytes([body[off], body[off + 1]]) as usize;
                off += 2;
                if body.len() < off + file_name_len {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: file_name truncated",
                    )?;
                    continue;
                }
                let file_name = match std::str::from_utf8(&body[off..off + file_name_len]) {
                    Ok(s) => s,
                    Err(_) => {
                        write_frame(
                            &mut stream,
                            Resp::Error as u8,
                            b"OpenSession: file_name not valid UTF-8",
                        )?;
                        continue;
                    }
                };
                off += file_name_len;
                // Reject NUL, empty, path separators, traversal, absolute
                if file_name.contains('\0')
                    || file_name.is_empty()
                    || file_name.contains('/')
                    || file_name.contains('\\')
                    || file_name.contains("..")
                    || Path::new(file_name).is_absolute()
                {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: invalid file name",
                    )?;
                    continue;
                }
                // session_id: 16 bytes
                if body.len() < off + 16 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: session_id truncated",
                    )?;
                    continue;
                }
                let mut session_id = [0u8; 16];
                session_id.copy_from_slice(&body[off..off + 16]);
                off += 16;
                // start_utc_unix_ms (i64 LE), mono_origin_ns (u64 LE)
                if body.len() < off + 8 + 8 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: timestamps truncated",
                    )?;
                    continue;
                }
                let start_utc_unix_ms = i64::from_le_bytes(body[off..off + 8].try_into().unwrap());
                off += 8;
                let mono_origin_ns = u64::from_le_bytes(body[off..off + 8].try_into().unwrap());
                off += 8;
                // producer_name_len (u16 LE), producer_name (UTF-8)
                if body.len() < off + 2 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: missing producer_name_len",
                    )?;
                    continue;
                }
                let producer_name_len = u16::from_le_bytes([body[off], body[off + 1]]) as usize;
                off += 2;
                if body.len() < off + producer_name_len {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: producer_name truncated",
                    )?;
                    continue;
                }
                let producer_name = match std::str::from_utf8(&body[off..off + producer_name_len]) {
                    Ok(s) => s,
                    Err(_) => {
                        write_frame(
                            &mut stream,
                            Resp::Error as u8,
                            b"OpenSession: producer_name not valid UTF-8",
                        )?;
                        continue;
                    }
                };
                off += producer_name_len;
                // producer_version_len (u16 LE), producer_version (UTF-8)
                if body.len() < off + 2 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: missing producer_version_len",
                    )?;
                    continue;
                }
                let producer_version_len = u16::from_le_bytes([body[off], body[off + 1]]) as usize;
                off += 2;
                if body.len() < off + producer_version_len {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: producer_version truncated",
                    )?;
                    continue;
                }
                let producer_version =
                    match std::str::from_utf8(&body[off..off + producer_version_len]) {
                        Ok(s) => s,
                        Err(_) => {
                            write_frame(
                                &mut stream,
                                Resp::Error as u8,
                                b"OpenSession: producer_version not valid UTF-8",
                            )?;
                            continue;
                        }
                    };
                // Validate producer_name and producer_version lengths against core limits
                if producer_name.len() > 32 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: producer_name longer than 32 bytes",
                    )?;
                    continue;
                }
                if producer_version.len() > 16 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"OpenSession: producer_version longer than 16 bytes",
                    )?;
                    continue;
                }
                // Construct FileHeader via core API
                let header = match FileHeader::new(
                    session_id,
                    start_utc_unix_ms,
                    mono_origin_ns,
                    producer_name,
                    producer_version,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        let msg = format!("OpenSession: invalid header: {:?}", e);
                        write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                        continue;
                    }
                };
                let data_path = data_dir.join(file_name);
                if let Err(e) = fs::create_dir_all(data_dir) {
                    let msg = format!("cannot create data dir: {:?}", e);
                    let _ = write_frame(&mut stream, Resp::Error as u8, msg.as_bytes());
                    return Err(io::Error::other(msg));
                }
                let out_path = data_path;
                let writer = match SessionWriter::create(&out_path, header) {
                    Ok(w) => w,
                    Err(e) => {
                        let msg = format!("create session failed: {:?}", e);
                        write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                        continue;
                    }
                };
                st.writer = Some(writer);
                st.session_opened = true;
                write_frame(&mut stream, Resp::OpenSessionOk as u8, b"")?;
            }

            // AppendEvent { monotonic_ns:u64, domain_id:u32, category_id:u32, event_name_id:u32,
            // correlation_id:u32, severity:u8, field_count:u16, fields... }
            c if c == Cmd::AppendEvent as u8 => {
                if !st.hello_done || !st.session_opened {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"Hello and OpenSession required first",
                    )?;
                    continue;
                }
                // Very small MVP: accept a single field only (field_count must be 1)
                if body.len() < 8 + 4 + 4 + 4 + 4 + 1 + 2 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"AppendEvent body too short",
                    )?;
                    continue;
                }
                let mut off = 0;
                let monotonic_ns = u64::from_le_bytes(body[off..off + 8].try_into().unwrap());
                off += 8;
                let domain_id = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
                let category_id = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
                let event_name_id = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
                let correlation_id = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
                let severity = match Severity::from_u8(body[off]) {
                    Ok(s) => s,
                    Err(_) => {
                        write_frame(&mut stream, Resp::Error as u8, b"bad severity")?;
                        continue;
                    }
                };
                off += 1;
                let field_count = u16::from_le_bytes(body[off..off + 2].try_into().unwrap());
                off += 2;
                if field_count != 1 {
                    write_frame(
                        &mut stream,
                        Resp::Error as u8,
                        b"MVP supports exactly one field",
                    )?;
                    continue;
                }
                // field: name_id u32, type_tag u8, reserved 3, value body (variable)
                if off + 4 + 1 + 3 > body.len() {
                    write_frame(&mut stream, Resp::Error as u8, b"field header truncated")?;
                    continue;
                }
                let name_id = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                off += 4;
                let type_tag = body[off];
                off += 1;
                off += 3; // reserved
                          // value body depends on type_tag; we support a subset matching existing Value enum
                let value = match type_tag {
                    0x01 => {
                        // BOOL 1 byte
                        if off + 1 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"bool truncated")?;
                            continue;
                        }
                        let v = body[off] != 0;
                        Value::Bool(v)
                    }
                    0x02 => {
                        // I32
                        if off + 4 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"i32 truncated")?;
                            continue;
                        }
                        let v = i32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        Value::I32(v)
                    }
                    0x03 => {
                        // I64
                        if off + 8 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"i64 truncated")?;
                            continue;
                        }
                        let v = i64::from_le_bytes(body[off..off + 8].try_into().unwrap());
                        Value::I64(v)
                    }
                    0x04 => {
                        // U32
                        if off + 4 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"u32 truncated")?;
                            continue;
                        }
                        let v = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        Value::U32(v)
                    }
                    0x05 => {
                        // U64
                        if off + 8 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"u64 truncated")?;
                            continue;
                        }
                        let v = u64::from_le_bytes(body[off..off + 8].try_into().unwrap());
                        Value::U64(v)
                    }
                    0x06 => {
                        // F32
                        if off + 4 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"f32 truncated")?;
                            continue;
                        }
                        let v = f32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        Value::F32(v)
                    }
                    0x07 => {
                        // F64
                        if off + 8 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"f64 truncated")?;
                            continue;
                        }
                        let v = f64::from_le_bytes(body[off..off + 8].try_into().unwrap());
                        Value::F64(v)
                    }
                    0x08 => {
                        // ENUM (opaque u32)
                        if off + 4 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"enum truncated")?;
                            continue;
                        }
                        let v = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        Value::Enum(v)
                    }
                    0x09 => {
                        // VEC2_F32
                        if off + 8 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"vec2 truncated")?;
                            continue;
                        }
                        let x = f32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        let y = f32::from_le_bytes(body[off + 4..off + 8].try_into().unwrap());
                        Value::Vec2F32([x, y])
                    }
                    0x0A => {
                        // VEC3_F32
                        if off + 12 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"vec3 truncated")?;
                            continue;
                        }
                        let x = f32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        let y = f32::from_le_bytes(body[off + 4..off + 8].try_into().unwrap());
                        let z = f32::from_le_bytes(body[off + 8..off + 12].try_into().unwrap());
                        Value::Vec3F32([x, y, z])
                    }
                    0x0B => {
                        // INTERNED
                        if off + 4 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"interned truncated")?;
                            continue;
                        }
                        let v = u32::from_le_bytes(body[off..off + 4].try_into().unwrap());
                        Value::InternedString(v)
                    }
                    0x0C => {
                        // BYTES length-prefixed
                        if off + 4 > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"bytes len truncated")?;
                            continue;
                        }
                        let len =
                            u32::from_le_bytes(body[off..off + 4].try_into().unwrap()) as usize;
                        if off + 4 + len > body.len() {
                            write_frame(&mut stream, Resp::Error as u8, b"bytes data truncated")?;
                            continue;
                        }
                        let v = body[off + 4..off + 4 + len].to_vec();
                        Value::Bytes(v)
                    }
                    _ => {
                        write_frame(&mut stream, Resp::Error as u8, b"unsupported type tag")?;
                        continue;
                    }
                };
                let mut payload = TypedPayload::new();
                payload.fields.push(dtj::Field { name_id, value });
                let evt = AppendEvent {
                    monotonic_ns,
                    domain_id,
                    category_id,
                    event_name_id,
                    correlation_id,
                    severity,
                    payload,
                };
                let flush_every = st.flush_every_event;
                let writer = match st.writer.as_mut() {
                    Some(w) => w,
                    None => {
                        write_frame(&mut stream, Resp::Error as u8, b"no open session")?;
                        continue;
                    }
                };
                match writer.append_event(evt) {
                    Ok(seq) => {
                        if flush_every {
                            if let Err(e) = writer.flush_chunk() {
                                eprintln!("[agent] flush_chunk() failed: {:?}", e);
                            }
                        }
                        eprintln!("[agent] append_event OK, seq={}, pending={}", seq, writer.pending_events_len());
                        let mut resp = Vec::new();
                        resp.extend_from_slice(&seq.to_le_bytes());
                        write_frame(&mut stream, Resp::AppendEventOk as u8, &resp)?;
                    }
                    Err(e) => {
                        let msg = format!("append_event failed: {:?}", e);
                        write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                    }
                }
            }

            // FinishSession
            c if c == Cmd::FinishSession as u8 => {
                eprintln!("[agent] FinishSession received");
                if let Some(writer) = st.writer.take() {
                    eprintln!("[agent] calling writer.finish()...");
                    match writer.finish() {
                        Ok(()) => {
                            eprintln!("[agent] writer.finish() OK");
                            write_frame(&mut stream, Resp::FinishSessionOk as u8, b"")?
                        }
                        Err(e) => {
                            let msg = format!("finish failed: {:?}", e);
                            write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                        }
                    }
                } else {
                    write_frame(&mut stream, Resp::Error as u8, b"no open session")?;
                }
            }

            // Ping
            c if c == Cmd::Ping as u8 => {
                write_frame(&mut stream, Resp::Pong as u8, b"")?;
            }

            // Flush — force write pending chunk to disk
            c if c == Cmd::Flush as u8 => {
                eprintln!("[agent] Flush received");
                if let Some(writer) = st.writer.as_mut() {
                    match writer.flush_chunk() {
                        Ok(()) => {
                            eprintln!("[agent] flush_chunk() OK");
                            write_frame(&mut stream, Resp::FlushOk as u8, b"")?;
                        }
                        Err(e) => {
                            let msg = format!("flush failed: {:?}", e);
                            write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                        }
                    }
                } else {
                    write_frame(&mut stream, Resp::Error as u8, b"no open session")?;
                }
            }

            _ => {
                write_frame(&mut stream, Resp::Error as u8, b"unknown command")?;
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Handle --version / -V before any required-argument validation so users
    // can identify the binary without supplying --socket / --data-dir / --config.
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("dtj-agent {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // Parse arguments supporting both formats:
    // Old: dtj-agent --socket <path> --data-dir <dir>
    // New: dtj-agent --socket <path> --config <path>
    // Both: dtj-agent --socket <path> --data-dir <dir> --config <path> (data-dir wins)

    let mut socket_path: Option<PathBuf> = None;
    let mut data_dir: Option<PathBuf> = None;
    let mut config_path: Option<PathBuf> = None;
    let mut flush_every_event = false;
    let mut max_events_per_chunk: Option<u32> = None;
    let mut max_file_size_mb: Option<u64> = None;
    let mut session_ttl_hours: Option<u64> = None;
    let mut max_sessions: Option<u32> = None;
    let mut socket_permissions: Option<u32> = None;
    let mut session_prefix: Option<String> = None;
    let mut max_pending_bytes: Option<u64> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --socket");
                    std::process::exit(1);
                }
                socket_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--data-dir" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --data-dir");
                    std::process::exit(1);
                }
                data_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--config" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --config");
                    std::process::exit(1);
                }
                config_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--flush-every-event" => {
                flush_every_event = true;
                i += 1;
            }
            "--max-events-per-chunk" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --max-events-per-chunk");
                    std::process::exit(1);
                }
                max_events_per_chunk = Some(args[i + 1].parse().unwrap_or(65535));
                i += 2;
            }
            "--max-file-size-mb" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --max-file-size-mb");
                    std::process::exit(1);
                }
                max_file_size_mb = Some(args[i + 1].parse().unwrap_or(0));
                i += 2;
            }
            "--session-ttl-hours" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --session-ttl-hours");
                    std::process::exit(1);
                }
                session_ttl_hours = Some(args[i + 1].parse().unwrap_or(0));
                i += 2;
            }
            "--max-sessions" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --max-sessions");
                    std::process::exit(1);
                }
                max_sessions = Some(args[i + 1].parse().unwrap_or(0));
                i += 2;
            }
            "--socket-permissions" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --socket-permissions");
                    std::process::exit(1);
                }
                socket_permissions = Some(u32::from_str_radix(&args[i + 1], 8).unwrap_or(0o600));
                i += 2;
            }
            "--session-prefix" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --session-prefix");
                    std::process::exit(1);
                }
                session_prefix = Some(args[i + 1].clone());
                i += 2;
            }
            "--max-pending-bytes" => {
                if i + 1 >= args.len() {
                    eprintln!("Missing value for --max-pending-bytes");
                    std::process::exit(1);
                }
                max_pending_bytes = Some(args[i + 1].parse().unwrap_or(0));
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Usage: dtj-agent --socket <path> [--data-dir <dir>] [--config <path>]");
                eprintln!("  --flush-every-event        Flush every event to disk");
                eprintln!("  --max-events-per-chunk N   Max events per chunk (default 65535)");
                eprintln!("  --max-file-size-mb N       Max file size in MB before rotation");
                eprintln!("  --session-ttl-hours N      Auto-delete sessions older than N hours");
                eprintln!("  --max-sessions N           Max session files to keep");
                eprintln!("  --socket-permissions OCT   Socket permissions (octal, default 0600)");
                eprintln!("  --session-prefix PREFIX    Session file prefix (default \"session\")");
                eprintln!("  --max-pending-bytes N      Flush when buffer exceeds N bytes");
                std::process::exit(1);
            }
        }
    }

    let socket_path = socket_path.ok_or_else(|| {
        eprintln!("--socket is required");
        io::Error::other("--socket is required")
    })?;

    // Determine data_dir with priority:
    // 1. Explicit --data-dir (highest priority)
    // 2. From --config file [storage].data_dir
    // 3. Error if neither provided (no silent fallback)

    let final_data_dir = if let Some(explicit_data_dir) = data_dir {
        explicit_data_dir
    } else if let Some(config_path) = config_path {
        // Read and parse config file
        let config_content = fs::read_to_string(&config_path).map_err(|e| {
            io::Error::other(format!(
                "failed to read config file {}: {:?}",
                config_path.display(),
                e
            ))
        })?;

        let config: Config = toml::from_str(&config_content).map_err(|e| {
            io::Error::other(format!(
                "failed to parse config file {}: {:?}",
                config_path.display(),
                e
            ))
        })?;

        let storage_config = config.storage.ok_or_else(|| {
            io::Error::other(format!(
                "missing [storage] section in config file {}",
                config_path.display()
            ))
        })?;

        let data_dir_str = storage_config.data_dir.ok_or_else(|| {
            io::Error::other(format!(
                "missing storage.data_dir in config file {}",
                config_path.display()
            ))
        })?;

        // CLI flags override config file settings
        if !flush_every_event {
            if let Some(flush_cfg) = storage_config.flush_every_event {
                flush_every_event = flush_cfg;
            }
        }
        if max_events_per_chunk.is_none() {
            max_events_per_chunk = storage_config.max_events_per_chunk;
        }
        if max_file_size_mb.is_none() {
            max_file_size_mb = storage_config.max_file_size_mb;
        }
        if session_ttl_hours.is_none() {
            session_ttl_hours = storage_config.session_ttl_hours;
        }
        if max_sessions.is_none() {
            max_sessions = storage_config.max_sessions;
        }
        if socket_permissions.is_none() {
            socket_permissions = storage_config.socket_permissions;
        }
        if session_prefix.is_none() {
            session_prefix = storage_config.session_prefix;
        }
        if max_pending_bytes.is_none() {
            max_pending_bytes = storage_config.max_pending_bytes;
        }

        // Resolve relative path from config file's directory
        let config_dir = config_path
            .parent()
            .ok_or_else(|| io::Error::other("config file has no parent directory"))?;

        let data_dir_path = PathBuf::from(data_dir_str);
        if data_dir_path.is_relative() {
            config_dir.join(data_dir_path)
        } else {
            data_dir_path
        }
    } else {
        eprintln!("Either --data-dir or --config must be provided");
        std::process::exit(1);
    };

    // Validate socket path: if exists, must be a Unix socket
    if socket_path.exists() {
        let meta = fs::symlink_metadata(&socket_path)
            .map_err(|e| io::Error::other(format!("cannot stat socket path: {:?}", e)))?;
        if meta.file_type().is_socket() {
            fs::remove_file(&socket_path)
                .map_err(|e| io::Error::other(format!("cannot remove existing socket: {:?}", e)))?;
        } else {
            return Err(io::Error::other(
                "socket path exists and is not a Unix socket",
            ));
        }
    }

    // Ensure data directory exists
    fs::create_dir_all(&final_data_dir).map_err(|e| {
        io::Error::other(format!(
            "cannot create data dir {}: {:?}",
            final_data_dir.display(),
            e
        ))
    })?;

    // Apply defaults for any remaining unset options
    let max_events_per_chunk = max_events_per_chunk.unwrap_or(65535);
    let max_file_size_mb = max_file_size_mb.unwrap_or(0);
    let session_ttl_hours = session_ttl_hours.unwrap_or(0);
    let max_sessions = max_sessions.unwrap_or(0);
    let socket_permissions = socket_permissions.unwrap_or(0o600);
    let session_prefix = session_prefix.unwrap_or_else(|| "session".to_string());
    let max_pending_bytes = max_pending_bytes.unwrap_or(0);

    let listener = UnixListener::bind(&socket_path)?;
    // Accept a single connection (MVP)
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_client(s, &final_data_dir, flush_every_event, max_events_per_chunk, max_file_size_mb, session_ttl_hours, max_sessions, socket_permissions, session_prefix, max_pending_bytes) {
                    // log error silently
                    let _ = e;
                }
                break; // MVP: one client then exit
            }
            Err(e) => {
                let _ = e;
            }
        }
    }

    // Clean up socket file on normal exit
    let _ = fs::remove_file(&socket_path);
    Ok(())
}
