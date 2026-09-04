#!/usr/bin/env bash
# Release DTJ Go package to pkg.go.dev
#
# Usage:
#   cd packages/dtj-go && ../dtj-go/release.sh [version]
#
# Steps:
#   1. Validate version format (semver)
#   2. Update go.mod version if needed
#   3. Run tests
#   4. Create git tag v{version}
#   5. Push tag → GitHub → pkg.go.dev auto-indexes
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PKG_DIR"

# Version from args or prompt
VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
    echo -n "Enter version (e.g. 0.1.2): "
    read -r VERSION
fi

# Validate semver
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "ERROR: Invalid version '$VERSION' — use semver (e.g. 0.1.2)" >&2
    exit 1
fi

TAG="v${VERSION}"

echo "=== DTJ Go Release v${VERSION} ==="

# 1. Tidy and test
echo "[1/5] go mod tidy..."
go mod tidy

echo "[2/5] Running tests..."
go test ./...

# 2. Check if go.mod needs update (only when releasing)
CURRENT_VERSION=$(grep "^go " go.mod | awk '{print $2}')
echo "[3/5] go.mod uses Go ${CURRENT_VERSION}"

# 3. Create tag
echo "[4/5] Creating git tag ${TAG}..."
if git rev-parse "refs/tags/${TAG}" >/dev/null 2>&1; then
    echo "ERROR: Tag ${TAG} already exists" >&2
    exit 1
fi
git tag -a "$TAG" -m "Release ${TAG}"

# 4. Push
echo "[5/5] Pushing to origin..."
git push origin "$TAG"

echo ""
echo "=== Done ==="
echo "Tag ${TAG} pushed. pkg.go.dev will index within minutes."
echo ""
echo "Verify at: https://pkg.go.dev/github.com/Papa2Carlro/dtj/packages/dtj-go@v${VERSION}"
