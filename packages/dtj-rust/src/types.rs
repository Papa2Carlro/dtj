/// Type tags for event values (matching dtj-agent format v1)
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

#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Int(i64),
    UInt(u64),
    F32(f32),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
}

impl Value {
    /// Returns the type tag for this value
    pub fn type_tag(&self) -> u8 {
        match self {
            Value::Bool(_) => TYPE_BOOL,
            Value::Int(_) => TYPE_I64,
            Value::UInt(_) => TYPE_U64,
            Value::F32(_) => TYPE_F32,
            Value::F64(_) => TYPE_F64,
            Value::String(_) => TYPE_INTERNED,
            Value::Bytes(_) => TYPE_BYTES,
        }
    }

    /// Encode value to bytes (without type tag)
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Value::Bool(b) => vec![if *b { 1 } else { 0 }],
            Value::Int(i) => i.to_le_bytes().to_vec(),
            Value::UInt(u) => u.to_le_bytes().to_vec(),
            Value::F32(f) => f.to_le_bytes().to_vec(),
            Value::F64(f) => f.to_le_bytes().to_vec(),
            // String values must be interned first - encode as raw bytes here
            // The intern() call will handle the actual string interning
            Value::String(s) => s.as_bytes().to_vec(),
            // Bytes need 4-byte length prefix followed by actual bytes
            Value::Bytes(b) => {
                let mut encoded = Vec::with_capacity(4 + b.len());
                encoded.extend_from_slice(&(b.len() as u32).to_le_bytes());
                encoded.extend_from_slice(b);
                encoded
            }
        }
    }
}
