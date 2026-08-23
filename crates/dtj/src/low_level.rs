//! Low-level DTJ v1 primitives for testing and advanced use.
//!
//! These are the building blocks that the public API is built on.
//! They are exposed for conformance testing and specialized use cases.

pub use crate::chunk::{encode_committed_chunk, ChunkHeader};
pub use crate::crc32::crc32;
pub use crate::format::{
    CHUNK_HEADER_SIZE, CHUNK_MAGIC, CHUNK_TRAILER_SIZE, CHUNK_TYPE_DICTIONARY, CHUNK_TYPE_EVENT,
    COMMITTED_MARKER, FILE_MAGIC, MAX_CHUNK_PAYLOAD,
};
