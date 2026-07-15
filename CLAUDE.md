# CLAUDE.md

SPACE-A는 **문서 우선** 프로젝트입니다. 구조·아키텍처 결정은 코드보다 먼저 `docs/`에 기록합니다.
(구현: `client/` Tauri 클라이언트, `hub/` 백엔드, `prototype/` 시각화)

## 문서 구조 (`docs/`)

| 위치 | 내용 |
|------|------|
| [docs/README.md](./docs/README.md) | 문서 인덱스 + 작성 규칙 |
| [docs/design/](./docs/design/) | 팀원별 컴포넌트 설계 안 (설계 단계 작업물) |
| [docs/adr/](./docs/adr/) | Architecture Decision Records (확정된 주요 결정 기록) |

## 문서 작성 규칙

- **문서 우선.** 구조·아키텍처 관련 결정은 코드보다 먼저 `docs/`에 기록한다.
- **중요한 결정은 ADR로.** 되돌리기 어려운 결정(레포 구성, 스택, 스키마 등)은
  `docs/adr/`에 `NNNN-title.md` 형식으로 남긴다. 채택된 ADR은 수정하지 않고 새 ADR로 대체한다.
- **설계 안은 `docs/design/`에.** 컴포넌트별 설계 안을 자유롭게 작성한다.
- **문서 형식.** 한국어 기준, 표·다이어그램을 적극 활용해 읽기 쉽게 쓴다.

## 커밋 메시지 규칙

**Conventional Commits**를 따르며, 메시지는 **영어**로 작성한다.

```
<type>(<scope>): <subject>
```

- **type** — `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `style`, `perf`, `build`, `ci`
- **scope** — 변경 영역. 예: `agent`, `backend`, `frontend`, `schema`, `docs`, `infra` (선택)
- **subject** — 명령형·현재형, 소문자 시작, 마침표 없음. 예: `add room creation API`
- 본문/푸터는 필요할 때만. Breaking change는 `!` 표시 또는 `BREAKING CHANGE:` 푸터 사용.

예시:
```
feat(backend): add room creation API
fix(agent): handle empty claude log directory
docs(design): draft frontend visualization spec
chore: set up gitignore and base structure
```

## Tauri 클라이언트 (`client/`) 개발 제약

`client/`(Cargo workspace: `crates/core`, `src-tauri`, `src/`)를 만질 때 적용:

- 제품명: Agent Mentor. 식별자 `agent-mentor`. Claude 외 타 에이전트 확장을 염두에 둔 이름이므로,
  에이전트별 로직은 하드코딩하지 말고 SourceAdapter / Engine 인터페이스 뒤로 추상화할 것.
- 스택: Tauri v2 + Rust 백엔드. v1 API(SystemTray, tauri::updater, WindowBuilder 등) 금지.
- 상주/업데이트/자동시작은 반드시 v2 공식 플러그인(tray-icon, updater, autostart)으로.
- 플랫폼: Windows 전용. macOS/Linux 분기 불필요.
- 프라이버시: 트랜스크립트는 기본 로컬 처리. 외부 전송은 Engine 선택(사내 on-prem 기본)으로만.
- 무거운 데이터 처리(JSONL 파싱/집계/감시)는 Rust 백엔드에서.
- 설계 스펙·구현 계획: [docs/design/overview-mentor/](./docs/design/overview-mentor/) 아래 `specs/`, `plans/`.

## 참고

- 프로젝트 개요: [`README.md`](./README.md)
- 멘토 앱 빌드·실행: [`docs/design/overview-mentor/build-and-run.md`](./docs/design/overview-mentor/build-and-run.md)
