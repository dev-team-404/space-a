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
cd space-a/client     # Tauri 클라이언트는 client/ 디렉터리

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

# 5) 프로젝트 빌드 & 실행 (Tauri 클라이언트는 client/ 디렉터리)
cd client
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
(앱 재시작 불필요), `client/`에 `.env`를 만드세요:

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

## 자주 쓰는 명령

전부 `client/` 디렉터리에서 실행합니다.

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
