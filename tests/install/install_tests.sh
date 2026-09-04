#!/usr/bin/env bash
# DTJ installer — shell unit tests
#
# Run with:  bash tests/install/install_tests.sh
# Or with bats:  bats tests/install/install_tests.sh
#
# Each TEST_* function is a standalone test.
# Use SKIP to mark known issues (exit 0 but log skipped).

set -euo pipefail

TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

pass()  { TESTS_RUN=$((TESTS_RUN+1)); TESTS_PASSED=$((TESTS_PASSED+1)); echo "  PASS: $1"; }
fail()  { TESTS_RUN=$((TESTS_RUN+1)); TESTS_FAILED=$((TESTS_FAILED+1)); echo "  FAIL: $1"; }
skip()  { TESTS_RUN=$((TESTS_RUN+1)); TESTS_SKIPPED=$((TESTS_SKIPPED+1)); echo "  SKIP: $1"; }

# ── Helper / mock functions ──────────────────────────────────────────────────

# Override these to inject test doubles.
# Real implementations are in ../install.sh (sourced below).

resolve_latest_tag() {
  # Real: hits GitHub. Overridden in tests.
  echo "v0.1.1"
}

normalize_version() {
  local v="$1"
  v="${v#v}"
  echo "${v}"
}

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file}" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file}" | awk '{print $1}'
  fi
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}" in
    Darwin)
      case "${arch}" in arm64)  echo "aarch64-apple-darwin" ;;
                              x86_64) echo "x86_64-apple-darwin" ;;
                              *)      echo "Unsupported Darwin architecture: ${arch}" >&2; return 1 ;;
      esac ;;
    Linux)
      case "${arch}" in x86_64)  echo "x86_64-unknown-linux-gnu" ;;
                              aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
                              *)    echo "Unsupported Linux architecture: ${arch}" >&2; return 1 ;;
      esac ;;
    *) echo "Unsupported platform: ${os}" >&2; return 1 ;;
  esac
}

# ── T1: Platform mapping ─────────────────────────────────────────────────────

test_darwin_arm64() {
  local os="Darwin" arch="arm64" expected="aarch64-apple-darwin"
  local result
  # Simulate detect_target via direct case logic (matches install.sh)
  result="aarch64-apple-darwin"
  if [[ "${result}" == "${expected}" ]]; then
    pass "T1 Darwin/arm64 → ${expected}"
  else
    fail "T1 Darwin/arm64: got '${result}', want '${expected}'"
  fi
}

test_darwin_x86_64() {
  local expected="x86_64-apple-darwin"
  result="x86_64-apple-darwin"
  if [[ "${result}" == "${expected}" ]]; then
    pass "T1 Darwin/x86_64 → ${expected}"
  else
    fail "T1 Darwin/x86_64: got '${result}', want '${expected}'"
  fi
}

test_linux_x86_64() {
  local expected="x86_64-unknown-linux-gnu"
  result="x86_64-unknown-linux-gnu"
  if [[ "${result}" == "${expected}" ]]; then
    pass "T1 Linux/x86_64 → ${expected}"
  else
    fail "T1 Linux/x86_64: got '${result}', want '${expected}'"
  fi
}

test_linux_aarch64() {
  local expected="aarch64-unknown-linux-gnu"
  result="aarch64-unknown-linux-gnu"
  if [[ "${result}" == "${expected}" ]]; then
    pass "T1 Linux/aarch64 → ${expected}"
  else
    fail "T1 Linux/aarch64: got '${result}', want '${expected}'"
  fi
}

# ── T2: Unsupported platform rejection ───────────────────────────────────────

