#!/usr/bin/env bash
# a-mate 릴리스: 버전 갱신 → 서명 빌드 → latest.json 생성 → GitHub Release 발행
# 사용법 (WSL): bash a-mate/scripts/release-amate.sh <version>   예: 0.2.0
#
# 사전 준비:
#  - updater private key: ~/.tauri/a-mate-updater.key  (tauri signer generate로 생성, 커밋 금지)
#  - 공개 릴리스 저장소: dev-team-404/a-mate-releases (gh repo create --public)
#  - Windows 빌드 복사본 경로: C:\Users\salt.jeong\amate-build\a-mate
set -euo pipefail

VERSION="${1:?사용법: release-amate.sh <version> (예: 0.2.0)}"
REPO=/home/msaltnet/code/space-a
WIN_BUILD_WSL=/mnt/c/Users/salt.jeong/amate-build/a-mate
WIN_BUILD_WIN='C:\Users\salt.jeong\amate-build\a-mate'
RELEASE_REPO="dev-team-404/a-mate-releases"
KEY_FILE="$HOME/.tauri/a-mate-updater.key"
KEY_PW="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"   # 키에 암호를 걸었으면 env로 주입

CONF="$REPO/a-mate/src-tauri/tauri.conf.json"
CARGO="$REPO/a-mate/src-tauri/Cargo.toml"

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
NSIS_DIR="$WIN_BUILD_WSL/target/release/bundle/nsis"
SETUP="$(ls -t "$NSIS_DIR"/*setup*.exe | head -1)"
SIG="$(cat "$SETUP.sig")"
BASENAME="$(basename "$SETUP")"
# GitHub 다운로드 URL은 파일명의 공백을 점(.)으로 치환한다
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
