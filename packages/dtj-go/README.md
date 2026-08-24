# dtj Go SDK

Go SDK for DTJ (Debug Trace Journal) - thin middleware to local `dtj-agent`.

## Architecture

```
Go Application → dtj Go SDK → local dtj-agent → Rust SessionWriter → .dtj files
```

The SDK **never writes `.dtj` bytes directly** - it communicates with a local `dtj-agent` binary via Unix domain socket using a versioned binary protocol.

## Installation

```bash
go get github.com/Papa2Carlro/dtj/packages/dtj-go
```

Requires Go 1.22+.

## Quick Start

```go
package main

import (
	"github.com/Papa2Carlro/dtj/packages/dtj-go"
)

func main() {
	trace := dtj.Open(dtj.Config{
		ProducerName:    "my-go-service",
		ProducerVersion: "0.1.0",
		DataDir:         "./traces",
	})
	defer trace.Close()

	trace.Emit(dtj.Event{
		Domain:      "api",
		Category:    "request",
		Name:        "completed",
		Severity:    dtj.Info,
		FieldName:   "duration_ms",
		Value:       12.5,
		Correlation: "request-42",
	})
}
```

## Agent Discovery Order

The SDK finds `dtj-agent` in this order:

1. Explicit `Config.AgentPath`
2. `DTJ_AGENT_PATH` environment variable
3. `PATH` lookup for `dtj-agent`
4. **Not found** → emits **one** warning via `WarningHandler` and returns a no-op session

## No-Op Behavior

If `dtj-agent` is unavailable:
- Exactly **one** warning is emitted via `WarningHandler` (default: stderr)
- All `Emit()` calls become no-ops (return `nil`)
- Application continues running normally
- **No `.dtj` files are created**
- No fallback writer exists

## Configuration

```go
type Config struct {
    DataDir         string        // Default: "./traces"
    ProducerName    string        // Max 32 bytes UTF-8
    ProducerVersion string        // Max 16 bytes UTF-8
    AgentPath       string        // Explicit dtj-agent path
    SocketPath      string        // Connect to existing agent
    SessionFileName string        // Default: "session-<timestamp>.dtj"
    Enabled         *bool         // Default: true (nil = true)
    WarningHandler  func(error)   // Default: prints to stderr
}
```

## API

### Open(Config) *Session
Opens a trace session. Returns a disabled no-op session if agent unavailable (never returns error).

### OpenStrict(Config) (*Session, error)
Opens a trace session. Returns error if agent unavailable (fail-fast).

### Session.Emit(Event) error
Emits a single event with one field. Returns `nil` on disabled session.

### Session.Close() error
Closes the session gracefully. Idempotent.

### Event
```go
type Event struct {
    Domain      string
    Category    string
    Name        string
    Severity    Severity  // Debug, Info, Warn, Error, Fatal
    FieldName   string
    Value       any       // bool, int*, uint*, float32/64, string, []byte
    Correlation string
}
```

## Supported Value Types (MVP: one field per event)

| Go Type | DTJ Encoding |
|---------|--------------|
| `bool` | BOOL |
| `int8`..`int64`, `int` | I64 (range checked) |
| `uint8`..`uint64`, `uint` | U64 |
| `float32`, `float64` | F64 |
| `string` | INTERNED (via dictionary) |
| `[]byte` | BYTES |

## Severity Levels

`Debug` | `Info` | `Warn` | `Error` | `Fatal` — maps directly to Rust `dtj::Severity`.

## MVP Limitations

- **One field per event** — multi-field events not supported
- **Single client, single session** — agent accepts one connection
- **Unix domain sockets only** — macOS/Linux (no Windows named pipes yet)
- **No backpressure** — socket buffers only

## Running Tests

```bash
# Unit tests (no agent required)
go test ./...

# E2E tests (requires dtj-agent and Unix sockets)
DTJ_RUN_AGENT_E2E=1 go test ./... -run TestE2E

# Vet
go vet ./...
```

## Building

```bash
go build ./...
gofmt -w *.go
```

## License

MIT OR Apache-2.0