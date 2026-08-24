# DTJ Agent v1 Architecture Contract

## Overview
This document defines the v1 boundary between SDKs and the local `dtj-agent` binary. The architecture follows the principle: **Rust DTJ core is the single writer implementation**; SDKs are thin middleware that communicate with a local agent.

## Architecture Boundary

```
Application SDK
  → discovers local dtj-agent binary
  → starts it or connects to an existing local agent
  → communicates via versioned local IPC

dtj-agent
  → owns SessionWriter and all DTJ byte serialization
  → writes local append-only .dtj sessions

CLI / MCP
  → read completed or recoverable .dtj files only
```

## Key Constraints (v1)

- **DTJ v1 is not a database, container, or GMem runtime** — it's an append-only binary journal format.
- **DTJ2** may become a separate container/database surface over GMem in the future, but that is **out of scope for v1**.
- **SDK is client middleware, not a writer** — it never produces `.dtj` bytes directly.
- If `dtj-agent` is not installed or unavailable, SDK emits an **explicit warning** and enters a **disabled/no-op state**.
- **No hidden fallback writer** — SDK never falls back to its own serialization.
- **MCP is never in the producer hot path** — only for post-hoc analysis.
- **Agent ingress is local-only and versioned**; exact protocol is not yet implemented.
- **Preferred first dev target**: macOS local companion binary + Unix domain socket.
- For backend later: sidecar/container deployment is possible, but **not the current implementation target**.
- Agent will support policies/backpressure in the future, but **do not design them now**.

## Explicitly Out of Scope for v1

- C ABI / FFI layer
- Python / C# SDK code (will be separate thin middleware later)
- Container orchestration
- Socket / IPC implementation code
- JSON event protocol
- GMem integration
- Format v2 changes
- SDK release promises or dates

## Versioning

- Agent protocol versioning is mandatory from day one.
- SDKs must negotiate protocol version on connect.
- Incompatible versions → explicit error, no silent degradation.

## Error Handling

- All agent communication failures are surfaced to the application as structured errors.
- SDK never silently drops events when agent is unreachable (unless explicitly configured for fire-and-forget, which is not v1).

---

*This contract is the single source of truth for the v1 boundary. Any implementation must be traceable to these bullets.*