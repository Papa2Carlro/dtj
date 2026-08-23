# Conformance Tests and Fixtures

This directory contains conformance tests and fixtures for the DTJ v1 byte format.
All tests are standalone — no Doc Hub dependencies.

## Directory Structure

```
fixtures/
  positive/
    - session_with_dictionary.dtj
    - session_with_events.dtj
    - session_v1_format.dtj
  negative/
    - session_corrupted_header.dtj
    - session_unknown_chunk_type.dtj
    - session_exceeds_payload_limit.dtj
    - session_wrong_magic.dtj

schemas/
  dtj-format-v1.json  — JSON schema for validation
  chunk-header.schema.json
  file-header.schema.json

tests/
  test_read_write.rs
  test_chunk_integrity.rs
  test_version_handling.rs
  test_dictionary.rs
```

## Running Tests

```bash
# Rust tests
cargo test --package dtj

# Or specific test suites
cargo test test_read_write
cargo test test_chunk_integrity
```

## License

MIT OR Apache-2.0