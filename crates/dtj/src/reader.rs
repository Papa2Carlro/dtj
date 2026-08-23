use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::chunk::ChunkHeader;
use crate::crc32::crc32;
use crate::dict::{DictKind, Dictionary};
use crate::error::{Error, Result};
use crate::event::EventRecord;
use crate::format::{
    CHUNK_HEADER_SIZE, CHUNK_MAGIC, CHUNK_TRAILER_SIZE, CHUNK_TYPE_DICTIONARY, CHUNK_TYPE_EVENT,
    COMMITTED_MARKER, HEADER_SIZE_USIZE, MAX_CHUNK_PAYLOAD,
};
use crate::header::FileHeader;
use crate::payload::Value;

/// Read-only view of a DTJ v1 session (recovery-aware).
///
/// Payload bytes are treated as opaque typed data only — never as paths, URLs,
/// code, commands, or dynamically executable types (§11 security model).
pub struct SessionReader {
    header: FileHeader,
    dictionary: Dictionary,
    events: Vec<EventRecord>,
    chunks_committed: u64,
    torn_tail: bool,
}

impl SessionReader {
    /// Open an existing `.dtj` file and recover all committed chunks.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut header_buf = [0u8; HEADER_SIZE_USIZE];
        file.read_exact(&mut header_buf)?;
        let header = FileHeader::decode(&header_buf)?;

        let file_len = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(HEADER_SIZE_USIZE as u64))?;

        let mut dictionary = Dictionary::new();
        let mut events = Vec::new();
        let mut offset = HEADER_SIZE_USIZE as u64;
        let mut expected_chunk_seq = 1u64;
        let mut expected_event_seq = 1u64;
        let mut torn_tail = false;

        while file_len.saturating_sub(offset) >= (CHUNK_HEADER_SIZE + CHUNK_TRAILER_SIZE) as u64 {
            file.seek(SeekFrom::Start(offset))?;
            let mut hdr_buf = [0u8; CHUNK_HEADER_SIZE];
            file.read_exact(&mut hdr_buf)?;

            if &hdr_buf[0..4] != CHUNK_MAGIC {
                // Partial/torn header or trailing garbage: recover prior commits.
                if offset == HEADER_SIZE_USIZE as u64 {
                    return Err(Error::InvalidChunkMagic);
                }
                torn_tail = true;
                break;
            }

            let chunk_hdr = ChunkHeader::decode(&hdr_buf)?;
            // Physical completeness before semantic MAX (§8): oversized declared
            // length on an incomplete final frame is torn recovery, not PayloadTooLarge.
            let need = (CHUNK_HEADER_SIZE as u64)
                .checked_add(u64::from(chunk_hdr.payload_len))
                .and_then(|n| n.checked_add(CHUNK_TRAILER_SIZE as u64))
                .ok_or_else(|| Error::MalformedRecord("chunk size overflow".into()))?;
            let remaining = file_len.saturating_sub(offset);
            if need > remaining {
                torn_tail = true;
                break;
            }

            if chunk_hdr.payload_len > MAX_CHUNK_PAYLOAD {
                return Err(Error::PayloadTooLarge {
                    len: chunk_hdr.payload_len,
                    max: MAX_CHUNK_PAYLOAD,
                });
            }

            let mut payload = vec![0u8; chunk_hdr.payload_len as usize];
            file.read_exact(&mut payload)?;
            let mut trailer = [0u8; CHUNK_TRAILER_SIZE];
            file.read_exact(&mut trailer)?;
            let checksum = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
            let committed = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);

            if committed != COMMITTED_MARKER {
                torn_tail = true;
                break;
            }

            let expected_crc = crc32(&payload);
            if checksum != expected_crc {
                return Err(Error::ChecksumMismatch {
                    sequence: chunk_hdr.sequence,
                });
            }

            if chunk_hdr.sequence != expected_chunk_seq {
                return Err(Error::SequenceGap {
                    expected: expected_chunk_seq,
                    found: chunk_hdr.sequence,
                });
            }
            expected_chunk_seq += 1;

            match chunk_hdr.chunk_type {
                CHUNK_TYPE_DICTIONARY => {
                    let entries = Dictionary::decode_entries(&payload)?;
                    for entry in entries {
                        dictionary.insert(entry)?;
                    }
                }
                CHUNK_TYPE_EVENT => {
                    let decoded = EventRecord::decode_many(&payload)?;
                    for ev in &decoded {
                        if ev.event_sequence != expected_event_seq {
                            return Err(Error::SequenceGap {
                                expected: expected_event_seq,
                                found: ev.event_sequence,
                            });
                        }
                        expected_event_seq += 1;
                        validate_event_refs(&dictionary, ev)?;
                    }
                    events.extend(decoded);
                }
                // §1.1: unknown/reserved committed chunk types are skipped after CRC.
                _ => {}
            }

            offset += need;
        }

        if file_len > offset && !torn_tail {
            torn_tail = true;
        }

        Ok(Self {
            header,
            dictionary,
            events,
            chunks_committed: expected_chunk_seq.saturating_sub(1),
            torn_tail,
        })
    }

    pub fn header(&self) -> &FileHeader {
        &self.header
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    pub fn events(&self) -> &[EventRecord] {
        &self.events
    }

    pub fn iter_events(&self) -> impl Iterator<Item = &EventRecord> {
        self.events.iter()
    }

    pub fn chunks_committed(&self) -> u64 {
        self.chunks_committed
    }

    pub fn had_torn_tail(&self) -> bool {
        self.torn_tail
    }
}

fn validate_event_refs(dict: &Dictionary, ev: &EventRecord) -> Result<()> {
    dict.require(DictKind::Domain, ev.domain_id)?;
    dict.require(DictKind::Category, ev.category_id)?;
    dict.require(DictKind::EventName, ev.event_name_id)?;
    if ev.correlation_id != 0 {
        dict.require(DictKind::String, ev.correlation_id)?;
    }
    for field in &ev.payload.fields {
        dict.require(DictKind::String, field.name_id)?;
        if let Value::InternedString(sid) = &field.value {
            dict.require(DictKind::String, *sid)?;
        }
    }
    Ok(())
}
