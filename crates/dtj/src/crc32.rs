//! CRC-32 ISO-HDLC / zlib (poly 0xEDB88320).

const POLY: u32 = 0xEDB88320;

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLY;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector_empty() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn known_vector_123456789() {
        // Standard CRC-32 of "123456789"
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
