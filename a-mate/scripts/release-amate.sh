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
