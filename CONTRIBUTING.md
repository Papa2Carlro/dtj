# CONTRIBUTING — DTJ Independent Project

Thank you for wanting to contribute to DTJ! This document explains how you can
help. Please note: this is a standalone open-source project, not a Doc Hub
plugin or paid pack component.

## Code of Conduct

Simple respect and professional courtesy. No harassment, no bigotry, no
exclusionary behavior. You're representing this project to the community.

## How to Contribute

### 1. Report Issues

Use the GitHub Issues at https://github.com/Papa2Carlro/dtj/issues. When reporting:
- Use the issue template
- Include `dtj --version` output
- Include OS and Rust version (`rustc --version`)
- Include a minimal reproduction case (`.dtj` file or event description)
- Label the issue appropriately (`bug`, `enhancement`, `question`, `docs`)

### 2. Bug Fixes

1. Fork the repository at https://github.com/Papa2Carlro/dtj
2. Create a branch: `git checkout -b fix/issue-description`
3. Write a test that reproduces the bug (see `dtj/fixtures/` for conventions)
4. Run existing tests: `cargo test` from `dtj/`
5. Fix the bug
6. Run tests again to confirm
7. Commit with clear message: `git commit -m "fix: describe the fix"`
8. Push and open a Pull Request

### 3. Conformance Fixtures

The `dtj/fixtures/` directory needs positive and negative test cases.

**Positive fixture requirements**:
- Valid `.dtj` file with FileHeader + DictionaryChunk + EventChunk
- All chunk types that v1 supports
- Edge cases: empty dictionary, single event, max limits

**Negative fixture requirements**:
- Corrupted magic bytes
- Unknown chunk type with valid checksum
- Chunk payload exceeding max length (16 MiB)
- Invalid format version
- Missing/reserved field violations

Add fixtures under `dtj/fixtures/positive/` or `dtj/fixtures/negative/`.

### 4. Documentation

- `specs/dtj-format-v1.md` — if format needs updating
- `OPEN_DECISIONS.md` — if new unresolved decision emerges
- `docs/extraction.md` — if ecosystem migration path changes
- `ROADMAP.md` — if next version targets shift
- `CONTRIBUTING.md` — if process changes

Documentation changes follow the same PR process as code changes.

### 5. License

By contributing, you agree that your contributions are dual-licensed under
MIT AND Apache-2.0, consistent with the project licensing. See
`LICENSE-MIT` and `LICENSE-APACHE` for details.

If you cannot dual-license your contribution, note it in the PR and
alternative arrangement will be discussed.

### 6. Development Workflow

```bash
# Clone
git clone https://github.com/Papa2Carlro/dtj.git
cd dtj

# Build
cargo build --release

# Test
cargo test

# Format
cargo fmt --check

# Lint
cargo clippy --all-targets -- -D warnings

# CLI help
./target/release/dtj --help

# Read a session
./target/release/dtj read-session path/to/session.dtj
```

### 6. Python MCP Development

```bash
# From dtj/ root
cd packages/dtj-mcp

# Install dependencies
uv sync

# Run tests
uv run pytest

# Or with pip
pip install -e ".[dev]"

# Run CLI
dtj-mcp --help
```

### 7. VS Code Extension Development

```bash
# From dtj/ root
cd packages/dtj-vscode

# Install dependencies
npm install

# Compile
npm run compile

# Run in development mode
code --install-ext .  # or use VS Code's extension development panel

# Make changes, press F5 to debug
```

## Recognition

Contributors who make significant contributions will be acknowledged in:
- `README.md` (Contributors section, if applicable)
- `ROADMAP.md` (relevant milestone notes)
- Release notes

## Questions?

- Open an issue for general questions
- Discussions (once available on GitHub)
- Check `docs/` directory for additional guides

Thank you for helping make DTJ better!