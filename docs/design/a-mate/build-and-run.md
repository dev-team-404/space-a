# Agent Mentor

Windows·WSL의 Claude Code 사용 기록을 **로컬에서** 분석해, AI 코딩 에이전트를 토큰 낭비 없이
쓰도록 코칭하는 Windows 상주형 데스크톱 앱 (Tauri v2 + Rust).

> 제품·아키텍처 소개는 [같은 폴더의 개요 문서](README.md)를 참고하세요.
> 이 문서는 **처음 클론한 팀원이 빌드·실행**하기 위한 셋업 가이드입니다.

---

## ⚠️ 실행 환경: 네이티브 Windows (WSL 아님)

이 앱은 **네이티브 Windows 데스크톱 애플리케이션**입니다. 빌드와 실행은 반드시
**Windows PowerShell / cmd**에서 하세요.

- ❌ **WSL(Ubuntu 등) 안에서 빌드/실행하지 마세요.** WSL에서 돌리면 Linux 바이너리를
  만들려 하거나 GUI가 뜨지 않는 등 정상 동작하지 않습니다.
- ✅ WSL은 **분석 대상**일 뿐입니다 — 앱(네이티브 Windows 프로세스)이 WSL 쪽 Claude Code
  트랜스크립트를 *읽어서* 분석합니다. 앱 자체를 WSL에서 돌리는 게 아닙니다.
- 플랫폼은 **Windows 전용**입니다. macOS/Linux는 지원하지 않습니다.

---

## 사전 요구사항 (한 번만 설치)

저장소에 포함되지 않는 시스템 도구들입니다. 아래 3가지 + WebView2가 필요합니다.

### 1. Node.js (LTS, v20+)

프론트엔드(Vite + Svelte)와 Tauri CLI 실행용.

```powershell
winget install OpenJS.NodeJS.LTS
```

설치 후 새 터미널에서 확인:

```powershell
node --version   # v20 이상
npm --version
```

### 2. Rust 툴체인 (MSVC)

```powershell
winget install Rustlang.Rustup
```

- rustup은 Windows에서 기본으로 **`stable-x86_64-pc-windows-msvc`** 툴체인을 설치합니다.
  이 프로젝트가 원하는 구성이므로 별도 설정이 필요 없습니다.
- 설치 후 새 터미널에서 확인:

```powershell
rustc --version
cargo --version
rustup show          # active toolchain이 ...-pc-windows-msvc 인지 확인
```

### 3. Visual Studio C++ Build Tools (MSVC 링커 + C 컴파일러)

Rust MSVC 툴체인은 링커로 MSVC를, 그리고 이 프로젝트는 SQLite를 C에서 컴파일하므로
(`rusqlite`의 `bundled` 기능) **C 컴파일러가 필수**입니다.

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
```

설치 관리자(Visual Studio Installer)가 뜨면 **"C++를 사용한 데스크톱 개발"
(Desktop development with C++)** 워크로드를 체크하고 설치하세요. 이 워크로드가
MSVC 링커(`link.exe`)와 C/C++ 컴파일러(`cl.exe`)를 모두 제공합니다.

> 이미 Visual Studio가 설치돼 있다면 위 워크로드만 추가하면 됩니다.

### 4. WebView2 런타임

Tauri v2가 UI를 렌더링하는 데 사용합니다. **Windows 11에는 기본 포함**되어 있어
대개 별도 설치가 필요 없습니다. 없다면
[Microsoft Edge WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)에서
Evergreen Runtime을 설치하세요.

---

## 빌드 & 실행

**Windows PowerShell 또는 cmd**에서 (WSL 아님):

```powershell
git clone <repo-url>
cd space-a/a-mate     # Tauri 클라이언트는 a-mate/ 디렉터리

npm install          # node_modules는 커밋되지 않으므로 먼저 필수
npm run tauri dev    # Vite(포트 1420) 기동 → Rust 앱 빌드 → 앱 실행
```

- `@tauri-apps/cli`는 `devDependencies`에 있으므로 `npm install`로 함께 설치됩니다.
  Tauri CLI를 전역 설치할 필요는 없습니다.
- 첫 실행은 Rust 크레이트를 전부 컴파일하므로 몇 분 걸릴 수 있습니다. 이후 실행은 빠릅니다.

---

## 자동 셋업 (에이전트·무인용)

위 "사전 요구사항"을 사람이 GUI로 클릭하는 대신, 아래 명령을 **순서대로** 실행하면
무인(silent)으로 설치됩니다. AI 에이전트(예: Claude Code)에게 셋업을 맡길 때 이 블록을
그대로 실행하게 하세요.

> **⚠️ 관리자 권한 PowerShell 필수.** `winget`의 시스템 설치는 UAC(관리자 권한 승인)를
> 요구합니다. 에이전트는 UAC 대화상자를 클릭할 수 없으므로, **관리자 권한으로 실행된
> PowerShell** 안에서 진행해야 무인 설치가 됩니다. (일반 셸이면 이 부분만은 사람이 승인)
>
> winget의 `install`은 이미 설치된 패키지를 건너뛰므로 재실행해도 대체로 안전합니다.

```powershell
# 1) 도구 설치 (silent — 라이선스/소스 동의 자동 수락)
winget install --id OpenJS.NodeJS.LTS -e --accept-package-agreements --accept-source-agreements
winget install --id Rustlang.Rustup   -e --accept-package-agreements --accept-source-agreements
winget install --id Microsoft.EdgeWebView2Runtime -e --accept-package-agreements --accept-source-agreements

