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

use dtj::{AppendEvent, FileHeader, SessionWriter, Severity, TypedPayload, Value};

/// Protocol version supported by this agent.
const PROTOCOL_VERSION: u32 = 1;

/// Maximum frame size we accept (1 MiB) to avoid unbounded allocations.
const MAX_FRAME_SIZE: usize = 1_048_576;

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
}

impl AgentState {
    fn new() -> Self {
        Self {
            writer: None,
            hello_done: false,
            session_opened: false,
        }
    }
}

/// Handle a single client connection.
fn handle_client(mut stream: UnixStream, data_dir: &Path) -> io::Result<()> {
    let state = Arc::new(Mutex::new(AgentState::new()));

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

            // OpenSession { header: 128 bytes, file_name: UTF-8 NUL-terminated }
            c if c == Cmd::OpenSession as u8 => {
                if !st.hello_done {
                    write_frame(&mut stream, Resp::Error as u8, b"Hello required first")?;
                    continue;
                }
                if body.len() < 128 {
                    write_frame(&mut stream, Resp::Error as u8, b"header too short")?;
                    continue;
                }
                let header_bytes: [u8; 128] = body[..128].try_into().unwrap();
                let header = match FileHeader::decode(&header_bytes) {
                    Ok(h) => h,
                    Err(e) => {
                        let msg = format!("invalid header: {:?}", e);
                        write_frame(&mut stream, Resp::Error as u8, msg.as_bytes())?;
                        continue;
                    }
                };
                // file name after header, NUL-terminated
                let name_bytes = &body[128..];
                let name_str = match std::str::from_utf8(name_bytes) {
                    Ok(s) => s,
                    Err(_) => {
                        write_frame(&mut stream, Resp::Error as u8, b"file name not valid UTF-8")?;
                        continue;
                    }
                };
                // Optional trailing NUL can be stripped (protocol: NUL-terminated optional)
                let name_str = name_str.trim_end_matches('\0');
                // Reject NUL anywhere in the filename (after stripping terminator)
                if name_str.contains('\0') {
                    write_frame(&mut stream, Resp::Error as u8, b"file name contains NUL")?;
                    continue;
                }
                if name_str.is_empty() {
                    write_frame(&mut stream, Resp::Error as u8, b"empty file name")?;
                    continue;
                }
                if name_str.contains('/')
                    || name_str.contains('\\')
                    || name_str.contains("..")
                    || Path::new(name_str).is_absolute()
                {
                    write_frame(&mut stream, Resp::Error as u8, b"invalid file name")?;
                    continue;
                }
                let data_path = data_dir.join(name_str);
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
                let writer = match st.writer.as_mut() {
                    Some(w) => w,
                    None => {
                        write_frame(&mut stream, Resp::Error as u8, b"no open session")?;
                        continue;
                    }
                };
                match writer.append_event(evt) {
                    Ok(seq) => {
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
                if let Some(writer) = st.writer.take() {
                    match writer.finish() {
                        Ok(()) => write_frame(&mut stream, Resp::FinishSessionOk as u8, b"")?,
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

            _ => {
                write_frame(&mut stream, Resp::Error as u8, b"unknown command")?;
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 || args[1] != "--socket" || args[3] != "--data-dir" {
        eprintln!("Usage: dtj-agent --socket <path> --data-dir <dir>");
        std::process::exit(1);
    }
    let socket_path = PathBuf::from(&args[2]);
    let data_dir = PathBuf::from(&args[4]);

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
    fs::create_dir_all(&data_dir)
        .map_err(|e| io::Error::other(format!("cannot create data dir: {:?}", e)))?;

    let listener = UnixListener::bind(&socket_path)?;
    // Accept a single connection (MVP)
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle_client(s, &data_dir) {
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
