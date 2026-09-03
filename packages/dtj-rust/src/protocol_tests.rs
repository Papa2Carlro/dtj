//! Protocol unit tests using in-memory buffers (no Unix sockets).

#[cfg(test)]
mod tests {
    use crate::protocol::{
        read_append_event_ok, read_error_frame, read_finish_session_ok_or_error, read_frame,
        read_hello_ok, read_hello_ok_or_error, read_intern_ok, read_open_session_ok,
        read_open_session_ok_or_error, write_append_event, write_finish_session, write_frame,
        write_hello, write_intern, write_open_session, OpenSessionPayload, OPCODE_ERROR,
        OPCODE_FINISH_SESSION, OPCODE_FINISH_SESSION_OK, OPCODE_HELLO, OPCODE_HELLO_OK,
        OPCODE_INTERN, OPCODE_INTERN_OK, OPCODE_OPEN_SESSION, OPCODE_OPEN_SESSION_OK,
        PROTOCOL_VERSION,
    };
    use std::io::{Cursor, Read};

    // =====================================================================
    // Frame tests
    // =====================================================================

    #[test]
    fn test_frame_write_read_exact_bytes() {
        // Write frame and verify exact wire bytes
        let mut buf = Vec::new();
        write_frame(&mut buf, 0x42, b"hello").unwrap();

        // 4-byte length (little-endian) + opcode + payload
        // length = 1 (opcode) + 5 (payload) = 6
        assert_eq!(&buf[0..4], &6u32.to_le_bytes());
        assert_eq!(buf[4], 0x42);
        assert_eq!(&buf[5..], b"hello");

        // Read it back
        let mut cursor = Cursor::new(buf);
        let (opcode, payload) = read_frame(&mut cursor).unwrap();
        assert_eq!(opcode, 0x42);
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn test_frame_zero_length_rejected() {
        let mut buf = Vec::new();
        // Manually write a zero-length frame (invalid)
        buf.extend_from_slice(&0u32.to_le_bytes()); // length = 0

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_too_large_rejected() {
        let mut buf = Vec::new();
        // Write frame with length > 1 MiB
        let large_len: u32 = 1024 * 1024 + 1;
        buf.extend_from_slice(&large_len.to_le_bytes());

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor);
        assert!(result.is_err());
    }

    // =====================================================================
    // Hello tests
    // =====================================================================

    #[test]
    fn test_hello_exact_wire_format() {
        let mut buf = Vec::new();
        write_hello(&mut buf).unwrap();

        // Frame: 4-byte len + opcode + 4-byte version
        // len = 1 + 4 = 5
        assert_eq!(&buf[0..4], &5u32.to_le_bytes());
        assert_eq!(buf[4], OPCODE_HELLO);
        assert_eq!(&buf[5..9], &PROTOCOL_VERSION.to_le_bytes());
    }

    #[test]
    fn test_hello_ok_version_mismatch() {
        // Write HelloOk with wrong version
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_HELLO_OK, &2u32.to_le_bytes()).unwrap(); // version 2

        let mut cursor = Cursor::new(buf);
        let result = read_hello_ok(&mut cursor);
        assert!(result.is_err()); // BadVersion
    }

