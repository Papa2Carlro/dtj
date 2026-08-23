# DTJ Roadmap — Independent Project

High-level roadmap for the independent DTJ project. Milestones are rough and
will be refined as the project matures.

## Version 1.0.0 (Current)

**Goal**: Stable DTJ v1 byte format, reference CLI, independent MCP, and
compliance documentation.

### Delivered (v1.0.0)
- [x] DT v1 byte format specification (`specs/dtj-format-v1.md`)
- [x] Rust reference core (`crates/dtj/`) with `cargo test` passing
- [x] CLI binary (`dtj read-session`, `dtj tail`, `dtj info`, `dtj verify`)
- [x] Independent Python MCP package (`packages/dtj-mcp/`) with `pyproject.toml`
- [x] VS Code integration skeleton (`packages/dtj-vscode/`)
- [x] Dual MIT/Apache-2.0 licensing (`LICENSE-MIT`, `LICENSE-APACHE`)
- [x] `OPEN_DECISIONS.md` with unresolved decisions documented
- [x] `docs/extraction.md` mapping old paths → new paths
- [x] Repository structure at `/Users/maksympryimak/dtj/`

### Open (v1.0.0)
- [ ] Fix license status (dual MIT/Apache vs. single choice)
- [ ] Populate `dtj/fixtures/` with conformance test suite
- [ ] Create `CONTRIBUTING.md` for external contributors
- [ ] Resolve repository hosting/organization name
- [ ] Decide Pro tier pricing/features (carried from extraction decision)
- [ ] Verify `cargo test` across all targets
- [ ] `dtj --help` output finalized
- [ ] Python MCP `uv sync` + tests pass

## Version 1.1.0 (Planned)

**Goal**: Enhanced tooling, fixture suite, and ecosystem migration completeness.

### Target Features
- [ ] Full conformance fixture suite (positive + negative cases)
- [ ] `dtj-mcp` HTTP endpoint stability
- [ ] VS Code extension package (`vsix`) publish instructions
- [ ] `docs/extraction.md` kept in sync with actual ecosystem state
- [ ] Migration bridges in Doc Hub updated to canonical DTJ URLs
- [ ] `ROADMAP.md` updated for next cycle

### Infrastructure
- [ ] CI workflow (GitHub Actions or equivalent — external, not in scope)
- [ ] Release draft process for 1.1.0
- [ ] Changelog process (semver-compliant)

## Version 2.0.0 (Future)

**Goal**: Significant extensions while maintaining v1 backward compatibility.

### Considered Features (not v1 scope)
- [ ] C# producer SDK (first priority after v1, per extraction decision)
- [ ] Python reader/writer SDK
- [ ] TypeScript SDK (after real Node/browser use case confirmed)
- [ ] Advanced analysis / causal workflows (Pro tier)
- [ ] CI/incident reporting integration
- [ ] VS Code visual trace explorer
- [ ] Team governance / policy packs
- [ ] Encryption / signing (as separate Pro addon, not in v1 scope)
- [ ] Remote transport / cloud telemetry (explicitly out of v1 scope)

### Versioning Policy
- Breaking changes require `format_version >= 2` and a new ADR
- Minor versions add backward-compatible features
- Patch versions fix bugs without format changes
- All format version bumps require new ADR and conformance suite update

---
*Roadmap is a living document. Milestones shift as conformance tests are
populated and ecosystem migration progresses. No features from the "Non-goals
for v1" list (remote collector, compression, encryption, replay, C ABI, native
Unity plugin, JSONL import/export) will be added until a separate ADR is approved.*