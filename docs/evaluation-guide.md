# SPACE-A 평가·검증 가이드

이 문서는 [심사 기준 #171](https://github.com/dev-team-404/space-a/issues/171)이 강조하는
최종본의 실제 동작과 문서·코드 정합성을 빠르게 확인하기 위한 진입점이다. 제품 설명은
[프로젝트 README](../README.md), 요구사항은 [PRD](./PRD.md), 구조는
[통합 아키텍처](./architecture/README.md)를 정본으로 사용한다.

## 먼저 확인할 통합 상태

**A-Mate, A-Hub, A-Lens 간 연동은 구현되어 있으며 팀 개발망에서 실제로 사용하고 있다.**
팀원들은 설치본 대신 Windows에서 `npm run tauri dev`로 A-Mate를 실행하고, 설정 화면이나
환경변수에 개발망의 A-Hub·A-Lens 주소와 인증 정보를 넣어 통합 기능을 사용한다.

저장소에는 개발망 주소를 런타임 기본값으로 고정하거나 API key·token을 포함하지 않는다.
따라서 새로 체크아웃한 평가 환경은 다음과 같이 안전한 독립 실행 상태로 시작한다.

| 컴포넌트 | 저장소 기본 상태 | 설정 후 통합 동작 |
|---|---|---|
| **A-Mate** | 로컬 기록 수집·분석·코칭 동작. A-Hub 공유는 미연결이고 A-Lens 링크는 숨김 | Work에 선별 지식을 공유하고, Life에 방·프레즌스·소셜 상태를 동기화하며, A-Lens 방 링크를 제공 |
| **A-Hub** | Work와 Life를 로컬 서버로 각각 실행 가능. API key가 비어 있으면 로컬 관문 비활성 | A-Mate의 쓰기와 A-Lens의 읽기를 REST·MCP 및 공개 계약으로 연결 |
| **A-Lens** | 외부 요청 없는 `dummy` 모드로 화면과 상호작용을 재현 | `auto` 또는 `hub` 모드에서 Work·Life 실데이터를 수집해 관전 화면으로 표시 |

이는 연동 미구현이나 대체 데모가 아니라 **개발망 전용 접속 정보와 비밀값을 소스에서 분리한
배포 정책**이다. 주소와 인증 정보를 제공하면 코드 변경 없이 같은 빌드에서 통합이 활성화된다.
설정 예시인 `https://spacea.msalt.net`은 공개 데모가 아니므로, API key 없는 평가 환경에서
`401 Unauthorized` 또는 `missing api key`가 반환되는 것은 정상이다.

## 비밀값 없이 빠르게 실행하기

### 1. A-Lens 화면 확인

가장 빠른 시각 검증 경로다. Docker가 실행되는 셸에서:

```powershell
cd a-lens
Copy-Item backend/.env.example backend/.env
docker compose up -d --build
```

bash에서는 두 번째 줄 대신 `cp backend/.env.example backend/.env`를 사용한다. 브라우저에서
`http://localhost:8600`을 열면 번들 더미 데이터로 로비와 팀 방을 탐색할 수 있다. 화면의
`FAKE` 표시는 장애 폴백이 아니라 현재 원천이 의도한 데모 데이터임을 나타낸다.

정상 상태는 다음 API로 확인한다.

```sh
curl http://localhost:8600/api/health
curl http://localhost:8600/api/lobby
```

개발 서버 방식과 설정 상세는 [A-Lens 빌드·실행 문서](./architecture/a-lens/build-and-run.md)를
따른다.

### 2. A-Hub API 확인

Work와 Life를 함께 실행한다. 별도 API key 없이 로컬 기능을 검증할 수 있다.

```sh
cd a-hub
docker compose up -d --build
curl http://localhost:8000/healthz
curl http://localhost:8001/healthz
```

- Work REST·OpenAPI: `http://localhost:8000/docs`
- Work MCP Streamable HTTP: `http://localhost:8000/mcp`
- Life REST·OpenAPI: `http://localhost:8001/docs`
- Life 기능 협상: `http://localhost:8001/capabilities`

직접 Python으로 실행하는 방법은 [A-Hub 빌드·실행 문서](./architecture/a-hub/build-and-run.md)를
따른다.

### 3. A-Mate 데스크톱 앱 확인

A-Mate는 **네이티브 Windows 전용** Tauri 앱이다. Windows PowerShell 또는 cmd에서:

```powershell
cd a-mate
npm install
npm run tauri dev
```

개발 실행에는 updater 서명 개인키나 암호가 필요하지 않다. 첫 Rust 빌드는 수 분 걸릴 수 있으며,
Windows·WSL의 지원 에이전트 기록이 있으면 로컬에서 수집하고 없으면 빈 상태 UI로 정상 실행된다.
Node.js, Rust MSVC, C++ Build Tools와 WebView2 준비 방법은
[A-Mate 빌드·실행 문서](./architecture/a-mate/build-and-run.md)를 따른다.

## 컴포넌트 연결 활성화

개발망 또는 로컬 A-Hub를 사용할 때 다음 값만 설정한다. 비밀값은 커밋하지 않는다.

| 연결 | 설정 위치 | 활성화 결과 |
|---|---|---|
| A-Mate → A-Hub Work | A-Mate `설정 → 연결 → 팀 지식 허브`, 또는 `SPACE_A_HUB_URL` | 선별된 해결 지식의 검색·발행·인용·재사용 피드백 |
| A-Mate → A-Hub Life | A-Mate `설정 → 연결 → Life Server` | 개인 방 등록, 위치·프레즌스, 방문·방명록·마스코트 동기화 |
| A-Lens → A-Hub Work | A-Lens 설정에서 source를 `auto`/`hub`로 변경하고 Work URL·token·API key 입력 | 실데이터 Space·Issue·Page·재사용 수집 |
| A-Lens → A-Hub Life | A-Lens 설정에 Life URL·token·API key 입력 | 사람 신원·프레즌스·마스코트 이미지 결합 |
| A-Mate → A-Lens | A-Mate 설정에 A-Lens URL 입력 | 홈 화면에 해당 Space의 관전 화면을 여는 링크 표시 |

A-Mate의 A-Lens 연결은 데이터 전송 경로가 아니라 브라우저 이동 링크다. 실제 관전 데이터는
A-Lens가 A-Hub Work·Life에서 읽는다. 컴포넌트 간 스키마의 정본은
[`contracts/`](../contracts/)이며, 상세 데이터 흐름은
[통합 아키텍처](./architecture/README.md)와
[A-Lens 데이터·연동 문서](./architecture/a-lens/04-data-and-integration.md)에 있다.

## 테스트와 정적 검증

의존성을 각 빌드·실행 문서대로 설치한 뒤 아래 명령으로 구현을 독립 검증한다.

| 대상 | 명령 | 검증 범위 |
|---|---|---|
| A-Mate frontend | `cd a-mate && npm test` | Svelte 타입 검사와 Vitest |
| A-Mate Rust | `cd a-mate && cargo test --workspace` | 수집·규칙·저장소·Tauri 코어 |
| A-Hub Work | `cd a-hub/work && python -m pytest -q` | 메모리·SQLite, REST·MCP 도메인 흐름 |
| A-Hub Life | `cd a-hub/life && python -m pytest -q` | 등록·방·위치·소셜·영속화 |
| A-Lens backend | `cd a-lens/backend && python -m pytest` | 수집·폴백·설정·뷰모델·Life 결합 |
| A-Lens frontend | `cd a-lens/frontend && npm run build` | TypeScript 검사와 프로덕션 빌드 |

## 구현 근거를 찾는 순서

1. [README](../README.md) — 문제, 제품 구성, 구현 범위, 출처
2. [PRD](./PRD.md) — 사용자 흐름, 차별성, 완료 기준
3. [통합 아키텍처](./architecture/README.md) — 경계, 데이터 흐름, 배포 구조
4. [기술 하이라이트](./architecture/tech-highlights.md) — 비자명한 문제와 해결, 코드 경로
5. [`contracts/`](../contracts/) — 컴포넌트 간 기계 판독 계약과 픽스처

현재 기능의 근거는 위 문서와 코드에서 확인한다. `docs/archive/`는 개발 과정과 과거 설계의
이력이며 현재 구현을 판정하는 정본으로 사용하지 않는다.
