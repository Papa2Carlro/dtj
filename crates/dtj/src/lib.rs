// DTJ v1 reference core — minimal lib.rs
// This is a stub; full implementation in crates/dtj/

/// Magic bytes for DTJ v1 file identification
pub const MAGIC: [u8; 4] = [0x44, 0x54, 0x4A, 0x31]; // "DTJ1"

/// Format version for DTJ v1
pub const FORMAT_VERSION: u16 = 1;

/// Maximum chunk payload length in bytes (16 MiB)
pub const MAX_CHUNK_PAYLOAD: u32 = 16_777_216;

/// Maximum dictionary entries
pub const MAX_DICT_ENTRIES: u32 = 65_535;

/// Maximum events per Event chunk
pub const MAX_EVENTS_PER_CHUNK: u32 = 65_535;

/// Maximum typed payload bytes per event
pub const MAX_PAYLOAD_BYTES: u32 = 65_535;

/// Maximum Bytes value length
pub const MAX_BYTES_VALUE: u32 = 4_096;

/// DTJ v1 session start marker
pub fn session_start() -> &'static str {
    "DTJ1 session started"
}
