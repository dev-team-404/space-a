---
status: done
archived: 2026-07-25
---

# release-amate.sh 팀 범용화 + 프리플라이트 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `a-mate/scripts/release-amate.sh`를 팀 소수가 각자 WSL에서 안전하게 실행하도록 개인 환경 하드코딩을 걷어내고, 변경 이전 프리플라이트 검증 + `--dry-run` + 순수 bash 단위 테스트를 추가한다.

**Architecture:** 스크립트를 `source` 가능하게 재구성한다(`main()` + `BASH_SOURCE` 가드). 환경 유도(저장소 루트·Windows 빌드 폴더·키 암호)는 `init_paths()`로, 검증은 개별 `check_*` 함수로 분리해, 하네스가 함수를 직접 호출·검증할 수 있게 한다. 빌드·서명·발행 로직은 그대로 두고 유도된 변수만 소비한다.

**Tech Stack:** Bash 5.x, `git`, `gh`, `powershell.exe`/`wslpath`(WSL), `node`/`sed`(버전 갱신). 테스트는 프레임워크 없이 순수 bash + PATH stub + 임시 git 저장소.

**설계 문서:** [../specs/2026-07-25-release-script-team-generalization-design.md](../specs/2026-07-25-release-script-team-generalization-design.md)

## Global Constraints

- **플랫폼:** a-mate는 Windows 전용. 릴리스는 **WSL에서 실행**하며 `powershell.exe`로 네이티브 Windows 빌드를 구동한다.
- **빌드/E2E 검증 불가(이 세션):** 이 WSL 세션에서 a-mate를 빌드/테스트할 수 없다. 실제 빌드·서명·발행 E2E는 **작성자 환경에서 수동** 검증. 여기서는 `bash -n` + 프리플라이트 단위 테스트까지만.
- **도구:** `shellcheck`·`bats` 미설치 → 구문 검사는 `bash -n`, 테스트는 순수 bash 하네스.
- **서명 키:** 팀 공용 단일 키페어(`tauri.conf.json`의 pubkey와 짝). 각자 `~/.tauri/a-mate-updater.{key,pass}`에 배치. **커밋 금지.**
- **릴리스 저장소:** `dev-team-404/a-mate-releases` — updater가 익명 fetch하므로 **반드시 public**.
- **버전 형식:** `^[0-9]+\.[0-9]+\.[0-9]+$` (예: `0.2.0`). 버전은 인자로 명시(자동 증가 없음).
- **커밋 규칙:** Conventional Commits, 영어. `Co-Authored-By` 트레일러 **금지**.
- **오버라이드 env:** `AMATE_RELEASE_REPO`, `AMATE_WIN_BUILD`(Windows 형식 경로), `AMATE_KEY_FILE`, `AMATE_KEY_PASS_FILE`.

---

### Task 1: Sourceable 재구성 + 하드코딩 제거(init_paths) + 테스트 하네스 + check_version_fmt

**Files:**
- Modify: `a-mate/scripts/release-amate.sh` (전체 재구성)
- Create: `a-mate/scripts/test-release-amate.sh`

**Interfaces:**
- Produces:
  - `check_version_fmt <version>` → 0 if `^[0-9]+\.[0-9]+\.[0-9]+$`, else prints error, returns 1
  - `init_paths()` → sets globals `REPO`, `CONF`, `CARGO`, `WIN_BUILD_WIN`, `WIN_BUILD_WSL`, `KEY_PW` (reads `SCRIPT_DIR`, `AMATE_WIN_BUILD`, `KEY_PASS_FILE`)
  - `main(...)` gated by `if [[ "${BASH_SOURCE[0]}" == "${0}" ]]`
  - Top-level globals: `RELEASE_REPO`, `KEY_FILE`, `KEY_PASS_FILE`, `PROC_VERSION_FILE`, `SCRIPT_DIR`
  - Test helpers: `ok`, `bad`, `assert_ok`, `assert_fail`, `setup_repo` (echoes a temp git repo path with a bare `origin` on branch `main`)

- [ ] **Step 1: 하네스 작성 (failing test)**

Create `a-mate/scripts/test-release-amate.sh`:

```bash
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

test_version_fmt
test_init_paths

echo "---"
echo "PASS=$pass FAIL=$fail"
[[ $fail -eq 0 ]]
```

- [ ] **Step 2: 테스트 실행 → 실패 확인**

Run: `bash a-mate/scripts/test-release-amate.sh`
Expected: FAIL — `source` 시점에 `check_version_fmt`/`init_paths` 미정의 (현재 스크립트는 함수 없이 선형 실행). 하네스가 비정상 종료하거나 `check_version_fmt: command not found`.

- [ ] **Step 3: release-amate.sh 재구성 (minimal implementation)**

Replace the **entire** `a-mate/scripts/release-amate.sh` with:

```bash
#!/usr/bin/env bash
# a-mate 릴리스: 버전 갱신 → 서명 빌드 → latest.json 생성 → GitHub Release 발행
# 사용법 (WSL): bash a-mate/scripts/release-amate.sh [--dry-run] <version>   예: 0.2.0
#
# 사전 준비:
#  - updater 개인키: ~/.tauri/a-mate-updater.key (팀 공용 키, 커밋 금지)
#  - 개인키 암호:   ~/.tauri/a-mate-updater.pass (또는 TAURI_SIGNING_PRIVATE_KEY_PASSWORD)
#  - 공개 릴리스 저장소: dev-team-404/a-mate-releases (최초 1회 gh repo create --public)
# 오버라이드 env: AMATE_RELEASE_REPO, AMATE_WIN_BUILD, AMATE_KEY_FILE, AMATE_KEY_PASS_FILE
set -euo pipefail

RELEASE_REPO="${AMATE_RELEASE_REPO:-dev-team-404/a-mate-releases}"
KEY_FILE="${AMATE_KEY_FILE:-$HOME/.tauri/a-mate-updater.key}"
KEY_PASS_FILE="${AMATE_KEY_PASS_FILE:-$HOME/.tauri/a-mate-updater.pass}"
PROC_VERSION_FILE="${PROC_VERSION_FILE:-/proc/version}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

check_version_fmt() {
  local v=$1
  if [[ ! $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "ERROR: 버전 형식이 잘못됨: '$v' (예: 0.2.0)" >&2
    return 1
  fi
}

init_paths() {
  REPO="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
  CONF="$REPO/a-mate/src-tauri/tauri.conf.json"
  CARGO="$REPO/a-mate/src-tauri/Cargo.toml"
  local win_home
  win_home="$(powershell.exe -NoProfile -Command '[Console]::Out.Write($env:USERPROFILE)')"
  win_home="${win_home%$'\r'}"
  WIN_BUILD_WIN="${AMATE_WIN_BUILD:-${win_home}\\amate-build\\a-mate}"
  WIN_BUILD_WSL="$(wslpath "$WIN_BUILD_WIN")"
  KEY_PW="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-$(cat "$KEY_PASS_FILE" 2>/dev/null || echo '')}"
}

main() {
  local VERSION="${1:?사용법: release-amate.sh [--dry-run] <version> (예: 0.2.0)}"
  check_version_fmt "$VERSION"
  init_paths

  echo "[1/6] 버전 $VERSION 반영 (tauri.conf.json + Cargo.toml)"
  node -e "const f='$CONF';const j=require(f);j.version='$VERSION';require('fs').writeFileSync(f, JSON.stringify(j,null,2)+'\n')"
  sed -i -E "0,/^version = \"[^\"]*\"/s//version = \"$VERSION\"/" "$CARGO"
  git -C "$REPO" add "$CONF" "$CARGO"
  git -C "$REPO" commit -m "chore(agent): release v$VERSION"
  git -C "$REPO" tag "v$VERSION"

  echo "[2/6] Windows 빌드 폴더 동기화"
  mkdir -p "$WIN_BUILD_WSL"
  rsync -a --delete --exclude 'node_modules/' --exclude 'target/' --exclude 'dist/' \
    --exclude '.git/' --exclude '.env' "$REPO/a-mate/" "$WIN_BUILD_WSL/"

  echo "[3/6] 서명 빌드 (Windows)"
  local KEY_CONTENT
  KEY_CONTENT="$(cat "$KEY_FILE")"
  powershell.exe -NoProfile -Command "\
    [Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
    \$env:CARGO_HTTP_CHECK_REVOKE='false'; \
    \$env:TAURI_SIGNING_PRIVATE_KEY='$KEY_CONTENT'; \
    \$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD='$KEY_PW'; \
    \$env:Path=\"\$env:USERPROFILE\.cargo\bin;\$env:Path\"; \
    Set-Location '$WIN_BUILD_WIN'; \
    npm install; \
    npm run tauri build"

  echo "[4/6] latest.json 생성"
  local NSIS_DIR SETUP SIG BASENAME URLNAME
  NSIS_DIR="$WIN_BUILD_WSL/target/release/bundle/nsis"
  SETUP="$(ls -t "$NSIS_DIR"/*setup*.exe | head -1)"
  SIG="$(cat "$SETUP.sig")"
  BASENAME="$(basename "$SETUP")"
  URLNAME="${BASENAME// /.}"
  cat > "$NSIS_DIR/latest.json" <<JSON
{
  "version": "$VERSION",
  "notes": "v$VERSION",
  "platforms": {
    "windows-x86_64": {
      "signature": "$SIG",
      "url": "https://github.com/$RELEASE_REPO/releases/download/v$VERSION/$URLNAME"
    }
  }
}
JSON

  echo "[5/6] GitHub Release 발행 ($RELEASE_REPO)"
  git -C "$REPO" push origin "v$VERSION"
  gh release create "v$VERSION" --repo "$RELEASE_REPO" \
    --title "v$VERSION" --notes "Agent Mentor v$VERSION" \
    "$SETUP" "$NSIS_DIR/latest.json"

  echo "[6/6] 완료: v$VERSION 발행됨 → $RELEASE_REPO"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main "$@"
fi
```

