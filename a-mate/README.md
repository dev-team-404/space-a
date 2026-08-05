# A-Mate (Agent Mentor) · Pillar 1

Windows·WSL의 Claude Code 사용 기록을 **로컬에서** 분석해, AI 코딩 에이전트를 토큰 낭비 없이
쓰도록 코칭하는 Windows 상주형 데스크톱 앱 (Tauri v2 + Rust).

> 폴더명 `a-mate`는 README의 **A-Mate** 축(Pillar 1)과 맞춘 것이다. 앱의 제품명·식별자는
> 그대로 **Agent Mentor** / `agent-mentor`(Cargo 크레이트)·`dev.agentmentor.app`(tauri identifier)를 유지한다.

- **제품 개요:** [`docs/architecture/a-mate/README.md`](../docs/architecture/a-mate/README.md)
- **아키텍처:** [`docs/architecture/a-mate.md`](../docs/architecture/a-mate/03-architecture.md)
- **기능 카탈로그:** [`docs/architecture/a-mate/02-features.md`](../docs/architecture/a-mate/02-features.md)
- **빌드·실행 가이드:** [`docs/architecture/a-mate/build-and-run.md`](../docs/architecture/a-mate/build-and-run.md)

무엇을 하나:

- **코칭** — MCP 대형 결과(R8) · 반복 지시의 스킬화(R6) · 모델 오남용(R7). 결정론 규칙 + LLM 판정
- **오늘의 배움** — 공식 CHANGELOG · 창시자 팁 · 플러그인 카탈로그 · 팀 지식을 역량에 맞춰 랭킹
- **1인칭 다이어리** · 오늘의 한마디 · 잡담 · 채팅 (LLM 엔진 설정 시)
- **팀 지식 허브 연동** — 발견을 팀에 공유하고, 팀이 쓰면 알림으로 돌아온다(인정 루프)
- **미니홈피** — 방 꾸미기 · 방문 · 방명록 (별도 서버)
- 트레이 상주 · 마스코트 말풍선 · 자동 업데이트

## ⚠️ 실행 환경: 네이티브 Windows (WSL 아님)

이 앱은 **네이티브 Windows 데스크톱 앱**입니다. 빌드·실행은 반드시 **Windows PowerShell / cmd**에서 하세요.

- ❌ WSL(Ubuntu 등) 안에서 빌드/실행하지 마세요 — 정상 동작하지 않습니다.
- ✅ WSL은 **분석 대상**일 뿐입니다. 앱(네이티브 Windows 프로세스)이 WSL 쪽 Claude Code 트랜스크립트를 *읽어서* 분석합니다.
- 플랫폼은 **Windows 전용**. macOS/Linux는 지원하지 않습니다.

## 구조 (Cargo workspace)

```
a-mate/
├── crates/core/    # 순수 도메인 로직 (패키지 `agent-mentor`, lib `agent_mentor`)
│                   #   JSONL 파싱·집계·규칙(rules)·다이어리·코칭·인벤토리 — UI/OS 비의존
├── src-tauri/      # Tauri v2 셸 (패키지 `agent-mentor-app`) — 트레이·커맨드·파이프라인 런타임
├── src/            # 프론트엔드 (Svelte 5 + Vite + TypeScript) — 미니홈피 UI·마스코트·설정·채팅
├── scripts/        # 유틸 (예: reset-diary.py)
└── .env.example    # LLM 엔진 설정 예시 (선택)
```

무거운 데이터 처리(JSONL 파싱/집계/감시)는 Rust 백엔드(`crates/core`)에서 하고, 프론트엔드는 렌더링만 담당합니다.

## 설치 (사전 요구사항 — 한 번만)

저장소에 포함되지 않는 시스템 도구입니다. **관리자 권한 PowerShell**에서:

