"""Unit tests for protocol encoding/decoding."""

import struct
import unittest
from dtj_sdk.protocol import (
    PROTOCOL_VERSION,
    Cmd, Resp, DictKind, SEVERITY_MAP, TypeTag,
    encode_frame, decode_frame,
    encode_hello, decode_hello_ok,
    encode_open_session, encode_intern, decode_intern_ok,
    encode_append_event, decode_append_event_ok,
    encode_finish_session, encode_ping,
    decode_error,
    OpenSessionMetadata,
    encode_value,
    DTJProtocolError,
    DTJValueError,
)


class TestFrameEncoding(unittest.TestCase):
    """Test frame encode/decode."""
    
    def test_encode_decode_frame(self):
        body = b"test payload"
        frame = encode_frame(0x01, body)
        opcode, decoded_body = decode_frame(frame)
        self.assertEqual(opcode, 0x01)
        self.assertEqual(decoded_body, body)
    
    def test_frame_length_includes_opcode(self):
        body = b"x" * 100
        frame = encode_frame(0x02, body)
        length = struct.unpack("<I", frame[:4])[0]
        self.assertEqual(length, 1 + len(body))
    
    def test_decode_truncated_frame(self):
        with self.assertRaises(DTJProtocolError):
            decode_frame(b"\x05\x00\x00\x00\x01")  # length 5, but only 1 byte payload
    
    def test_decode_oversized_frame(self):
        large_frame = struct.pack("<I", 2_000_000) + b"\x01" + b"x" * 10
        with self.assertRaises(DTJProtocolError):
            decode_frame(large_frame)


class TestHello(unittest.TestCase):
    """Test Hello/HelloOk."""
    
    def test_encode_hello(self):
        frame = encode_hello()
        opcode, body = decode_frame(frame)
        self.assertEqual(opcode, Cmd.HELLO)
        self.assertEqual(body, struct.pack("<I", PROTOCOL_VERSION))
    
    def test_decode_hello_ok(self):
        body = struct.pack("<I", PROTOCOL_VERSION)
        version = decode_hello_ok(body)
        self.assertEqual(version, PROTOCOL_VERSION)
    
    def test_decode_hello_ok_wrong_size(self):
        with self.assertRaises(DTJProtocolError):
            decode_hello_ok(b"short")


class TestOpenSession(unittest.TestCase):
    """Test OpenSession metadata encoding (no 128-byte header)."""
    
    def test_encode_open_session(self):
        session_id = b"\x01" * 16
        frame = encode_open_session(
            file_name="test.dtj",
            session_id=session_id,
            start_utc_unix_ms=1722470400000,
            mono_origin_ns=1234567890,
            producer_name="test-prod",
            producer_version="1.0.0",
        )
        opcode, body = decode_frame(frame)
        self.assertEqual(opcode, Cmd.OPEN_SESSION)
        
        # Verify structure: file_name_len + file_name + session_id + timestamps + producer_name_len + producer_name + producer_version_len + producer_version
        offset = 0
        file_name_len = struct.unpack("<H", body[offset:offset+2])[0]
        offset += 2
        self.assertEqual(body[offset:offset+file_name_len], b"test.dtj")
        offset += file_name_len
        self.assertEqual(body[offset:offset+16], session_id)
        offset += 16
        start_utc = struct.unpack("<q", body[offset:offset+8])[0]
        self.assertEqual(start_utc, 1722470400000)
        offset += 8
        mono = struct.unpack("<Q", body[offset:offset+8])[0]
        self.assertEqual(mono, 1234567890)
        offset += 8
        prod_name_len = struct.unpack("<H", body[offset:offset+2])[0]
        offset += 2
        self.assertEqual(body[offset:offset+prod_name_len], b"test-prod")
        offset += prod_name_len
        prod_ver_len = struct.unpack("<H", body[offset:offset+2])[0]
        offset += 2
        self.assertEqual(body[offset:offset+prod_ver_len], b"1.0.0")
    
    def test_open_session_validates_producer_name_length(self):
        with self.assertRaises(ValueError):
            encode_open_session(
                file_name="test.dtj",
                session_id=b"\x01" * 16,
                start_utc_unix_ms=0,
                mono_origin_ns=0,
                producer_name="x" * 33,  # > 32 bytes
                producer_version="1.0.0",
            )
    
    def test_open_session_validates_producer_version_length(self):
        with self.assertRaises(ValueError):
            encode_open_session(
                file_name="test.dtj",
                session_id=b"\x01" * 16,
                start_utc_unix_ms=0,
                mono_origin_ns=0,
                producer_name="test",
                producer_version="x" * 17,  # > 16 bytes
            )
    
    def test_open_session_validates_session_id_length(self):
        with self.assertRaises(ValueError):
            encode_open_session(
                file_name="test.dtj",
                session_id=b"\x01" * 15,  # wrong length
                start_utc_unix_ms=0,
                mono_origin_ns=0,
                producer_name="test",
                producer_version="1.0.0",
            )


class TestIntern(unittest.TestCase):
    """Test Intern/InternOk."""
    
    def test_encode_intern(self):
        frame = encode_intern(DictKind.DOMAIN, "test-domain")
        opcode, body = decode_frame(frame)
        self.assertEqual(opcode, Cmd.INTERN)
        self.assertEqual(body[0], DictKind.DOMAIN)
        name_len = struct.unpack("<H", body[1:3])[0]
        self.assertEqual(body[3:3+name_len], b"test-domain")
    
    def test_decode_intern_ok(self):
        body = struct.pack("<I", 42)
        dict_id = decode_intern_ok(body)
        self.assertEqual(dict_id, 42)
    
    def test_intern_name_too_long(self):
        with self.assertRaises(ValueError):
            encode_intern(DictKind.DOMAIN, "x" * 1025)


