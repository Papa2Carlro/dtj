use crate::error::{Error, Result};
use crate::format::{MAX_EVENTS_PER_CHUNK, MAX_EVENT_PAYLOAD};
use crate::payload::TypedPayload;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Severity {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl Severity {
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Trace),
            1 => Ok(Self::Debug),
            2 => Ok(Self::Info),
            3 => Ok(Self::Warn),
            4 => Ok(Self::Error),
            5 => Ok(Self::Fatal),
            other => Err(Error::InvalidSeverity(other)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub monotonic_ns: u64,
    pub event_sequence: u64,
    pub domain_id: u32,
    pub category_id: u32,
    pub event_name_id: u32,
    pub correlation_id: u32,
    pub severity: Severity,
    pub payload: TypedPayload,
}

impl EventRecord {
    pub fn encode(events: &[EventRecord]) -> Result<Vec<u8>> {
        if events.len() as u32 > MAX_EVENTS_PER_CHUNK {
            return Err(Error::LimitExceeded(
                "too many events in Event chunk".into(),
            ));
        }
        let mut out = Vec::new();
        out.extend_from_slice(&(events.len() as u32).to_le_bytes());
        for ev in events {
            let payload = ev.payload.encode()?;
            if payload.len() as u32 > MAX_EVENT_PAYLOAD {
                return Err(Error::LimitExceeded("event payload too large".into()));
            }
            out.extend_from_slice(&ev.monotonic_ns.to_le_bytes());
            out.extend_from_slice(&ev.event_sequence.to_le_bytes());
            out.extend_from_slice(&ev.domain_id.to_le_bytes());
            out.extend_from_slice(&ev.category_id.to_le_bytes());
            out.extend_from_slice(&ev.event_name_id.to_le_bytes());
            out.extend_from_slice(&ev.correlation_id.to_le_bytes());
            out.push(ev.severity as u8);
            out.push(0); // flags
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&payload);
        }
        Ok(out)
    }

    pub fn decode_many(buf: &[u8]) -> Result<Vec<EventRecord>> {
        if buf.len() < 4 {
            return Err(Error::MalformedRecord("event chunk too short".into()));
        }
        let count = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if count > MAX_EVENTS_PER_CHUNK {
            return Err(Error::LimitExceeded(
                "too many events in Event chunk".into(),
            ));
        }
        let mut offset = 4;
        let mut events = Vec::with_capacity(count as usize);
        for _ in 0..count {
            if offset + 40 > buf.len() {
                return Err(Error::MalformedRecord("truncated event header".into()));
            }
            let monotonic_ns = read_u64(&buf[offset..offset + 8]);
            let event_sequence = read_u64(&buf[offset + 8..offset + 16]);
            let domain_id = read_u32(&buf[offset + 16..offset + 20]);
            let category_id = read_u32(&buf[offset + 20..offset + 24]);
            let event_name_id = read_u32(&buf[offset + 24..offset + 28]);
            let correlation_id = read_u32(&buf[offset + 28..offset + 32]);
            let severity = Severity::from_u8(buf[offset + 32])?;
            // flags + reserved at 33..36 ignored per §1.1
            let payload_len = read_u32(&buf[offset + 36..offset + 40]) as usize;
            offset += 40;
            if payload_len as u32 > MAX_EVENT_PAYLOAD {
                return Err(Error::LimitExceeded("event payload too large".into()));
            }
            if offset + payload_len > buf.len() {
                return Err(Error::MalformedRecord("truncated event payload".into()));
            }
            let payload = TypedPayload::decode(&buf[offset..offset + payload_len])?;
            offset += payload_len;
            events.push(EventRecord {
                monotonic_ns,
                event_sequence,
                domain_id,
                category_id,
                event_name_id,
                correlation_id,
                severity,
                payload,
            });
        }
        if offset != buf.len() {
            return Err(Error::MalformedRecord(format!(
                "event chunk has {} trailing bytes",
                buf.len() - offset
            )));
        }
        Ok(events)
    }
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().unwrap())
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}
