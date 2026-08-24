# Conformance Tests and Fixtures

This directory contains conformance tests and fixtures for the DTJ v1 byte format.
All tests are standalone — no Doc Hub dependencies.

## What Exists

**Canonical binary fixture:**
- `crates/dtj/tests/fixtures/minimal_session.dtj` — minimal valid DTJ v1 session

**Negative coverage:**
- Binary negative fixtures (corrupted header, unknown chunk type, etc.) are **not yet ported** to this repository.
- Current negative coverage is generated programmatically by conformance tests in `crates/dtj/tests/conformance.rs` (21 tests covering checksum mismatch, sequence gaps, unknown dictionary IDs, malformed records, oversized payloads, torn chunks, etc.).

**Test files (in `crates/dtj/tests/`):**
- `cli_read_session.rs` — 2 CLI integration tests
- `conformance.rs` — 21 conformance tests (positive + negative)

## Running Tests

```bash
# From crates/dtj directory
cd crates/dtj
cargo test

# Or specific test suites
cargo test cli_read_session
cargo test conformance
```

## License

Unresolved — see `OPEN_DECISIONS.md`