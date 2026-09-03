# dtj-rust SDK

Rust SDK for the DTJ binary protocol over Unix sockets.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
dtj_sdk = { path = "packages/dtj-rust" }
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
    data_dir: None,              // Agent's data directory (discovered or /tmp)
    producer_name: "my-app".to_string(),
    producer_version: "1.0.0".to_string(),
    agent_path: None,           // Path to dtj-agent binary (optional, uses discovery)
    socket_path: None,          // Unix socket path (discovered or discovered)
    session_file_name: None,    // Custom session file name (optional)
    enabled: true,              // Set to false to disable (no-op mode)
    warning_handler: None,      // Custom warning handler (optional)
};
```

### Config Defaults

| Field | Default |
|-------|---------|
| `data_dir` | Discovered from environment or `/tmp` |
| `producer_name` | `"unknown"` |
| `producer_version` | `"0.0.0"` |
| `agent_path` | None (uses discovery) |
| `socket_path` | Discovered from environment or `DTJ_SOCKET` |
| `session_file_name` | Auto-generated UUID |
| `enabled` | `true` |
| `warning_handler` | None (warnings logged to stderr) |

## Modes: no-op vs strict

- **no-op mode** (`enabled = false`): All operations succeed without doing anything
- **strict mode** (`enabled = true`): Connects to dtj-agent via Unix socket

```rust
// no-op mode - useful for testing or when tracing is disabled
let config = Config {
    enabled: false,
    ..Default::default()
};
let session = Session::open_strict(&config).unwrap(); // Always succeeds
```

## Discovery Order

The SDK discovers the agent socket in this order:

1. `Config.socket_path` if explicitly set
2. `DTJ_SOCKET` environment variable
3. `DTJ_DATA_DIR` environment variable + default socket name
4. `/tmp` directory + default socket name

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

- **Unix sockets only**: This SDK only works on Unix-like systems (Linux, macOS)
- Requires `dtj-agent` running and accessible via socket path
- Requires appropriate permissions to access the socket

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
