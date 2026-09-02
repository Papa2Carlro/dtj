# dtj-rust SDK
Quick start: `use dtj_sdk::{Config, open};`
Limits: communicates via Unix socket with dtj-agent only.
Check: `cargo test --manifest-path packages/dtj-rust/Cargo.toml`
E2E: `DTJ_RUN_AGENT_E2E=1 cargo test --manifest-path packages/dtj-rust/Cargo.toml`