- [ ] **Step 4: 구문 검사 + 테스트 → 통과 확인**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `bash -n` 무출력(0), 테스트 `PASS=<n> FAIL=0`. (WSL이 아니면 `test_init_paths`는 skip되고 나머지 통과.)

- [ ] **Step 5: Commit**

```bash
git add a-mate/scripts/release-amate.sh a-mate/scripts/test-release-amate.sh
git commit -m "refactor(a-mate): make release-amate.sh sourceable + de-hardcode paths"
```

---

### Task 2: check_signing_key

**Files:**
- Modify: `a-mate/scripts/release-amate.sh` (add `check_signing_key`)
- Modify: `a-mate/scripts/test-release-amate.sh` (add `test_signing_key`)

**Interfaces:**
- Consumes: globals `KEY_FILE`, `KEY_PASS_FILE`, `KEY_PW`
- Produces: `check_signing_key()` → 0 if `KEY_FILE` exists and `KEY_PW` non-empty, else prints error+fix, returns 1

- [ ] **Step 1: 실패 테스트 추가**

In `test-release-amate.sh`, add before the `test_version_fmt` call line:

```bash
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
```

And add `test_signing_key` to the run list (after `test_version_fmt`).

- [ ] **Step 2: 실행 → 실패 확인**

Run: `bash a-mate/scripts/test-release-amate.sh`
Expected: FAIL — `check_signing_key: command not found`.

- [ ] **Step 3: 함수 구현**

In `release-amate.sh`, add after `check_version_fmt`:

```bash
check_signing_key() {
  if [[ ! -f "$KEY_FILE" ]]; then
    echo "ERROR: 서명 키 없음: $KEY_FILE" >&2
    echo "  → 팀 공용 updater 개인키를 안전 채널로 받아 위 경로에 두세요 (chmod 600)." >&2
    return 1
  fi
  if [[ -z "${KEY_PW:-}" ]]; then
    echo "ERROR: 서명 키 암호가 비어 있음 ($KEY_PASS_FILE 또는 TAURI_SIGNING_PRIVATE_KEY_PASSWORD)." >&2
    return 1
  fi
}
```

- [ ] **Step 4: 실행 → 통과 확인**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `FAIL=0`.

- [ ] **Step 5: Commit**

```bash
git add a-mate/scripts/release-amate.sh a-mate/scripts/test-release-amate.sh
git commit -m "feat(a-mate): add signing-key preflight check"
```

---

### Task 3: check_gh_auth + check_release_repo

