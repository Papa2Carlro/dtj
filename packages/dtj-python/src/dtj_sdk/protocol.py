"""Binary protocol implementation for dtj-agent communication."""

import struct
import uuid
import time
from typing import Optional
from dataclasses import dataclass

from .exceptions import DTJProtocolError, DTJValueError

# Protocol version
PROTOCOL_VERSION = 1

# Command opcodes (client -> server)
class Cmd:
    HELLO = 0x01
    OPEN_SESSION = 0x02
    APPEND_EVENT = 0x03
    FINISH_SESSION = 0x04
    PING = 0x05
    INTERN = 0x06

# Response opcodes (server -> client)
class Resp:
    HELLO_OK = 0x81
    OPEN_SESSION_OK = 0x82
    APPEND_EVENT_OK = 0x83
    FINISH_SESSION_OK = 0x84
    PONG = 0x85
    INTERN_OK = 0x86
    ERROR = 0xFF

# Dictionary kinds
class DictKind:
    DOMAIN = 1
    CATEGORY = 2
    EVENT_NAME = 3
    STRING = 4

# Severity mapping (matches dtj::Severity)
SEVERITY_MAP = {
    "debug": 0,
    "info": 1,
    "warn": 2,
    "error": 3,
    "fatal": 4,
}

# Type tags (match dtj::Value)
class TypeTag:
    BOOL = 0x01
    I32 = 0x02
    I64 = 0x03
    U32 = 0x04
    U64 = 0x05
    F32 = 0x06
    F64 = 0x07
    ENUM = 0x08
    VEC2_F32 = 0x09
    VEC3_F32 = 0x0A
    INTERNED = 0x0B
    BYTES = 0x0C

MAX_FRAME_SIZE = 1_048_576  # 1 MiB


def encode_frame(opcode: int, body: bytes) -> bytes:
    """Encode a length-prefixed frame: 4-byte LE length + opcode + body."""
    length = 1 + len(body)  # includes opcode
    if length > MAX_FRAME_SIZE:
        raise DTJProtocolError(f"Frame too large: {length} > {MAX_FRAME_SIZE}")
    return struct.pack("<I", length) + bytes([opcode]) + body


def decode_frame(data: bytes) -> tuple[int, bytes]:
    """Decode a frame, returning (opcode, body)."""
    if len(data) < 5:
        raise DTJProtocolError("Frame too short")
    length = struct.unpack("<I", data[:4])[0]
    if length > MAX_FRAME_SIZE:
        raise DTJProtocolError(f"Frame too large: {length}")
    if len(data) < 4 + length:
        raise DTJProtocolError("Frame truncated")
    opcode = data[4]
    body = data[5:5 + length - 1]
    return opcode, body


def encode_hello() -> bytes:
    """Encode Hello frame with protocol version."""
    body = struct.pack("<I", PROTOCOL_VERSION)
    return encode_frame(Cmd.HELLO, body)


def decode_hello_ok(body: bytes) -> int:
    """Decode HelloOk response, return protocol version."""
    if len(body) != 4:
        raise DTJProtocolError("HelloOk body must be 4 bytes")
    return struct.unpack("<I", body)[0]


def encode_open_session(
    file_name: str,
    session_id: bytes,
    start_utc_unix_ms: int,
    mono_origin_ns: int,
    producer_name: str,
    producer_version: str,
) -> bytes:
    """Encode OpenSession metadata payload (no 128-byte header)."""
    # Validate lengths
    if len(session_id) != 16:
        raise ValueError("session_id must be 16 bytes")
    if len(producer_name.encode("utf-8")) > 32:
        raise ValueError("producer_name must be <= 32 bytes")
    if len(producer_version.encode("utf-8")) > 16:
        raise ValueError("producer_version must be <= 16 bytes")
    
    file_name_bytes = file_name.encode("utf-8")
    producer_name_bytes = producer_name.encode("utf-8")
    producer_version_bytes = producer_version.encode("utf-8")
    
    body = bytearray()
    body.extend(struct.pack("<H", len(file_name_bytes)))
    body.extend(file_name_bytes)
    body.extend(session_id)
    body.extend(struct.pack("<q", start_utc_unix_ms))
    body.extend(struct.pack("<Q", mono_origin_ns))
    body.extend(struct.pack("<H", len(producer_name_bytes)))
    body.extend(producer_name_bytes)
    body.extend(struct.pack("<H", len(producer_version_bytes)))
    body.extend(producer_version_bytes)
    
    return encode_frame(Cmd.OPEN_SESSION, bytes(body))