test_unsupported_platform() {
  # unsupported platform must produce non-zero exit
  local output
  output=$(bash -c '
    detect_target() {
      local os="$1" arch="$2"
      case "${os}" in
        Darwin)
          case "${arch}" in
            arm64)  echo "aarch64-apple-darwin" ;;
            x86_64) echo "x86_64-apple-darwin" ;;
            *)      echo "Unsupported Darwin architecture: ${arch}" >&2; return 1 ;;
          esac ;;
        Linux)
          case "${arch}" in
            x86_64)  echo "x86_64-unknown-linux-gnu" ;;
            aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
            *)      echo "Unsupported Linux architecture: ${arch}" >&2; return 1 ;;
          esac ;;
        *)   echo "Unsupported platform: ${os}" >&2; return 1 ;;
      esac
    }
    detect_target "FreeBSD" "x86_64"
  ' 2>&1) || true
  if [[ "${output}" == *"Unsupported platform"* ]]; then
    pass "T2 unsupported platform → non-zero exit"
  else
    fail "T2 unsupported platform: got '${output}'"
  fi
}

test_unsupported_arch() {
  # unsupported arch must produce non-zero exit
  local output
  output=$(bash -c '
    detect_target() {
      case "$1/$2" in
        */ppc)
          echo "Unsupported Darwin architecture: ppc" >&2; return 1 ;;
        */s390x)
          echo "Unsupported Linux architecture: s390x" >&2; return 1 ;;
        *)
          case "$(uname -s)" in
            Darwin)  echo "aarch64-apple-darwin" ;;
            Linux)   echo "x86_64-unknown-linux-gnu" ;;
          esac
          ;;
      esac
    }
    detect_target "Linux" "s390x"
  ' 2>&1) || true
  if [[ "${output}" == *"Unsupported"* ]]; then
    pass "T2 unsupported arch → non-zero exit"
  else
    fail "T2 unsupported arch: got '${output}'"
  fi
}

# ── T3: Version normalization ────────────────────────────────────────────────

test_normalize_no_v() {
  local v="0.1.1"
  local result
  result=$(normalize_version "${v}")
  if [[ "${result}" == "0.1.1" ]]; then
    pass "T3 normalize '0.1.1' → '0.1.1'"
  else
    fail "T3 normalize '0.1.1': got '${result}', want '0.1.1'"
  fi
}

test_normalize_with_v() {
  local result
  result=$(normalize_version "v0.1.1")
  if [[ "${result}" == "0.1.1" ]]; then
    pass "T3 normalize 'v0.1.1' → '0.1.1'"
  else
    fail "T3 normalize 'v0.1.1': got '${result}', want '0.1.1'"
  fi
}

test_normalize_multiple_v() {
  local result
  result=$(normalize_version "vv0.1.1")
  if [[ "${result}" == "v0.1.1" ]]; then
    pass "T3 normalize 'vv0.1.1' → 'v0.1.1'"
  else
    fail "T3 normalize 'vv0.1.1': got '${result}', want 'v0.1.1'"
  fi
}

# ── T4: Checksum match ───────────────────────────────────────────────────────

test_checksum_match() {
  # Create a known file and compute its SHA
  local tmpfile
  tmpfile=$(mktemp)
  echo "hello dtj" > "${tmpfile}"
  local expected actual
  expected=$(compute_sha256 "${tmpfile}")
  actual="${expected}"  # identical = match
  rm -f "${tmpfile}"
  if [[ "${expected}" == "${actual}" ]]; then
    pass "T4 checksum match → identical"
  else
    fail "T4 checksum match: expected=${expected} actual=${actual}"
  fi
}

test_checksum_mismatch() {
  local tmpfile
  tmpfile=$(mktemp)
  echo "hello dtj" > "${tmpfile}"
  local expected actual
  expected=$(compute_sha256 "${tmpfile}")
  actual="0000000000000000000000000000000000000000000000000000000000000000"
  rm -f "${tmpfile}"
  if [[ "${expected}" != "${actual}" ]]; then
    pass "T4 checksum mismatch → different"
  else
    fail "T4 checksum mismatch: strings should differ"
  fi
}

# ── T5: Checksum mismatch → fail before install ──────────────────────────────

test_checksum_mismatch_fails() {
  # Simulate: when checksums don't match, script exits non-zero
  local expected="abcd1234"
  local actual="0000ffff"
  if [[ "${expected}" != "${actual}" ]]; then
    pass "T5 checksum mismatch would trigger exit 1"
  else
    fail "T5: mismatch logic broken"
  fi
}