# 2) VS C++ Build Tools — "C++ 데스크톱 개발" 워크로드를 CLI로 지정해 무인 설치
#    (GUI 워크로드 선택 단계를 --override로 대체)
winget install --id Microsoft.VisualStudio.2022.BuildTools -e `
  --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" `
  --accept-package-agreements --accept-source-agreements

# 3) 현재 셸에 PATH 반영 (새 터미널을 열지 않고 세션 안에서 갱신)
$env:Path = [Environment]::GetEnvironmentVariable("Path","Machine") + ";" + `
            [Environment]::GetEnvironmentVariable("Path","User")

# 4) Rust MSVC 툴체인 확정 + 검증
rustup default stable-x86_64-pc-windows-msvc
rustc --version ; cargo --version ; node --version ; npm --version

# 5) 프로젝트 빌드 & 실행 (Tauri 클라이언트는 a-mate/ 디렉터리)
cd a-mate
npm install
Copy-Item .env.example .env   # (선택) LLM 엔진 쓸 때만
npm run tauri dev
```

> `winget` 자체가 없거나(구버전 Windows) 사내 정책으로 막혀 있으면 위 자동 설치는 실패할 수
> 있습니다. 그 경우 위쪽 "사전 요구사항"의 수동 절차를 따르세요.

---

## (선택) LLM 엔진 설정 — 다이어리·채팅 코칭

기본적으로 **`.env` 없이도 앱은 정상 부팅**됩니다. 다만 LLM 기능은 다음처럼 폴백합니다.

- 다이어리 → **mock**으로 생성
- 채팅 탭 → 설정 안내 표시 (에러 아님)

실제 LLM 코칭/일기를 쓰려면 **트레이 우클릭 → 설정**에서 엔드포인트를 입력하거나
(앱 재시작 불필요), `a-mate/`에 `.env`를 만드세요:

```powershell
Copy-Item .env.example .env
```

그리고 `.env`에서 OpenAI 호환 엔드포인트를 채웁니다 (변수명은 그대로 유지):

```dotenv
AGENT_MENTOR_ENGINE_URL=http://localhost:4444/v1
AGENT_MENTOR_ENGINE_KEY=
AGENT_MENTOR_ENGINE_MODEL=gpt-4.1-mini
```

> 이 앱은 네이티브 Windows 프로세스이므로 프록시 주소는 **`localhost`**로 접속합니다.
> `host.docker.internal`은 Docker 컨테이너 내부 전용 DNS라 네이티브 Windows에선 해석되지
> 않습니다. 프록시가 컨테이너라면 포트를 호스트에 노출하세요 (예: `-p 4444:4444`).
> `.env`는 dev 빌드에서만 자동 로드되며 `.gitignore` 대상입니다.

---

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
→ `latest.json` 생성 → `gh release create`로 setup.exe·latest.json 업로드. 실행 전
**프리플라이트**가 서명 키·`gh` 인증·릴리스 저장소·워킹트리·태그 중복을 검사해, 실패하면
변경 이전에 해결 명령과 함께 중단한다.

### 최초 1회 셋업

| 항목 | 방법 |
|------|------|
| 공개 릴리스 저장소 | `gh repo create dev-team-404/a-mate-releases --public` (updater가 익명 fetch하므로 **반드시 public**) |
| 서명 키(팀 공용) | updater 개인키 + 암호를 안전 채널(1Password 등)로 받아 `~/.tauri/a-mate-updater.{key,pass}`에 배치(`chmod 600`). **커밋 금지.** 모두 **같은 키**여야 자동 업데이트가 깨지지 않는다 |
| `gh` 인증 | `gh auth login` |

> **서명 키 출처 (어디서 받나).** 이 키페어는 최초에 `tauri signer generate`로 **한 번 생성**됐다.
> public key는 `tauri.conf.json`의 `pubkey`에 커밋돼 있고(공개, 안전), **private key + 암호는
> 릴리스 최초 셋업자가 자신의 `~/.tauri/`에 보관**한다. 중앙 발급처는 없다 — 새 릴리서는
> **그 보관자에게서** 받는다. 노트북 분실 시 키가 사라지면 재생성해야 하고, 그러면 pubkey가 바뀌어
> **기존 설치본 자동 업데이트가 깨진다.** 따라서 개인키+암호를 **팀 공유 비밀번호 관리자
> (예: 1Password 공유 볼트)에 백업**해 두고, 새 릴리서는 거기서 받아 `~/.tauri/a-mate-updater.{key,pass}`에
> 배치(`chmod 600`)한다.

