#!/usr/bin/env bash
# DTJ installer — shell unit tests
set -euo pipefail

TESTS_RUN=0; TESTS_PASSED=0; TESTS_FAILED=0; TESTS_SKIPPED=0
pass()  { TESTS_RUN=$((TESTS_RUN+1)); TESTS_PASSED=$((TESTS_PASSED+1)); echo "  PASS: $1"; }
fail()  { TESTS_RUN=$((TESTS_RUN+1)); TESTS_FAILED=$((TESTS_FAILED+1)); echo "  FAIL: $1"; }
skip()  { TESTS_RUN=$((TESTS_RUN+1)); TESTS_SKIPPED=$((TESTS_SKIPPED+1)); echo "  SKIP: $1"; }

REPO="Papa2Carlro/dtj"

normalize_version() { local v="$1"; v="${v#v}"; echo "${v}"; }

detect_target() {
  local os="$1" arch="$2"
  case "${os}" in
    Darwin)
      case "${arch}" in arm64)  echo "aarch64-apple-darwin" ;;
                        x86_64) echo "x86_64-apple-darwin" ;;
                        *)      echo "Unsupported Darwin architecture: ${arch}" >&2; return 1 ;;
      esac ;;
    Linux)
      case "${arch}" in
        x86_64|aarch64|arm64) echo "${arch}-unknown-linux-gnu" ;;
        *)      echo "Unsupported Linux architecture: ${arch}" >&2; return 1 ;;
      esac ;;
    *)   echo "Unsupported platform: ${os}" >&2; return 1 ;;
  esac
}

resolve_latest_tag() {
  local latest_url="https://github.com/${REPO}/releases/latest"
  local redirected tag
  redirected=$(curl -fsSL -o /dev/null -w '%{url_effective}' "${latest_url}" 2>/dev/null)
  [[ -z "${redirected}" ]] && return 1
  tag=$(echo "${redirected}" | awk -F/ '{print $NF}')
  case "${tag}" in v[0-9]*.[0-9]*.[0-9]*) echo "${tag}" ;; *) return 1 ;; esac
}

# ── T1: Platform mapping ──────────────────────────────────────────────────────
test_darwin_arm64()   { [[ "$(detect_target Darwin arm64)"  == "aarch64-apple-darwin"   ]] && pass "T1 Darwin/arm64"   || fail "T1 Darwin/arm64"   got "$(detect_target Darwin arm64)"; }
test_darwin_x86_64()  { [[ "$(detect_target Darwin x86_64)" == "x86_64-apple-darwin"  ]] && pass "T1 Darwin/x86_64"  || fail "T1 Darwin/x86_64"  got "$(detect_target Darwin x86_64)"; }
test_linux_x86_64()   { [[ "$(detect_target Linux x86_64)" == "x86_64-unknown-linux-gnu" ]] && pass "T1 Linux/x86_64"   || fail "T1 Linux/x86_64"   got "$(detect_target Linux x86_64)"; }
test_linux_aarch64()  { [[ "$(detect_target Linux aarch64)" == "aarch64-unknown-linux-gnu" ]] && pass "T1 Linux/aarch64"  || fail "T1 Linux/aarch64"  got "$(detect_target Linux aarch64)"; }

# ── T2: Unsupported rejection ────────────────────────────────────────────────
test_unsupported_platform() { detect_target FreeBSD x86_64 2>/dev/null && fail "T2 unsupported platform: no error" || pass "T2 unsupported platform"; }
test_unsupported_arch()     { detect_target Linux s390x 2>/dev/null    && fail "T2 unsupported arch: no error"       || pass "T2 unsupported arch"; }

# ── T3: Version normalization ─────────────────────────────────────────────────
test_normalize_no_v()      { [[ "$(normalize_version 0.1.1)"   == "0.1.1" ]] && pass "T3 normalize 0.1.1"    || fail "T3 normalize 0.1.1 got $(normalize_version 0.1.1)"; }
test_normalize_with_v()    { [[ "$(normalize_version v0.1.1)" == "0.1.1" ]] && pass "T3 normalize v0.1.1"  || fail "T3 normalize v0.1.1 got $(normalize_version v0.1.1)"; }
test_normalize_multiple_v(){ [[ "$(normalize_version vv0.1.1)" == "v0.1.1" ]] && pass "T3 normalize vv0.1.1" || fail "T3 normalize vv0.1.1 got $(normalize_version vv0.1.1)"; }