def encode_intern(kind: int, name: str) -> bytes:
    """Encode Intern request."""
    name_bytes = name.encode("utf-8")
    if len(name_bytes) > 1024:
        raise ValueError("name too long (max 1024 bytes)")
    body = bytearray()
    body.append(kind)
    body.extend(struct.pack("<H", len(name_bytes)))
    body.extend(name_bytes)
    return encode_frame(Cmd.INTERN, bytes(body))


def decode_intern_ok(body: bytes) -> int:
    """Decode InternOk response, return dictionary ID."""
    if len(body) != 4:
        raise DTJProtocolError("InternOk body must be 4 bytes")
    return struct.unpack("<I", body)[0]


def encode_append_event(
    monotonic_ns: int,
    domain_id: int,
    category_id: int,
    event_name_id: int,
    correlation_id: int,
    severity: int,
    field_name_id: int,
    type_tag: int,
    value_body: bytes,
) -> bytes:
    """Encode AppendEvent with single field (MVP)."""
    body = bytearray()
    body.extend(struct.pack("<Q", monotonic_ns))
    body.extend(struct.pack("<I", domain_id))
    body.extend(struct.pack("<I", category_id))
    body.extend(struct.pack("<I", event_name_id))
    body.extend(struct.pack("<I", correlation_id))
    body.append(severity)
    body.extend(struct.pack("<H", 1))  # field_count = 1
    body.extend(struct.pack("<I", field_name_id))
    body.append(type_tag)
    body.extend(b"\x00\x00\x00")  # reserved
    body.extend(value_body)
    return encode_frame(Cmd.APPEND_EVENT, bytes(body))


def decode_append_event_ok(body: bytes) -> int:
    """Decode AppendEventOk response, return event sequence."""
    if len(body) != 8:
        raise DTJProtocolError("AppendEventOk body must be 8 bytes")
    return struct.unpack("<Q", body)[0]


def encode_finish_session() -> bytes:
    """Encode FinishSession (empty body)."""
    return encode_frame(Cmd.FINISH_SESSION, b"")


def encode_ping() -> bytes:
    """Encode Ping (empty body)."""
    return encode_frame(Cmd.PING, b"")


def decode_error(body: bytes) -> str:
    """Decode Error frame, return error message."""
    return body.decode("utf-8", errors="replace")


@dataclass
class OpenSessionMetadata:
    """Metadata for OpenSession."""
    file_name: str
    session_id: bytes
    start_utc_unix_ms: int
    mono_origin_ns: int
    producer_name: str
    producer_version: str
    
    @classmethod
    def create(
        cls,
        file_name: str,
        producer_name: str,
        producer_version: str,
        session_id: Optional[bytes] = None,
        start_utc_unix_ms: Optional[int] = None,
        mono_origin_ns: Optional[int] = None,
    ) -> "OpenSessionMetadata":
        """Create metadata with auto-generated values."""
        if session_id is None:
            session_id = uuid.uuid4().bytes
        if start_utc_unix_ms is None:
            start_utc_unix_ms = int(time.time() * 1000)
        if mono_origin_ns is None:
            mono_origin_ns = time.monotonic_ns()
        return cls(
            file_name=file_name,
            session_id=session_id,
            start_utc_unix_ms=start_utc_unix_ms,
            mono_origin_ns=mono_origin_ns,
            producer_name=producer_name,
            producer_version=producer_version,
        )


def encode_value(value) -> tuple[int, bytes]:
    """Encode a Python value to (type_tag, value_body)."""
    if isinstance(value, bool):
        return TypeTag.BOOL, struct.pack("B", 1 if value else 0)
    elif isinstance(value, int):
        # Check if fits in i64
        if -(1 << 63) <= value < (1 << 63):
            return TypeTag.I64, struct.pack("<q", value)
        elif 0 <= value < (1 << 64):
            return TypeTag.U64, struct.pack("<Q", value)
        else:
            raise DTJValueError(f"Integer out of range: {value}")
    elif isinstance(value, float):
        return TypeTag.F64, struct.pack("<d", value)
    elif isinstance(value, str):
        # Strings are interned separately, this returns INTERNED tag
        # The actual dictionary ID must be provided by caller
        raise DTJValueError("Strings must be interned first; use field_name_id from intern")
    elif isinstance(value, bytes):
        if len(value) > (1 << 32) - 1:
            raise DTJValueError("Bytes too long")
        return TypeTag.BYTES, struct.pack("<I", len(value)) + value
    else:
        raise DTJValueError(f"Unsupported value type: {type(value).__name__}")