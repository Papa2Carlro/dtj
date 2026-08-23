//! Normative constants for DTJ v1.

pub const FILE_MAGIC: &[u8; 4] = b"DTJ1";
pub const CHUNK_MAGIC: &[u8; 4] = b"DTJC";
pub const FORMAT_VERSION: u16 = 1;
pub const HEADER_SIZE: u16 = 128;
pub const HEADER_SIZE_USIZE: usize = 128;
pub const CHUNK_HEADER_SIZE: usize = 24;
pub const CHUNK_TRAILER_SIZE: usize = 8;
pub const ENDIAN_MAGIC: u32 = 0x0102_0304;
pub const COMMITTED_MARKER: u32 = 0xD7C0_FFEE;

pub const MAX_CHUNK_PAYLOAD: u32 = 16_777_216;
pub const MAX_DICT_NAME_LEN: u16 = 1_024;
pub const MAX_DICT_ENTRIES: u32 = 65_535;
pub const MAX_EVENTS_PER_CHUNK: u32 = 65_535;
pub const MAX_EVENT_PAYLOAD: u32 = 65_535;
pub const MAX_BYTES_VALUE: u16 = 4_096;

pub const CHUNK_TYPE_DICTIONARY: u16 = 1;
pub const CHUNK_TYPE_EVENT: u16 = 2;
/// Reserved (§4.2); reference writers must not emit. Readers skip after CRC.
pub const CHUNK_TYPE_PAYLOAD_RESERVED: u16 = 3;
/// Reserved (§4.2); reference writers must not emit. Readers skip after CRC.
pub const CHUNK_TYPE_FOOTER_RESERVED: u16 = 4;

pub const DICT_KIND_DOMAIN: u8 = 1;
pub const DICT_KIND_CATEGORY: u8 = 2;
pub const DICT_KIND_EVENT_NAME: u8 = 3;
pub const DICT_KIND_STRING: u8 = 4;

pub const TYPE_BOOL: u8 = 0x01;
pub const TYPE_I32: u8 = 0x02;
pub const TYPE_I64: u8 = 0x03;
pub const TYPE_U32: u8 = 0x04;
pub const TYPE_U64: u8 = 0x05;
pub const TYPE_F32: u8 = 0x06;
pub const TYPE_F64: u8 = 0x07;
pub const TYPE_ENUM: u8 = 0x08;
pub const TYPE_VEC2_F32: u8 = 0x09;
pub const TYPE_VEC3_F32: u8 = 0x0A;
pub const TYPE_INTERNED: u8 = 0x0B;
pub const TYPE_BYTES: u8 = 0x0C;
