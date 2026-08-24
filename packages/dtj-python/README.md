# dtj-sdk — Python SDK for DTJ Agent

Python middleware client for the local `dtj-agent` binary. The SDK never writes `.dtj` bytes directly — it communicates with the agent via Unix domain socket using the binary protocol defined in `docs/dtj-agent-protocol-v1.md`.

## Install

```bash
# From local path (development)
pip install -e /path/to/dtj/packages/dtj-python

# Or from the dtj repo root
pip install -e packages/dtj-python
```

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

## Discovery Order

The SDK locates `dtj-agent` in this order:

1. `agent_path` in `TraceConfig`
2. `DTJ_AGENT_PATH` environment variable
3. `shutil.which("dtj-agent")` (PATH lookup)

If not found, the SDK emits a single `RuntimeWarning` and enters **disabled/no-op mode** — the application continues without tracing, no `.dtj` files are created, and no agent process is spawned.

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