**Files:**
- Modify: `a-mate/scripts/release-amate.sh`
- Modify: `a-mate/scripts/test-release-amate.sh`

**Interfaces:**
- Consumes: `RELEASE_REPO`; external `gh`
- Produces:
  - `check_gh_auth()` → 0 if `gh auth status` succeeds, else error+fix, 1
  - `check_release_repo()` → 0 if `gh repo view "$RELEASE_REPO"` succeeds, else error+fix, 1

- [ ] **Step 1: 실패 테스트 추가**

In `test-release-amate.sh`, add a stub helper (after `setup_repo`):

```bash
# writes a `gh` stub into $1/bin that exits with $STUB_GH_EXIT (default 0)
make_gh_stub() {
  mkdir -p "$1/bin"
  cat > "$1/bin/gh" <<'EOF'
#!/usr/bin/env bash
exit ${STUB_GH_EXIT:-0}
EOF
  chmod +x "$1/bin/gh"
}
```

And add the test + run-list entry:

```bash
test_gh_checks() {
  echo "test_gh_checks"
  local tmp; tmp="$(mktemp -d)"; make_gh_stub "$tmp"
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=0 assert_ok   check_gh_auth
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=1 assert_fail check_gh_auth
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=0 assert_ok   check_release_repo
  PATH="$tmp/bin:$PATH" STUB_GH_EXIT=1 assert_fail check_release_repo
  rm -rf "$tmp"
}
```

- [ ] **Step 2: 실행 → 실패 확인**

Run: `bash a-mate/scripts/test-release-amate.sh`
Expected: FAIL — `check_gh_auth: command not found`.

- [ ] **Step 3: 함수 구현**

In `release-amate.sh`, add after `check_signing_key`:

```bash
check_gh_auth() {
  if ! gh auth status >/dev/null 2>&1; then
    echo "ERROR: gh 미인증. → gh auth login" >&2
    return 1
  fi
}

check_release_repo() {
  if ! gh repo view "$RELEASE_REPO" >/dev/null 2>&1; then
    echo "ERROR: 릴리스 저장소 없음: $RELEASE_REPO" >&2
    echo "  → 최초 1회: gh repo create $RELEASE_REPO --public  (updater가 익명으로 받으므로 반드시 public)" >&2
    return 1
  fi
}
```

- [ ] **Step 4: 실행 → 통과 확인**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `FAIL=0`.

- [ ] **Step 5: Commit**

```bash
git add a-mate/scripts/release-amate.sh a-mate/scripts/test-release-amate.sh
git commit -m "feat(a-mate): add gh-auth and release-repo preflight checks"
```

---

### Task 4: check_clean_tree + check_tag_absent + warn_branch

**Files:**
- Modify: `a-mate/scripts/release-amate.sh`
- Modify: `a-mate/scripts/test-release-amate.sh`

**Interfaces:**
- Consumes: global `REPO` (git work-tree with an `origin` remote)
- Produces:
  - `check_clean_tree()` → 0 if `git -C "$REPO" status --porcelain` empty, else error, 1
  - `check_tag_absent <version>` → 0 if `v<version>` absent locally **and** on `origin`, else error, 1
  - `warn_branch()` → prints WARN if current branch ≠ `main`; **always returns 0**

- [ ] **Step 1: 실패 테스트 추가**

In `test-release-amate.sh`, add tests + run-list entries:

```bash
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
```

- [ ] **Step 2: 실행 → 실패 확인**

Run: `bash a-mate/scripts/test-release-amate.sh`
Expected: FAIL — `check_clean_tree: command not found`.

- [ ] **Step 3: 함수 구현**

In `release-amate.sh`, add after `check_release_repo`:

```bash
check_clean_tree() {
  if [[ -n "$(git -C "$REPO" status --porcelain)" ]]; then
    echo "ERROR: 워킹트리에 커밋 안 된 변경이 있습니다. 먼저 커밋/스태시하세요." >&2
    return 1
  fi
}

check_tag_absent() {
  local v=$1
  if git -C "$REPO" rev-parse -q --verify "refs/tags/v$v" >/dev/null 2>&1; then
    echo "ERROR: 태그 v$v 가 이미 로컬에 존재합니다." >&2
    return 1
  fi
  if [[ -n "$(git -C "$REPO" ls-remote origin "refs/tags/v$v" 2>/dev/null)" ]]; then
    echo "ERROR: 태그 v$v 가 이미 원격(origin)에 존재합니다." >&2
    return 1
  fi
}

warn_branch() {
  local br
  br="$(git -C "$REPO" rev-parse --abbrev-ref HEAD)"
  if [[ "$br" != "main" ]]; then
    echo "WARN: 현재 브랜치가 '$br' 입니다 (main 아님). 계속 진행합니다." >&2
  fi
}
```

