# CLAUDE.md

SPACE-A는 **문서 우선** 프로젝트입니다. 구조·아키텍처 결정은 코드보다 먼저 `docs/`에 기록합니다.

## 저장소 구조

| 위치 | 내용 |
|---|---|
| [a-mate/](./a-mate/) | A-Mate — 로컬 AI 사용 코칭 데스크톱 앱. 제약: [a-mate/CLAUDE.md](./a-mate/CLAUDE.md) |
| [a-hub/](./a-hub/) | A-Hub — Work 업무 지식 서버와 Life 공간·소셜 서버. 제약: [a-hub/CLAUDE.md](./a-hub/CLAUDE.md) |
| [a-lens/](./a-lens/) | A-Lens — 팀 활동 관전·시각화 웹 서비스. 제약: [a-lens/CLAUDE.md](./a-lens/CLAUDE.md) |
| [contracts/](./contracts/) | 컴포넌트 간 기계 판독 API 계약과 픽스처 |

## 문서 구조

| 위치 | 내용 |
|---|---|
| [README.md](./README.md) | 프로젝트 진입점과 현재 구현 범위 |
| [docs/PRD.md](./docs/PRD.md) | 현재 제품 요구사항, 사용자 흐름, 범위와 완료 기준 |
| [docs/architecture/](./docs/architecture/) | 현재 구현의 통합·컴포넌트 아키텍처와 실행 문서 |
| [docs/adr/](./docs/adr/) | 채택된 Architecture Decision Records |
| [docs/archive/](./docs/archive/) | 완료·폐기된 하이레벨·설계·작업 문서 이력 |

## 문서 작성 규칙

- **문서 우선.** 구조·아키텍처 관련 결정은 코드보다 먼저 기록합니다.
- **현재 상태는 PRD와 architecture에.** 제품 범위 변경은 `docs/PRD.md`, 구조·운영 변경은 `docs/architecture/`를 코드와 같은 변경에서 갱신합니다.
- **중요한 결정은 ADR로.** 되돌리기 어려운 결정은 `docs/adr/NNNN-title.md`로 남깁니다. 채택된 ADR은 의미를 수정하지 않고 새 ADR로 대체합니다.
- **경계 계약은 contracts가 정본.** C1/C2/C4 등 컴포넌트 간 스키마가 바뀌면 `contracts/`를 먼저 갱신합니다.
- **과거 문서는 archive에.** `docs/archive/`는 이력 조사 목적일 때만 사용하며 현재 기능의 근거로 인용하지 않습니다.
- **현재와 구상을 섞지 않습니다.** 코드에 없는 아이디어는 제출용 README·PRD·architecture에 구현 기능처럼 쓰지 않습니다.
- **문서만 바뀌는 변경에는 코드 테스트를 돌리지 않습니다.** 문서의 수치를 확인할 때만 필요한 명령을 골라 실행합니다.
- 문서는 한국어를 기본으로 하고 표와 다이어그램을 활용합니다.

## 커밋 메시지 규칙

Conventional Commits를 따르며 메시지는 영어로 작성합니다.

```text
<type>(<scope>): <subject>
```

- type: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `style`, `perf`, `build`, `ci`
- scope: 변경 영역(선택). 예: `agent`, `backend`, `frontend`, `schema`, `docs`, `infra`
- subject: 명령형·현재형, 소문자로 시작하고 마침표를 붙이지 않습니다.

## 참고

- 전체 문서 안내: [docs/README.md](./docs/README.md)
- A-Mate 빌드·실행: [docs/architecture/a-mate/build-and-run.md](./docs/architecture/a-mate/build-and-run.md)
