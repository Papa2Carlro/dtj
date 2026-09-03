use crate::error::Error;
use std::io::{Read, Write};

// =============================================================================
// Frame read/write
// =============================================================================

/// Write a frame: 4-byte little-endian length (including opcode) + opcode + payload
pub fn write_frame<W: Write>(writer: &mut W, opcode: u8, payload: &[u8]) -> Result<(), Error> {
    let len = 1u32 + payload.len() as u32;
    let mut frame = Vec::with_capacity(4 + 1 + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.push(opcode);
    frame.extend_from_slice(payload);
    writer.write_all(&frame).map_err(|_| Error::IoError)?;
    Ok(())
}

/// Read a frame: 4-byte length + opcode + payload
pub fn read_frame<R: Read>(reader: &mut R) -> Result<(u8, Vec<u8>), Error> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .map_err(|_| Error::IoError)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 1024 * 1024 {
        return Err(Error::FrameTooLarge);
    }
    let mut frame = vec![0u8; len];
    reader.read_exact(&mut frame).map_err(|_| Error::IoError)?;
    let opcode = frame[0];
    let payload = frame[1..].to_vec();
    Ok((opcode, payload))
}

// =============================================================================
// Hello / HelloOk
// =============================================================================

pub const OPCODE_HELLO: u8 = 0x01;
pub const OPCODE_HELLO_OK: u8 = 0x81;
pub const PROTOCOL_VERSION: u32 = 1;

pub fn write_hello<W: Write>(writer: &mut W) -> Result<(), Error> {
    write_frame(writer, OPCODE_HELLO, &PROTOCOL_VERSION.to_le_bytes())
}

pub fn read_hello_ok<R: Read>(reader: &mut R) -> Result<(), Error> {
    let (opcode, payload) = read_frame(reader)?;
    if opcode != OPCODE_HELLO_OK {
        return Err(Error::Protocol);
    }
    if payload.len() != 4 {
        return Err(Error::Protocol);
    }
    let version = u32::from_le_bytes(payload[..4].try_into().unwrap());
    if version != PROTOCOL_VERSION {
        return Err(Error::BadVersion);
    }
    Ok(())
}

/// Read HelloOk or Error frame.
/// Returns Ok(true) for HelloOk, Ok(false) for Error, Err for other frames.
pub fn read_hello_ok_or_error<R: Read>(reader: &mut R) -> Result<bool, Error> {
    let (opcode, _payload) = read_frame(reader)?;
    match opcode {
        OPCODE_HELLO_OK => Ok(true),
        OPCODE_ERROR => Ok(false),
        _ => Err(Error::Protocol),
    }
}

// =============================================================================
// OpenSession / OpenSessionOk
// =============================================================================

pub const OPCODE_OPEN_SESSION: u8 = 0x02;
pub const OPCODE_OPEN_SESSION_OK: u8 = 0x82;

/// Payload for OpenSession frame
#[derive(Debug, Clone)]
pub struct OpenSessionPayload {
    pub file_name: String,
    pub session_id: [u8; 16],
    pub start_utc_unix_ms: i64,
    pub mono_origin_ns: u64,
    pub producer_name: String,
    pub producer_version: String,
}

pub fn write_open_session<W: Write>(writer: &mut W, p: &OpenSessionPayload) -> Result<(), Error> {
    let mut buf = Vec::new();
    let file_name = p.file_name.as_bytes();
    buf.extend_from_slice(&(file_name.len() as u16).to_le_bytes());
    buf.extend_from_slice(file_name);
    buf.extend_from_slice(&p.session_id);
    buf.extend_from_slice(&p.start_utc_unix_ms.to_le_bytes());
    buf.extend_from_slice(&p.mono_origin_ns.to_le_bytes());
    let producer_name = p.producer_name.as_bytes();
    buf.extend_from_slice(&(producer_name.len() as u16).to_le_bytes());
    buf.extend_from_slice(producer_name);
    let producer_version = p.producer_version.as_bytes();
    buf.extend_from_slice(&(producer_version.len() as u16).to_le_bytes());
    buf.extend_from_slice(producer_version);
    write_frame(writer, OPCODE_OPEN_SESSION, &buf)
}

pub fn read_open_session_ok<R: Read>(reader: &mut R) -> Result<(), Error> {
    let (opcode, payload) = read_frame(reader)?;
    if opcode != OPCODE_OPEN_SESSION_OK {
        return Err(Error::Protocol);
    }
    if !payload.is_empty() {
        return Err(Error::Protocol);
    }
    Ok(())
}

/// Read OpenSessionOk or Error frame.
/// Returns Ok(true) for OpenSessionOk, Ok(false) for Error, Err for other frames.
pub fn read_open_session_ok_or_error<R: Read>(reader: &mut R) -> Result<bool, Error> {
    let (opcode, payload) = read_frame(reader)?;
    match opcode {
        OPCODE_OPEN_SESSION_OK if payload.is_empty() => Ok(true),
        OPCODE_ERROR => Ok(false),
        _ => Err(Error::Protocol),
    }
}