- [ ] **Step 4: 실행 → 통과 확인**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `FAIL=0`.

- [ ] **Step 5: Commit**

```bash
git add a-mate/scripts/release-amate.sh a-mate/scripts/test-release-amate.sh
git commit -m "feat(a-mate): add clean-tree, tag-absent, branch preflight checks"
```

---

### Task 5: check_wsl

**Files:**
- Modify: `a-mate/scripts/release-amate.sh`
- Modify: `a-mate/scripts/test-release-amate.sh`

**Interfaces:**
- Consumes: global `PROC_VERSION_FILE`; external `powershell.exe`
- Produces: `check_wsl()` → 0 if `PROC_VERSION_FILE` contains `microsoft` (case-insensitive) **and** `powershell.exe` is on PATH, else error, 1

- [ ] **Step 1: 실패 테스트 추가**

In `test-release-amate.sh`, add test + run-list entry:

```bash
test_wsl() {
  echo "test_wsl"
  local tmp; tmp="$(mktemp -d)"
  mkdir -p "$tmp/bin"
  printf '#!/usr/bin/env bash\ntrue\n' > "$tmp/bin/powershell.exe"
  chmod +x "$tmp/bin/powershell.exe"
  PROC_VERSION_FILE="$tmp/proc_ms"; echo "Linux x microsoft-standard-WSL2 x" > "$PROC_VERSION_FILE"
  PATH="$tmp/bin:$PATH" assert_ok   check_wsl                 # microsoft + powershell.exe
  PROC_VERSION_FILE="$tmp/proc_plain"; echo "Linux x generic x" > "$PROC_VERSION_FILE"
  PATH="$tmp/bin:$PATH" assert_fail check_wsl                 # microsoft 없음
  PROC_VERSION_FILE="$tmp/proc_ms"
  PATH="/usr/bin:/bin" assert_fail check_wsl                  # microsoft 있으나 powershell.exe 없음
  rm -rf "$tmp"
}
```

- [ ] **Step 2: 실행 → 실패 확인**

Run: `bash a-mate/scripts/test-release-amate.sh`
Expected: FAIL — `check_wsl: command not found`.

- [ ] **Step 3: 함수 구현**

In `release-amate.sh`, add after `check_version_fmt` (order within file is cosmetic; place near other checks):

```bash
check_wsl() {
  if ! grep -qi microsoft "$PROC_VERSION_FILE" 2>/dev/null; then
    echo "ERROR: WSL 환경이 아닙니다. 이 스크립트는 WSL에서 실행하세요." >&2
    return 1
  fi
  if ! command -v powershell.exe >/dev/null 2>&1; then
    echo "ERROR: powershell.exe를 찾을 수 없습니다 (WSL↔Windows 상호운용 필요)." >&2
    return 1
  fi
}
```

- [ ] **Step 4: 실행 → 통과 확인**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `FAIL=0`.

- [ ] **Step 5: Commit**

```bash
git add a-mate/scripts/release-amate.sh a-mate/scripts/test-release-amate.sh
git commit -m "feat(a-mate): add WSL-environment preflight check"
```

---

### Task 6: preflight 오케스트레이션 + --dry-run + main 연결

**Files:**
- Modify: `a-mate/scripts/release-amate.sh` (add `preflight`, rewrite `main` head)
- Modify: `a-mate/scripts/test-release-amate.sh` (integration tests)

**Interfaces:**
- Consumes: all `check_*`, `init_paths`
- Produces:
  - `preflight <version>` → runs checks 2–8 in order (version_fmt → signing_key → gh_auth → release_repo → clean_tree → tag_absent → warn_branch); returns non-zero on first failure
  - `main [--dry-run] <version>` → `check_wsl` → `init_paths` → `preflight` → if `--dry-run` print plan and return 0; else run steps [1/6]–[6/6]

