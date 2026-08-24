# DTJ Roadmap — Independent Project

High-level roadmap for the independent DTJ project. Milestones are rough and
will be refined as the project matures.

## Phase 0 (Current)

**Goal**: Stable DTJ v1 byte format, reference CLI (`dtj read-session`), and conformance documentation.

### Delivered
- [x] DT v1 byte format specification (`specs/dtj-format-v1.md`)
- [x] Rust reference core (`crates/dtj/`) with `cargo test` passing (25 tests)
- [x] CLI binary (`dtj read-session`) — JSON output, structured errors
- [x] Canonical fixture: `crates/dtj/tests/fixtures/minimal_session.dtj`

### Open
- [ ] Fix license status (unresolved — see `OPEN_DECISIONS.md`)
- [ ] Populate `crates/dtj/tests/fixtures/` with conformance test suite
- [ ] Create `CONTRIBUTING.md` for external contributors
- [ ] Resolve repository hosting/organization name

### Versioning Policy
- Breaking changes require `format_version >= 2` and a new ADR
- Minor versions add backward-compatible features
- Patch versions fix bugs without format changes

---
*Roadmap is a living document. Milestones shift as conformance tests are
populated. No features from the "Non-goals for v1" list will be added until a
separate ADR is approved.*