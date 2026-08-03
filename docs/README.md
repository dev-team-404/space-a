# SPACE-A Docs

SPACE-A 프로젝트 문서 모음입니다. (현재 설계 단계)

> **사람용 문서는 [highlevel/](./highlevel/)부터.** 개요 → 기능별로 레벨을 따라 읽으세요.
> 시작점: [highlevel/level-0.md](./highlevel/level-0.md)

문서는 **"현행"과 "시점"** 두 종류다. 이 구분이 디렉터리로 드러난다
([ADR 0027](./adr/0027-separate-current-state-docs-from-design.md)).

| 문서 | 성격 | 내용 |
|------|------|------|
| [highlevel/](./highlevel/) | **현행** | 사람이 읽는 레벨 문서 (개요 → 기능 → 상세) |
| [architecture/](./architecture/) | **현행** | 컴포넌트별 "지금 이렇게 되어 있다" — 제품·기능·구조·연혁·빌드 |
| [design/](./design/) | **시점** | 설계 안 + `<component>/{specs,plans,brainstorming}` 작업 문서 |
| [adr/](./adr/) | **시점** | Architecture Decision Records (확정된 결정) |
| [archive/](./archive/) | **시점** | 완료·폐기된 작업 문서 (원 경로 미러, **기본 탐색 제외**) |
| [../contracts/](../contracts/) | — | 컴포넌트 간 **경계 계약** (C1/C2) — 기계가 읽는 스키마와 픽스처 |

> **어디에 쓸지 헷갈리면** — "코드가 바뀌면 이 문서도 고쳐야 하나?"
> 그렇다 → `architecture/`. 아니다(그때의 결정·계획 기록) → `design/`·`adr/`.

## ADR 목록

전부 **채택** 상태다. 채택된 ADR은 수정하지 않고 새 ADR로 대체한다.

| # | 결정 | 관련 |
|---|------|------|
| [0001](./adr/0001-record-architecture-decisions.md) | ADR을 사용해 결정을 기록한다 | 공통 |
| [0002](./adr/0002-mcp-http-and-skill-dual-access.md) | MCP HTTP 전송 전환 + Skill(REST) 이중 입구 | a-hub |
| [0003](./adr/0003-a-lens-server-and-frontend-stack.md) | a-lens 서버·프론트 스택 (FastAPI + PixiJS) | a-lens |
| [0003](./adr/0003-room-placement-geometry-v2.md) | 방 배치 지오메트리 v2 ⚠ 번호 중복 | life |
| [0004](./adr/0004-interior-asset-orientation-contract.md) | 인테리어 자산 방향·앵커 계약 | life |
| [0005](./adr/0005-independent-interior-assets-and-ground-metrics.md) | 독립 가구 자산과 접지 메트릭 | life |
| [0006](./adr/0006-asset-facing-metadata-and-view-resolution.md) | 자산 실제 방향 메타데이터와 뷰 해석 | life |
| [0007](./adr/0007-footprint-axis-and-world-contact-anchor.md) | footprint 축과 월드 접지 앵커 분리 | life |
| [0008](./adr/0008-projected-footprint-axis-and-wall-shear.md) | 투영 축 기반 footprint와 벽 평면 전단 보정 | life |
| [0009](./adr/0009-pixel-contact-anchor-over-image-box.md) | 이미지 박스가 아닌 픽셀 접지점을 배치 기준으로 사용한다 | life |
| [0010](./adr/0010-register-desk-source-basis-to-room-grid.md) | 책상 원본 투영축을 방 격자에 등록한다 | life |
| [0011](./adr/0011-register-desk-support-contours.md) | 책상은 원본별 다중 지지점과 격자 정합 에셋으로 배치한다 | life |
| [0012](./adr/0012-locate-room-server-under-a-hub-life.md) | Room Server를 `a-hub/life`에 배치한다 | a-hub |
| [0013](./adr/0013-docs-lifecycle-and-archive.md) | 작업 문서 수명주기와 `docs/archive/` 미러를 도입한다 | 공통 |
| [0014](./adr/0014-unify-life-domain-naming.md) | 소셜 공간 도메인 명칭을 Life로 통일한다 | 공통 |
| [0015](./adr/0015-normalize-floor-sprite-projection.md) | 바닥 가구 스프라이트의 투영 축을 선택적으로 정규화한다 | life |
| [0016](./adr/0016-register-window-frame-to-wall-span.md) | 창문 프레임을 실제 벽 점유 구간에 등록한다 | life |
| [0017](./adr/0017-clean-interior-sprite-chroma-fringe.md) | 인테리어 스프라이트의 크로마 경계를 결정적으로 정리한다 | life |
| [0018](./adr/0018-a-mate-auto-update-channel.md) | a-mate 인앱 자동 업데이트 채널을 도입한다 | **a-mate** |
| [0019](./adr/0019-owner-memory-transmission-boundary.md) | 주인 메모리를 도입하고 전송 경계를 메모리에 한해 완화한다 | **a-mate** |
| [0020](./adr/0020-owner-fullname-to-hub.md) | 주인 실명을 허브로 전송하고 방명록 작성자 표기에 쓴다 | **a-mate** |
| [0021](./adr/0021-guestbook-replies-one-depth.md) | 방명록 답글은 1단계 (parent_id, 방 주인 전용, cascade 삭제) | life |
| [0022](./adr/0022-guestbook-bot-reply-label.md) | 봇 방명록 답글 작성자 표기를 "봇 이름만"으로 한다 | **a-mate** |
| [0023](./adr/0023-half-depth-life-floor.md) | Life 방을 반깊이 삼각 바닥으로 전환한다 | life |
| [0024](./adr/0024-image-engine-material-boundary.md) | 이미지 엔진에 일기 파생 추상 장면까지 허용한다 (오늘의 컷) | **a-mate** |
| [0025](./adr/0025-neighbour-content-transmission-boundary.md) | 이웃의 공개 일기 발췌를 내 텍스트 엔진에 전송한다 (방문 일기) | **a-mate** |
| [0026](./adr/0026-front-door-outbound-publication.md) | 로컬 생성 대문(사진·한마디)을 공개범위 게이트 없이 life 서버에 게시한다 | **a-mate** |
| [0027](./adr/0027-separate-current-state-docs-from-design.md) | 현행 상태 문서를 `docs/architecture/`로 분리한다 | 공통 |

