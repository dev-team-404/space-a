#!/usr/bin/env bash
# Unit tests for release-amate.sh preflight/helpers (no build/publish).
# Run: bash a-mate/scripts/test-release-amate.sh
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUT="$HERE/release-amate.sh"

pass=0; fail=0
ok()  { echo "  ok:   $1"; pass=$((pass+1)); }
bad() { echo "  FAIL: $1" >&2; fail=$((fail+1)); }
assert_ok()   { if "$@"; then ok "$*"; else bad "$* (expected 0, got $?)"; fi; }
assert_fail() { if "$@"; then bad "$* (expected non-zero, got 0)"; else ok "$* -> non-zero"; fi; }

# temp git repo with a bare origin on branch main; echoes the work-tree path
setup_repo() {
  local dir; dir="$(mktemp -d)"
  git init -q "$dir/work"
  git -C "$dir/work" config user.email t@t
  git -C "$dir/work" config user.name t
  git -C "$dir/work" commit -q --allow-empty -m init
  git -C "$dir/work" branch -M main
  git init -q --bare "$dir/origin.git"
  git -C "$dir/work" remote add origin "$dir/origin.git"
  git -C "$dir/work" push -q origin main
  echo "$dir/work"
}

# writes a `gh` stub into $1/bin that exits with $STUB_GH_EXIT (default 0)
make_gh_stub() {
  mkdir -p "$1/bin"
  cat > "$1/bin/gh" <<'EOF'
#!/usr/bin/env bash
exit ${STUB_GH_EXIT:-0}
EOF
  chmod +x "$1/bin/gh"
}

# shellcheck disable=SC1090
source "$SUT"
set +eu +o pipefail   # SUT enabled `set -euo pipefail`; relax so all tests run

test_version_fmt() {
  echo "test_version_fmt"
  assert_ok   check_version_fmt 0.2.0
  assert_ok   check_version_fmt 10.0.11
  assert_fail check_version_fmt v0.2.0
  assert_fail check_version_fmt 0.2
  assert_fail check_version_fmt abc
}

test_init_paths() {
  echo "test_init_paths"
  if ! command -v wslpath >/dev/null 2>&1; then echo "  skip: wslpath 없음 (WSL 아님)"; return; fi
  local tmp repo; tmp="$(mktemp -d)"; repo="$(setup_repo)"
  mkdir -p "$tmp/bin"
  cat > "$tmp/bin/powershell.exe" <<'EOF'
#!/usr/bin/env bash
printf '%s' 'C:\Users\tester'
EOF
  chmod +x "$tmp/bin/powershell.exe"
  SCRIPT_DIR="$repo"; unset AMATE_WIN_BUILD
  PATH="$tmp/bin:$PATH" init_paths
  [[ "$WIN_BUILD_WIN" == 'C:\Users\tester\amate-build\a-mate' ]] && ok "WIN_BUILD_WIN" || bad "WIN_BUILD_WIN=$WIN_BUILD_WIN"
  [[ "$WIN_BUILD_WSL" == '/mnt/c/Users/tester/amate-build/a-mate' ]] && ok "WIN_BUILD_WSL" || bad "WIN_BUILD_WSL=$WIN_BUILD_WSL"
  [[ "$REPO" == "$repo" ]] && ok "REPO derived" || bad "REPO=$REPO"
  rm -rf "$tmp"
}

test_signing_key() {
  echo "test_signing_key"
  local tmp; tmp="$(mktemp -d)"
  KEY_FILE="$tmp/key"; KEY_PW=""
  assert_fail check_signing_key            # 키 파일 없음
  : > "$KEY_FILE"; KEY_PW=""
  assert_fail check_signing_key            # 키는 있으나 암호 빔
  KEY_PW="secret"
  assert_ok   check_signing_key            # 둘 다 있음
  rm -rf "$tmp"
}

test_gh_checks() {
  echo "test_gh_checks"
  local tmp; tmp="$(mktemp -d)"; make_gh_stub "$tmp"
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=0 assert_ok   check_gh_auth
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=1 assert_fail check_gh_auth
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=0 assert_ok   check_release_repo
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=1 assert_fail check_release_repo
  rm -rf "$tmp"
}

test_version_fmt
test_init_paths
test_signing_key
test_gh_checks

echo "---"
echo "PASS=$pass FAIL=$fail"
[[ $fail -eq 0 ]]