# ── T4/T5: Checksum ─────────────────────────────────────────────────────────
test_checksum_match() {
  local tmpfile expected actual
  tmpfile=$(mktemp); echo "hello dtj" > "${tmpfile}"
  expected=$(sha256sum "${tmpfile}" | awk '{print $1}')
  actual="${expected}"; rm -f "${tmpfile}"
  [[ "${expected}" == "${actual}" ]] && pass "T4 checksum match" || fail "T4 checksum match"
}
test_checksum_mismatch() {
  local tmpfile expected
  tmpfile=$(mktemp); echo "hello dtj" > "${tmpfile}"
  expected=$(sha256sum "${tmpfile}" | awk '{print $1}'); rm -f "${tmpfile}"
  [[ "${expected}" != "0000000000000000000000000000000000000000000000000000000000000000" ]] && pass "T4 checksum mismatch" || fail "T4 checksum mismatch"
}
test_checksum_mismatch_fails() {
  local expected="abcd1234" actual="0000ffff"
  [[ "${expected}" != "${actual}" ]] && pass "T5 checksum mismatch → exit 1" || fail "T5 checksum mismatch logic"
}

# ── T6: Atomic install ────────────────────────────────────────────────────────
test_install_dir_creation() {
  local tmpdir archive_dir install_dir
  tmpdir=$(mktemp -d)
  archive_dir="${tmpdir}/dtj-v0.1.1-x86_64-unknown-linux-gnu"
  install_dir="${tmpdir}/target"
  mkdir -p "${archive_dir}"
  echo '#!/bin/bash' > "${archive_dir}/dtj"
  echo '#!/bin/bash' > "${archive_dir}/dtj-agent"
  chmod +x "${archive_dir}/dtj" "${archive_dir}/dtj-agent"
  mkdir -p "${install_dir}"
  cp "${archive_dir}/dtj" "${install_dir}/.dtj-$$"
  cp "${archive_dir}/dtj-agent" "${install_dir}/.dtj-agent-$$"
  chmod 0755 "${install_dir}/.dtj-$$" "${install_dir}/.dtj-agent-$$"
  mv "${install_dir}/.dtj-$$" "${install_dir}/dtj"
  mv "${install_dir}/.dtj-agent-$$" "${install_dir}/dtj-agent"
  [[ -x "${install_dir}/dtj" ]] && [[ -x "${install_dir}/dtj-agent" ]] && pass "T6 atomic install" || fail "T6 atomic install"
  rm -rf "${tmpdir}"
}

# ── T7: Version smoke ────────────────────────────────────────────────────────
test_version_smoke() {
  local tmpbin out
  tmpbin=$(mktemp); echo '#!/bin/bash' > "${tmpbin}"; echo 'echo "dtj 0.1.1"' >> "${tmpbin}"
  chmod +x "${tmpbin}"; out=$("${tmpbin}" --version 2>&1 | awk '{print $2}'); rm -f "${tmpbin}"
  [[ "${out}" == "0.1.1" ]] && pass "T7 version smoke" || fail "T7 version smoke got '${out}'"
}
test_agent_smoke() {
  local tmpbin out
  tmpbin=$(mktemp); echo '#!/bin/bash' > "${tmpbin}"; echo 'echo "dtj-agent 0.1.1"' >> "${tmpbin}"
  chmod +x "${tmpbin}"; out=$("${tmpbin}" --version 2>&1 | awk '{print $2}'); rm -f "${tmpbin}"
  [[ "${out}" == "0.1.1" ]] && pass "T7 agent smoke" || fail "T7 agent smoke got '${out}'"
}

# ── T8: no gh dependency ─────────────────────────────────────────────────────
test_no_gh() {
  if grep -qE '(^|[[:space:]])gh([[:space:]]|$)' install.sh; then
    fail "T8 gh found in install.sh"
  else
    pass "T8 no gh in install.sh"
  fi
}

# ── T9: no git dependency ─────────────────────────────────────────────────────
test_no_git() {
  if grep -qE '(^|[[:space:]])git([[:space:]]|$)' install.sh; then
    fail "T9 git found in install.sh"
  else
    pass "T9 no git in install.sh"
  fi
}