- [ ] **Step 1: 통합 실패 테스트 추가**

In `test-release-amate.sh`, add tests + run-list entries:

```bash
# builds a sandbox where every check passes; echoes "tmpbin repo" (space-separated)
setup_green_env() {
  local tmp repo; tmp="$(mktemp -d)"; repo="$(setup_repo)"
  mkdir -p "$tmp/bin"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$tmp/bin/gh"
  cat > "$tmp/bin/powershell.exe" <<'EOF'
#!/usr/bin/env bash
printf '%s' 'C:\Users\tester'
EOF
  chmod +x "$tmp/bin/gh" "$tmp/bin/powershell.exe"
  KEY_FILE="$tmp/key"; : > "$KEY_FILE"
  KEY_PASS_FILE="$tmp/pass"; echo secret > "$KEY_PASS_FILE"
  PROC_VERSION_FILE="$tmp/proc"; echo microsoft > "$PROC_VERSION_FILE"
  SCRIPT_DIR="$repo"
  RELEASE_REPO="owner/repo"
  unset AMATE_WIN_BUILD TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  echo "$tmp/bin $repo"
}

test_preflight_pass_and_fail() {
  echo "test_preflight_pass_and_fail"
  if ! command -v wslpath >/dev/null 2>&1; then echo "  skip: wslpath 없음"; return; fi
  local env bin repo; env="$(setup_green_env)"; bin="${env% *}"; repo="${env#* }"
  init_paths                                  # sets KEY_PW from KEY_PASS_FILE
  ( PATH="$bin:$PATH"; preflight 9.9.9 ); [[ $? -eq 0 ]] && ok "preflight all-green" || bad "preflight should pass"
  # break gh auth → preflight must fail
  printf '#!/usr/bin/env bash\nexit 1\n' > "$bin/gh"
  ( PATH="$bin:$PATH"; preflight 9.9.9 ); [[ $? -ne 0 ]] && ok "preflight fails on gh" || bad "preflight should fail"
  rm -rf "$bin" "$repo"
}

test_dry_run_no_mutation() {
  echo "test_dry_run_no_mutation"
  if ! command -v wslpath >/dev/null 2>&1; then echo "  skip: wslpath 없음"; return; fi
  local env bin repo before after rc; env="$(setup_green_env)"; bin="${env% *}"; repo="${env#* }"
  before="$(git -C "$repo" tag -l)"
  ( PATH="$bin:$PATH"; main --dry-run 9.9.9 ) >/dev/null 2>&1; rc=$?
  after="$(git -C "$repo" tag -l)"
  [[ $rc -eq 0 ]] && ok "dry-run exit 0" || bad "dry-run exit=$rc"
  [[ "$before" == "$after" ]] && ok "dry-run created no tag" || bad "dry-run mutated tags: '$after'"
  rm -rf "$bin" "$repo"
}
```

- [ ] **Step 2: 실행 → 실패 확인**

Run: `bash a-mate/scripts/test-release-amate.sh`
Expected: FAIL — `preflight: command not found` 및 `main --dry-run`이 실제 mutation 시도(태그 생성) 또는 미정의로 실패.

- [ ] **Step 3: preflight 추가 + main 재작성**

In `release-amate.sh`, add `preflight` after the check functions (before `main`):

```bash
preflight() {
  local v=$1
  check_version_fmt "$v" || return 1
  check_signing_key      || return 1
  check_gh_auth          || return 1
  check_release_repo     || return 1
  check_clean_tree       || return 1
  check_tag_absent "$v"  || return 1
  warn_branch
}
```

Replace the **head** of `main` (the first three lines: `local VERSION=...`, `check_version_fmt "$VERSION"`, `init_paths`) with argument parsing + orchestration:

