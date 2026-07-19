# CLAUDE.md

SPACE-A는 **문서 우선** 프로젝트입니다. 구조·아키텍처 결정은 코드보다 먼저 `docs/`에 기록합니다.

## 저장소 구조

| 위치 | 내용 |
|------|------|
| [a-mate/](./a-mate/) | Pillar 1 — AI 사용 코칭 데스크톱 앱 (Agent Mentor). 제약: [a-mate/CLAUDE.md](./a-mate/CLAUDE.md) |
| [a-hub/](./a-hub/) | Pillar 2 — 협업 공간 백엔드 (`work/`=업무, `life/`=방 방문·소셜). 제약: [a-hub/CLAUDE.md](./a-hub/CLAUDE.md) |
| [a-lens/](./a-lens/) | Pillar 3 — 커뮤니티 시각화 웹 UI. 제약: [a-lens/CLAUDE.md](./a-lens/CLAUDE.md) |
| [contracts/](./contracts/) | 컴포넌트 간 **경계 계약** (C1 MCP / C2 REST / C4 admin 스키마 + fixtures) |

## 문서 구조 (`docs/`)

| 위치 | 내용 |
|------|------|
| [docs/README.md](./docs/README.md) | 문서 인덱스 + 작성 규칙 |
| [docs/highlevel/](./docs/highlevel/) | 사람이 읽는 레벨 문서 (개요→기능→상세, 시작점 `level-0.md`) |
| [docs/design/](./docs/design/) | 팀원별 컴포넌트 설계 안 (설계 단계 작업물) |
| [docs/adr/](./docs/adr/) | Architecture Decision Records (확정된 주요 결정 기록) |
| [docs/archive/](./docs/archive/) | 완료·폐기 작업 문서 미러 (**기본 탐색 제외**) |

## 문서 작성 규칙

- **문서 우선.** 구조·아키텍처 관련 결정은 코드보다 먼저 `docs/`에 기록한다.
- **중요한 결정은 ADR로.** 되돌리기 어려운 결정(레포 구성, 스택, 스키마 등)은
  `docs/adr/`에 `NNNN-title.md` 형식으로 남긴다. 채택된 ADR은 수정하지 않고 새 ADR로 대체한다.
- **경계 계약은 `contracts/`가 정답.** 컴포넌트 간 계약(C1/C2/C4)이 바뀌면 `contracts/`의
  스키마·픽스처를 먼저 갱신한다. 문서와 어긋나면 파일을 따른다.
- **설계 안은 `docs/design/`에.** 컴포넌트별 설계 안을 자유롭게 작성한다.
- **작업 문서 위치.** spec/plan/kickoff 문서는 해당 컴포넌트의
  `docs/design/<component>/{specs,plans,brainstorming}`에 저장한다 (superpowers 등
  스킬의 기본 저장 경로보다 이 규칙이 우선). 컴포넌트가 애매한 레포 공통 작업은
  `docs/design/common/{specs,plans}`에. `docs/superpowers/`는 동결 — 신규 생성 금지.
- **완료 시 아카이브 (DoD).** 구현 plan이 완료되면 같은 PR에서 `docs-archive` 스킬을
  실행해 관련 작업 문서를 `docs/archive/` 미러로 옮긴다. 규칙 상세: [ADR 0013](./docs/adr/0013-docs-lifecycle-and-archive.md)
- **`docs/archive/`는 기본 탐색에서 제외.** 과거 이력 조사가 목적일 때만 명시적으로 읽는다.
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

## 참고

- 프로젝트 개요: [`README.md`](./README.md)
- 멘토 앱 빌드·실행: [`docs/design/a-mate/build-and-run.md`](./docs/design/a-mate/build-and-run.md)
