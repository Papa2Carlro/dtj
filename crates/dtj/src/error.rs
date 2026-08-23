//! Structured DTJ errors — never panic on corrupted bytes.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Io(String),
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidHeaderSize(u16),
    InvalidEndian,
    InvalidChunkMagic,
    ChecksumMismatch { sequence: u64 },
    SequenceGap { expected: u64, found: u64 },
    PayloadTooLarge { len: u32, max: u32 },
    MalformedRecord(String),
    UnknownDictionaryId { kind: u8, id: u32 },
    DuplicateDictionaryId { kind: u8, id: u32 },
    UnknownTypeTag(u8),
    InvalidSeverity(u8),
    LimitExceeded(String),
    SessionClosed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::InvalidMagic => write!(f, "invalid DTJ file magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported DTJ format_version {v}"),
            Self::InvalidHeaderSize(n) => write!(f, "invalid header_size {n}"),
            Self::InvalidEndian => write!(f, "invalid endian_magic (expected LE 0x01020304)"),
            Self::InvalidChunkMagic => write!(f, "invalid chunk magic"),
            Self::ChecksumMismatch { sequence } => {
                write!(f, "checksum mismatch for chunk sequence {sequence}")
            }
            Self::SequenceGap { expected, found } => {
                write!(f, "sequence gap: expected {expected}, found {found}")
            }
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload_len {len} exceeds max {max}")
            }
            Self::MalformedRecord(msg) => write!(f, "malformed record: {msg}"),
            Self::UnknownDictionaryId { kind, id } => {
                write!(f, "unknown dictionary id kind={kind} id={id}")
            }
            Self::DuplicateDictionaryId { kind, id } => {
                write!(f, "dictionary id conflict kind={kind} id={id}")
            }
            Self::UnknownTypeTag(t) => write!(f, "unknown typed payload tag 0x{t:02X}"),
            Self::InvalidSeverity(s) => write!(f, "invalid severity {s}"),
            Self::LimitExceeded(msg) => write!(f, "limit exceeded: {msg}"),
            Self::SessionClosed => write!(f, "session already finished"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
