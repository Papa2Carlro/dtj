"""Bounded native DTJ v1 byte reader (Python port of crates/dtj SessionReader).

Normative contract: Docs/guides/dtj-format-v1.md
Does not shell out to the Rust CLI. Does not parse JSONL.
Payload bytes are opaque typed data only — never paths, URLs, or executable code.
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass, field
from enum import IntEnum
from pathlib import Path
from typing import Any

# --- format constants (must match crates/dtj/src/format.rs) ---
FILE_MAGIC = b"DTJ1"
CHUNK_MAGIC = b"DTJC"
FORMAT_VERSION = 1
HEADER_SIZE = 128
CHUNK_HEADER_SIZE = 24
CHUNK_TRAILER_SIZE = 8
ENDIAN_MAGIC = 0x0102_0304
COMMITTED_MARKER = 0xD7C0_FFEE

MAX_CHUNK_PAYLOAD = 16_777_216
MAX_DICT_NAME_LEN = 1_024
MAX_DICT_ENTRIES = 65_535
MAX_EVENTS_PER_CHUNK = 65_535
MAX_EVENT_PAYLOAD = 65_535
MAX_BYTES_VALUE = 4_096

CHUNK_TYPE_DICTIONARY = 1
CHUNK_TYPE_EVENT = 2

DICT_KIND_DOMAIN = 1
DICT_KIND_CATEGORY = 2
DICT_KIND_EVENT_NAME = 3
DICT_KIND_STRING = 4

TYPE_BOOL = 0x01
TYPE_I32 = 0x02
TYPE_I64 = 0x03
TYPE_U32 = 0x04
TYPE_U64 = 0x05
TYPE_F32 = 0x06
TYPE_F64 = 0x07
TYPE_ENUM = 0x08
TYPE_VEC2_F32 = 0x09
TYPE_VEC3_F32 = 0x0A
TYPE_INTERNED = 0x0B
TYPE_BYTES = 0x0C

ADAPTER_NAME = "dtj-python"
ADAPTER_VERSION = 1

SEVERITY_NAMES = {
    0: "trace",
    1: "debug",
    2: "info",
    3: "warn",
    4: "error",
    5: "fatal",
}

DICT_KIND_NAMES = {
    DICT_KIND_DOMAIN: "domain",
    DICT_KIND_CATEGORY: "category",
    DICT_KIND_EVENT_NAME: "event_name",
    DICT_KIND_STRING: "string",
}


class DtjError(Exception):
    """Structured DTJ decode failure (fail-closed; never panic)."""

    def __init__(self, kind: str, message: str, **fields: Any) -> None:
        super().__init__(message)
        self.kind = kind
        self.message = message
        self.fields = fields

    def to_error_dict(self) -> dict[str, Any]:
        out: dict[str, Any] = {"kind": self.kind, **self.fields, "message": self.message}
        return out


class DictKind(IntEnum):
    DOMAIN = DICT_KIND_DOMAIN
    CATEGORY = DICT_KIND_CATEGORY
    EVENT_NAME = DICT_KIND_EVENT_NAME
    STRING = DICT_KIND_STRING

    @classmethod
    def from_u8(cls, v: int) -> DictKind:
        try:
            return cls(v)
        except ValueError as exc:
            raise DtjError(
                "MalformedRecord", f"malformed record: unknown dict kind {v}"
            ) from exc


@dataclass
class DictEntry:
    kind: DictKind
    id: int
    name: str


@dataclass
class Dictionary:
    by_key: dict[tuple[DictKind, int], str] = field(default_factory=dict)

    def get_name(self, kind: DictKind, id_: int) -> str | None:
        return self.by_key.get((kind, id_))

    def require(self, kind: DictKind, id_: int) -> str:
        name = self.get_name(kind, id_)
        if name is None:
            raise DtjError(
                "UnknownDictionaryId",
                f"unknown dictionary id kind={int(kind)} id={id_}",
                dict_kind=int(kind),
                id=id_,
            )
        return name

    def insert(self, entry: DictEntry) -> None:
        if entry.id == 0:
            raise DtjError(
                "MalformedRecord", "malformed record: dictionary id must be >= 1"
            )
        if len(entry.name) > MAX_DICT_NAME_LEN:
            raise DtjError(
                "LimitExceeded", "limit exceeded: dictionary name too long"
            )
        key = (entry.kind, entry.id)
        existing = self.by_key.get(key)
        if existing is not None:
            if existing != entry.name:
                raise DtjError(
                    "DuplicateDictionaryId",
                    f"dictionary id conflict kind={int(entry.kind)} id={entry.id}",
                    dict_kind=int(entry.kind),
                    id=entry.id,
                )
            return
        self.by_key[key] = entry.name

    def iter_entries(self) -> list[tuple[DictKind, int, str]]:
        entries = [(k, i, n) for (k, i), n in self.by_key.items()]
        entries.sort(key=lambda t: (int(t[0]), t[1]))
        return entries


@dataclass
class FieldValue:
    type_name: str
    value: Any = None
    id: int | None = None
    hex: str | None = None


@dataclass
class Field:
    name_id: int
    value: FieldValue


@dataclass
class TypedPayload:
    fields: list[Field]


@dataclass
class EventRecord:
    monotonic_ns: int
    event_sequence: int
    domain_id: int
    category_id: int
    event_name_id: int
    correlation_id: int
    severity: int
    payload: TypedPayload


@dataclass
class FileHeader:
    format_version: int
    flags: int
    session_id: bytes
    start_utc_unix_ms: int
    mono_origin_ns: int
    producer_name: str
    producer_version: str


@dataclass
class SessionReader:
    header: FileHeader
    dictionary: Dictionary
    events: list[EventRecord]
    chunks_committed: int
    torn_tail: bool


def crc32(data: bytes) -> int:
    """CRC-32 ISO-HDLC / zlib (poly 0xEDB88320) — same as crates/dtj."""
    return zlib.crc32(data) & 0xFFFFFFFF


def _nul_terminated(raw: bytes) -> str:
    end = raw.find(b"\x00")
    if end < 0:
        end = len(raw)
    return raw[:end].decode("utf-8", errors="replace")


def _hex_bytes(data: bytes) -> str:
    return data.hex()


def decode_header(buf: bytes) -> FileHeader:
    if len(buf) < HEADER_SIZE:
        raise DtjError(
            "MalformedRecord", "malformed record: file shorter than header"
        )
    if buf[0:4] != FILE_MAGIC:
        raise DtjError("InvalidMagic", "invalid DTJ file magic")
    format_version = struct.unpack_from("<H", buf, 4)[0]
    if format_version != FORMAT_VERSION:
        raise DtjError(
            "UnsupportedVersion",
            f"unsupported DTJ format_version {format_version}",
            format_version=format_version,
        )
    header_size = struct.unpack_from("<H", buf, 6)[0]
    if header_size != HEADER_SIZE:
        raise DtjError(
            "InvalidHeaderSize",
            f"invalid header_size {header_size}",
            header_size=header_size,
        )
    if struct.unpack_from("<I", buf, 8)[0] != ENDIAN_MAGIC:
        raise DtjError(
            "InvalidEndian",
            "invalid endian_magic (expected LE 0x01020304)",
        )
    flags = struct.unpack_from("<I", buf, 12)[0]
    session_id = bytes(buf[16:32])
    start_utc_unix_ms = struct.unpack_from("<q", buf, 32)[0]
    mono_origin_ns = struct.unpack_from("<Q", buf, 40)[0]
    producer_name = _nul_terminated(buf[48:80])
    producer_version = _nul_terminated(buf[80:96])
    return FileHeader(
        format_version=format_version,
        flags=flags,
        session_id=session_id,
        start_utc_unix_ms=start_utc_unix_ms,
        mono_origin_ns=mono_origin_ns,
        producer_name=producer_name,
        producer_version=producer_version,
    )


def _decode_dict_entries(buf: bytes) -> list[DictEntry]:
    if len(buf) < 4:
        raise DtjError(
            "MalformedRecord", "malformed record: dictionary payload too short"
        )
    count = struct.unpack_from("<I", buf, 0)[0]
    if count > MAX_DICT_ENTRIES:
        raise DtjError(
            "LimitExceeded", "limit exceeded: too many dictionary entries"
        )
    offset = 4
    entries: list[DictEntry] = []
    for _ in range(count):
        if offset + 10 > len(buf):
            raise DtjError(
                "MalformedRecord", "malformed record: truncated dictionary entry"
            )
        kind = DictKind.from_u8(buf[offset])
        id_ = struct.unpack_from("<I", buf, offset + 4)[0]
        name_len = struct.unpack_from("<H", buf, offset + 8)[0]
        offset += 10
        if name_len > MAX_DICT_NAME_LEN:
            raise DtjError(
                "LimitExceeded", "limit exceeded: dictionary name too long"
            )
        if offset + name_len > len(buf):
            raise DtjError(
                "MalformedRecord", "malformed record: dictionary name truncated"
            )
        try:
            name = buf[offset : offset + name_len].decode("utf-8")
        except UnicodeDecodeError as exc:
            raise DtjError(
                "MalformedRecord", "malformed record: dictionary name not UTF-8"
            ) from exc
        offset += name_len
        entries.append(DictEntry(kind=kind, id=id_, name=name))
    if offset != len(buf):
        raise DtjError(
            "MalformedRecord",
            "malformed record: dictionary payload has trailing bytes",
        )
    return entries


def _decode_value(tag: int, buf: bytes) -> tuple[FieldValue, int]:
    if tag == TYPE_BOOL:
        if not buf:
            raise DtjError("MalformedRecord", "malformed record: bool truncated")
        if buf[0] == 0:
            return FieldValue("bool", False), 1
        if buf[0] == 1:
            return FieldValue("bool", True), 1
        raise DtjError("MalformedRecord", "malformed record: bool not 0/1")
    if tag == TYPE_I32:
        if len(buf) < 4:
            raise DtjError("MalformedRecord", "malformed record: i32 truncated")
        return FieldValue("i32", struct.unpack_from("<i", buf, 0)[0]), 4
    if tag == TYPE_I64:
        if len(buf) < 8:
            raise DtjError("MalformedRecord", "malformed record: i64 truncated")
        return FieldValue("i64", struct.unpack_from("<q", buf, 0)[0]), 8
    if tag == TYPE_U32:
        if len(buf) < 4:
            raise DtjError("MalformedRecord", "malformed record: u32 truncated")
        return FieldValue("u32", struct.unpack_from("<I", buf, 0)[0]), 4
    if tag == TYPE_U64:
        if len(buf) < 8:
            raise DtjError("MalformedRecord", "malformed record: u64 truncated")
        return FieldValue("u64", struct.unpack_from("<Q", buf, 0)[0]), 8
    if tag == TYPE_F32:
        if len(buf) < 4:
            raise DtjError("MalformedRecord", "malformed record: f32 truncated")
        return FieldValue("f32", struct.unpack_from("<f", buf, 0)[0]), 4
    if tag == TYPE_F64:
        if len(buf) < 8:
            raise DtjError("MalformedRecord", "malformed record: f64 truncated")
        return FieldValue("f64", struct.unpack_from("<d", buf, 0)[0]), 8
    if tag == TYPE_ENUM:
        if len(buf) < 4:
            raise DtjError("MalformedRecord", "malformed record: enum truncated")
        return FieldValue("enum", struct.unpack_from("<I", buf, 0)[0]), 4
    if tag == TYPE_VEC2_F32:
        if len(buf) < 8:
            raise DtjError("MalformedRecord", "malformed record: vec2 truncated")
        x, y = struct.unpack_from("<ff", buf, 0)
        return FieldValue("vec2_f32", [x, y]), 8
    if tag == TYPE_VEC3_F32:
        if len(buf) < 12:
            raise DtjError("MalformedRecord", "malformed record: vec3 truncated")
        x, y, z = struct.unpack_from("<fff", buf, 0)
        return FieldValue("vec3_f32", [x, y, z]), 12
    if tag == TYPE_INTERNED:
        if len(buf) < 4:
            raise DtjError(
                "MalformedRecord", "malformed record: interned truncated"
            )
        sid = struct.unpack_from("<I", buf, 0)[0]
        return FieldValue("interned_string", id=sid), 4
    if tag == TYPE_BYTES:
        if len(buf) < 2:
            raise DtjError(
                "MalformedRecord", "malformed record: bytes len truncated"
            )
        length = struct.unpack_from("<H", buf, 0)[0]
        if length > MAX_BYTES_VALUE:
            raise DtjError(
                "LimitExceeded", "limit exceeded: Bytes value > 4096"
            )
        if len(buf) < 2 + length:
            raise DtjError("MalformedRecord", "malformed record: bytes truncated")
        raw = bytes(buf[2 : 2 + length])
        return FieldValue("bytes", hex=_hex_bytes(raw)), 2 + length
    raise DtjError(
        "UnknownTypeTag",
        f"unknown typed payload tag 0x{tag:02X}",
        tag=tag,
    )


def _decode_payload(buf: bytes) -> TypedPayload:
    if len(buf) < 2:
        raise DtjError(
            "MalformedRecord",
            "malformed record: payload shorter than field_count",
        )
    field_count = struct.unpack_from("<H", buf, 0)[0]
    offset = 2
    fields: list[Field] = []
    for _ in range(field_count):
        if offset + 6 > len(buf):
            raise DtjError(
                "MalformedRecord", "malformed record: truncated field header"
            )
        name_id = struct.unpack_from("<I", buf, offset)[0]
        tag = buf[offset + 4]
        offset += 6
        value, consumed = _decode_value(tag, buf[offset:])
        offset += consumed
        fields.append(Field(name_id=name_id, value=value))
    if offset != len(buf):
        raise DtjError(
            "MalformedRecord",
            f"malformed record: payload has {len(buf) - offset} trailing bytes",
        )
    return TypedPayload(fields=fields)


def _decode_events(buf: bytes) -> list[EventRecord]:
    if len(buf) < 4:
        raise DtjError(
            "MalformedRecord", "malformed record: event chunk too short"
        )
    count = struct.unpack_from("<I", buf, 0)[0]
    if count > MAX_EVENTS_PER_CHUNK:
        raise DtjError(
            "LimitExceeded", "limit exceeded: too many events in Event chunk"
        )
    offset = 4
    events: list[EventRecord] = []
    for _ in range(count):
        if offset + 40 > len(buf):
            raise DtjError(
                "MalformedRecord", "malformed record: truncated event header"
            )
        monotonic_ns = struct.unpack_from("<Q", buf, offset)[0]
        event_sequence = struct.unpack_from("<Q", buf, offset + 8)[0]
        domain_id = struct.unpack_from("<I", buf, offset + 16)[0]
        category_id = struct.unpack_from("<I", buf, offset + 20)[0]
        event_name_id = struct.unpack_from("<I", buf, offset + 24)[0]
        correlation_id = struct.unpack_from("<I", buf, offset + 28)[0]
        severity_byte = buf[offset + 32]
        if severity_byte not in SEVERITY_NAMES:
            raise DtjError(
                "InvalidSeverity",
                f"invalid severity {severity_byte}",
                severity=severity_byte,
            )
        payload_len = struct.unpack_from("<I", buf, offset + 36)[0]
        offset += 40
        if payload_len > MAX_EVENT_PAYLOAD:
            raise DtjError(
                "LimitExceeded", "limit exceeded: event payload too large"
            )
        if offset + payload_len > len(buf):
            raise DtjError(
                "MalformedRecord", "malformed record: event payload truncated"
            )
        payload = _decode_payload(buf[offset : offset + payload_len])
        offset += payload_len
        events.append(
            EventRecord(
                monotonic_ns=monotonic_ns,
                event_sequence=event_sequence,
                domain_id=domain_id,
                category_id=category_id,
                event_name_id=event_name_id,
                correlation_id=correlation_id,
                severity=severity_byte,
                payload=payload,
            )
        )
    if offset != len(buf):
        raise DtjError(
            "MalformedRecord",
            "malformed record: event chunk has trailing bytes",
        )
    return events


def _validate_event_refs(dictionary: Dictionary, ev: EventRecord) -> None:
    dictionary.require(DictKind.DOMAIN, ev.domain_id)
    dictionary.require(DictKind.CATEGORY, ev.category_id)
    dictionary.require(DictKind.EVENT_NAME, ev.event_name_id)
    if ev.correlation_id != 0:
        dictionary.require(DictKind.STRING, ev.correlation_id)
    for fld in ev.payload.fields:
        dictionary.require(DictKind.STRING, fld.name_id)
        if fld.value.type_name == "interned_string" and fld.value.id is not None:
            dictionary.require(DictKind.STRING, fld.value.id)


def open_session(path: str | Path) -> SessionReader:
    """Open a `.dtj` file and recover committed chunks (torn-tail aware)."""
    p = Path(path)
    try:
        data = p.read_bytes()
    except OSError as exc:
        raise DtjError("Io", f"I/O error: {exc}") from exc

    if len(data) < HEADER_SIZE:
        raise DtjError(
            "MalformedRecord", "malformed record: file shorter than header"
        )
    header = decode_header(data[:HEADER_SIZE])

    dictionary = Dictionary()
    events: list[EventRecord] = []
    offset = HEADER_SIZE
    expected_chunk_seq = 1
    expected_event_seq = 1
    torn_tail = False
    file_len = len(data)

    while file_len - offset >= CHUNK_HEADER_SIZE + CHUNK_TRAILER_SIZE:
        hdr = data[offset : offset + CHUNK_HEADER_SIZE]
        if hdr[0:4] != CHUNK_MAGIC:
            if offset == HEADER_SIZE:
                raise DtjError("InvalidChunkMagic", "invalid chunk magic")
            torn_tail = True
            break

        chunk_type = struct.unpack_from("<H", hdr, 4)[0]
        sequence = struct.unpack_from("<Q", hdr, 8)[0]
        payload_len = struct.unpack_from("<I", hdr, 16)[0]

        # Physical completeness before semantic MAX (torn oversized trailing header).
        need = CHUNK_HEADER_SIZE + payload_len + CHUNK_TRAILER_SIZE
        if need < 0 or need != CHUNK_HEADER_SIZE + payload_len + CHUNK_TRAILER_SIZE:
            # Python int can't overflow; still guard absurd sizes via remaining.
            pass
        remaining = file_len - offset
        if need > remaining:
            torn_tail = True
            break

        if payload_len > MAX_CHUNK_PAYLOAD:
            raise DtjError(
                "PayloadTooLarge",
                f"payload_len {payload_len} exceeds max {MAX_CHUNK_PAYLOAD}",
                len=payload_len,
                max=MAX_CHUNK_PAYLOAD,
            )

        payload_start = offset + CHUNK_HEADER_SIZE
        payload_end = payload_start + payload_len
        payload = data[payload_start:payload_end]
        trailer = data[payload_end : payload_end + CHUNK_TRAILER_SIZE]
        checksum = struct.unpack_from("<I", trailer, 0)[0]
        committed = struct.unpack_from("<I", trailer, 4)[0]

        if committed != COMMITTED_MARKER:
            torn_tail = True
            break

        expected_crc = crc32(payload)
        if checksum != expected_crc:
            raise DtjError(
                "ChecksumMismatch",
                f"checksum mismatch for chunk sequence {sequence}",
                sequence=sequence,
            )

        if sequence != expected_chunk_seq:
            raise DtjError(
                "SequenceGap",
                f"sequence gap: expected {expected_chunk_seq}, found {sequence}",
                expected=expected_chunk_seq,
                found=sequence,
            )
        expected_chunk_seq += 1

        if chunk_type == CHUNK_TYPE_DICTIONARY:
            for entry in _decode_dict_entries(payload):
                dictionary.insert(entry)
        elif chunk_type == CHUNK_TYPE_EVENT:
            decoded = _decode_events(payload)
            for ev in decoded:
                if ev.event_sequence != expected_event_seq:
                    raise DtjError(
                        "SequenceGap",
                        f"sequence gap: expected {expected_event_seq}, "
                        f"found {ev.event_sequence}",
                        expected=expected_event_seq,
                        found=ev.event_sequence,
                    )
                expected_event_seq += 1
                _validate_event_refs(dictionary, ev)
            events.extend(decoded)
        # else: unknown/reserved committed chunk types skipped after CRC (§1.1)

        offset += need

    if file_len > offset and not torn_tail:
        torn_tail = True

    return SessionReader(
        header=header,
        dictionary=dictionary,
        events=events,
        chunks_committed=expected_chunk_seq - 1,
        torn_tail=torn_tail,
    )


def _json_float(v: float) -> float | None:
    if v != v or v in (float("inf"), float("-inf")):  # NaN / Inf
        return None
    return float(v)


def _field_to_json(dictionary: Dictionary, fld: Field) -> dict[str, Any]:
    name = dictionary.get_name(DictKind.STRING, fld.name_id)
    out: dict[str, Any] = {
        "name_id": fld.name_id,
        "name": name,
        "type": fld.value.type_name,
    }
    vt = fld.value.type_name
    if vt == "bytes":
        out["hex"] = fld.value.hex
    elif vt == "interned_string":
        out["id"] = fld.value.id
        out["value"] = (
            dictionary.get_name(DictKind.STRING, fld.value.id)
            if fld.value.id is not None
            else None
        )
    elif vt in ("f32", "f64"):
        out["value"] = _json_float(float(fld.value.value))
    elif vt == "vec2_f32":
        x, y = fld.value.value
        out["value"] = [_json_float(float(x)), _json_float(float(y))]
    elif vt == "vec3_f32":
        x, y, z = fld.value.value
        out["value"] = [
            _json_float(float(x)),
            _json_float(float(y)),
            _json_float(float(z)),
        ]
    else:
        out["value"] = fld.value.value
    return out


def event_to_json(dictionary: Dictionary, ev: EventRecord) -> dict[str, Any]:
    corr = None
    if ev.correlation_id != 0:
        corr = dictionary.get_name(DictKind.STRING, ev.correlation_id)
    return {
        "monotonic_ns": ev.monotonic_ns,
        "event_sequence": ev.event_sequence,
        "domain_id": ev.domain_id,
        "domain": dictionary.get_name(DictKind.DOMAIN, ev.domain_id),
        "category_id": ev.category_id,
        "category": dictionary.get_name(DictKind.CATEGORY, ev.category_id),
        "event_name_id": ev.event_name_id,
        "event_name": dictionary.get_name(DictKind.EVENT_NAME, ev.event_name_id),
        "correlation_id": ev.correlation_id,
        "correlation": corr,
        "severity": SEVERITY_NAMES[ev.severity],
        "payload": [_field_to_json(dictionary, f) for f in ev.payload.fields],
    }


def session_to_projection(
    session_path: str, reader: SessionReader
) -> dict[str, Any]:
    header = reader.header
    dictionary = reader.dictionary
    return {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_path,
        "header": {
            "format_version": header.format_version,
            "flags": header.flags,
            "session_id_hex": _hex_bytes(header.session_id),
            "start_utc_unix_ms": header.start_utc_unix_ms,
            "mono_origin_ns": header.mono_origin_ns,
            "producer_name": header.producer_name,
            "producer_version": header.producer_version,
        },
        "chunks_committed": reader.chunks_committed,
        "torn_tail": reader.torn_tail,
        "event_count": len(reader.events),
        "dictionary": [
            {
                "kind": DICT_KIND_NAMES[int(kind)],
                "id": id_,
                "name": name,
            }
            for kind, id_, name in dictionary.iter_entries()
        ],
        "events": [event_to_json(dictionary, ev) for ev in reader.events],
    }


def error_projection(session_path: str, err: DtjError) -> dict[str, Any]:
    return {
        "ok": False,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_path,
        "error": err.to_error_dict(),
    }