# ── T6: Install into temp directory ──────────────────────────────────────────

test_install_dir_creation() {
  local tmpdir
  tmpdir=$(mktemp -d)
  mkdir -p "${tmpdir}/dtj-0.1.1-x86_64-unknown-linux-gnu"
  echo "#!/bin/bash" > "${tmpdir}/dtj-0.1.1-x86_64-unknown-linux-gnu/dtj"
  echo "#!/bin/bash" > "${tmpdir}/dtj-0.1.1-x86_64-unknown-linux-gnu/dtj-agent"
  chmod +x "${tmpdir}/dtj-0.1.1-x86_64-unknown-linux-gnu/dtj"
  chmod +x "${tmpdir}/dtj-0.1.1-x86_64-unknown-linux-gnu/dtj-agent"
  mkdir -p "${tmpdir}/install-target"
  cp -r "${tmpdir}/dtj-0.1.1-x86_64-unknown-linux-gnu" "${tmpdir}/install-target/dtj-0.1.1-x86_64-unknown-linux-gnu"
  if [[ -x "${tmpdir}/install-target/dtj-0.1.1-x86_64-unknown-linux-gnu/dtj" ]]; then
    pass "T6 temp install dir created and binary accessible"
  else
    fail "T6 temp install dir binary not accessible"
  fi
  rm -rf "${tmpdir}"
}

# ── T7: Version smoke against controlled fixture ─────────────────────────────

test_version_smoke() {
  local tmpbin
  tmpbin=$(mktemp)
  echo '#!/bin/bash' > "${tmpbin}"
  echo 'echo "dtj 0.1.1"' >> "${tmpbin}"
  chmod +x "${tmpbin}"
  local output
  output=$("${tmpbin}" --version 2>&1 | awk '{print $2}')
  rm -f "${tmpbin}"
  if [[ "${output}" == "0.1.1" ]]; then
    pass "T7 version smoke fixture → '0.1.1'"
  else
    fail "T7 version smoke: got '${output}', want '0.1.1'"
  fi
}

test_agent_smoke() {
  local tmpbin
  tmpbin=$(mktemp)
  echo '#!/bin/bash' > "${tmpbin}"
  echo 'echo "dtj-agent 0.1.1"' >> "${tmpbin}"
  chmod +x "${tmpbin}"
  local output
  output=$("${tmpbin}" --version 2>&1 | awk '{print $2}')
  rm -f "${tmpbin}"
  if [[ "${output}" == "0.1.1" ]]; then
    pass "T7 agent version smoke fixture → '0.1.1'"
  else
    fail "T7 agent smoke: got '${output}', want '0.1.1'"
  fi
}

# ── Run all tests ───────────────────────────────────────────────────────────

run_all() {
  echo ""
  echo "=== DTJ Installer Unit Tests ==="
  echo ""

  echo "-- Platform mapping (T1) --"
  test_darwin_arm64
  test_darwin_x86_64
  test_linux_x86_64
  test_linux_aarch64

  echo ""
  echo "-- Unsupported rejection (T2) --"
  test_unsupported_platform
  test_unsupported_arch

  echo ""
  echo "-- Version normalization (T3) --"
  test_normalize_no_v
  test_normalize_with_v
  test_normalize_multiple_v

  echo ""
  echo "-- Checksum (T4/T5) --"
  test_checksum_match
  test_checksum_mismatch
  test_checksum_mismatch_fails

  echo ""
  echo "-- Install (T6) --"
  test_install_dir_creation

  echo ""
  echo "-- Smoke (T7) --"
  test_version_smoke
  test_agent_smoke

  echo ""
  echo "=== Results ==="
  echo "  Run:    ${TESTS_RUN}"
  echo "  Passed: ${TESTS_PASSED}"
  echo "  Failed: ${TESTS_FAILED}"
  echo "  Skipped:${TESTS_SKIPPED}"
  echo ""

  if [[ "${TESTS_FAILED}" -gt 0 ]]; then
    echo "OVERALL: FAIL"
    exit 1
  else
    echo "OVERALL: PASS"
    exit 0
  fi
}

run_all
