use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::chunk::encode_committed_chunk;
use crate::dict::{DictEntry, DictKind, Dictionary};
use crate::error::{Error, Result};
use crate::event::{EventRecord, Severity};
use crate::format::{CHUNK_TYPE_DICTIONARY, CHUNK_TYPE_EVENT, MAX_EVENTS_PER_CHUNK};
use crate::header::FileHeader;
use crate::payload::TypedPayload;

/// Fields for one typed event append (no JSON / formatted strings).
#[derive(Debug, Clone)]
pub struct AppendEvent {
    pub monotonic_ns: u64,
    pub domain_id: u32,
    pub category_id: u32,
    pub event_name_id: u32,
    /// Interned string id, or `0` for none.
    pub correlation_id: u32,
    pub severity: Severity,
    pub payload: TypedPayload,
}

/// Creates a new DTJ v1 session file and appends dictionary/event chunks.
///
/// Hot-path producers should buffer typed events in memory and call
/// [`Self::flush_chunk`] / [`Self::finish`] at safe boundaries — this writer
/// performs filesystem I/O and is not itself the Unity hot path.
pub struct SessionWriter {
    file: BufWriter<File>,
    header: FileHeader,
    dictionary: Dictionary,
    pending_dict: Vec<DictEntry>,
    pending_events: Vec<EventRecord>,
    next_chunk_seq: u64,
    next_event_seq: u64,
    finished: bool,
}

impl SessionWriter {
    /// Create a new session file and write the fixed 128-byte [`FileHeader`].
    pub fn create(path: impl AsRef<Path>, header: FileHeader) -> Result<Self> {
        Self::open_new(path, header)
    }

    /// Alias for [`Self::create`] — open a brand-new writer (never appends to
    /// an existing journal; DTJ sessions are one capture window per file).
    pub fn open_new(path: impl AsRef<Path>, header: FileHeader) -> Result<Self> {
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(&header.encode())?;
        file.flush()?;
        Ok(Self {
            file,
            header,
            dictionary: Dictionary::new(),
            pending_dict: Vec::new(),
            pending_events: Vec::new(),
            next_chunk_seq: 1,
            next_event_seq: 1,
            finished: false,
        })
    }

    pub fn header(&self) -> &FileHeader {
        &self.header
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    pub fn intern_domain(&mut self, name: &str) -> Result<u32> {
        self.intern(DictKind::Domain, name)
    }

    pub fn intern_category(&mut self, name: &str) -> Result<u32> {
        self.intern(DictKind::Category, name)
    }

    pub fn intern_event_name(&mut self, name: &str) -> Result<u32> {
        self.intern(DictKind::EventName, name)
    }

    pub fn intern_string(&mut self, name: &str) -> Result<u32> {
        self.intern(DictKind::String, name)
    }

    fn intern(&mut self, kind: DictKind, name: &str) -> Result<u32> {
        self.ensure_open()?;
        let (id, newly) = self.dictionary.intern(kind, name)?;
        if newly {
            self.pending_dict.push(DictEntry {
                kind,
                id,
                name: name.to_string(),
            });
        }
        Ok(id)
    }

    pub fn append_event(&mut self, event: AppendEvent) -> Result<u64> {
        self.ensure_open()?;
        self.dictionary.require(DictKind::Domain, event.domain_id)?;
        self.dictionary
            .require(DictKind::Category, event.category_id)?;
        self.dictionary
            .require(DictKind::EventName, event.event_name_id)?;
        if event.correlation_id != 0 {
            self.dictionary
                .require(DictKind::String, event.correlation_id)?;
        }
        for field in &event.payload.fields {
            self.dictionary.require(DictKind::String, field.name_id)?;
            if let crate::payload::Value::InternedString(sid) = &field.value {
                self.dictionary.require(DictKind::String, *sid)?;
            }
        }

        let event_sequence = self.next_event_seq;
        self.next_event_seq += 1;
        self.pending_events.push(EventRecord {
            monotonic_ns: event.monotonic_ns,
            event_sequence,
            domain_id: event.domain_id,
            category_id: event.category_id,
            event_name_id: event.event_name_id,
            correlation_id: event.correlation_id,
            severity: event.severity,
            payload: event.payload,
        });
        if self.pending_events.len() as u32 >= MAX_EVENTS_PER_CHUNK {
            self.flush_chunk()?;
        }
        Ok(event_sequence)
    }

    /// Seal pending dictionary + event records into committed chunk(s).
    pub fn flush_chunk(&mut self) -> Result<()> {
        self.ensure_open()?;
        if !self.pending_dict.is_empty() {
            let payload = Dictionary::encode_entries(&self.pending_dict)?;
            let bytes =
                encode_committed_chunk(CHUNK_TYPE_DICTIONARY, self.next_chunk_seq, &payload)?;
            self.file.write_all(&bytes)?;
            self.next_chunk_seq += 1;
            self.pending_dict.clear();
        }
        if !self.pending_events.is_empty() {
            let payload = EventRecord::encode(&self.pending_events)?;
            let bytes = encode_committed_chunk(CHUNK_TYPE_EVENT, self.next_chunk_seq, &payload)?;
            self.file.write_all(&bytes)?;
            self.next_chunk_seq += 1;
            self.pending_events.clear();
        }
        self.file.flush()?;
        Ok(())
    }

    /// Finish the session: flush any pending data and close the file.
    pub fn finish(mut self) -> Result<()> {
        if self.finished {
            return Err(Error::SessionClosed);
        }
        self.flush_chunk()?;
        self.file.flush()?;
        self.finished = true;
        Ok(())
    }

    fn ensure_open(&self) -> Result<()> {
        if self.finished {
            Err(Error::SessionClosed)
        } else {
            Ok(())
        }
    }

    /// Returns the number of pending events not yet flushed to a chunk.
    pub fn pending_events_len(&self) -> usize {
        self.pending_events.len()
    }
}
