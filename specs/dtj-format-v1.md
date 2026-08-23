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
Chunk*                        (zero or more committed chunks)
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
| 4 | 2 | `u16` | `format_version` | `1` |
| 6 | 2 | `u16` | `header_size` | `128` |
| 8 | 4 | `u32` | `endian_magic` | Writers store `0x01020304` in LE → on-disk bytes `04 03 02 01`. Readers require exactly those four bytes |
| 12 | 4 | `u32` | `flags` | `0` in v1 |
| 16 | 16 | `[u8;16]` | `session_id` | Opaque 16 bytes (UUID recommended) |
| 32 | 8 | `i64` | `start_utc_unix_ms` | Unix epoch milliseconds (UTC) |
| 40 | 8 | `u64` | `mono_origin_ns` | Producer monotonic clock at session open |
| 48 | 32 | `[u8;32]` | `producer_name` | UTF-8, NUL-padded; first NUL or full 32 |
| 80 | 16 | `[u8;16]` | `producer_version` | UTF-8, NUL-padded |
| 96 | 32 | `[u8;32]` | `reserved` | Must be zero |

Total: **128** bytes. No variable-length region follows the header in v1.

## Chunk Framing

Each chunk is:

```
ChunkHeader (24 bytes)
payload (payload_len bytes)
```

ChunkHeader:

| Offset | Size | Type | Field |
| ---: | ---: | --- | --- |
| 0 | 4 | `u32` | `chunk_type` |
| 4 | 4 | `u32` | `payload_len` |
| 8 | 8 | `u64` | `chunk_seq` |
| 16 | 8 | `u64` | `checksum` |

Chunk types (u32):

- `0x00000001` — Dictionary
- `0x00000002` — Event
- `0x00000003` — Reserved
- `0x00000004` — Reserved
- `0x00000005` — Reserved
- `0x00000006` — Reserved
- `0x00000007` — Reserved
- `0x00000008` — Reserved

Dictionary entries use numeric indices into a shared string table.

Event records contain typed inline payloads (no external files).

## Format Versioning

- `format_version = 1` for this document
- Unknown `format_version`: readers **MUST** return an error and refuse the file
- Breaking changes require `format_version >= 2` and a new ADR

## Limits (v1)

| Limit | Value |
| --- | --- |
| Max chunk payload length | `16_777_216` bytes (16 MiB) |
| Max dictionary name length | `1_024` bytes |
| Max dictionary entries | `65_535` |
| Max events per Event chunk | `65_535` |
| Max typed payload bytes per event | `65_535` |
| Max `Bytes` value length | `4_096` |

Exceeding a limit while writing is an error. A reader that encounters a length above these limits **MUST** return a structured error (fail-closed).