// =============================================================================
// Intern / InternOk
// =============================================================================

pub const OPCODE_INTERN: u8 = 0x06;
pub const OPCODE_INTERN_OK: u8 = 0x86;

pub const DICT_KIND_DOMAIN: u8 = 1;
pub const DICT_KIND_CATEGORY: u8 = 2;
pub const DICT_KIND_EVENT_NAME: u8 = 3;
pub const DICT_KIND_STRING: u8 = 4;

pub fn write_intern<W: Write>(writer: &mut W, dict_kind: u8, name: &str) -> Result<(), Error> {
    let name_bytes = name.as_bytes();
    let mut payload = Vec::with_capacity(1 + 2 + name_bytes.len());
    payload.push(dict_kind);
    payload.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    payload.extend_from_slice(name_bytes);
    write_frame(writer, OPCODE_INTERN, &payload)
}

pub fn read_intern_ok<R: Read>(reader: &mut R) -> Result<u32, Error> {
    let (opcode, payload) = read_frame(reader)?;
    if opcode != OPCODE_INTERN_OK {
        return Err(Error::Protocol);
    }
    if payload.len() != 4 {
        return Err(Error::Protocol);
    }
    Ok(u32::from_le_bytes(payload[..4].try_into().unwrap()))
}

// =============================================================================
// AppendEvent / AppendEventOk
// =============================================================================

pub const OPCODE_APPEND_EVENT: u8 = 0x03;
pub const OPCODE_APPEND_EVENT_OK: u8 = 0x83;

/// Frame for writing an AppendEvent record.
pub struct AppendEventFrame<'a> {
    pub monotonic_ns: u64,
    pub domain_id: u32,
    pub category_id: u32,
    pub event_name_id: u32,
    pub correlation_id: u32,
    pub severity: u8,
    pub field_name_id: u32,
    pub type_tag: u8,
    pub value_body: &'a [u8],
}

pub fn write_append_event<W: Write>(
    writer: &mut W,
    frame: AppendEventFrame<'_>,
) -> Result<(), Error> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&frame.monotonic_ns.to_le_bytes());
    payload.extend_from_slice(&frame.domain_id.to_le_bytes());
    payload.extend_from_slice(&frame.category_id.to_le_bytes());
    payload.extend_from_slice(&frame.event_name_id.to_le_bytes());
    payload.extend_from_slice(&frame.correlation_id.to_le_bytes());
    payload.push(frame.severity);
    payload.extend_from_slice(&1u16.to_le_bytes()); // field_count = 1
    payload.extend_from_slice(&frame.field_name_id.to_le_bytes());
    payload.push(frame.type_tag);
    payload.extend_from_slice(&[0u8; 3]); // reserved
    payload.extend_from_slice(frame.value_body);
    write_frame(writer, OPCODE_APPEND_EVENT, &payload)
}

pub fn read_append_event_ok<R: Read>(reader: &mut R) -> Result<u64, Error> {
    let (opcode, payload) = read_frame(reader)?;
    if opcode != OPCODE_APPEND_EVENT_OK {
        return Err(Error::Protocol);
    }
    if payload.len() != 8 {
        return Err(Error::Protocol);
    }
    Ok(u64::from_le_bytes(payload[..8].try_into().unwrap()))
}

// =============================================================================
// FinishSession / FinishSessionOk
// =============================================================================

pub const OPCODE_FINISH_SESSION: u8 = 0x04;
pub const OPCODE_FINISH_SESSION_OK: u8 = 0x84;

pub fn write_finish_session<W: Write>(writer: &mut W) -> Result<(), Error> {
    write_frame(writer, OPCODE_FINISH_SESSION, &[])
}

pub fn read_finish_session_ok<R: Read>(reader: &mut R) -> Result<(), Error> {
    let (opcode, payload) = read_frame(reader)?;
    if opcode != OPCODE_FINISH_SESSION_OK {
        return Err(Error::Protocol);
    }
    if !payload.is_empty() {
        return Err(Error::Protocol);
    }
    Ok(())
}

/// Read FinishSessionOk or Error frame.
/// Returns Ok(true) for FinishSessionOk, Ok(false) for Error, Err for other frames.
pub fn read_finish_session_ok_or_error<R: Read>(reader: &mut R) -> Result<bool, Error> {
    let (opcode, payload) = read_frame(reader)?;
    match opcode {
        OPCODE_FINISH_SESSION_OK if payload.is_empty() => Ok(true),
        OPCODE_ERROR => Ok(false),
        _ => Err(Error::Protocol),
    }
}

// =============================================================================
// Error frame
// =============================================================================

pub const OPCODE_ERROR: u8 = 0xFF;

pub fn read_error_frame<R: Read>(reader: &mut R) -> Result<String, Error> {
    let (opcode, payload) = read_frame(reader)?;
    if opcode != OPCODE_ERROR {
        return Err(Error::Protocol);
    }
    String::from_utf8(payload).map_err(|_| Error::Protocol)
}
