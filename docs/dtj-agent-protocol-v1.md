# DTJ Agent Protocol v1

## Overview
Binary, length‑prefixed frame protocol over a Unix domain socket.  
The agent is the **sole writer** of `.dtj` files; clients never serialize DTJ bytes.

## Frame Layout
```
+----------------+----------------+-------------------+
| 4 bytes (LE)   | 1 byte         | N bytes           |
| frame_length   | opcode         | payload           |
+----------------+----------------+-------------------+
```
`frame_length` includes the opcode byte (i.e. `1 + payload.len()`).

## Opcodes
| Direction | Name | Opcode | Payload |
|-----------|------|--------|---------|
| C→S | Hello | 0x01 | `protocol_version` (u32 LE) |
| S→C | HelloOk | 0x81 | `protocol_version` (u32 LE) |
| C→S | OpenSession | 0x02 | 128‑byte `FileHeader` + UTF‑8 file name (NUL‑terminated optional) |
| S→C | OpenSessionOk | 0x82 | empty |
| C→S | Intern | 0x06 | `dict_kind` (u8) + `name_len` (u16 LE) + `name` (UTF‑8 bytes) |
| S→C | InternOk | 0x86 | `dictionary_id` (u32 LE) |
| C→S | AppendEvent | 0x03 | see below |
| S→C | AppendEventOk | 0x83 | `event_sequence` (u64 LE) |
| C→S | FinishSession | 0x04 | empty |
| S→C | FinishSessionOk | 0x84 | empty |
| C→S | Ping | 0x05 | empty |
| S→C | Pong | 0x85 | empty |
| S→C | Error | 0xFF | UTF‑8 error message |

## Intern Payload
```
dict_kind         u8   (1=Domain, 2=Category, 3=EventName, 4=String)
name_len          u16 LE
name              UTF‑8 bytes (name_len bytes, max 1024, no NUL inside)
```

## OpenSession Payload
```
FileHeader        128 bytes (exact)
file_name         UTF‑8, NUL‑terminated optional
```
The agent is started with `--data-dir <dir>` which defines the root directory for session files. The `file_name` in OpenSession is a simple file name (no path separators, no `..`, not absolute). The agent writes the session file to `<data-dir>/<file_name>`.

## AppendEvent Payload (MVP)
```
monotonic_ns      u64 LE
domain_id         u32 LE
category_id       u32 LE
event_name_id     u32 LE
correlation_id    u32 LE
severity          u8  (matches dtj::Severity)
field_count       u16 LE   (must be 1 in MVP)
field:
  name_id         u32 LE
  type_tag        u8
  reserved        u8[3]   (zero)
  value_body      variable (see type tags)
```

### Supported Type Tags (match `dtj::Value`)
| Tag | Name | Value Body |
|-----|------|------------|
| 0x01 | BOOL | 1 byte (0/1) |
| 0x02 | I32 | 4 bytes LE |
| 0x03 | I64 | 8 bytes LE |
| 0x04 | U32 | 4 bytes LE |
| 0x05 | U64 | 8 bytes LE |
| 0x06 | F32 | 4 bytes LE (IEEE‑754) |
| 0x07 | F64 | 8 bytes LE (IEEE‑754) |
| 0x08 | ENUM | 4 bytes LE (opaque) |
| 0x09 | VEC2_F32 | 8 bytes (two F32 LE) |
| 0x0A | VEC3_F32 | 12 bytes (three F32 LE) |
| 0x0B | INTERNED | 4 bytes LE (dictionary id) |
| 0x0C | BYTES | 4 bytes LE length + N bytes data |

## Lifecycle
1. Client connects.
2. Client sends **Hello** with desired protocol version.
3. Server replies **HelloOk** (same version) or **Error** (unsupported).
4. Client sends **OpenSession** with a 128‑byte `FileHeader` and file name.
5. Server creates `SessionWriter` and replies **OpenSessionOk**.
6. Zero or more **Intern** / **InternOk** exchanges to populate dictionary.
7. Zero or more **AppendEvent** / **AppendEventOk** exchanges.
8. Client sends **FinishSession**, server flushes and replies **FinishSessionOk**.
9. Either side may send **Ping**/**Pong** at any time after Hello.

## Error Handling
* All malformed frames, unknown opcodes, or protocol violations produce an **Error** frame; the agent never panics.
* After an Error the connection stays open; client may retry or close.
* If the transport frame is physically truncated and the peer closes the socket before the frame is complete, the agent cleanly terminates the connection without sending a response (the full request was never received).

## MVP Limitations
* **Single client, single session** – the agent accepts one connection, serves one session, then exits.
* **One field per AppendEvent** – `field_count` must be `1`.
* No authentication, TLS, or access control (local Unix socket only).
* No back‑pressure or flow control beyond TCP‑style socket buffers.
* Protocol version is fixed at `1`; future versions will add a version negotiation step.

## Versioning
* The first 4 bytes of Hello are the protocol version.
* Incompatible versions → immediate Error, connection may be closed.
* Future versions must keep the frame layout and Hello/HelloOk semantics.

## Security
* Only local Unix domain socket; file system permissions control access.
* No credentials transmitted.
* Agent restricts session files to `--data-dir` root; path traversal attempts are rejected.

## References
* DTJ v1 byte format: `specs/dtj-format-v1.md`
* Rust core API: `crates/dtj/src/lib.rs`