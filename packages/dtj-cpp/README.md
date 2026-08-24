# dtj-cpp - C++ RAII Wrapper for DTJ

C++ RAII wrapper over the dtj-c C API for DTJ (Debug Trace Journal) tracing system.

## Architecture

```
C++ Application → dtj-cpp → dtj-c → local dtj-agent → Rust SessionWriter → .dtj files
```

The wrapper **never writes `.dtj` bytes directly** - all serialization is handled by the dtj-agent via binary protocol.

## Installation

```bash
cd packages/dtj-cpp
cmake -B build
cmake --build build
```

Requires C++17 and the `dtj-c` library built and available.

## Quick Start

```cpp
#include <dtj_cpp/trace_session.hpp>

int main() {
    try {
        dtj::config cfg;
        cfg.producer_name = "my-cpp-service";
        cfg.producer_version = "0.1.0";
        cfg.data_dir = "./traces";

        // Returns disabled no-op session if agent unavailable
        auto trace = dtj::trace_session::open(cfg);

        if (trace.is_enabled()) {
            trace.emit({
                "api",           // domain
                "request",       // category
                "completed",     // name
                DTJ_SEVERITY_INFO,
                "duration_ms",   // field_name
                12.5             // value (double)
            });
        }

        // Automatic cleanup via RAII
    } catch (const dtj::dtj_error& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }

    return 0;
}
```

## API Overview

### Configuration
```cpp
struct config {
    std::string data_dir = "./traces";
    std::string producer_name;       // Required, max 32 bytes UTF-8
    std::string producer_version;    // Required, max 16 bytes UTF-8
    std::string agent_path;          // Optional explicit agent path
    std::string socket_path;         // Optional existing agent socket
    std::string session_file_name;   // Optional auto-generated
    bool enabled = true;             // Default true

    void (*warning_handler)(const char* message, void* user_data) = nullptr;
    void* warning_user_data = nullptr;
};
```

### Value Types (MVP: exactly one field per event)

```cpp
dtj::value v_bool(true);           // bool -> BOOL
dtj::value v_int(int64_t(42));     // int64 -> I64  
dtj::value v_double(3.14);         // double -> F64
dtj::value v_string("hello");      // string -> INTERNED (interned via dictionary)
dtj::value v_bytes(ptr, len);      // bytes -> BYTES
```

### Session Operations

```cpp
// Open session (returns disabled no-op if agent unavailable)
auto trace = dtj::trace_session::open(config);

// Open strict (throws on missing agent)
auto trace = dtj::trace_session::open_strict(config);

// Check if connected to agent
if (trace.is_enabled()) { ... }

// Emit event with exactly one field (MVP)
trace.emit({
    "api",           // domain
    "request",       // category  
    "completed",     // name
    DTJ_SEVERITY_INFO,
    "duration_ms",   // field_name
    12.5             // value (double)
});

// Automatic RAII cleanup on destruction
// trace.close() and trace.free() also available explicitly

// Move-only type, automatically closes on destruction
```

### Severity Levels

```cpp
DTJ_SEVERITY_DEBUG, DTJ_SEVERITY_INFO, DTJ_SEVERITY_WARN,
DTJ_SEVERITY_ERROR, DTJ_SEVERITY_FATAL
```

## Agent Discovery Order

Same as dtj-c:
1. Explicit `config.agent_path`
2. `DTJ_AGENT_PATH` environment variable  
3. `PATH` lookup for `dtj-agent`
4. **Not found** → disabled no-op + one warning via handler

## No-Op Behavior

If `dtj-agent` is unavailable:
- Exactly **one** warning via handler (default: stderr)
- All `emit()` calls become no-ops (return success)
- Application continues running normally
- **No `.dtj` files are created**
- No fallback writer exists

## Building and Testing

```bash
# Requires dtj-c library built first in ../dtj-c/build

# Build wrapper and tests
cmake -B build -DDTJ_C_DIR=../dtj-c
cmake --build build

# Run unit tests (no agent required)
ctest --test-dir build --output-on-failure

# E2E test (requires dtj-agent and Unix sockets)
DTJ_RUN_AGENT_E2E=1 ctest --test-dir build --output-on-failure -R cpp_e2e

# Or run test executable directly:
build/cpp_wrapper_tests
DTJ_RUN_AGENT_E2E=1 build/cpp_e2e_tests  # if E2E test exists
```

## Requirements

- C++17 compiler (GCC 7+, Clang 5+, MSVC 2017+)
- dtj-c library (built from ../dtj-c)
- POSIX environment (Linux/macOS)

## License

MIT OR Apache-2.0
