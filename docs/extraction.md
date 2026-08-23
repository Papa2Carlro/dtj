# DTJ Extraction — Migration Path from Doc Hub Ecosystem

This document maps the "old path → new path → migration status → remaining dependency"
for all DTJ-related files and concepts moved from the Doc Hub ecosystem to the
independent `/Users/maksympryimak/dtj` repository.

## Purpose

Provide a thin compatibility bridge in the old ecosystem without duplicating
normative specification. All canonical source of truth now lives in `/Users/maksympryimak/dtj`.

## Legend

| Symbol | Meaning |
|--------|---------|
| `✅` | Migrated / canonical in DTJ repo |
| `🔄` | Temporary compatibility bridge in ecosystem |
| `ℹ️` | Reference only, no action needed |
| `❌` | Unrelated, leave unchanged |

## 1. Format Specification

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| `doc-hub/Docs/guides/dtj-format-v1.md` | `dtj/specs/dtj-format-v1.md` | ✅ | Canonical spec. Ecosystem links now point here via `https://github.com/<owner>/dtj/blob/main/specs/dtj-format-v1.md` |
| `doc-hub/Docs/ADR/0008-dtj-as-a-portable-debug-trace-journal-fo.md` | `dtj/docs/adr-index.md` | ✅ | Historical reference. Not duplicated in ecosystem. |
| `doc-hub/ADR/...` | `dtj/docs/adr-index.md` | ✅ | See above |

## 2. Rust Reference Core

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| `doc-hub/crates/dtj/` | `dtj/crates/dtj/` | ✅ | Rust source of truth. Ecosystem no longer reads this directly. |

## 3. CLI Tools

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| `dochub-pack-dtj/debug-trace-mcp/dtj` CLI | `dtj/crates/dtj/target/debug/dtj` | ✅ | Built from `dtj/crates/dtj/`. CLI help: `dtj --help` |
| `dochub-pack-dtj/debug-trace-mcp` Python package | `dtj/packages/dtj-mcp/` | ✅ | New Python package with `pyproject.toml`. No Doc Hub branding. |

## 4. Python MCP

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| `dochub-pack-dtj/debug-trace-mcp/` (full Python package) | `dtj/packages/dtj-mcp/` (standalone) | ✅ | `pyproject.toml`, `src/debug_trace_mcp/`. See OPEN_DECISIONS.md for license status. |

## 5. VS Code Integration

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| `dochub-pack-dtj/dtj-vscode/` | `dtj/packages/dtj-vscode/` | ✅ | README refactored. No Doc Hub licensing. VS Code install instructions updated. |

## 6. Conformance Fixtures

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| (none in Doc Hub — DTJ was paid-pack locked) | `dtj/fixtures/` | ✅ | Empty initially. To be populated with positive/negative test cases. |
| `doc-hub/packages/debug-trace-mcp/tests/` | `dtj/fixtures/tests/` | 🔄 | Temporary bridge: copies of test fixtures until full suite is ported. |

## 7. Licensing

| Old Path | New Path | Status | Notes |
|----------|----------|--------|-------|
| `dochub-pack-dtj/LICENSE-KEYS.md` | `dtj/LICENSE-MIT` + `dtj/LICENSE-APACHE` | ✅ | Dual licensed. No paid activation. |
| `doc-hub/crates/dtj/README.md` (mentions) | `dtj/OPEN_DECISIONS.md` (license section) | 🔄 | License status explicit unresolved — see OPEN_DECISIONS.md |

## 8. Ecosystem Compatibility Bridges (🔄 = Temporary)

These files in `/Users/maksympryimak/doc-hub-ecosystem` remain but are now
**read-only references** with links to DTJ repo:

| File/Path | Bridge Type | DTJ Link |
|-----------|-------------|----------|
| `dochub-pack-dtj/README.md` | 🔄 | `https://github.com/<owner>/dtj/blob/main/README.md` |
| `dochub-pack-dtj/ECOSYSTEM.md` | 🔄 | `https://github.com/<owner>/dtj/blob/main/ECOSYSTEM.md` |
| `dochub-pack-dtj/EXTERNAL.md` | 🔄 | `https://github.com/<owner>/dtj/blob/main/specs/dtj-format-v1.md` |
| `doc-hub/packages/EXTERNAL_PLUGINS.md` | 🔄 | Updated to reference `dtj/` canonical paths |
| `doc-hub/Docs/guides/plugin-sdk.md` (DTJ Trace Gate section) | 🔄 | Links to `dtj/specs/dtj-format-v1.md` |
| `doc-hub/Docs/ADR/0008-...` | 🔄 | Kept for historical reference, not normative |
| `doc-hub/src-tauri/src/lib.rs` (editor context) | ❌ | Unrelated to DTJ — leave unchanged |
| `doc-hub/BUDGET.md` | ❌ | Unrelated — leave unchanged |

## 9. Migration Rules (Ecosystem Side)

1. **No duplication**: Ecosystem does not repeat normative DTJ spec. Only
   compatibility bridges with `https://github.com/<owner>/dtj/` URLs.
2. **No absolute local paths**: User-facing docs use repository-relative or
   URL placeholder format, not `/Users/maksympryimak/dtj/...`.
3. **Compatibility bridges only**: Any DTJ references in ecosystem docs now
   point to the canonical DTJ repo, not local copies.
4. **No git submodule, no remote push, no package publish**: Per task rules.
5. **Paid pack stays as migration stub**: `dochub-pack-dtj` becomes a thin
   compatibility layer with explicit links to DTJ repo and status markers.
6. **Byte format, CLI output, MCP semantics**: Unchanged from v1. No breaking
   changes to the contract.
7. **If Doc Hub runtime needs local DTJ component**: Left temporarily in place
   but flagged as `🔄 compatibility bridge` with criterion for removal.

## 10. Roadmap Forward

| Priority | Item | Owner | Target |
|----------|------|-------|--------|
| P0 | Fix license status in OPEN_DECISIONS.md | Project lead | Immediate |
| P0 | Create canonical GitHub repo (external, not in scope) | infra team | N/A (per rules) |
| P1 | Populate `dtj/fixtures/` with conformance tests | QA lead | Next sprint |
| P1 | Update all `dochub-pack-dtj` docs to use DTJ repo URLs | doc team | 2 weeks |
| P2 | Draft ROADMAP.md for independent project | PM | Next release |
| P2 | Create CONTRIBUTING.md | community | 1 month |
| P3 | Evaluate SDK readiness (C#, Python, TS) | tech lead | Q4 2026 |
| P3 | Decide Pro tier pricing/features | product team | Q4 2026 |

---
*This document is the single source of truth for migration status in the ecosystem.
Any DTJ-related link found in Doc Hub that is not annotated here should be
treated as stale and flagged for review.*