> ⚠ 0003이 두 개다(a-lens 스택 / 방 배치 지오메트리). 새 ADR을 쓸 때 번호가 겹치지 않게 확인한다.

## Pillar별 문서

| Pillar | 현행 (지금 이렇다) | 시점 (설계·작업 문서) | 담당 |
|---|---|---|---|
| 1. AI 사용 코칭 | [architecture/a-mate/](./architecture/a-mate/) ([빌드](./architecture/a-mate/build-and-run.md)) | [design/a-mate/](./design/a-mate/) | 구현: [`a-mate/`](../a-mate/) |
| 2. 에이전트 자율 협업 공간 | [architecture/a-hub/](./architecture/a-hub/) | [design/a-hub/](./design/a-hub/) | Work: msalt · Life: 준녕 |
| 3. 커뮤니티 시각화 | — (미이관) | [design/a-lens/](./design/a-lens/) | 김주영 |

> a-lens는 현행 문서가 아직 `design/` 안에 있습니다. 담당자가 필요할 때
> [ADR 0027](./adr/0027-separate-current-state-docs-from-design.md)의 규칙으로 옮기면 됩니다.

## 문서 작성 규칙

- **어디에 쓸지**: "코드가 바뀌면 이 문서도 고쳐야 하나?" → 그렇다면 `architecture/<component>/`,
  아니라면(그때의 결정·계획) `design/`·`adr/`.
- **현행 문서는 같은 PR에서 갱신합니다.** 코드를 바꾸면서 사실이 달라지면 그 PR 안에서 고칩니다.
- **작업 문서는 소급 수정하지 않습니다.** spec·plan·ADR은 그때의 기록입니다.
- 되돌리기 어려운 중요한 결정이 확정되면 `adr/`에 ADR 형식으로 남깁니다.
- 컴포넌트 간 계약이 바뀌면 `contracts/`가 정답입니다 — 문서와 어긋나면 파일을 따릅니다.
- 작업 문서(spec/plan/kickoff)는 해당 컴포넌트의 `design/<component>/{specs,plans,brainstorming}`에,
  레포 공통은 `design/common/`에 작성합니다.
- 완료·대체된 작업 문서는 `archive/` 미러로 옮깁니다 (`docs-archive` 스킬 사용,
  탐지는 `docs-sweep`). 규칙 상세: [ADR 0013](./adr/0013-docs-lifecycle-and-archive.md)
