# DTJ — Independent Debug Trace Journal

## Overview

DTJ (Debug Trace Journal) is an independent, open format for debug trace data.
This repository provides the core specification, reference implementation, and
conformance tools — completely standalone, with no dependency on Doc Hub,
paid packs, or any commercial platform.

## Status

**DTJ v0.1 release candidate.** Core CLI (`dtj`) and `dtj-agent` are implemented
and tested across all supported platforms.

SDKs source-available (locally tested) for:

- Rust
- Python
- Go
- TypeScript

Package-manager publication of SDKs (crates.io / PyPI / npm / Go module proxy)
is a separate release track and is **not** part of the v0.1 binary release.

## Platform Support (v0.1)

DTJ `v0.1` targets the following platforms:

- **macOS** — supported (Apple Silicon / `aarch64-apple-darwin`, Intel / `x86_64-apple-darwin`).
- **Linux** — supported (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`).
- **Windows** — **not supported in `v0.1`**. The `dtj-agent` runtime and all official SDKs (Rust, Python, Go, TypeScript) communicate over Unix domain sockets, which are not available on the Windows target. Windows support is planned for a future transport/version track; no specific release or date is committed.

`musl`-based Linux builds (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`) are a **future / experimental candidate** and are **not** officially supported in `v0.1`. They will be promoted to an officially supported target only after CI build and runtime verification.

> Note: the DTJ v1 byte format and the `dtj` CLI reader are portable Rust code without platform-specific dependencies. The Windows limitation above applies to the **`dtj-agent` runtime and SDK integration**, not to reading already-completed `.dtj` session files.

## Features (v0.1)

- Append-only binary journal format (DTJ v1 byte contract)
- Versioned committed chunks with crash recovery
- Numeric string dictionary
- Typed inline event payloads
- Lossless session policy or flight-recorder overwrite mode
- Rebuildable indexes (not source of truth)
- Language-neutral: readable from Rust, Python, Go, TypeScript (SDK source available; see Status)
- CLI: `dtj read-session` (JSON output, structured errors)
- Local `dtj-agent` runtime (Unix domain socket transport)

## Non-features (v0.1 — no paid pack lock-in)

- No remote transport, HTTP, or cloud telemetry
- No compression, encryption, or replay
- No C ABI or native Unity plugin
- No JSONL import/export
- No paid licensing or activation requirements
- No Doc Hub-specific branding or manifests
- No `dtj tail`, `dtj info`, `dtj verify` commands

## Quick Start

```bash
# Install on macOS (Homebrew — recommended)
brew install Papa2Carlro/dtj/dtj

# Read a .dtj session
dtj read-session path/to/session.dtj
```

## Installation

Binary releases of `dtj` and `dtj-agent` are published as GitHub Release
artifacts on this repository. Each archive contains:

```
dtj
dtj-agent
README.md
LICENSE-MIT
LICENSE-APACHE
```

### Supported platforms

- **macOS**: Apple Silicon (`aarch64-apple-darwin`), Intel (`x86_64-apple-darwin`).
- **Linux**: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`.
- **Windows**: not supported in `v0.1` (see Platform Support section above).

### Download

Release archives are published under:

```
https://github.com/Papa2Carlro/dtj/releases
```

Artifact naming:

```
dtj-v<VERSION>-aarch64-apple-darwin.tar.gz
dtj-v<VERSION>-x86_64-apple-darwin.tar.gz
dtj-v<VERSION>-x86_64-unknown-linux-gnu.tar.gz
dtj-v<VERSION>-aarch64-unknown-linux-gnu.tar.gz
```

Pick the archive that matches your platform and architecture.

### macOS — Homebrew (recommended)

On macOS, the recommended install method is [Homebrew](https://brew.sh):

```bash
brew install Papa2Carlro/dtj/dtj
```

One command installs both binaries — `dtj` and `dtj-agent`. No separate
`dtj-agent` formula exists.

Supported architectures: **Apple Silicon (arm64)** and **Intel (x86_64)**.

The Homebrew tap is at [Papa2Carlro/homebrew-dtj](https://github.com/Papa2Carlro/homebrew-dtj).

### Linux — install.sh (recommended)

The official installer works on macOS and Linux and is the recommended
path on Linux. On macOS, Homebrew is the recommended install method;
install.sh remains a supported portable alternative.

```bash
curl -fsSL https://raw.githubusercontent.com/Papa2Carlro/dtj/master/install.sh -o install.sh
bash install.sh
```

Options:

```bash
# Install a specific version
bash install.sh --version 0.1.1

# Install to a custom directory
bash install.sh --install-dir ~/.local/bin

# Combine both
bash install.sh --version 0.1.1 --install-dir ~/.local/bin
```

The installer detects your platform and architecture, downloads the matching
release archive and its `SHA256SUMS` checksum file, verifies the archive before
extracting anything, then copies the `dtj` and `dtj-agent` binaries to the
target directory. A post-install smoke test confirms the binaries run correctly.

> **No `curl | sh`:** the installer script is always saved to disk first and
> reviewed before execution.

If `~/.local/bin` is not yet on your `PATH`, add this line to your shell config:

```bash
export PATH="${HOME}/.local/bin:${PATH}"
```

### Manual install

Extract the archive:

```bash
tar -xzf dtj-v<VERSION>-<TARGET>.tar.gz
cd dtj-v<VERSION>-<TARGET>
```

Place the two binaries on a directory listed in your `PATH`. Pick one of:

User-local (no root required):

```bash
mkdir -p ~/.local/bin
cp dtj dtj-agent ~/.local/bin/
```

Ensure `~/.local/bin` is on your `PATH`.

System-wide (requires root):

```bash
sudo cp dtj dtj-agent /usr/local/bin/
```

### Verify

```bash
dtj --version
dtj-agent --version
```

Each command prints `dtj <version>` and `dtj-agent <version>` respectively,
where `<version>` matches the archive you downloaded.

### Verify checksums (recommended)

Each release ships a `SHA256SUMS` file alongside the archives. Verify your
downloaded archive matches the published checksum:

macOS:

```bash
shasum -a 256 dtj-v<VERSION>-<TARGET>.tar.gz
```

Linux:

```bash
sha256sum dtj-v<VERSION>-<TARGET>.tar.gz
```

Compare the output against the corresponding line in `SHA256SUMS`.

> Note: binary signing is **not** implemented in `v0.1`. Checksum verification
> is the available integrity mechanism.

## Build from Source (Rust)

```bash
git clone https://github.com/Papa2Carlro/dtj.git
cd dtj
cargo build --release
```

## License

`MIT OR Apache-2.0` for all current packages (see `LICENSE-MIT` and `LICENSE-APACHE`).

## Communication

- Issues: https://github.com/Papa2Carlro/dtj/issues
- Discussions: https://github.com/Papa2Carlro/dtj/discussions
- Releases: https://github.com/Papa2Carlro/dtj/releases
- Spec: `specs/dtj-format-v1.md`