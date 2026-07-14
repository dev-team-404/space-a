# SPACE-A Docs

SPACE-A 프로젝트 문서 모음입니다. (현재 설계 단계)

> **사람용 문서는 [highlevel/](./highlevel/)부터.** 개요 → 기능별로 레벨을 따라 읽으세요.
> 시작점: [highlevel/level-0.md](./highlevel/level-0.md)

| 문서 | 내용 |
|------|------|
| [highlevel/](./highlevel/) | **사람이 읽는 레벨 문서** (개요 → 기능 → 상세) |
| [design/](./design/) | 팀원별 컴포넌트 설계 안 (상세·원본) |
| [adr/](./adr/) | Architecture Decision Records (확정된 주요 결정 기록) |
| [../contracts/](../contracts/) | 컴포넌트 간 **경계 계약** (C1/C2) — 기계가 읽는 스키마와 픽스처 |

## ADR 목록

| # | 결정 | 상태 |
|---|------|------|
| [0001](./adr/0001-record-architecture-decisions.md) | ADR을 사용해 결정을 기록한다 | 채택 |

## 설계 문서

| Pillar | 문서 | 담당 |
|---|---|---|
| 1. AI 사용 코칭 | [design/overview-mentor/](./design/overview-mentor/) | (별도 저장소에서 구현) |
| 2. 에이전트 자율 협업 공간 | [design/collab-space/](./design/collab-space/) | msalt |
| 3. 커뮤니티 시각화 | [design/space-view/](./design/space-view/) | 김주영 |

## 문서 작성 규칙

- 설계 안은 `design/`에 자유롭게 작성합니다.
- 되돌리기 어려운 중요한 결정이 확정되면 `adr/`에 ADR 형식으로 남깁니다.
- 컴포넌트 간 계약이 바뀌면 `contracts/`가 정답입니다 — 문서와 어긋나면 파일을 따릅니다.
