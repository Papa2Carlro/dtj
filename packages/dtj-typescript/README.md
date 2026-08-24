# @dtj/sdk - TypeScript SDK for DTJ

Thin middleware to local `dtj-agent`. The SDK **never writes `.dtj` bytes directly** — it communicates with a local `dtj-agent` binary via Unix domain socket using a versioned binary protocol.

## Architecture

```
TypeScript Application → @dtj/sdk → local dtj-agent → Rust SessionWriter → .dtj files
```

## Installation

```bash
npm install @dtj/sdk
```

Requires Node.js >= 20.

## Quick Start

```typescript
import { TraceSession } from "@dtj/sdk";

const trace = await TraceSession.open({
  producerName: "my-node-service",
  producerVersion: "0.1.0",
  dataDir: "./traces",
});

try {
  await trace.emit({
    domain: "api",
    category: "request",
    name: "completed",
    severity: "info",
    fieldName: "duration_ms",
    value: 12.5,
    correlation: "request-42",
  });
} finally {
  await trace.close();
}
```

## Agent Discovery Order

The SDK finds `dtj-agent` in this order:

1. Explicit `agentPath` in config
2. `DTJ_AGENT_PATH` environment variable
3. `PATH` lookup for `dtj-agent`
4. **Not found** → emits **one** Node warning (`DTJWarning`) and returns a no-op session

## No-Op Behavior

If `dtj-agent` is unavailable:
- Exactly **one** Node warning (`DTJWarning`) is emitted on first `emit()` call
- All `emit()` calls become no-ops
- Application continues running normally
- **No `.dtj` files are created**
- No fallback writer exists

## Configuration

```typescript
interface TraceConfig {
  dataDir?: string;           // Default: "./traces"
  producerName: string;       // Max 32 bytes UTF-8
  producerVersion: string;    // Max 16 bytes UTF-8
  agentPath?: string;         // Explicit dtj-agent path
  socketPath?: string;        // Connect to existing agent
  sessionFileName?: string;   // Default: "session-<timestamp>.dtj"
  enabled?: boolean;          // Default: true
}
```

## Supported Value Types (MVP: one field per event)

| Type | Encoding |
|------|----------|
| `boolean` | BOOL |
| `bigint` | I64 (signed) |
| `number` (integer, safe range) | I64 |
| `number` (non-integer or out of range) | F64 |
| `Uint8Array` | BYTES |
| `string` | **Not directly** — use `fieldName` for string values |

## Severity Levels

`debug` | `info` | `warn` | `error` | `fatal` — maps directly to Rust `dtj::Severity`.

## MVP Limitations

- **One field per event** — multi-field events not supported
- **Single client, single session** — agent accepts one connection
- **Unix domain sockets only** — macOS/Linux (no Windows named pipes yet)
- **No backpressure** — socket buffers only

## Running Tests

```bash
# Unit tests (no agent required)
npm test

# E2E tests (requires dtj-agent and Unix sockets)
DTJ_RUN_AGENT_E2E=1 npm run test:e2e
```

## Building

```bash
npm run build
```

Outputs to `dist/` with TypeScript declarations.

## License

MIT OR Apache-2.0