```bash
main() {
  local DRY_RUN=0 VERSION=""
  for a in "$@"; do
    case "$a" in
      --dry-run) DRY_RUN=1 ;;
      -*) echo "ERROR: 알 수 없는 옵션: $a" >&2; return 2 ;;
      *) VERSION="$a" ;;
    esac
  done
  : "${VERSION:?사용법: release-amate.sh [--dry-run] <version> (예: 0.2.0)}"

  check_wsl   || return 1
  init_paths
  preflight "$VERSION" || return 1

  if [[ $DRY_RUN -eq 1 ]]; then
    echo "[dry-run] 릴리스 대상 : $RELEASE_REPO"
    echo "[dry-run] 저장소      : $REPO"
    echo "[dry-run] 빌드 폴더   : $WIN_BUILD_WIN  ($WIN_BUILD_WSL)"
    echo "[dry-run] 태그        : v$VERSION"
    echo "[dry-run] 프리플라이트 통과. 실제 발행은 --dry-run 없이 실행하세요."
    return 0
  fi

  echo "[1/6] 버전 $VERSION 반영 (tauri.conf.json + Cargo.toml)"
```

> 주의: 위 블록은 기존 `[1/6] …` 줄에서 이어지도록 붙인다. 즉 원래 `main`의 `local VERSION=...` / `check_version_fmt` / `init_paths` 세 줄과, 그 아래 `echo "[1/6] …"` 한 줄을 위 블록으로 대체한다. `[2/6]`~`[6/6]` 이후 로직은 그대로 둔다.

- [ ] **Step 4: 실행 → 통과 확인**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `FAIL=0` — `preflight all-green`, `preflight fails on gh`, `dry-run exit 0`, `dry-run created no tag` 모두 ok.

- [ ] **Step 5: Commit**

```bash
git add a-mate/scripts/release-amate.sh a-mate/scripts/test-release-amate.sh
git commit -m "feat(a-mate): add preflight orchestration and --dry-run to release script"
```

---

### Task 7: build-and-run.md 배포 섹션 갱신

**Files:**
- Modify: `docs/design/a-mate/build-and-run.md` (배포 섹션)

**Interfaces:** 없음 (문서)

- [ ] **Step 1: "배포" 섹션 교체**

`docs/design/a-mate/build-and-run.md`의 `## 배포 — 동료에게 설치 파일 전달` 섹션(현재 "아직 CI가 없으므로 …" 문단부터 그 아래 표·주의사항까지)을, 아래로 교체한다. 상단의 수동 `npm run tauri build` 문단·산출물 경로는 **폴백으로 유지**하고, stale한 표(*코드 서명 없음 / 자동 업데이트 없음*)만 아래 내용으로 대체:

```markdown
## 릴리스 — 자동 업데이트 채널로 발행

updater·서명이 연결돼 있어(`tauri.conf.json`의 `plugins.updater` + `createUpdaterArtifacts`),
새 버전은 **공개 릴리스 저장소** `dev-team-404/a-mate-releases`에 발행하면 설치본이
앱 내에서 자동 업데이트된다. 발행은 **WSL에서 로컬 스크립트**로 한다(현재 CI 미도입).

```bash
# 저장소 루트에서 (WSL)
bash a-mate/scripts/release-amate.sh --dry-run 0.2.0   # 프리플라이트만 점검
bash a-mate/scripts/release-amate.sh 0.2.0             # 실제 발행
```

스크립트가 하는 일: 버전 갱신(commit·tag) → NTFS 빌드 폴더로 rsync → Windows에서 서명 빌드
→ `latest.json` 생성 → `gh release create`로 setup.exe·latest.json 업로드.

### 최초 1회 셋업

| 항목 | 방법 |
|------|------|
| 공개 릴리스 저장소 | `gh repo create dev-team-404/a-mate-releases --public` (updater가 익명 fetch하므로 **반드시 public**) |
| 서명 키(팀 공용) | updater 개인키 + 암호를 안전 채널(1Password 등)로 받아 `~/.tauri/a-mate-updater.{key,pass}`에 배치(`chmod 600`). **커밋 금지.** 모두 **같은 키**여야 자동 업데이트가 깨지지 않는다 |
| `gh` 인증 | `gh auth login` |

오버라이드 env: `AMATE_RELEASE_REPO`, `AMATE_WIN_BUILD`, `AMATE_KEY_FILE`, `AMATE_KEY_PASS_FILE`.

### 주의사항

| 항목 | 내용 |
|------|------|
| **코드 서명 인증서 없음** | updater 서명키(무료)만 사용. 최초 **수동 설치 1회**만 SmartScreen 경고("추가 정보 → 실행"). 이후 인앱 자동 업데이트엔 지장 없음 |
| **버전 인자 필수** | `release-amate.sh <version>` — 형식 `X.Y.Z`. 이미 존재하는 태그면 프리플라이트가 중단 |
| **WSL에서 실행** | 스크립트가 `powershell.exe`로 Windows 빌드를 구동. 네이티브 Windows 단독 실행용 아님 |

> 그냥 설치 파일 하나만 전달하려면 위 `npm run tauri build` 폴백을 쓰면 된다(자동 업데이트 없음).
```

