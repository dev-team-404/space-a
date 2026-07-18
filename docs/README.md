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
| [0002](./adr/0002-mcp-http-and-skill-dual-access.md) | MCP·HTTP·skill 이중 접근을 제공한다 | 채택 |
| [0003](./adr/0003-room-placement-geometry-v2.md) | 방 배치 좌표와 footprint 의미를 통일한다 | 채택 |
| [0004](./adr/0004-interior-asset-orientation-contract.md) | 인테리어 자산 방향·앵커 계약을 정의한다 | 채택 |
| [0005](./adr/0005-independent-interior-assets-and-ground-metrics.md) | 테이블·의자를 분리하고 접지 메트릭을 사용한다 | 채택 |
| [0006](./adr/0006-asset-facing-metadata-and-view-resolution.md) | 자산 실제 방향 메타데이터로 목표 뷰를 해석한다 | 채택 |
| [0007](./adr/0007-footprint-axis-and-world-contact-anchor.md) | footprint 축과 월드 접지 앵커를 분리한다 | 채택 |
| [0008](./adr/0008-projected-footprint-axis-and-wall-shear.md) | 투영 축 기반 footprint와 벽 평면 전단 보정을 정의한다 | 채택 |
| [0009](./adr/0009-pixel-contact-anchor-over-image-box.md) | 이미지 박스 대신 실제 픽셀 접지점을 배치 기준으로 사용한다 | 채택 |
| [0010](./adr/0010-register-desk-source-basis-to-room-grid.md) | 책상 원본 투영축을 방 격자에 등록하고 전체 박스를 footprint에 맞춘다 | 채택 |
| [0011](./adr/0011-register-desk-support-contours.md) | 책상 원본별 다중 지지점 윤곽으로 네 방향 배치를 파생한다 | 채택 |

## 설계 문서

| Pillar | 문서 | 담당 |
|---|---|---|
| 1. AI 사용 코칭 | [design/overview-mentor/](./design/overview-mentor/) | 구현: [`a-mate/`](../a-mate/) ([빌드 가이드](./design/overview-mentor/build-and-run.md)) |
| 2. 에이전트 자율 협업 공간 | [design/collab-space/](./design/collab-space/) | msalt |
| 3. 커뮤니티 시각화 | [design/a-lens/](./design/a-lens/) | 김주영 |

## 문서 작성 규칙

- 설계 안은 `design/`에 자유롭게 작성합니다.
- 되돌리기 어려운 중요한 결정이 확정되면 `adr/`에 ADR 형식으로 남깁니다.
- 컴포넌트 간 계약이 바뀌면 `contracts/`가 정답입니다 — 문서와 어긋나면 파일을 따릅니다.
