# dtj — DTJ v1 reference core

Language-neutral Rust reference for the Debug Trace Journal (`.dtj`) byte format.

- Spec: `specs/dtj-format-v1.md`
- ADR: `docs/adr/`

Standalone crate — not affiliated with Doc Hub, not a paid pack component.

## Build

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

## CLI

```bash
cargo build --bin dtj
./target/debug/dtj read-session path/to/session.dtj
```

## License

Unresolved — see `OPEN_DECISIONS.md`