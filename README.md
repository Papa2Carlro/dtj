# DTJ — Independent Debug Trace Journal

## Overview

DTJ (Debug Trace Journal) is an independent, open format for debug trace data.
This repository provides the core specification, reference implementation, and
conformance tools — completely standalone, with no dependency on Doc Hub,
paid packs, or any commercial platform.

## Status

**Phase 0** — Rust core with `dtj read-session` CLI, 25 passing tests, canonical `minimal_session.dtj` fixture.

## Features (Phase 0)

- Append-only binary journal format (DTJ v1 byte contract)
- Versioned committed chunks with crash recovery
- Numeric string dictionary
- Typed inline event payloads
- Lossless session policy or flight-recorder overwrite mode
- Rebuildable indexes (not source of truth)
- Language-neutral: readable from Rust, C#, Python, TypeScript
- CLI: `dtj read-session` (JSON output, structured errors)

## Non-features (Phase 0 — no paid pack lock-in)

- No remote transport, HTTP, or cloud telemetry
- No compression, encryption, or replay
- No C ABI or native Unity plugin
- No JSONL import/export
- No paid licensing or activation requirements
- No Doc Hub-specific branding or manifests
- No `dtj tail`, `dtj info`, `dtj verify` commands
- No MCP implementation (planned, not yet built)
- No SDKs (C#, Python, TypeScript — planned, not yet built)

## Quick Start

```bash
# Read a .dtj session
dtj read-session path/to/session.dtj
```

## Build from Source (Rust)

```bash
git clone https://github.com/dtj-standard/dtj.git
cd dtj
cargo build --release
```

## License

Unresolved — see `OPEN_DECISIONS.md`

## Communication

- Issues: https://github.com/dtj-standard/dtj/issues
- Discussions: https://github.com/dtj-standard/djg/discussions
- Spec: `specs/dtj-format-v1.md`