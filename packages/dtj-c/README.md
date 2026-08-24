# dtj-c - C SDK for DTJ

C middleware client for DTJ (Debug Trace Journal) tracing system.

## Architecture

```
C Application → dtj-c → local dtj-agent → Rust SessionWriter → .dtj files
```

The SDK **never writes `.dtj` bytes directly** - it communicates with a local `dtj-agent` binary via Unix domain socket using a versioned binary protocol.

## Installation

```bash
cd packages/dtj-c
cmake -B build
cmake --build build
# Optional: install
sudo cmake --install build
```

Requires C11 compiler and POSIX environment (Linux/macOS).

## Quick Start

```c
#include <dtj/dtj.h>

int main(void) {
    dtj_config config = {
        .data_dir = "./traces",
        .producer_name = "my-c-service",
        .producer_version = "0.1.0",
        .enabled = 1,
    };

    dtj_error err;
    dtj_session *sess = dtj_open_strict(&config, &err);
    if (!sess) {
        fprintf(stderr, "Failed to open session: %s\n", err.message);
        return 1;
    }

    dtj_event event = {
        .domain = "api",
        .category = "request",
        .name = "completed",
        .severity = DTJ_SEVERITY_INFO,
        .field_name = "duration_ms",
        .value = dtj_make_f64(12.5),
        .correlation = "request-42",
    };

    dtj_emit(sess, &event, NULL);
    dtj_close(sess, NULL);
    dtj_session_free(sess);

    return 0;
}
```

## API Overview

### Configuration
```c
typedef struct {
    const char *data_dir;            /* Default: "./traces" */
    const char *producer_name;       /* Required, max 32 bytes UTF-8 */
    const char *producer_version;    /* Required, max 16 bytes UTF-8 */
    const char *agent_path;          /* Optional explicit agent path */
    const char *socket_path;         /* Optional existing agent socket */
    const char *session_file_name;   /* Optional auto-generated */
    int enabled;                     /* Default 1 (true) */

    void (*warning_handler)(const char *message, void *user_data);
    void *warning_user_data;
} dtj_config;
```

### Functions

| Function | Description |
|----------|-------------|
| `dtj_open()` | Opens session, returns disabled no-op on missing agent |
| `dtj_open_strict()` | Opens session, returns error on missing agent |
| `dtj_emit()` | Emits single event with one field |
| `dtj_close()` | Gracefully closes session |
| `dtj_session_free()` | Frees session resources (idempotent) |
| `dtj_session_is_enabled()` | Checks if session is active |

### Value Types (MVP: exactly one field per event)

| Type | Helper | DTJ Encoding |
|------|--------|--------------|
| `bool` | `dtj_make_bool()` | BOOL |
| `int64_t` | `dtj_make_i64()` | I64 |
| `double` | `dtj_make_f64()` | F64 |
| `uint32_t` (dict ID) | `dtj_make_interned()` | INTERNED |
| `bytes` | `dtj_make_bytes()` | BYTES |

Strings are interned via dictionary - use `dtj_make_interned(dict_id)` after interning.

## Agent Discovery Order

1. Explicit `config.agent_path`
2. `DTJ_AGENT_PATH` environment variable
3. `PATH` lookup for `dtj-agent`
4. **Not found** → disabled no-op + one warning via handler

## No-Op Behavior

If `dtj-agent` is unavailable:
- Exactly **one** warning via handler (default: stderr)
- All `dtj_emit()` calls become no-ops (return success)
- Application continues running normally
- **No `.dtj` files are created**
- No fallback writer exists

## Building and Testing

```bash
# Build
cmake -B build
cmake --build build

# Unit tests (no agent required)
ctest --test-dir build --output-on-failure

# E2E test (requires dtj-agent and Unix sockets)
DTJ_RUN_AGENT_E2E=1 ctest --test-dir build --output-on-failure -R dtj_e2e

# Or run test executables directly:
build/dtj_tests
DTJ_RUN_AGENT_E2E=1 build/dtj_e2e
```

## C++ Wrapper

For C++ applications, use [dtj-cpp](../dtj-cpp/) which provides RAII wrapper over this C API.

## License

MIT OR Apache-2.0