오버라이드 env: `AMATE_RELEASE_REPO`, `AMATE_WIN_BUILD`, `AMATE_KEY_FILE`, `AMATE_KEY_PASS_FILE`.

### 주의사항

| 항목 | 내용 |
|------|------|
| **코드 서명 인증서 없음** | updater 서명키(무료)만 사용. 최초 **수동 설치 1회**만 SmartScreen 경고("추가 정보 → 실행"). 이후 인앱 자동 업데이트엔 지장 없음 |
| **버전 인자 필수** | `release-amate.sh <version>` — 형식 `X.Y.Z`. 이미 존재하는 태그면 프리플라이트가 중단 |
| **WSL에서 실행** | 스크립트가 `powershell.exe`로 Windows 빌드를 구동. 네이티브 Windows 단독 실행용 아님 |
| **WebView2 필요** | Windows 11엔 기본 포함. 없는 환경이면 NSIS 설치기가 안내하거나, [사전 요구사항 4](#4-webview2-런타임)를 먼저 설치해야 합니다 |

### 릴리스 저장소 변경

릴리스 저장소를 다른 repo로 옮기려면 **두 곳**을 바꿔야 한다 — 성격이 다르다.

| 곳 | 역할 | 변경 방법 |
|----|------|-----------|
| `scripts/release-amate.sh`의 `RELEASE_REPO` | 릴리스를 **발행(push)** 하는 대상 | 한 번만: `AMATE_RELEASE_REPO=owner/new-repo bash a-mate/scripts/release-amate.sh 0.2.0` · 영구: 스크립트 기본값 수정. `latest.json` 다운로드 URL은 `$RELEASE_REPO`로 자동 생성돼 함께 일관됨 |
| `src-tauri/tauri.conf.json`의 `plugins.updater.endpoints` | 설치본이 업데이트를 **확인(fetch)** 하는 주소 | 값 안의 `dev-team-404/a-mate-releases`를 새 `owner/repo`로 수정 후 **그 버전을 빌드** |

⚠️ `endpoints`는 빌드 시 앱 바이너리에 **컴파일**되므로, **기존 설치본은 옛 주소를 계속 폴링**한다. 깔끔히 이전하려면:

1. `tauri.conf.json`의 endpoint를 **새 repo**로 수정한다.
2. **옛 repo에** 한 버전 더 발행한다 → 기존 사용자가 이 업데이트를 받으며 endpoint가 새 repo로 전환된다.
3. 이후부터 **새 repo에** 발행한다.

> 사용자가 적어 재설치를 감수한다면, 두 곳만 바꾸고 기존 사용자는 새 repo에서 재설치하게 하면 된다.

- 새 저장소도 **반드시 public**: `gh repo create owner/new-repo --public`
- 서명 키(pubkey)는 저장소와 무관하다 — **그대로 유지**하면 자동 업데이트가 안 깨진다(repo만 바뀌고 키가 동일하면 OK). [서명 키 공유 원칙](#릴리스--자동-업데이트-채널로-발행) 참고.

### 폴백 — 설치 파일만 전달 (자동 업데이트 없음)

자동 업데이트 없이 설치 파일 하나만 동료에게 넘기려면 Windows에서 직접 빌드한다:

```powershell
cd a-mate
npm install                 # 최초 1회 (또는 의존성 변경 시)
npm run tauri build         # 프론트 빌드 → Rust 릴리스 컴파일 → NSIS 설치 파일 번들
```

산출물: `a-mate/src-tauri/target/release/bundle/nsis/Agent Mentor_<버전>_x64-setup.exe` —
이 `*-setup.exe`를 전달하면 시작 메뉴 등록·아이콘·제거 기능과 함께 설치된다.

> MSI도 함께 만들려면 `tauri.conf.json`의 `bundle.targets`를 `["nsis", "msi"]`로 두면
> `bundle/msi/`에도 산출된다.

---

## 자주 쓰는 명령

전부 `a-mate/` 디렉터리에서 실행합니다.

```powershell
npm run tauri dev    # 개발 모드 실행 (핫리로드)
npm run tauri build  # 릴리스 빌드
npm test             # 프론트엔드 테스트 (Vitest)
cargo test           # Rust 백엔드 테스트 (워크스페이스 전체)
```

---

## 문제 해결

- **`link.exe`/`cl.exe` not found, `error: linker ... not found`**
  → Visual Studio C++ Build Tools의 "C++를 사용한 데스크톱 개발" 워크로드가 없습니다.
  위 [사전 요구사항 3](#3-visual-studio-c-build-tools-msvc-링커--c-컴파일러)을 설치하세요.
- **WSL에서 실행했더니 GUI가 안 뜸 / 빌드가 이상함**
  → WSL이 아니라 **Windows PowerShell/cmd**에서 실행해야 합니다. 맨 위 실행 환경 안내 참조.
- **`cargo`/`node`를 찾을 수 없음**
  → 도구 설치 후 **새 터미널**을 열어 PATH를 갱신하세요.
- **포트 1420 사용 중**
  → Vite는 `strictPort`라 1420이 점유되면 실패합니다. 해당 포트를 쓰는 프로세스를 종료하세요.
