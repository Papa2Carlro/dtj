# DTJ — Independent Debug Trace Journal

## Overview

DTJ (Debug Trace Journal) is an independent, open format for debug trace data.
This repository provides the core specification, reference implementation, and
conformance tools — completely standalone, with no dependency on Doc Hub,
paid packs, or any commercial platform.

## Status

**v1.0.0** — Initial release with core byte format, CLI, and conformance fixtures.

## Features (v1)

- Append-only binary journal format
- Versioned committed chunks with crash recovery
- Numeric string dictionary
- Typed inline event payloads
- Lossless session policy or flight-recorder overwrite mode
- Rebuildable indexes (not source of truth)
- Language-neutral: readable from Rust, C#, Python, TypeScript
- CLI: `dtj read-session`, `dtj tail`, `dtj info`, `dtj verify`
- MCP read-only boundary: stdio-based session analysis

## Non-features (v1 — no paid pack lock-in)

- No remote transport, HTTP, or cloud telemetry
- No compression, encryption, or replay
- No C ABI or native Unity plugin
- No JSONL import/export
- No paid licensing or activation requirements
- No Doc Hub-specific branding or manifests

## Quick Start

```bash
# Read a .dtj session
dtj read-session path/to/session.dtj

# Show last N events
dtj tail path/to/session.dtj 10

# Show session info
dtj info path/to/session.dtj

# Verify integrity
dtj verify path/to/session.dtj
```

## Build from Source (Rust)

```bash
git clone https://github.com/dtj-standard/dtj.git
cd dtj
cargo build --release
```

## License

MIT OR Apache-2.0

## Communication

- Issues: https://github.com/dtj-standard/dtj/issues
- Discussions: https://github.com/dtj-standard/djg/discussions
- Spec: `specs/dtj-format-v1.md`