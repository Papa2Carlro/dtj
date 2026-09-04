#!/usr/bin/env bash
# Package DTJ release artifacts for a single target triple.
#
# Usage:
#   scripts/release/package.sh <version> <target-triple> <bin-dir>
#
# Example:
#   scripts/release/package.sh 0.1.0 aarch64-apple-darwin target/release
#
# Produces:
#   dtj-v<version>-<target-triple>.tar.gz
# with deterministic contents inside dtj-v<version>-<target-triple>/ :
#   dtj, dtj-agent, README.md, LICENSE-MIT, LICENSE-APACHE
#
# The two binary version identities (dtj --version / dtj-agent --version)
# must already match <version>; this script asserts that before packaging.
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "usage: $0 <version> <target-triple> <bin-dir>" >&2
    exit 2
fi

VERSION="$1"
TARGET="$2"
BIN_DIR="$3"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ARTIFACT_NAME="dtj-v${VERSION}-${TARGET}.tar.gz"
STAGE_DIR="$(mktemp -d -t dtj-release-XXXXXX)"
ROOT_DIR="${STAGE_DIR}/dtj-v${VERSION}-${TARGET}"

cleanup() {
    rm -rf "${STAGE_DIR}"
}
trap cleanup EXIT

mkdir -p "${ROOT_DIR}"

cp "${BIN_DIR}/dtj"      "${ROOT_DIR}/dtj"
cp "${BIN_DIR}/dtj-agent" "${ROOT_DIR}/dtj-agent"
cp "${REPO_ROOT}/README.md"     "${ROOT_DIR}/README.md"
cp "${REPO_ROOT}/LICENSE-MIT"   "${ROOT_DIR}/LICENSE-MIT"
cp "${REPO_ROOT}/LICENSE-APACHE" "${ROOT_DIR}/LICENSE-APACHE"

# Assert binary version identity BEFORE packaging — fail fast.
EXPECTED_DTJ_VERSION_LINE="dtj ${VERSION}"
EXPECTED_AGENT_VERSION_LINE="dtj-agent ${VERSION}"

ACTUAL_DTJ_VERSION_LINE="$("${ROOT_DIR}/dtj" --version)"
ACTUAL_AGENT_VERSION_LINE="$("${ROOT_DIR}/dtj-agent" --version)"

if [[ "${ACTUAL_DTJ_VERSION_LINE}" != "${EXPECTED_DTJ_VERSION_LINE}" ]]; then
    echo "FAIL: dtj --version output mismatch:" >&2
    echo "  expected: ${EXPECTED_DTJ_VERSION_LINE}" >&2
    echo "  actual:   ${ACTUAL_DTJ_VERSION_LINE}" >&2
    exit 1
fi

if [[ "${ACTUAL_AGENT_VERSION_LINE}" != "${EXPECTED_AGENT_VERSION_LINE}" ]]; then
    echo "FAIL: dtj-agent --version output mismatch:" >&2
    echo "  expected: ${EXPECTED_AGENT_VERSION_LINE}" >&2
    echo "  actual:   ${ACTUAL_AGENT_VERSION_LINE}" >&2
    exit 1
fi

# Build tar.gz. Order is deterministic because we copy files into the root
# directory in a fixed sequence above (dtj, dtj-agent, README.md,
# LICENSE-MIT, LICENSE-APACHE), and tar appends entries in insertion order
# by default. `--owner=0 --group=0 --numeric-owner` keeps uid/gid stable
# across host machines. Both GNU tar and BSD tar (macOS default) accept
# these flags.
tar -C "${STAGE_DIR}" -czf "${ARTIFACT_NAME}" \
    --owner=0 --group=0 --numeric-owner \
    "dtj-v${VERSION}-${TARGET}"

# Emit the absolute path on stdout so the workflow can capture it.
echo "${PWD}/${ARTIFACT_NAME}"
