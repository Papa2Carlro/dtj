//! Typed payload codec (no JSON on the event path).

use crate::error::{Error, Result};
use crate::format::{
    MAX_BYTES_VALUE, TYPE_BOOL, TYPE_BYTES, TYPE_ENUM, TYPE_F32, TYPE_F64, TYPE_I32, TYPE_I64,
    TYPE_INTERNED, TYPE_U32, TYPE_U64, TYPE_VEC2_F32, TYPE_VEC3_F32,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Enum(u32),
    Vec2F32([f32; 2]),
    Vec3F32([f32; 3]),
    InternedString(u32),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name_id: u32,
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypedPayload {
    pub fields: Vec<Field>,
}

impl TypedPayload {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, name_id: u32, value: Value) {
        self.fields.push(Field { name_id, value });
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.fields.len() > u16::MAX as usize {
            return Err(Error::LimitExceeded("field_count".into()));
        }
        let mut out = Vec::new();
        out.extend_from_slice(&(self.fields.len() as u16).to_le_bytes());
        for field in &self.fields {
            out.extend_from_slice(&field.name_id.to_le_bytes());
            let (tag, body) = encode_value(&field.value)?;
            out.push(tag);
            out.push(0);
            out.extend_from_slice(&body);
        }
        Ok(out)
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < 2 {
            return Err(Error::MalformedRecord(
                "payload shorter than field_count".into(),
            ));
        }
        let field_count = u16::from_le_bytes([buf[0], buf[1]]) as usize;
        let mut offset = 2;
        let mut fields = Vec::with_capacity(field_count);
        for _ in 0..field_count {
            if offset + 6 > buf.len() {
                return Err(Error::MalformedRecord("truncated field header".into()));
            }
            let name_id = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap());
            let tag = buf[offset + 4];
            // reserved at offset+5
            offset += 6;
            let (value, consumed) = decode_value(tag, &buf[offset..])?;
            offset += consumed;
            fields.push(Field { name_id, value });
        }
        if offset != buf.len() {
            return Err(Error::MalformedRecord(format!(
                "payload has {} trailing bytes",
                buf.len() - offset
            )));
        }
        Ok(Self { fields })
    }
}

fn encode_value(value: &Value) -> Result<(u8, Vec<u8>)> {
    Ok(match value {
        Value::Bool(v) => (TYPE_BOOL, vec![u8::from(*v)]),
        Value::I32(v) => (TYPE_I32, v.to_le_bytes().to_vec()),
        Value::I64(v) => (TYPE_I64, v.to_le_bytes().to_vec()),
        Value::U32(v) => (TYPE_U32, v.to_le_bytes().to_vec()),
        Value::U64(v) => (TYPE_U64, v.to_le_bytes().to_vec()),
        Value::F32(v) => (TYPE_F32, v.to_le_bytes().to_vec()),
        Value::F64(v) => (TYPE_F64, v.to_le_bytes().to_vec()),
        Value::Enum(v) => (TYPE_ENUM, v.to_le_bytes().to_vec()),
        Value::Vec2F32([x, y]) => (TYPE_VEC2_F32, [x.to_le_bytes(), y.to_le_bytes()].concat()),
        Value::Vec3F32([x, y, z]) => (
            TYPE_VEC3_F32,
            [x.to_le_bytes(), y.to_le_bytes(), z.to_le_bytes()].concat(),
        ),
        Value::InternedString(id) => (TYPE_INTERNED, id.to_le_bytes().to_vec()),
        Value::Bytes(bytes) => {
            if bytes.len() > MAX_BYTES_VALUE as usize {
                return Err(Error::LimitExceeded("bytes value too long".into()));
            }
            let mut body = Vec::with_capacity(4 + bytes.len());
            body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            body.extend_from_slice(bytes);
            (TYPE_BYTES, body)
        }
    })
}

fn decode_value(tag: u8, buf: &[u8]) -> Result<(Value, usize)> {
    Ok(match tag {
        TYPE_BOOL => {
            if buf.len() < 1 {
                return Err(Error::MalformedRecord("bool value truncated".into()));
            }
            (Value::Bool(buf[0] != 0), 1)
        }
        TYPE_I32 => {
            if buf.len() < 4 {
                return Err(Error::MalformedRecord("i32 value truncated".into()));
            }
            (
                Value::I32(i32::from_le_bytes(buf[0..4].try_into().unwrap())),
                4,
            )
        }
        TYPE_I64 => {
            if buf.len() < 8 {
                return Err(Error::MalformedRecord("i64 value truncated".into()));
            }
            (
                Value::I64(i64::from_le_bytes(buf[0..8].try_into().unwrap())),
                8,
            )
        }
        TYPE_U32 => {
            if buf.len() < 4 {
                return Err(Error::MalformedRecord("u32 value truncated".into()));
            }
            (
                Value::U32(u32::from_le_bytes(buf[0..4].try_into().unwrap())),
                4,
            )
        }
        TYPE_U64 => {
            if buf.len() < 8 {
                return Err(Error::MalformedRecord("u64 value truncated".into()));
            }
            (
                Value::U64(u64::from_le_bytes(buf[0..8].try_into().unwrap())),
                8,
            )
        }
        TYPE_F32 => {
            if buf.len() < 4 {
                return Err(Error::MalformedRecord("f32 value truncated".into()));
            }
            (
                Value::F32(f32::from_le_bytes(buf[0..4].try_into().unwrap())),
                4,
            )
        }
        TYPE_F64 => {
            if buf.len() < 8 {
                return Err(Error::MalformedRecord("f64 value truncated".into()));
            }
            (
                Value::F64(f64::from_le_bytes(buf[0..8].try_into().unwrap())),
                8,
            )
        }
        TYPE_ENUM => {
            if buf.len() < 4 {
                return Err(Error::MalformedRecord("enum value truncated".into()));
            }
            (
                Value::Enum(u32::from_le_bytes(buf[0..4].try_into().unwrap())),
                4,
            )
        }
        TYPE_VEC2_F32 => {
            if buf.len() < 8 {
                return Err(Error::MalformedRecord("vec2_f32 value truncated".into()));
            }
            let x = f32::from_le_bytes(buf[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(buf[4..8].try_into().unwrap());
            (Value::Vec2F32([x, y]), 8)
        }
        TYPE_VEC3_F32 => {
            if buf.len() < 12 {
                return Err(Error::MalformedRecord("vec3_f32 value truncated".into()));
            }
            let x = f32::from_le_bytes(buf[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(buf[4..8].try_into().unwrap());
            let z = f32::from_le_bytes(buf[8..12].try_into().unwrap());
            (Value::Vec3F32([x, y, z]), 12)
        }
        TYPE_INTERNED => {
            if buf.len() < 4 {
                return Err(Error::MalformedRecord(
                    "interned_string value truncated".into(),
                ));
            }
            (
                Value::InternedString(u32::from_le_bytes(buf[0..4].try_into().unwrap())),
                4,
            )
        }
        TYPE_BYTES => {
            if buf.len() < 4 {
                return Err(Error::MalformedRecord(
                    "bytes value truncated (length)".into(),
                ));
            }
            let len = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
            if len > MAX_BYTES_VALUE as usize {
                return Err(Error::LimitExceeded("bytes value too long".into()));
            }
            if buf.len() < 4 + len {
                return Err(Error::MalformedRecord(
                    "bytes value truncated (data)".into(),
                ));
            }
            (Value::Bytes(buf[4..4 + len].to_vec()), 4 + len)
        }
        _ => return Err(Error::UnknownTypeTag(tag)),
    })
}