```powershell
winget install --id OpenJS.NodeJS.LTS -e --accept-package-agreements --accept-source-agreements   # Node.js LTS (v20+)
winget install --id Rustlang.Rustup   -e --accept-package-agreements --accept-source-agreements   # Rust (MSVC 툴체인)
winget install --id Microsoft.EdgeWebView2Runtime -e --accept-package-agreements --accept-source-agreements

# Visual Studio C++ Build Tools — "C++ 데스크톱 개발" 워크로드 (MSVC 링커 + C 컴파일러; rusqlite bundled에 필수)
winget install --id Microsoft.VisualStudio.2022.BuildTools -e `
  --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" `
  --accept-package-agreements --accept-source-agreements

rustup default stable-x86_64-pc-windows-msvc
```

새 터미널에서 확인: `node --version` (v20+), `rustc --version`, `cargo --version`.

> WebView2는 Windows 11에 기본 포함되어 대개 별도 설치가 필요 없습니다.
> winget이 막혀 있거나 실패하면 [build-and-run.md](../docs/architecture/a-mate/build-and-run.md)의 수동 절차를 따르세요.

## 빌드 & 실행

**Windows PowerShell / cmd**에서 (WSL 아님):

```powershell
cd a-mate
npm install          # node_modules는 커밋되지 않으므로 먼저 필수 (@tauri-apps/cli도 함께 설치됨)
npm run tauri dev    # Vite(포트 1420) 기동 → Rust 앱 빌드 → 앱 실행
```

첫 실행은 Rust 크레이트를 전부 컴파일하므로 몇 분 걸릴 수 있습니다. 이후 실행은 빠릅니다.

## (선택) LLM 엔진 설정 — 다이어리·채팅·판정

`.env` 없이도 앱은 정상 부팅됩니다 (수집·규칙·UI는 완전 로컬로 동작하고, 서사·번역·판정만 쉽니다).
실제 LLM 기능을 쓰려면 **트레이 우클릭 → 설정 → 연결**에서 엔드포인트를 입력하거나,
`a-mate/`에 `.env`를 만드세요:

```powershell
Copy-Item .env.example .env
```

```dotenv
AGENT_MENTOR_ENGINE_URL=http://localhost:4444/v1   # OpenAI 호환 엔드포인트 (필수 — 없으면 mock)
AGENT_MENTOR_ENGINE_KEY=                           # 선택 (기본: 빈 문자열)
AGENT_MENTOR_ENGINE_MODEL=gpt-4.1-mini             # 선택 (기본: gpt-4o-mini)
```

> 네이티브 Windows 프로세스이므로 프록시 주소는 **`localhost`**로 접속합니다 (`host.docker.internal` 불가).
> `.env`는 **dev 빌드에서만** 자동 로드되며 `.gitignore` 대상입니다 —
> 릴리스 빌드에서는 설정 창 입력이 유일한 경로입니다.

## (선택) 팀 지식 허브

기본은 **미연결**입니다. **설정 → 연결 → 팀 지식 허브**에서 URL과 필요한 인증값을
저장하거나 `SPACE_A_HUB_URL`을 설정한 뒤에만 공유가 동작합니다. 팀 배포 예시는
`https://spacea.msalt.net` / 공간 `sw-innov`이며, `x-api-key`는 소스나 설치본에 포함하지 않습니다.
끄려면 같은 화면의 공유 토글을 off로 두면 됩니다(이 opt-out은 환경변수로 되살아나지 않습니다).

## 자주 쓰는 명령

전부 `a-mate/` 디렉터리에서 실행합니다.

```powershell
npm run tauri dev    # 개발 모드 실행 (핫리로드)
npm run tauri build  # 릴리스 빌드
npm test             # 프론트엔드 테스트 (Vitest)
cargo test           # Rust 백엔드 테스트 (워크스페이스 전체)
```

## 문제 해결

- **`link.exe`/`cl.exe` not found** → VS C++ Build Tools의 "C++ 데스크톱 개발" 워크로드 미설치. 위 설치 절차 참조.
- **WSL에서 GUI가 안 뜸 / 빌드가 이상함** → WSL이 아니라 Windows PowerShell/cmd에서 실행하세요.
- 그 외: [build-and-run.md 문제 해결](../docs/architecture/a-mate/build-and-run.md#문제-해결) 참조.
