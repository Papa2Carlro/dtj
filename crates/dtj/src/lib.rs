//! DTJ v1 reference core — Debug Trace Journal.
//!
//! Normative byte contract: `specs/dtj-format-v1.md`
//!
//! # Public surface
//!
//! - [`SessionWriter::create`] / [`SessionWriter::open_new`] — write header, intern
//!   dictionary entries, append typed events, seal committed chunks
//! - [`SessionReader::open`] — recover committed chunks/events with torn-tail
//!   recovery and fail-closed checksum validation
//! - [`TypedPayload`] / [`Value`] — TLV event values (no JSON on the event path)
//! - [`Error`] — structured errors; corrupted bytes must not panic

mod chunk;
mod crc32;
mod dict;
mod error;
mod event;
mod format;
mod header;
pub mod low_level;
mod payload;
mod reader;
mod writer;

pub use chunk::{encode_committed_chunk, ChunkHeader};
pub use crc32::crc32;
pub use dict::{DictEntry, DictKind, Dictionary};
pub use error::{Error, Result};
pub use event::{EventRecord, Severity};
pub use format::{
    CHUNK_TYPE_DICTIONARY, CHUNK_TYPE_EVENT, CHUNK_TYPE_FOOTER_RESERVED,
    CHUNK_TYPE_PAYLOAD_RESERVED, COMMITTED_MARKER, FORMAT_VERSION, HEADER_SIZE, MAX_BYTES_VALUE,
    MAX_CHUNK_PAYLOAD, MAX_DICT_NAME_LEN, MAX_EVENTS_PER_CHUNK, MAX_EVENT_PAYLOAD,
};
pub use header::FileHeader;
pub use low_level::*;
pub use payload::{Field, TypedPayload, Value};
pub use reader::SessionReader;
pub use writer::{AppendEvent, SessionWriter};
