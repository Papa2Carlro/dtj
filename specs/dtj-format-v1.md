# DTJ v1 — Debug Trace Journal (byte contract)

DTJ (`.dtj`) is a portable, append-only **binary** journal for high-volume debug
trace sessions. It is a capture format, not a query database and not a
Unity-specific API.

It is now an independent specification, not tied to Doc Hub paid packs.

## Goals

- Enabled events are recorded without JSON serialization, formatted strings,
  filesystem I/O, or an MCP call in the producer hot path.
- A completed journal is readable by any language that implements this byte contract.
- A crash leaves all prior **committed** chunks recoverable.
- `LosslessSession` never silently overwrites an accepted event.
- Indexes are rebuildable derivatives, not the source of truth.

## Non-goals for v1

- Remote collector, HTTP, cloud telemetry.
- Embedded database in the game process.
- Compression, encryption, replay, C ABI, native Unity plugin.
- JSONL / Wire Trace import or export.

## File Layout

```
FileHeader                    (exactly 128 bytes)
CommittedChunk*               (zero or more committed chunks)
[optional torn tail]          (ignored on recovery)
```

Chunk order is append-only. Typical order:

```
DictionaryChunk*
EventChunk*
```

## FileHeader (128 bytes)

| Offset | Size | Type | Field | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 4 | `[u8;4]` | `magic` | ASCII `DTJ1` → `0x44 0x54 0x4A 0x31` |
| 4 | 2 | `u16` | `format_version` | `1` (little-endian) |
| 6 | 2 | `u16` | `header_size` | `128` (little-endian) |
| 8 | 4 | `u32` | `endian_magic` | Writers store `0x01020304` in LE → on-disk bytes `04 03 02 01`. Readers require exactly those four bytes |
| 12 | 4 | `u32` | `flags` | `0` in v1 (little-endian) |
| 16 | 16 | `[u8;16]` | `session_id` | Opaque 16 bytes (UUID recommended) |
| 32 | 8 | `i64` | `start_utc_unix_ms` | Unix epoch milliseconds (UTC), little-endian |
| 40 | 8 | `u64` | `mono_origin_ns` | Producer monotonic clock at session open, little-endian |
| 48 | 32 | `[u8;32]` | `producer_name` | UTF-8, NUL-padded; first NUL or full 32 |
| 80 | 16 | `[u8;16]` | `producer_version` | UTF-8, NUL-padded |
| 96 | 32 | `[u8;32]` | `reserved` | Ignored (writers write 0; readers do not validate) |

Total: **128** bytes. No variable-length region follows the header in v1.

## Committed Chunk Framing

Each committed chunk is:

```
ChunkHeader (24 bytes)
payload (payload_len bytes)
ChunkTrailer (8 bytes)
```

### ChunkHeader (24 bytes)

| Offset | Size | Type | Field | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 4 | `[u8;4]` | `magic` | ASCII `DTJC` → `0x44 0x54 0x4A 0x43` |
| 4 | 2 | `u16` | `chunk_type` | Little-endian. See Chunk Types below |
| 6 | 2 | `u16` | `reserved` | **Ignored** (writers write 0; readers do not validate) |
| 8 | 8 | `u64` | `chunk_sequence` | Monotonically increasing from 1, little-endian |
| 16 | 4 | `u32` | `payload_len` | Length of payload in bytes, little-endian |
| 20 | 4 | `u32` | `reserved` | **Ignored** (writers write 0; readers do not validate) |

Total: **24** bytes.

### Chunk Types (u16)

| Value | Name | Description |
| ---: | --- | --- |
| 1 | `DICTIONARY` | Dictionary entries chunk |
| 2 | `EVENT` | Event records chunk |
| 3 | `PAYLOAD_RESERVED` | Reserved; reference writers must not emit. Readers **skip after CRC verification** (do not reject). |
| 4 | `FOOTER_RESERVED` | Reserved; reference writers must not emit. Readers **skip after CRC verification** (do not reject). |
| Other | — | Any other `chunk_type` value: readers **skip after CRC verification** (do not reject). |

### ChunkTrailer (8 bytes)

| Offset | Size | Type | Field | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 4 | `u32` | `crc32` | CRC-32 (ISO-HDLC, polynomial 0x04C11DB7, init 0xFFFFFFFF, reflect in/out, xorout 0xFFFFFFFF) of payload bytes only, little-endian |
| 4 | 4 | `u32` | `committed_marker` | Must equal `0xD7C0FFEE` (little-endian) |

Total: **8** bytes.

A chunk is **committed** iff the trailer is fully present and the CRC matches and the committed marker equals `0xD7C0FFEE`. Readers must verify both before accepting the chunk.

## Dictionary Chunk Payload

Dictionary chunks (`chunk_type = 1`) contain a sequence of dictionary entries.

```
u32 entry_count (little-endian)
entry_1
entry_2
...
entry_N
```

Each entry:

| Offset | Size | Type | Field | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 1 | `u8` | `kind` | See Dictionary Kinds below |
| 1 | 4 | `u32` | `id` | Little-endian, must be ≥ 1 |
| 5 | 2 | `u16` | `name_len` | Length of name in bytes, little-endian |
| 7 | `name_len` | `bytes` | `name` | UTF-8 encoded string |

### Dictionary Kinds (u8)

| Value | Name | Description |
| ---: | --- | --- |
| 1 | `DOMAIN` | Domain namespace |
| 2 | `CATEGORY` | Category within a domain |
| 3 | `EVENT_NAME` | Event name within a category |
| 4 | `STRING` | General string (field names, correlation strings) |

Entries are written in the order they are interned. Readers must accept any order but must reject duplicate `(kind, id)` pairs.

## Event Chunk Payload

Event chunks (`chunk_type = 2`) contain a sequence of event records.

```
u32 event_count (little-endian)
event_record_1
event_record_2
...
event_record_N
```

### Event Record Header (40 bytes)

Each event record begins with a fixed 40-byte header followed by a variable-length typed payload.

| Offset | Size | Type | Field | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 8 | `u64` | `monotonic_ns` | Monotonic timestamp in nanoseconds since session `mono_origin_ns`, little-endian |
| 8 | 8 | `u64` | `event_sequence` | Global event sequence number starting at 1, little-endian |
| 16 | 4 | `u32` | `domain_id` | Dictionary ID (kind=DOMAIN), little-endian |
| 20 | 4 | `u32` | `category_id` | Dictionary ID (kind=CATEGORY), little-endian |
| 24 | 4 | `u32` | `event_name_id` | Dictionary ID (kind=EVENT_NAME), little-endian |
| 28 | 4 | `u32` | `correlation_id` | Dictionary ID (kind=STRING), 0 = none, little-endian |
| 32 | 1 | `u8` | `severity` | See Severity Values below |
| 33 | 1 | `u8` | `flags` | **Ignored** (writers write 0; readers do not validate) |
| 34 | 2 | `u16` | `reserved` | **Ignored** (writers write 0; readers do not validate) |
| 36 | 4 | `u32` | `payload_len` | Length of typed payload in bytes, little-endian |

Total header: **40** bytes. Immediately followed by `payload_len` bytes of typed payload.

### Severity Values (u8)

| Value | Name |
| ---: | --- |
| 0 | `TRACE` |
| 1 | `DEBUG` |
| 2 | `INFO` |
| 3 | `WARN` |
| 4 | `ERROR` |
| 5 | `FATAL` |

Values outside 0..5 are invalid; readers must reject.

## Typed Payload

The typed payload is a sequence of fields. No JSON, no formatted strings.

```
u16 field_count (little-endian)
field_1
field_2
...
field_N
```

Each field:

| Offset | Size | Type | Field | Notes |
| ---: | ---: | --- | --- | --- |
| 0 | 4 | `u32` | `name_id` | Dictionary ID (kind=STRING), little-endian |
| 4 | 1 | `u8` | `type_tag` | See Type Tags below |
| 5 | 1 | `u8` | `reserved` | Ignored (writers write 0; readers do not validate) |
| 6 | variable | `bytes` | `value` | Type-dependent encoding (see below) |

### Type Tags (u8)

| Value | Name | Value Encoding | Dictionary Validation |
| ---: | --- | --- | --- |
| 0x01 | `BOOL` | 1 byte: 0 = false, 1 = true | — |
| 0x02 | `I32` | 4 bytes, little-endian | — |
| 0x03 | `I64` | 8 bytes, little-endian | — |
| 0x04 | `U32` | 4 bytes, little-endian | — |
| 0x05 | `U64` | 8 bytes, little-endian | — |
| 0x06 | `F32` | 4 bytes, IEEE 754 little-endian | — |
| 0x07 | `F64` | 8 bytes, IEEE 754 little-endian | — |
| 0x08 | `ENUM` | 4 bytes, little-endian | **None** (opaque u32, not a dictionary reference) |
| 0x09 | `VEC2_F32` | 8 bytes: two F32 little-endian | — |
| 0x0A | `VEC3_F32` | 12 bytes: three F32 little-endian | — |
| 0x0B | `INTERNED` | 4 bytes, little-endian (dictionary ID, kind=STRING) | **Required** — must exist in dictionary |
| 0x0C | `BYTES` | 4 bytes length (u32 LE) + N bytes data. Max length 4096. | — |

Unknown type tags: readers must reject with a structured error (fail-closed).

## CRC-32 (ISO-HDLC)

Algorithm parameters:
- Polynomial: `0x04C11DB7` (reversed `0xEDB88320`)
- Initial value: `0xFFFFFFFF`
- Reflect input: true
- Reflect output: true
- Final XOR: `0xFFFFFFFF`

