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

test_clean_tree() {
  echo "test_clean_tree"
  REPO="$(setup_repo)"
  assert_ok   check_clean_tree             # clean
  echo x > "$REPO/dirty.txt"
  assert_fail check_clean_tree             # untracked change
}

test_tag_absent() {
  echo "test_tag_absent"
  REPO="$(setup_repo)"
  assert_ok   check_tag_absent 9.9.9       # 없음
  git -C "$REPO" tag v9.9.9
  assert_fail check_tag_absent 9.9.9       # 로컬 태그 존재

  REPO="$(setup_repo)"                     # fresh repo for remote case
  git -C "$REPO" tag v8.8.8
  git -C "$REPO" push -q origin v8.8.8
  git -C "$REPO" tag -d v8.8.8             # 로컬만 삭제, 원격 유지
  assert_fail check_tag_absent 8.8.8       # 원격 태그 존재
}

test_warn_branch() {
  echo "test_warn_branch"
  REPO="$(setup_repo)"
  assert_ok warn_branch                    # main → 0
  git -C "$REPO" checkout -q -b feature
  assert_ok warn_branch                    # non-main → 여전히 0 (경고만)
}

test_wsl() {
  echo "test_wsl"
  local tmp; tmp="$(mktemp -d)"
  mkdir -p "$tmp/bin"
  printf '#!/usr/bin/env bash\ntrue\n' > "$tmp/bin/powershell.exe"
  chmod +x "$tmp/bin/powershell.exe"
  PROC_VERSION_FILE="$tmp/proc_ms"; echo "Linux x microsoft-standard-WSL2 x" > "$PROC_VERSION_FILE"
  PATH="$tmp/bin:$PATH" assert_ok   check_wsl                 # microsoft + powershell.exe 실행 가능
  printf '#!/usr/bin/env bash\nexit 1\n' > "$tmp/bin/powershell.exe"  # interop 꺼짐: 실행 실패
  PATH="$tmp/bin:$PATH" assert_fail check_wsl                 # powershell.exe 있으나 실행 실패
  printf '#!/usr/bin/env bash\ntrue\n' > "$tmp/bin/powershell.exe"    # 원복
  PROC_VERSION_FILE="$tmp/proc_plain"; echo "Linux x generic x" > "$PROC_VERSION_FILE"
  PATH="$tmp/bin:$PATH" assert_fail check_wsl                 # microsoft 없음
  PROC_VERSION_FILE="$tmp/proc_ms"
  PATH="/usr/bin:/bin" assert_fail check_wsl                  # microsoft 있으나 powershell.exe 없음
  rm -rf "$tmp"
}

# builds a sandbox where every check passes; sets globals GREEN_BIN, GREEN_REPO
# + the check globals. Call WITHOUT $() so the assignments reach the caller.
setup_green_env() {
  local tmp; tmp="$(mktemp -d)"
  GREEN_REPO="$(setup_repo)"
  GREEN_BIN="$tmp/bin"
  mkdir -p "$GREEN_BIN"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$GREEN_BIN/gh"
  cat > "$GREEN_BIN/powershell.exe" <<'EOF'
#!/usr/bin/env bash
printf '%s' 'C:\Users\tester'
EOF
  chmod +x "$GREEN_BIN/gh" "$GREEN_BIN/powershell.exe"
  KEY_FILE="$tmp/key"; : > "$KEY_FILE"
  KEY_PASS_FILE="$tmp/pass"; echo secret > "$KEY_PASS_FILE"
  PROC_VERSION_FILE="$tmp/proc"; echo microsoft > "$PROC_VERSION_FILE"
  SCRIPT_DIR="$GREEN_REPO"
  RELEASE_REPO="owner/repo"
  unset AMATE_WIN_BUILD TAURI_SIGNING_PRIVATE_KEY_PASSWORD
}

test_preflight_pass_and_fail() {
  echo "test_preflight_pass_and_fail"
  if ! command -v wslpath >/dev/null 2>&1; then echo "  skip: wslpath 없음"; return; fi
  setup_green_env
  PATH="$GREEN_BIN:$PATH" init_paths           # stub powershell + sets KEY_PW
  ( PATH="$GREEN_BIN:$PATH"; preflight 9.9.9 ); [[ $? -eq 0 ]] && ok "preflight all-green" || bad "preflight should pass"
  printf '#!/usr/bin/env bash\nexit 1\n' > "$GREEN_BIN/gh"
  ( PATH="$GREEN_BIN:$PATH"; preflight 9.9.9 ); [[ $? -ne 0 ]] && ok "preflight fails on gh" || bad "preflight should fail"
  rm -rf "$GREEN_BIN" "$GREEN_REPO"
}

test_dry_run_no_mutation() {
  echo "test_dry_run_no_mutation"
  if ! command -v wslpath >/dev/null 2>&1; then echo "  skip: wslpath 없음"; return; fi
  setup_green_env
  local before after rc
  before="$(git -C "$GREEN_REPO" tag -l)"
  ( PATH="$GREEN_BIN:$PATH"; main --dry-run 9.9.9 ) >/dev/null 2>&1; rc=$?
  after="$(git -C "$GREEN_REPO" tag -l)"
  [[ $rc -eq 0 ]] && ok "dry-run exit 0" || bad "dry-run exit=$rc"
  [[ "$before" == "$after" ]] && ok "dry-run created no tag" || bad "dry-run mutated tags: '$after'"
  rm -rf "$GREEN_BIN" "$GREEN_REPO"
}

test_version_fmt
test_init_paths
test_signing_key
test_gh_checks
test_clean_tree
test_tag_absent
test_warn_branch
test_wsl
test_preflight_pass_and_fail
test_dry_run_no_mutation

echo "---"
echo "PASS=$pass FAIL=$fail"
[[ $fail -eq 0 ]]
