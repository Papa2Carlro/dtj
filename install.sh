#!/usr/bin/env bash
# DTJ installer — Unix/macOS
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

resolve_latest_tag() {
  # Use gh (authenticated) to resolve latest tag; fall back to git ls-remote
  if command -v gh >/dev/null 2>&1; then
    local tag
    tag=$(gh api "repos/${REPO}/releases/latest" --jq '.tag_name' 2>/dev/null)
    if [[ -n "${tag}" ]]; then
      echo "${tag}"
      return 0
    fi
  fi
  # Fallback: git ls-remote (works for public repos without GitHub auth)
  local tag
  tag=$(git ls-remote --tags "https://github.com/${REPO}.git" 2>/dev/null | \
    grep -v '\^{}' | awk -F/ '{print $NF}' | sort -V | tail -1)
  if [[ -n "${tag}" ]]; then
    echo "${tag}"
    return 0
  fi
  echo "Failed to resolve latest release tag" >&2
  return 1
}

# ── Version normalization ─────────────────────────────────────────────────────

normalize_version() {
  # Strips leading 'v' so callers get clean VERSION; preserves TAG with 'v'
  local v="$1"
  v="${v#v}"   # remove leading 'v' if present
  echo "${v}"
}

# ── Checksum verification ─────────────────────────────────────────────────────

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
  # Normalize: strip leading 'v', then re-add for tag
  VERSION=$(normalize_version "${VERSION}")
  TAG="v${VERSION}"
fi

echo "Version: ${VERSION:-$(echo "${TAG}" | sed 's/^v//')}"
echo "Install dir: ${INSTALL_DIR}"

# ── Download ─────────────────────────────────────────────────────────────────

REQUIRED_TOOLS="curl"
for tool in ${REQUIRED_TOOLS}; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "Required tool not found: ${tool}" >&2
    exit 1
  fi
done

TARBALL="dtj-${TAG}-${TARGET}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

TMPDIR=""
cleanup() {
  if [[ -n "${TMPDIR}" && -d "${TMPDIR}" ]]; then
    rm -rf "${TMPDIR}"
  fi
}
trap cleanup EXIT INT TERM

TMPDIR=$(mktemp -d)
echo "Downloading..."
if command -v gh >/dev/null 2>&1 && \
   gh release download "${TAG}" --repo "${REPO}" --dir "${TMPDIR}" \
     --pattern "${TARBALL}" --pattern "SHA256SUMS" 2>/dev/null; then
  :  # gh download succeeded, files are in TMPDIR
else
  # Fallback: use GitHub API to resolve CDN download URLs
  local download_urls
  download_urls=$(gh api "repos/${REPO}/releases/tags/${TAG}" \
    --jq '.assets[] | select(.name == "'"${TARBALL}"'" or .name == "SHA256SUMS") | .url' 2>/dev/null)
  if [[ -z "${download_urls}" ]]; then
    echo "Failed to resolve download URLs for ${TAG}. Is the release published?" >&2
    exit 1
  fi
  local url
  while IFS= read -r url; do
    # Decode percent-encoding: %20 → space, %3A → colon, etc.
    local fname
    fname=$(basename "${url}" | python3 -c 'import sys,urllib.parse;print(urllib.parse.unquote(sys.stdin.read().strip()))' 2>/dev/null \
      || basename "${url}" | sed 's/%20/ /g;s/%3A/:/g')
    curl -fsL "${url}" -o "${TMPDIR}/${fname}"
  done <<< "${download_urls}"
fi

# Verify files landed
if [[ ! -f "${TMPDIR}/${TARBALL}" ]]; then
  echo "FAIL: ${TARBALL} not found after download" >&2
  exit 1
fi
if [[ ! -f "${TMPDIR}/SHA256SUMS" ]]; then
  echo "FAIL: SHA256SUMS not found after download" >&2
  exit 1
fi

# ── Checksum verification ──────────────────────────────────────────────────────

echo "Verifying SHA-256..."

# Extract expected hash for our exact basename (basenames only in SHA256SUMS)
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

# ── Install ───────────────────────────────────────────────────────────────────

echo "Installing..."
mkdir -p "${INSTALL_DIR}"

if command -v install >/dev/null 2>&1; then
  install -m 0755 "${ARCHIVE_DIR}/dtj" "${INSTALL_DIR}/dtj"
  install -m 0755 "${ARCHIVE_DIR}/dtj-agent" "${INSTALL_DIR}/dtj-agent"
else
  cp "${ARCHIVE_DIR}/dtj" "${INSTALL_DIR}/dtj"
  cp "${ARCHIVE_DIR}/dtj-agent" "${INSTALL_DIR}/dtj-agent"
  chmod 0755 "${INSTALL_DIR}/dtj"
  chmod 0755 "${INSTALL_DIR}/dtj-agent"
fi

# ── Smoke test ────────────────────────────────────────────────────────────────

INSTALLED_VERSION=$("${INSTALL_DIR}/dtj" --version 2>&1 | awk '{print $2}')
if [[ "${INSTALLED_VERSION}" != "${VERSION:-${TAG#v}}" ]]; then
  echo "FAIL: version smoke mismatch (expected ${VERSION:-${TAG#v}}, got ${INSTALLED_VERSION})" >&2
  exit 1
fi

INSTALLED_AGENT_VERSION=$("${INSTALL_DIR}/dtj-agent" --version 2>&1 | awk '{print $2}')
if [[ "${INSTALLED_AGENT_VERSION}" != "${VERSION:-${TAG#v}}" ]]; then
  echo "FAIL: agent version smoke mismatch (expected ${VERSION:-${TAG#v}}, got ${INSTALLED_AGENT_VERSION})" >&2
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
