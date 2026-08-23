use crate::error::{Error, Result};
use crate::format::{ENDIAN_MAGIC, FILE_MAGIC, FORMAT_VERSION, HEADER_SIZE, HEADER_SIZE_USIZE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub format_version: u16,
    pub flags: u32,
    pub session_id: [u8; 16],
    pub start_utc_unix_ms: i64,
    pub mono_origin_ns: u64,
    pub producer_name: String,
    pub producer_version: String,
}

impl FileHeader {
    pub fn new(
        session_id: [u8; 16],
        start_utc_unix_ms: i64,
        mono_origin_ns: u64,
        producer_name: impl Into<String>,
        producer_version: impl Into<String>,
    ) -> Result<Self> {
        let producer_name = producer_name.into();
        let producer_version = producer_version.into();
        if producer_name.len() > 32 {
            return Err(Error::LimitExceeded(
                "producer_name longer than 32 bytes".into(),
            ));
        }
        if producer_version.len() > 16 {
            return Err(Error::LimitExceeded(
                "producer_version longer than 16 bytes".into(),
            ));
        }
        Ok(Self {
            format_version: FORMAT_VERSION,
            flags: 0,
            session_id,
            start_utc_unix_ms,
            mono_origin_ns,
            producer_name,
            producer_version,
        })
    }

    pub fn encode(&self) -> [u8; HEADER_SIZE_USIZE] {
        let mut buf = [0u8; HEADER_SIZE_USIZE];
        buf[0..4].copy_from_slice(FILE_MAGIC);
        buf[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        buf[6..8].copy_from_slice(&HEADER_SIZE.to_le_bytes());
        buf[8..12].copy_from_slice(&ENDIAN_MAGIC.to_le_bytes());
        buf[12..16].copy_from_slice(&self.flags.to_le_bytes());
        buf[16..32].copy_from_slice(&self.session_id);
        buf[32..40].copy_from_slice(&self.start_utc_unix_ms.to_le_bytes());
        buf[40..48].copy_from_slice(&self.mono_origin_ns.to_le_bytes());
        let name = self.producer_name.as_bytes();
        buf[48..48 + name.len()].copy_from_slice(name);
        let ver = self.producer_version.as_bytes();
        buf[80..80 + ver.len()].copy_from_slice(ver);
        buf
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < HEADER_SIZE_USIZE {
            return Err(Error::MalformedRecord("file shorter than header".into()));
        }
        if &buf[0..4] != FILE_MAGIC {
            return Err(Error::InvalidMagic);
        }
        let format_version = u16::from_le_bytes([buf[4], buf[5]]);
        if format_version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(format_version));
        }
        let header_size = u16::from_le_bytes([buf[6], buf[7]]);
        if header_size != HEADER_SIZE {
            return Err(Error::InvalidHeaderSize(header_size));
        }
        if buf[8..12] != ENDIAN_MAGIC.to_le_bytes() {
            return Err(Error::InvalidEndian);
        }
        let flags = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        let mut session_id = [0u8; 16];
        session_id.copy_from_slice(&buf[16..32]);
        let start_utc_unix_ms = i64::from_le_bytes(buf[32..40].try_into().unwrap());
        let mono_origin_ns = u64::from_le_bytes(buf[40..48].try_into().unwrap());
        let producer_name = nul_terminated(&buf[48..80]);
        let producer_version = nul_terminated(&buf[80..96]);
        Ok(Self {
            format_version,
            flags,
            session_id,
            start_utc_unix_ms,
            mono_origin_ns,
            producer_name,
            producer_version,
        })
    }
}

fn nul_terminated(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
