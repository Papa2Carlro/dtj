# DTG DTJ Session Explorer

Read-only VS Code custom editor for plain `.dtj` journals (ADR 0021).

The extension **does not** parse DTJ bytes in TypeScript. It spawns a prebuilt
Rust `dtj` binary and speaks only the frozen `ui-session` protocol v1.

## Setup

1. Build the binary yourself (extension never runs cargo):

```bash
cargo build --manifest-path crates/dtj/Cargo.toml --release
```

2. In VS Code / Cursor settings, set an **absolute** path:

```json
{
  "dtg.sessionExplorer.dtjBinaryPath": "/absolute/path/to/crates/dtj/target/release/dtj"
}
```

3. Open a trusted workspace and open a `*.dtj` file.

## TraceQL query bar (explorer subset)

The explorer has a DB-like query bar. **Run** parses a small TraceQL subset in the
extension and maps it to existing exact `ui-session` filters — it does **not**
invoke a TraceQL engine or derived index (see ADR 0013 for the full language).

Supported shape:

```text
FROM events
[ WHERE domain = "wire" AND severity = info ]
[ SELECT * ]
LIMIT 100
```

**Open .traceql** creates/opens `<session>.traceql` beside the journal. The
extension contributes language `dtg-traceql` (TextMate grammar + snippets +
`files.associations`).

From a `.traceql` editor: use the title **▶ Run** button, CodeLens
**Run TraceQL** / **Run against…**, or ⌘/Ctrl+Enter. Resolve order: sibling
`<name>.dtj` → linked open explorer → last picked journal → file picker.
**Run against…** always prompts. Opens **Session Explorer** and runs the subset
via the configured `dtj` binary (`dtg.sessionExplorer.dtjBinaryPath` — never
PATH / cargo). Problems panel shows live subset parse errors. Editing alone does
not auto-run; save syncs text into an open explorer bar when the sidecar is
linked. Dirty query bar blocks paging until **Run**.

Pagination: **First / Prev / Next / Last** with page counters over matched rows
(BigInt-safe offsets).

## Non-goals (MVP)

- PATH / `DTJ_BIN` / bundled binary / auto-download
- `.dtgb`, `.dtgb.age`, DTJP, full TraceQL engine/index, graph, vault, MCP
- Edit / capture / writer paths