class TestAppendEvent(unittest.TestCase):
    """Test AppendEvent encoding."""
    
    def test_encode_append_event(self):
        frame = encode_append_event(
            monotonic_ns=1_250_000_000,
            domain_id=1,
            category_id=2,
            event_name_id=3,
            correlation_id=4,
            severity=1,  # info
            field_name_id=5,
            type_tag=TypeTag.F64,
            value_body=struct.pack("<d", 12.5),
        )
        opcode, body = decode_frame(frame)
        self.assertEqual(opcode, Cmd.APPEND_EVENT)
        
        # Verify structure
        offset = 0
        self.assertEqual(struct.unpack("<Q", body[offset:offset+8])[0], 1_250_000_000)
        offset += 8
        self.assertEqual(struct.unpack("<I", body[offset:offset+4])[0], 1)
        offset += 4
        self.assertEqual(struct.unpack("<I", body[offset:offset+4])[0], 2)
        offset += 4
        self.assertEqual(struct.unpack("<I", body[offset:offset+4])[0], 3)
        offset += 4
        self.assertEqual(struct.unpack("<I", body[offset:offset+4])[0], 4)
        offset += 4
        self.assertEqual(body[offset], 1)  # severity
        offset += 1
        self.assertEqual(struct.unpack("<H", body[offset:offset+2])[0], 1)  # field_count
        offset += 2
        self.assertEqual(struct.unpack("<I", body[offset:offset+4])[0], 5)  # field_name_id
        offset += 4
        self.assertEqual(body[offset], TypeTag.F64)
        offset += 1
        self.assertEqual(body[offset:offset+3], b"\x00\x00\x00")  # reserved
        offset += 3
        self.assertEqual(body[offset:offset+8], struct.pack("<d", 12.5))
    
    def test_decode_append_event_ok(self):
        body = struct.pack("<Q", 42)
        seq = decode_append_event_ok(body)
        self.assertEqual(seq, 42)


class TestOpenSessionMetadata(unittest.TestCase):
    """Test OpenSessionMetadata helper."""
    
    def test_create_auto_generates_values(self):
        meta = OpenSessionMetadata.create(
            file_name="test.dtj",
            producer_name="test",
            producer_version="1.0",
        )
        self.assertEqual(meta.file_name, "test.dtj")
        self.assertEqual(meta.producer_name, "test")
        self.assertEqual(meta.producer_version, "1.0")
        self.assertEqual(len(meta.session_id), 16)
        self.assertGreater(meta.start_utc_unix_ms, 0)
        self.assertGreater(meta.mono_origin_ns, 0)
    
    def test_create_with_custom_values(self):
        custom_id = b"\x02" * 16
        custom_time = 1234567890000
        custom_mono = 9876543210
        meta = OpenSessionMetadata.create(
            file_name="test.dtj",
            producer_name="test",
            producer_version="1.0",
            session_id=custom_id,
            start_utc_unix_ms=custom_time,
            mono_origin_ns=custom_mono,
        )
        self.assertEqual(meta.session_id, custom_id)
        self.assertEqual(meta.start_utc_unix_ms, custom_time)
        self.assertEqual(meta.mono_origin_ns, custom_mono)


class TestEncodeValue(unittest.TestCase):
    """Test Python value to DTJ value encoding."""
    
    def test_encode_bool(self):
        tag, body = encode_value(True)
        self.assertEqual(tag, TypeTag.BOOL)
        self.assertEqual(body, b"\x01")
        
        tag, body = encode_value(False)
        self.assertEqual(tag, TypeTag.BOOL)
        self.assertEqual(body, b"\x00")
    
    def test_encode_int_i64(self):
        tag, body = encode_value(42)
        self.assertEqual(tag, TypeTag.I64)
        self.assertEqual(struct.unpack("<q", body)[0], 42)
        
        tag, body = encode_value(-100)
        self.assertEqual(tag, TypeTag.I64)
        self.assertEqual(struct.unpack("<q", body)[0], -100)
    
    def test_encode_int_u64(self):
        tag, body = encode_value(1 << 63)  # too large for i64
        self.assertEqual(tag, TypeTag.U64)
        self.assertEqual(struct.unpack("<Q", body)[0], 1 << 63)
    
    def test_encode_float(self):
        tag, body = encode_value(3.14)
        self.assertEqual(tag, TypeTag.F64)
        self.assertAlmostEqual(struct.unpack("<d", body)[0], 3.14)
    
    def test_encode_bytes(self):
        tag, body = encode_value(b"hello")
        self.assertEqual(tag, TypeTag.BYTES)
        length = struct.unpack("<I", body[:4])[0]
        self.assertEqual(length, 5)
        self.assertEqual(body[4:], b"hello")
    
    def test_encode_string_raises(self):
        with self.assertRaises(DTJValueError):
            encode_value("hello")
    
    def test_encode_unsupported_type(self):
        with self.assertRaises(DTJValueError):
            encode_value([1, 2, 3])
    
    def test_encode_int_out_of_range(self):
        with self.assertRaises(DTJValueError):
            encode_value(1 << 64)


class TestSeverityMap(unittest.TestCase):
    """Test severity mapping."""
    
    def test_severity_values(self):
        expected = {
            "trace": 0,
            "debug": 1,
            "info": 2,
            "warn": 3,
            "error": 4,
            "fatal": 5,
        }
        self.assertEqual(SEVERITY_MAP, expected)


if __name__ == "__main__":
    unittest.main()