use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::format::{
    DICT_KIND_CATEGORY, DICT_KIND_DOMAIN, DICT_KIND_EVENT_NAME, DICT_KIND_STRING, MAX_DICT_ENTRIES,
    MAX_DICT_NAME_LEN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DictKind {
    Domain = DICT_KIND_DOMAIN,
    Category = DICT_KIND_CATEGORY,
    EventName = DICT_KIND_EVENT_NAME,
    String = DICT_KIND_STRING,
}

impl DictKind {
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            DICT_KIND_DOMAIN => Ok(Self::Domain),
            DICT_KIND_CATEGORY => Ok(Self::Category),
            DICT_KIND_EVENT_NAME => Ok(Self::EventName),
            DICT_KIND_STRING => Ok(Self::String),
            _ => Err(Error::MalformedRecord(format!("unknown dict kind {v}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictEntry {
    pub kind: DictKind,
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Default, Clone)]
pub struct Dictionary {
    by_key: HashMap<(DictKind, u32), String>,
    by_name: HashMap<(DictKind, String), u32>,
    next_id: HashMap<DictKind, u32>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_name(&self, kind: DictKind, id: u32) -> Option<&str> {
        self.by_key.get(&(kind, id)).map(String::as_str)
    }

    /// Stable iteration of committed dictionary entries (kind, id, name), sorted by kind then id.
    pub fn iter_entries(&self) -> Vec<(DictKind, u32, &str)> {
        let mut entries: Vec<_> = self
            .by_key
            .iter()
            .map(|(&(kind, id), name)| (kind, id, name.as_str()))
            .collect();
        entries.sort_by_key(|(kind, id, _)| (*kind as u8, *id));
        entries
    }

    pub fn id_of(&self, kind: DictKind, name: &str) -> Option<u32> {
        self.by_name.get(&(kind, name.to_string())).copied()
    }

    pub fn require(&self, kind: DictKind, id: u32) -> Result<&str> {
        self.get_name(kind, id).ok_or(Error::UnknownDictionaryId {
            kind: kind as u8,
            id,
        })
    }

    /// Intern `name`, returning `(id, newly_created)`.
    pub fn intern(&mut self, kind: DictKind, name: &str) -> Result<(u32, bool)> {
        if name.len() > MAX_DICT_NAME_LEN as usize {
            return Err(Error::LimitExceeded("dictionary name too long".into()));
        }
        if let Some(id) = self.by_name.get(&(kind, name.to_string())) {
            return Ok((*id, false));
        }
        let next = self.next_id.entry(kind).or_insert(1);
        let id = *next;
        *next += 1;
        self.insert(DictEntry {
            kind,
            id,
            name: name.to_string(),
        })?;
        Ok((id, true))
    }

    pub fn insert(&mut self, entry: DictEntry) -> Result<()> {
        if entry.id == 0 {
            return Err(Error::MalformedRecord("dictionary id must be >= 1".into()));
        }
        if entry.name.len() > MAX_DICT_NAME_LEN as usize {
            return Err(Error::LimitExceeded("dictionary name too long".into()));
        }
        if self.by_key.contains_key(&(entry.kind, entry.id)) {
            return Err(Error::DuplicateDictionaryId {
                kind: entry.kind as u8,
                id: entry.id,
            });
        }
        self.by_key
            .insert((entry.kind, entry.id), entry.name.clone());
        self.by_name
            .insert((entry.kind, entry.name.clone()), entry.id);
        Ok(())
    }

    pub fn encode_entries(entries: &[DictEntry]) -> Result<Vec<u8>> {
        if entries.len() as u32 > MAX_DICT_ENTRIES {
            return Err(Error::LimitExceeded("too many dictionary entries".into()));
        }
        let mut out = Vec::new();
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for entry in entries {
            out.push(entry.kind as u8);
            out.extend_from_slice(&entry.id.to_le_bytes());
            let name_bytes = entry.name.as_bytes();
            if name_bytes.len() > MAX_DICT_NAME_LEN as usize {
                return Err(Error::LimitExceeded("dictionary name too long".into()));
            }
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(name_bytes);
        }
        Ok(out)
    }

    pub fn decode_entries(buf: &[u8]) -> Result<Vec<DictEntry>> {
        if buf.len() < 4 {
            return Err(Error::MalformedRecord(
                "dictionary chunk shorter than entry count".into(),
            ));
        }
        let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if count > MAX_DICT_ENTRIES {
            return Err(Error::LimitExceeded("too many dictionary entries".into()));
        }
        let mut offset = 4;
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            if offset + 6 > buf.len() {
                return Err(Error::MalformedRecord(
                    "truncated dictionary entry header".into(),
                ));
            }
            let kind = DictKind::from_u8(buf[offset])?;
            let id = u32::from_le_bytes(buf[offset + 1..offset + 5].try_into().unwrap());
            offset += 5;
            if offset + 2 > buf.len() {
                return Err(Error::MalformedRecord(
                    "truncated dictionary entry name length".into(),
                ));
            }
            let name_len = u16::from_le_bytes([buf[offset], buf[offset + 1]]) as usize;
            offset += 2;
            if offset + name_len > buf.len() {
                return Err(Error::MalformedRecord(
                    "truncated dictionary entry name".into(),
                ));
            }
            let name = String::from_utf8(buf[offset..offset + name_len].to_vec())
                .map_err(|_| Error::MalformedRecord("dictionary name not valid UTF-8".into()))?;
            offset += name_len;
            entries.push(DictEntry { kind, id, name });
        }
        if offset != buf.len() {
            return Err(Error::MalformedRecord(format!(
                "dictionary chunk has {} trailing bytes",
                buf.len() - offset
            )));
        }
        Ok(entries)
    }
}
