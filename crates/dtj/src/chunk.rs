use crate::crc32::crc32;
use crate::error::{Error, Result};
use crate::format::{
    CHUNK_HEADER_SIZE, CHUNK_MAGIC, CHUNK_TRAILER_SIZE, COMMITTED_MARKER, MAX_CHUNK_PAYLOAD,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkHeader {
    pub chunk_type: u16,
    pub sequence: u64,
    pub payload_len: u32,
}

impl ChunkHeader {
    pub fn encode(&self) -> [u8; CHUNK_HEADER_SIZE] {
        let mut buf = [0u8; CHUNK_HEADER_SIZE];
        buf[0..4].copy_from_slice(CHUNK_MAGIC);
        buf[4..6].copy_from_slice(&self.chunk_type.to_le_bytes());
        buf[6..8].copy_from_slice(&0u16.to_le_bytes());
        buf[8..16].copy_from_slice(&self.sequence.to_le_bytes());
        buf[16..20].copy_from_slice(&self.payload_len.to_le_bytes());
        buf[20..24].copy_from_slice(&0u32.to_le_bytes());
        buf
    }

    /// Structural decode of the fixed 24-byte chunk header only.
    ///
    /// Does **not** enforce [`MAX_CHUNK_PAYLOAD`]: the reader must first decide
    /// whether the declared chunk is physically complete (torn-tail recovery)
    /// before applying semantic length limits.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < CHUNK_HEADER_SIZE {
            return Err(Error::MalformedRecord("chunk header truncated".into()));
        }
        if &buf[0..4] != CHUNK_MAGIC {
            return Err(Error::InvalidChunkMagic);
        }
        let chunk_type = u16::from_le_bytes([buf[4], buf[5]]);
        let sequence = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let payload_len = u32::from_le_bytes(buf[16..20].try_into().unwrap());
        Ok(Self {
            chunk_type,
            sequence,
            payload_len,
        })
    }
}

/// Encode a fully committed chunk: header + payload + checksum + committed.
pub fn encode_committed_chunk(chunk_type: u16, sequence: u64, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() as u32 > MAX_CHUNK_PAYLOAD {
        return Err(Error::PayloadTooLarge {
            len: payload.len() as u32,
            max: MAX_CHUNK_PAYLOAD,
        });
    }
    let header = ChunkHeader {
        chunk_type,
        sequence,
        payload_len: payload.len() as u32,
    };
    let mut out = Vec::with_capacity(CHUNK_HEADER_SIZE + payload.len() + CHUNK_TRAILER_SIZE);
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(payload);
    let sum = crc32(payload);
    out.extend_from_slice(&sum.to_le_bytes());
    out.extend_from_slice(&COMMITTED_MARKER.to_le_bytes());
    Ok(out)
}
