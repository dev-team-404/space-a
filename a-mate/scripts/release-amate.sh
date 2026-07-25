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

check_version_fmt() {
  local v=$1
  if [[ ! $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "ERROR: 버전 형식이 잘못됨: '$v' (예: 0.2.0)" >&2
    return 1
  fi
}

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