- [ ] **Step 2: 확인**

Run: `git diff --stat docs/design/a-mate/build-and-run.md`
Expected: 1 file changed. 육안으로 stale 표(*자동 업데이트 없음*)가 사라지고 릴리스 스크립트 흐름이 반영됐는지 확인.

- [ ] **Step 3: Commit**

```bash
git add docs/design/a-mate/build-and-run.md
git commit -m "docs(a-mate): update release section to updater/signing flow"
```

---

### Task 8: DoD — 아카이브

**Files:**
- Move (via skill): `docs/design/a-mate/specs/2026-07-25-release-script-team-generalization-design.md` + `docs/design/a-mate/plans/2026-07-25-release-script-team-generalization.md` → `docs/archive/` 미러

- [ ] **Step 1: 최종 검증**

Run: `bash -n a-mate/scripts/release-amate.sh && bash a-mate/scripts/test-release-amate.sh`
Expected: `FAIL=0`.

- [ ] **Step 2: 수동 E2E 안내(작성자)**

이 세션에선 빌드 불가. 작성자에게 다음을 남긴다: 본인 WSL에서 (a) 공용 키 배치 + `gh repo create … --public` 1회, (b) `bash a-mate/scripts/release-amate.sh --dry-run <다음버전>`으로 프리플라이트 통과 확인, (c) 실제 발행 후 이전 버전 설치본이 자동 업데이트되는지 확인.

- [ ] **Step 3: 아카이브 스킬 실행**

프로젝트 DoD 규칙(ADR 0013)에 따라 `docs-archive` 스킬로 이 spec·plan을 `docs/archive/` 미러로 이동하고 커밋.

---

## Self-Review

**1. Spec coverage** (spec 섹션 → task 매핑):
- 하드코딩 제거(REPO/WIN빌드/RELEASE_REPO/키경로) → Task 1(init_paths, 오버라이드 defaults)
- 프리플라이트 1 WSL → Task 5 / 2 버전 → Task 1 / 3 키 → Task 2 / 4 gh인증·5 repo → Task 3 / 6 클린트리·7 태그 → Task 4 / 8 브랜치(경고만) → Task 4 / 오케스트레이션·`--dry-run` → Task 6
- 단위 테스트(stub·임시 HOME/repo, sourceable 구조) → Task 1 하네스 + 각 Task의 test_*
- 1회성 셋업(repo 생성·키 배포) → Task 7 문서 + Task 8 E2E 안내
- 문서 갱신 → Task 7 / 검증 제약(빌드 불가) → Global Constraints + Task 8 수동 E2E
- DoD 아카이브 → Task 8

**2. Placeholder scan:** "TBD/이후 구현" 없음. 모든 code step에 실제 코드·명령·기대 출력 포함. ✅

**3. Type consistency:** 함수명·전역 일관 확인 — `check_version_fmt`, `check_signing_key`, `check_gh_auth`, `check_release_repo`, `check_clean_tree`, `check_tag_absent`, `warn_branch`, `check_wsl`, `init_paths`, `preflight`, `main`. 전역 `REPO/CONF/CARGO/WIN_BUILD_WIN/WIN_BUILD_WSL/KEY_FILE/KEY_PASS_FILE/KEY_PW/RELEASE_REPO/PROC_VERSION_FILE/SCRIPT_DIR`. Task 6의 `preflight`가 Task 1–5에서 정의한 이름을 그대로 호출. 하네스 헬퍼 `ok/bad/assert_ok/assert_fail/setup_repo/make_gh_stub/setup_green_env` 일관. ✅
