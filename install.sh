#!/usr/bin/env bash
# DTJ installer — Unix/macOS
#
# Requires only: bash, curl, tar, sha256sum or shasum, mktemp
# No gh, git, jq, python, node, cargo, or rustc required.
#
# Usage:
#   ./install.sh                       # latest release
#   ./install.sh --version 0.1.1      # explicit (with or without 'v')
#   ./install.sh --version v0.1.1
#   ./install.sh --install-dir /path   # default: ~/.local/bin
#   ./install.sh --help

set -euo pipefail

REPO="Papa2Carlro/dtj"
INSTALL_DIR="${HOME}/.local/bin"
VERSION=""
HELP=""

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    --help|-h)
      HELP="yes"
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      HELP="yes"
      shift
      ;;
  esac
done

if [[ -n "$HELP" ]]; then
  cat << 'EOF'
DTJ installer

Usage:
  ./install.sh                       # latest GitHub release
  ./install.sh --version 0.1.1      # explicit version (with or without 'v')
  ./install.sh --version v0.1.1
  ./install.sh --install-dir /path  # default: ~/.local/bin
  ./install.sh --help

Supported platforms:
  Darwin/arm64  → aarch64-apple-darwin
  Darwin/x86_64 → x86_64-apple-darwin
  Linux/x86_64  → x86_64-unknown-linux-gnu
  Linux/aarch64 → aarch64-unknown-linux-gnu
EOF
  exit 0
fi

# ── Platform detection ────────────────────────────────────────────────────────

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}" in
    Darwin)
      case "${arch}" in
        arm64)  echo "aarch64-apple-darwin" ;;
        x86_64) echo "x86_64-apple-darwin" ;;
        *)
          echo "Unsupported Darwin architecture: ${arch}" >&2
          return 1
          ;;
      esac
      ;;
    Linux)
      case "${arch}" in
        x86_64)  echo "x86_64-unknown-linux-gnu" ;;
        aarch64) echo "aarch64-unknown-linux-gnu" ;;
        arm64)   echo "aarch64-unknown-linux-gnu" ;;
        *)
          echo "Unsupported Linux architecture: ${arch}" >&2
          return 1
          ;;
      esac
      ;;
    *)
      echo "Unsupported platform: ${os}" >&2
      return 1
      ;;
  esac
}

# ── Latest version resolution ──────────────────────────────────────────────────
# Uses GitHub redirect: curl -L follows the redirect to /releases/tag/vX.Y.Z
# No JSON API, no gh, no git.

resolve_latest_tag() {
  local latest_url="https://github.com/${REPO}/releases/latest"
  local redirected
  # -fsSL: fail silently, follow redirects, no progress, location header only
  # -o /dev/null: discard response body (not needed)
  # -w '%{url_effective}': write the final URL after redirects
  redirected=$(curl -fsSL -o /dev/null -w '%{url_effective}' "${latest_url}" 2>/dev/null)

  if [[ -z "${redirected}" ]]; then
    echo "Failed to resolve latest release URL (is GitHub reachable?)" >&2
    return 1
  fi

  # redirected looks like: https://github.com/Papa2Carlro/dtj/releases/tag/v0.1.1
  # Extract the last path component (the tag).
  local tag
  tag=$(echo "${redirected}" | awk -F/ '{print $NF}')
  if [[ -z "${tag}" ]]; then
    echo "Failed to extract tag from: ${redirected}" >&2
    return 1
  fi

  # Validate tag format: must be vMAJOR.MINOR.PATCH (e.g. v0.1.1)
  case "${tag}" in
    v[0-9]*.[0-9]*.[0-9]*)
      echo "${tag}"
      return 0
      ;;
    *)
      echo "Unexpected tag format from latest redirect: '${tag}'" >&2
      echo "Expected format: vMAJOR.MINOR.PATCH" >&2
      return 1
      ;;
  esac
}

# ── Version normalization ─────────────────────────────────────────────────────

normalize_version() {
  # Strips leading 'v' so callers get clean VERSION; preserves TAG with 'v'
  local v="$1"
  v="${v#v}"   # remove leading 'v' if present
  echo "${v}"
}

# ── SHA-256 computation ───────────────────────────────────────────────────────

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file}" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file}" | awk '{print $1}'
  else
    echo "No SHA-256 tool found (need sha256sum or shasum)" >&2
    return 1
  fi
}

# ── Entry point ────────────────────────────────────────────────────────────────

TARGET=$(detect_target)
echo "DTJ installer"
echo "Target: ${TARGET}"

if [[ -z "${VERSION}" ]]; then
  echo "Resolving latest release..."
  TAG=$(resolve_latest_tag)
else
  # Explicit version: normalize, build TAG, skip latest lookup
  VERSION=$(normalize_version "${VERSION}")
  TAG="v${VERSION}"
fi

echo "Version: ${VERSION:-$(echo "${TAG}" | sed 's/^v//')}"
echo "Install dir: ${INSTALL_DIR}"

# ── Require curl ───────────────────────────────────────────────────────────────

if ! command -v curl >/dev/null 2>&1; then
  echo "Required tool not found: curl" >&2
  exit 1
fi

# ── Build download URLs ────────────────────────────────────────────────────────
# Direct CDN URLs — no API, no auth required.

TARBALL="dtj-${TAG}-${TARGET}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

# ── Temp directory with cleanup ───────────────────────────────────────────────

TMPDIR=""
cleanup() {
  if [[ -n "${TMPDIR}" && -d "${TMPDIR}" ]]; then
    rm -rf "${TMPDIR}"
  fi
}
trap cleanup EXIT INT TERM