This is the standard **CRC-32/ISO-HDLC** (polynomial 0x04C11DB7, init 0xFFFFFFFF, reflected, xorout 0xFFFFFFFF). The CRC is computed over the **payload bytes only** (not including the chunk header or trailer).

## Limits (v1)

| Limit | Value |
| --- | --- |
| Max chunk payload length | `16_777_216` bytes (16 MiB) |
| Max dictionary name length | `1_024` bytes |
| Max dictionary entries per chunk | `65_535` |
| Max events per Event chunk | `65_535` |
| Max typed payload bytes per event | `65_535` |
| Max `BYTES` value length | `4_096` |
| Max fields per event payload | `65_535` (u16) |
| Max producer_name length | `32` bytes (before NUL padding) |
| Max producer_version length | `16` bytes (before NUL padding) |

Exceeding a limit while writing is an error. A reader that encounters a length above these limits **MUST** return a structured error (fail-closed).

## Committed Chunk Sequencing

- `chunk_sequence` starts at 1 for the first committed chunk after the file header.
- Each committed chunk increments `chunk_sequence` by 1.
- Readers must verify that `chunk_sequence` matches the expected value. A gap or duplicate is a structured error (fail-closed).
- `event_sequence` starts at 1 for the first event in the session and increments by 1 for each event across all Event chunks. Readers must verify sequential ordering.

## Torn-Tail Recovery

A session file may end with an incomplete (torn) chunk due to crash or power loss. Readers must:

1. Read the file header.
2. Iterate chunks from offset 128.
3. For each chunk position:
   - If fewer than 24 bytes remain → stop (torn header).
   - Read 24-byte header. If magic != `DTJC`:
     - If this is the first chunk position (offset == 128) → error `InvalidChunkMagic`.
     - Otherwise → stop (torn header or trailing garbage).
   - Compute `need = 24 + payload_len + 8`. If `need > remaining_bytes` → stop (torn payload/trailer).
   - Read payload and 8-byte trailer.
   - Verify `committed_marker == 0xD7C0FFEE`. If not → stop (uncommitted/torn).
   - Verify CRC-32 of payload. If mismatch → error (fail-closed).
   - Verify `chunk_sequence` matches expected. If not → error (fail-closed).
   - Process chunk (dictionary or event).
4. Any bytes after the last committed chunk are ignored.

Readers must expose whether a torn tail was detected (e.g., `had_torn_tail()`).

## Fail-Closed Error Conditions

Readers must reject the file (return a structured error, do not silently continue) on any of:

- File header magic != `DTJ1`
- File header `format_version` != 1
- File header `header_size` != 128
- File header `endian_magic` != `0x01020304` (LE on disk)
- Chunk header magic != `DTJC` **at first chunk position (offset 128)**
- Chunk `payload_len` > `MAX_CHUNK_PAYLOAD` (16_777_216)
- Chunk trailer CRC-32 mismatch
- Chunk `chunk_sequence` gap or duplicate
- Dictionary entry `id` == 0
- Dictionary entry `name_len` > `MAX_DICT_NAME_LEN` (1024)
- Dictionary entry count > `MAX_DICT_ENTRIES` (65_535)
- Duplicate `(kind, id)` in dictionary
- Event chunk `event_count` > `MAX_EVENTS_PER_CHUNK` (65_535)
- Event record `severity` not in 0..5
- Event record `payload_len` > `MAX_EVENT_PAYLOAD` (65_535)
- Event record `domain_id`, `category_id`, `event_name_id` not found in dictionary
- Event record `correlation_id` != 0 and not found in dictionary (kind=STRING)
- Field `name_id` not found in dictionary (kind=STRING)
- Field `type_tag` unknown
- Field `INTERNED` value not found in dictionary (kind=STRING)
- Field `BYTES` length > `MAX_BYTES_VALUE` (4096)
- Field count > 65_535 (u16 max)
- Event `event_sequence` gap or duplicate
- Trailing bytes after a chunk payload/trailer or dictionary/event list

**Not fail-closed (readers ignore):**
- Reserved bytes in file header, chunk header, event header, field header
- Unknown/reserved `chunk_type` values (skipped after CRC verification)
- `ENUM` type tag values (treated as opaque u32, not validated as dictionary reference)
- Invalid `committed_marker` in chunk trailer (treated as torn/uncommitted tail; see Torn‑Tail Recovery)

## Format Versioning

- `format_version = 1` for this document
- Unknown `format_version`: readers **MUST** return an error and refuse the file
- Breaking changes require `format_version >= 2` and a new ADR

## Canonical Fixture

The file `crates/dtj/tests/fixtures/minimal_session.dtj` is the canonical minimal valid DTJ v1 session. All compliant readers must successfully read it and produce the expected event sequence.

---

*This specification documents the byte contract as implemented by the Rust reference core in `crates/dtj/src/`. The Rust core + canonical fixture define the factual DTJ v1 byte contract. This spec does not change any byte of the Rust implementation, fixture, or `format_version`.*