    #[test]
    fn test_hello_error_frame() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_ERROR, b"unsupported").unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_hello_ok_or_error(&mut cursor).unwrap();
        assert!(!result); // false = error received
    }

    #[test]
    fn test_hello_unknown_opcode_rejected() {
        let mut buf = Vec::new();
        write_frame(&mut buf, 0x99, &[]).unwrap(); // Unknown opcode

        let mut cursor = Cursor::new(buf);
        let result = read_hello_ok_or_error(&mut cursor);
        assert!(result.is_err());
    }

    // =====================================================================
    // OpenSession tests
    // =====================================================================

    #[test]
    fn test_open_session_payload_encoding() {
        let payload = OpenSessionPayload {
            file_name: "test.dtj".to_string(),
            session_id: [0x11u8; 16],
            start_utc_unix_ms: 0x1234567890abcdef_i64,
            mono_origin_ns: 0xFEDCBA9876543210_u64,
            producer_name: "myproducer".to_string(),
            producer_version: "2.0".to_string(),
        };

        let mut buf = Vec::new();
        write_open_session(&mut buf, &payload).unwrap();

        // Parse and verify exact bytes
        let mut cursor = Cursor::new(buf);

        // Read frame header
        let (opcode, frame_payload) = read_frame(&mut cursor).unwrap();
        assert_eq!(opcode, OPCODE_OPEN_SESSION);

        // Parse payload
        let mut p = frame_payload.as_slice();

        // file_name: u16 len + bytes
        let file_len = {
            let mut len_buf = [0u8; 2];
            p.read_exact(&mut len_buf).unwrap();
            u16::from_le_bytes(len_buf) as usize
        };
        let mut file_name_buf = vec![0u8; file_len];
        p.read_exact(&mut file_name_buf).unwrap();
        assert_eq!(String::from_utf8(file_name_buf).unwrap(), "test.dtj");

        // session_id: 16 bytes
        let mut session_id_buf = [0u8; 16];
        p.read_exact(&mut session_id_buf).unwrap();
        assert_eq!(session_id_buf, [0x11u8; 16]);

        // start_utc_unix_ms: i64
        let mut ts_buf = [0u8; 8];
        p.read_exact(&mut ts_buf).unwrap();
        assert_eq!(i64::from_le_bytes(ts_buf), payload.start_utc_unix_ms);

        // mono_origin_ns: u64
        let mut mono_buf = [0u8; 8];
        p.read_exact(&mut mono_buf).unwrap();
        assert_eq!(u64::from_le_bytes(mono_buf), payload.mono_origin_ns);

        // producer_name: u16 len + bytes
        let name_len = {
            let mut len_buf = [0u8; 2];
            p.read_exact(&mut len_buf).unwrap();
            u16::from_le_bytes(len_buf) as usize
        };
        let mut name_buf = vec![0u8; name_len];
        p.read_exact(&mut name_buf).unwrap();
        assert_eq!(String::from_utf8(name_buf).unwrap(), "myproducer");

        // producer_version: u16 len + bytes
        let ver_len = {
            let mut len_buf = [0u8; 2];
            p.read_exact(&mut len_buf).unwrap();
            u16::from_le_bytes(len_buf) as usize
        };
        let mut ver_buf = vec![0u8; ver_len];
        p.read_exact(&mut ver_buf).unwrap();
        assert_eq!(String::from_utf8(ver_buf).unwrap(), "2.0");
    }

    #[test]
    fn test_open_session_ok_empty_payload() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_OPEN_SESSION_OK, &[]).unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_open_session_ok(&mut cursor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_session_error_frame() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_ERROR, b"session failed").unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_open_session_ok_or_error(&mut cursor).unwrap();
        assert!(!result);
    }

    // =====================================================================
    // Intern tests
    // =====================================================================

    #[test]
    fn test_intern_exact_wire_format() {
        let mut buf = Vec::new();
        write_intern(&mut buf, 1, "mydomain").unwrap();

        // Parse frame
        let mut cursor = Cursor::new(buf);
        let (opcode, payload) = read_frame(&mut cursor).unwrap();
        assert_eq!(opcode, OPCODE_INTERN);
        assert_eq!(payload.len(), 1 + 2 + 8); // dict_kind + len(2) + "mydomain"

        // dict_kind = 1
        assert_eq!(payload[0], 1);
        // string len = 8 (u16 le)
        assert_eq!(&payload[1..3], &8u16.to_le_bytes());
        // string bytes
        assert_eq!(&payload[3..], b"mydomain");
    }

    #[test]
    fn test_intern_ok_response() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_INTERN_OK, &42u32.to_le_bytes()).unwrap();

        let mut cursor = Cursor::new(buf);
        let id = read_intern_ok(&mut cursor).unwrap();
        assert_eq!(id, 42);
    }

    #[test]
    fn test_intern_wrong_opcode_rejected() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_ERROR, b"error").unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_intern_ok(&mut cursor);
        assert!(result.is_err());
    }

    // =====================================================================
    // AppendEvent tests
    // =====================================================================

    #[test]
    fn test_append_event_encoding() {
        let mut buf = Vec::new();
        write_append_event(
            &mut buf,
            0x1122334455667788_u64, // monotonic_ns
            1,
            2,
            3,
            4,                             // domain, category, event_name, correlation IDs
            0x01,                          // severity: Info
            5,                             // field_name_id
            0x02,                          // type_tag
            &0xAABBCCDD_u64.to_le_bytes(), // value
        )
        .unwrap();

        let mut cursor = Cursor::new(buf);
        let (opcode, payload) = read_frame(&mut cursor).unwrap();
        assert_eq!(opcode, 0x03);

        // Verify payload layout
        let mut p = payload.as_slice();

        // monotonic_ns: 8 bytes
        let mut ts_buf = [0u8; 8];
        p.read_exact(&mut ts_buf).unwrap();
        assert_eq!(u64::from_le_bytes(ts_buf), 0x1122334455667788_u64);

        // domain_id, category_id, event_name_id, correlation_id: 4 bytes each
        let mut id_buf = [0u8; 4];
        p.read_exact(&mut id_buf).unwrap();
        assert_eq!(u32::from_le_bytes(id_buf), 1);
        p.read_exact(&mut id_buf).unwrap();
        assert_eq!(u32::from_le_bytes(id_buf), 2);
        p.read_exact(&mut id_buf).unwrap();
        assert_eq!(u32::from_le_bytes(id_buf), 3);
        p.read_exact(&mut id_buf).unwrap();
        assert_eq!(u32::from_le_bytes(id_buf), 4);

        // severity: 1 byte
        let mut sev_buf = [0u8; 1];
        p.read_exact(&mut sev_buf).unwrap();
        assert_eq!(sev_buf[0], 0x01);

        // field_count: 2 bytes (should be 1)
        let mut fc_buf = [0u8; 2];
        p.read_exact(&mut fc_buf).unwrap();
        assert_eq!(u16::from_le_bytes(fc_buf), 1);

        // field_name_id: 4 bytes
        p.read_exact(&mut id_buf).unwrap();
        assert_eq!(u32::from_le_bytes(id_buf), 5);

        // type_tag: 1 byte
        let mut tt_buf = [0u8; 1];
        p.read_exact(&mut tt_buf).unwrap();
        assert_eq!(tt_buf[0], 0x02);

        // reserved: 3 bytes
        p.read_exact(&mut &mut [0u8; 3][..]).unwrap();

        // value_body: remaining bytes
        let mut value_buf = vec![0u8; p.len()];
        p.read_exact(&mut value_buf).unwrap();
        assert_eq!(value_buf, &0xAABBCCDD_u64.to_le_bytes());
    }

    #[test]
    fn test_append_event_ok_response() {
        let mut buf = Vec::new();
        write_frame(&mut buf, 0x83, &99u64.to_le_bytes()).unwrap();

        let mut cursor = Cursor::new(buf);
        let seq = read_append_event_ok(&mut cursor).unwrap();
        assert_eq!(seq, 99);
    }

    // =====================================================================
    // FinishSession tests
    // =====================================================================

    #[test]
    fn test_finish_session_exact_wire_format() {
        let mut buf = Vec::new();
        write_finish_session(&mut buf).unwrap();

        // len = 1 (opcode) + 0 (payload) = 1
        assert_eq!(&buf[0..4], &1u32.to_le_bytes());
        assert_eq!(buf[4], OPCODE_FINISH_SESSION);
    }

    #[test]
    fn test_finish_session_ok_empty_payload() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_FINISH_SESSION_OK, &[]).unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_finish_session_ok_or_error(&mut cursor).unwrap();
        assert!(result);
    }

    #[test]
    fn test_finish_session_error_frame() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_ERROR, b"closed").unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_finish_session_ok_or_error(&mut cursor).unwrap();
        assert!(!result);
    }

    // =====================================================================
    // Error frame tests
    // =====================================================================

    #[test]
    fn test_error_frame_parsing() {
        let mut buf = Vec::new();
        write_frame(&mut buf, OPCODE_ERROR, b"something went wrong").unwrap();

        let mut cursor = Cursor::new(buf);
        let msg = read_error_frame(&mut cursor).unwrap();
        assert_eq!(msg, "something went wrong");
    }
}