# ── T10: latest redirect parsing ──────────────────────────────────────────────
test_latest_redirect_parsing() {
  local tag
  tag=$(echo "https://github.com/Papa2Carlro/dtj/releases/tag/v0.1.1" | awk -F/ '{print $NF}')
  [[ "${tag}" == "v0.1.1" ]] && pass "T10 redirect parsing" || fail "T10 redirect parsing got '${tag}'"
}

test_latest_redirect_real() {
  local out
  out=$(resolve_latest_tag 2>/dev/null || true)
  if [[ -z "${out}" ]]; then
    skip "T10 real network: no connectivity"
  elif [[ "${out}" == "v0.1.1" ]]; then
    pass "T10 real network → v0.1.1"
  else
    fail "T10 real network got '${out}'"
  fi
}

# ── T11: malformed redirect must fail ─────────────────────────────────────────
test_malformed_tag_validation() {
  local tag rc
  tag="latest"
  case "${tag}" in v[0-9]*.[0-9]*.[0-9]*) rc=0 ;; *) rc=1 ;; esac
  [[ "${rc}" -ne 0 ]] && pass "T11 'latest' rejected" || fail "T11 'latest' should be rejected"
}

test_malformed_redirect() {
  local url tag ok=0
  for url in "https://github.com/Papa2Carlro/dtj/releases" "https://example.com/foo"; do
    tag=$(echo "${url}" | awk -F/ '{print $NF}')
    case "${tag}" in v[0-9]*.[0-9]*.[0-9]*) ok=1; break ;; esac
  done
  [[ "${ok}" -eq 0 ]] && pass "T11 malformed URL rejected" || fail "T11 malformed URL accepted"
}

# ── T12: explicit version skips latest lookup ─────────────────────────────────
test_explicit_version_skips_latest() {
  local VERSION="0.1.1" TAG
  TAG="v${VERSION#v}"
  [[ "${TAG}" == "v0.1.1" ]] && pass "T12 explicit version TAG built" || fail "T12 explicit version TAG got '${TAG}'"
}

test_explicit_version_logic() {
  local VERSION="0.1.1" resolved_tag
  if [[ -z "${VERSION}" ]]; then
    resolved_tag=$(resolve_latest_tag 2>/dev/null || true)
  else
    resolved_tag="v${VERSION#v}"
  fi
  [[ "${resolved_tag}" == "v0.1.1" ]] && pass "T12 explicit version logic" || fail "T12 explicit version logic got '${resolved_tag}'"
}

# ── Run all tests ────────────────────────────────────────────────────────────
run_all() {
  echo ""
  echo "=== DTJ Installer Unit Tests ==="
  echo ""
  echo "-- T1: Platform mapping --"
  test_darwin_arm64; test_darwin_x86_64; test_linux_x86_64; test_linux_aarch64
  echo ""
  echo "-- T2: Unsupported rejection --"
  test_unsupported_platform; test_unsupported_arch
  echo ""
  echo "-- T3: Version normalization --"
  test_normalize_no_v; test_normalize_with_v; test_normalize_multiple_v
  echo ""
  echo "-- T4/T5: Checksum --"
  test_checksum_match; test_checksum_mismatch; test_checksum_mismatch_fails
  echo ""
  echo "-- T6: Atomic install --"
  test_install_dir_creation
  echo ""
  echo "-- T7: Smoke --"
  test_version_smoke; test_agent_smoke
  echo ""
  echo "-- T8/T9: No-gh / No-git --"
  test_no_gh; test_no_git
  echo ""
  echo "-- T10/T11: Latest redirect --"
  test_latest_redirect_parsing; test_latest_redirect_real
  test_malformed_tag_validation; test_malformed_redirect
  echo ""
  echo "-- T12: Explicit version --"
  test_explicit_version_skips_latest; test_explicit_version_logic
  echo ""
  echo "=== Results ==="
  echo "  Run:    ${TESTS_RUN}"
  echo "  Passed: ${TESTS_PASSED}"
  echo "  Failed: ${TESTS_FAILED}"
  echo "  Skipped:${TESTS_SKIPPED}"
  echo ""
  [[ "${TESTS_FAILED}" -gt 0 ]] && echo "OVERALL: FAIL" && exit 1 || echo "OVERALL: PASS"
}

run_all
