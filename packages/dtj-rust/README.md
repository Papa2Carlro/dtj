# dtj-rust SDK

Rust SDK for the DTJ binary protocol over Unix sockets.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
dtj-sdk = "0.1.0"
```

Or with Cargo edit:

```sh
cargo add dtj-sdk
```

## Quick Start

```rust
use dtj_sdk::{Config, Session, Event, Value, Severity};

fn main() {
    let config = Config::new();
    let mut session = Session::open_strict(&config).expect("failed to open session");

    let event = Event {
        domain: "my.app".to_string(),
        category: "requests".to_string(),
        name: "request_complete".to_string(),
        severity: Severity::Info,
        field_name: "status_code".to_string(),
        value: Value::Int(200),
        correlation: None,
    };

    session.emit(event).expect("failed to emit event");
    session.close().expect("failed to close session");
}
```

## Config

```rust
let config = Config {
    data_dir: None,              // None → ./traces relative to current working directory
    producer_name: "my-app".to_string(),
    producer_version: "1.0.0".to_string(),
    agent_path: None,           // Path to dtj-agent binary (optional, uses discovery)
    socket_path: None,          // Unix socket path (discovered or discovered)
    session_file_name: None,    // None → session-<unix-ms>.dtj
    enabled: true,              // Set to false to disable (no-op mode)
    warning_handler: None,      // Custom warning handler (optional)
};
```

### Config Defaults

| Field | Config value | Effective fallback |
|-------|-------------|-------------------|
| `data_dir` | `None` | `./traces` relative to current working directory |
| `producer_name` | `"dtj-rust"` | — |
| `producer_version` | `"0.1.0"` | — |
| `agent_path` | `None` | Uses discovery |
| `socket_path` | `None` | Uses discovery |
| `session_file_name` | `None` | `session-<unix-ms>.dtj` |
| `enabled` | `true` | — |
| `warning_handler` | `None` | — |

### Config Validation

`Config::validate()` checks two constraints before any discovery or socket connection:

- `producer_name` ≤ 32 bytes — returns `Error::BadLength` otherwise
- `session_file_name` must not contain `..` (path traversal) or start with `/` — returns `Error::BadName` otherwise

`Session::open_strict` calls `validate()` first and propagates errors as `Err(...)`. Use it for fail-fast configuration checking during application startup.

`Session::open` does not call `validate()` — configuration errors are silently absorbed into the disabled session.

## Session Opening

The SDK has two session constructors with distinct error semantics:

### `Session::open(config)` — graceful fallback

- Validates `config.enabled` only; does **not** validate producer name length or session file name.
- On discovery failure, connection failure, or handshake failure: calls the warning handler (if set) and returns a **disabled/no-op session**.
- The returned session is fully functional — `emit()` becomes a no-op, `close()` is a no-op.
- Use when instrumentation must **not** break the host application.

### `Session::open_strict(config)` — strict mode

- Runs `Config::validate()` **first** (producer name ≤ 32 bytes, no path traversal in session file name).
- On any failure — validation, discovery, connection, or handshake — returns `Err(...)`.
- Does **not** fall back to a disabled session.
- Use when SDK or agent failures must be **observable as errors** by the caller.

```rust
// Graceful: application continues even if agent is unavailable
let session = Session::open(&config); // always Ok(Session)

// Strict: caller must handle errors explicitly
let session = Session::open_strict(&config)?; // Err(Error) propagates

// no-op mode — events are silently discarded
let config = Config { enabled: false, ..Default::default() };
let session = Session::open(&config).unwrap(); // always succeeds
```

## Discovery Modes

The SDK has two distinct modes:

### Explicit external socket

If `Config.socket_path` is set, the SDK connects to that socket directly. The agent process is **external** — the SDK does not spawn, own, kill, or clean up the agent process or its socket directory.

### Agent discovery and auto-launch

If `Config.socket_path` is not set, the SDK searches for the `dtj-agent` binary and launches it automatically:

1. `Config.agent_path` if set
2. `DTJ_AGENT_PATH` environment variable
3. `dtj-agent` in `PATH`
4. macOS fallback paths: `/opt/homebrew/bin/dtj-agent`, `/usr/local/bin/dtj-agent`, `~/.cargo/bin/dtj-agent`

When the SDK auto-launches the agent, it creates a temporary directory for the agent's socket and data files. The SDK **owns** the agent process — `Session::close()` terminates the session and cleans up the spawned process and its temp directory. If `close()` is not called, `Drop` acts as a fallback cleanup path.

The Rust SDK never writes `.dtj` files directly; `dtj-agent` is the sole writer.

## Value Types

```rust
Value::Bool(bool)           // Type tag 0x01
Value::Int(i64)             // Type tag 0x03
Value::UInt(u64)            // Type tag 0x05
Value::F32(f32)             // Type tag 0x06
Value::F64(f64)             // Type tag 0x07
Value::String(String)       // Type tag 0x0B (interned)
Value::Bytes(Vec<u8>)       // Type tag 0x0C
```

## Severity Levels

```rust
Severity::Debug  // 0
Severity::Info  // 1
Severity::Warn  // 2
Severity::Error // 3
Severity::Fatal // 4
```

## Platform Limitations

- **Unix only**: This SDK works on Unix-like systems (Linux, macOS). Windows is not supported.
- **dtj-agent availability**: The agent binary or a Unix socket must be accessible. If `Config.socket_path` is set, the socket must exist. Otherwise, the SDK searches for `dtj-agent` and auto-launches it — see Discovery Modes above.
- **Permissions**: the process must have access to the socket path and, when launching, execute permission on the agent binary.

## Testing

```bash
# Run unit tests
cargo test --manifest-path packages/dtj-rust/Cargo.toml

# Run specific test
cargo test --manifest-path packages/dtj-rust/Cargo.toml --test vertical_slice

# Run with output
cargo test --manifest-path packages/dtj-rust/Cargo.toml -- --nocapture

# Run E2E test (requires dtj-agent)
DTJ_RUN_AGENT_E2E=1 DTJ_AGENT_PATH="$(pwd)/crates/dtj/target/debug/dtj-agent" \
    cargo test --manifest-path packages/dtj-rust/Cargo.toml --test e2e -- --nocapture
```

## Error Handling

```rust
use dtj_sdk::Error;

match session.emit(event) {
    Ok(()) => println!("Event emitted"),
    Err(Error::SessionClosed) => println!("Session was closed"),
    Err(Error::Protocol) => println!("Protocol error"),
    Err(Error::IoError) => println!("I/O error"),
    Err(Error::FrameTooLarge) => println!("Frame too large"),
}
```
