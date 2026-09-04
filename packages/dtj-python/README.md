# dtj-sdk — Python SDK for DTJ Agent

Python middleware client for the local `dtj-agent` binary. The SDK never writes `.dtj` bytes directly — it communicates with the agent via Unix domain socket using the binary protocol defined in `docs/dtj-agent-protocol-v1.md`.

## Install

```bash
pip install dtj-sdk
```

### Development install of dtj-agent

During development `dtj` and `dtj-agent` are installed via Cargo:

```bash
cargo install --path crates/dtj --bin dtj --bin dtj-agent
```

This places binaries in `~/.cargo/bin`. macOS GUI apps like Blender may not inherit shell PATH, so the SDK falls back to `~/.cargo/bin/dtj-agent`.

## Quick Start

```python
from dtj_sdk import TraceSession, TraceConfig

with TraceSession.open(
    producer_name="my-python-service",
    producer_version="0.1.0",
    data_dir="./traces",
) as trace:
    trace.emit(
        domain="api",
        category="request",
        name="completed",
        severity="info",
        field_name="duration_ms",
        value=12.5,
        correlation="request-42",
    )
```

## Configuration via `.dtj/config.toml`

The SDK supports configuring the traces storage directory via a TOML config file. Create `.dtj/config.toml` in your project root:

```toml
# .dtj/config.toml
[storage]
data_dir = "traces"
```

This results in traces being written to `<project>/.dtj/traces/*.dtj` (relative to the config file location).

**Config discovery order** (for `TraceSession.open` and `TraceConfig`):

1. Explicit `config_path` argument
2. `DTJ_CONFIG_PATH` environment variable  
3. Search for `.dtj/config.toml` from current working directory upwards to filesystem root

**Storage location resolution order**:

1. Explicit `data_dir` argument → agent started with `--data-dir`
2. Found config file → agent started with `--config`
3. Neither provided → fallback to `./traces` (backward compatible)

### Using config with TraceSession.open

```python
from dtj_sdk import TraceSession

# Explicit config path
with TraceSession.open(
    producer_name="my-service",
    producer_version="0.1.0",
    config_path="/path/to/.dtj/config.toml",
) as trace:
    trace.emit(...)

# Or rely on auto-discovery (searches for .dtj/config.toml from cwd upwards)
with TraceSession.open(
    producer_name="my-service",
    producer_version="0.1.0",
) as trace:
    trace.emit(...)
```

### Using config with TraceConfig

```python
from dtj_sdk import TraceConfig

config = TraceConfig(
    producer_name="my-service",
    producer_version="0.1.0",
    config_path="/path/to/.dtj/config.toml",
)

with config.open_session() as trace:
    trace.emit(...)
```

### Environment variable override

```bash
export DTJ_CONFIG_PATH=/path/to/.dtj/config.toml
python your_script.py
```

## Discovery Order

The SDK locates `dtj-agent` in this order:

1. `agent_path` in `TraceConfig`
2. `DTJ_AGENT_PATH` environment variable
3. `shutil.which("dtj-agent")` (PATH lookup)
4. macOS Homebrew fallback: `/opt/homebrew/bin/dtj-agent`, then `/usr/local/bin/dtj-agent`
5. Cargo dev install fallback: `~/.cargo/bin/dtj-agent`

If not found, the SDK emits a single `RuntimeWarning` and enters **disabled/no-op mode** — the application continues without tracing, no `.dtj` files are created, and no agent process is spawned.

### Blender/macOS caveat

Python/Blender launched from GUI may not have Homebrew in `PATH`. The SDK now checks the common Homebrew prefixes `/opt/homebrew/bin` (Apple Silicon) and `/usr/local/bin` (Intel) as a fallback. `DTJ_AGENT_PATH` remains the reliable explicit override.

## Explicit Disabled/No-Op Behavior

- When `enabled=False` in `TraceConfig`, or when `dtj-agent` is unavailable
- Returns a `TraceSession` that accepts all calls but does nothing
- Emits exactly one `RuntimeWarning` on first use
- No subprocess, no socket, no files written

## Mandatory Dependency

**`dtj-agent` binary must be installed and on PATH** (or provided via config/env) for tracing to work. The SDK is middleware only — it never serializes DTJ format or writes `.dtj` files.

## MVP Limitations

- **One field per event** — the agent currently requires `field_count = 1`
- **Unix domain socket only** — local machine, no network
- **Single session per agent** — agent exits after `FinishSession`
- **No backpressure** — bounded by socket buffers

## Supported Value Types (MVP)

| Python Type | DTJ Type Tag |
|-------------|--------------|
| `bool` | BOOL (0x01) |
| `int` (fits i64) | I64 (0x03) |
| `float` | F64 (0x07) |
| `str` | INTERNED (0x0B) via dictionary |
| `bytes` | BYTES (0x0C) |

## Severity Levels

`"debug" | "info" | "warn" | "error" | "fatal"` — maps to Rust `dtj::Severity`.