TMPDIR=$(mktemp -d)

# ── Download tarball and SHA256SUMS ──────────────────────────────────────────

echo "Downloading..."
curl -fL "${BASE_URL}/${TARBALL}" -o "${TMPDIR}/${TARBALL}"
curl -fL "${BASE_URL}/SHA256SUMS" -o "${TMPDIR}/SHA256SUMS"

# ── Verify files landed ───────────────────────────────────────────────────────

if [[ ! -f "${TMPDIR}/${TARBALL}" ]]; then
  echo "FAIL: ${TARBALL} not found after download" >&2
  exit 1
fi
if [[ ! -f "${TMPDIR}/SHA256SUMS" ]]; then
  echo "FAIL: SHA256SUMS not found after download" >&2
  exit 1
fi

# ── Checksum verification (BEFORE extraction) ─────────────────────────────────

echo "Verifying SHA-256..."

# SHA256SUMS contains lines like:  abc123  dtj-v0.1.1-aarch64-apple-darwin.tar.gz
# We look up by basename so the path in SHA256SUMS matches what we downloaded.
EXPECTED_HASH=$(grep -F " ${TARBALL}" "${TMPDIR}/SHA256SUMS" | awk '{print $1}')
if [[ -z "${EXPECTED_HASH}" ]]; then
  echo "FAIL: no checksum entry for ${TARBALL} in SHA256SUMS" >&2
  exit 1
fi

ACTUAL_HASH=$(compute_sha256 "${TMPDIR}/${TARBALL}")

# Case-insensitive compare
EXPECTED_LC=$(echo "${EXPECTED_HASH}" | tr '[:upper:]' '[:lower:]')
ACTUAL_LC=$(echo "${ACTUAL_HASH}" | tr '[:upper:]' '[:lower:]')

if [[ "${EXPECTED_LC}" != "${ACTUAL_LC}" ]]; then
  echo "FAIL: checksum mismatch" >&2
  echo "  expected: ${EXPECTED_LC}" >&2
  echo "  actual:   ${ACTUAL_LC}" >&2
  exit 1
fi
echo "Verifying SHA-256... OK"

# ── Extract ───────────────────────────────────────────────────────────────────

echo "Extracting..."
tar -xzf "${TMPDIR}/${TARBALL}" -C "${TMPDIR}"

# Archive extracts to dtj-v0.1.1-aarch64-apple-darwin/ (TAG-based name)
ARCHIVE_DIR="${TMPDIR}/dtj-${TAG}-${TARGET}"
if [[ ! -d "${ARCHIVE_DIR}" ]]; then
  echo "FAIL: unexpected archive structure, dtj dir not found" >&2
  exit 1
fi

if [[ ! -x "${ARCHIVE_DIR}/dtj" ]]; then
  echo "FAIL: dtj binary not found or not executable in archive" >&2
  exit 1
fi
if [[ ! -x "${ARCHIVE_DIR}/dtj-agent" ]]; then
  echo "FAIL: dtj-agent binary not found or not executable in archive" >&2
  exit 1
fi

# ── Install (atomic: copy to temp file then rename) ───────────────────────────

echo "Installing..."
mkdir -p "${INSTALL_DIR}"

# Copy to a hidden temp file in the target dir, then rename atomically.
# This avoids leaving a half-written binary if the copy is interrupted.
DTJ_TMP="${INSTALL_DIR}/.dtj-$$"
AGENT_TMP="${INSTALL_DIR}/.dtj-agent-$$"

cp "${ARCHIVE_DIR}/dtj" "${DTJ_TMP}"
cp "${ARCHIVE_DIR}/dtj-agent" "${AGENT_TMP}"

# Set permissions before making visible
chmod 0755 "${DTJ_TMP}"
chmod 0755 "${AGENT_TMP}"

# Atomic rename to final names
mv "${DTJ_TMP}" "${INSTALL_DIR}/dtj"
mv "${AGENT_TMP}" "${INSTALL_DIR}/dtj-agent"

# ── Post-install version smoke test ──────────────────────────────────────────

INSTALLED_VERSION=$("${INSTALL_DIR}/dtj" --version 2>&1 | awk '{print $2}')
EXPECTED_VERSION="${VERSION:-$(echo "${TAG}" | sed 's/^v//')}"
if [[ "${INSTALLED_VERSION}" != "${EXPECTED_VERSION}" ]]; then
  echo "FAIL: version smoke mismatch (expected ${EXPECTED_VERSION}, got ${INSTALLED_VERSION})" >&2
  exit 1
fi

INSTALLED_AGENT_VERSION=$("${INSTALL_DIR}/dtj-agent" --version 2>&1 | awk '{print $2}')
if [[ "${INSTALLED_AGENT_VERSION}" != "${EXPECTED_VERSION}" ]]; then
  echo "FAIL: agent version smoke mismatch (expected ${EXPECTED_VERSION}, got ${INSTALLED_AGENT_VERSION})" >&2
  exit 1
fi

echo "${INSTALL_DIR}/dtj --version"
"${INSTALL_DIR}/dtj" --version
echo "${INSTALL_DIR}/dtj-agent --version"
"${INSTALL_DIR}/dtj-agent" --version

# ── PATH guidance ─────────────────────────────────────────────────────────────

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*)
    ;;
  *)
    echo ""
    echo "Add ${INSTALL_DIR} to your PATH:"
    echo "  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
    ;;
esac

echo ""
echo "Installed successfully."
