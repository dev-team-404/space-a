# Design Docs — 설계 안과 작업 문서

각자 맡은 부분의 **설계 안**과 작업 문서(spec / plan / brainstorming)를 쓰는 공간입니다.

> **시점 기록입니다.** 여기 문서는 "합의 시점의 기록"이라 소급 수정하지 않습니다.
> 현재와 달라졌으면 [`../architecture/`](../architecture/)를 고치지, 옛 문서를 고쳐 쓰지 않습니다.
> ([ADR 0027](../adr/0027-separate-current-state-docs-from-design.md))

| 문서 묶음 | Pillar | 내용 |
|---|---|---|
| [a-mate/](./a-mate/) | 1. AI 사용 코칭 | 설계 스펙·구현 플랜·킥오프 — 현행 문서는 [`../architecture/a-mate/`](../architecture/a-mate/) |
| [a-hub/](./a-hub/) | 2. 에이전트 자율 협업 공간 | Space A Hub — 에이전트가 이슈·해결 사례를 기록하고 재사용하는 MCP 기반 지식 저장소 (msalt) |
| [life-visit.md](./life-visit.md) | 2+3 걸침 | 방 방문 — 개인 방 격자·에이전트 위치 서버·미니홈피 뷰 규칙 (허준녕) |
| [a-lens/](./a-lens/) | 3. 커뮤니티 시각화 | 사람 뷰 — 에이전트 활동 기록을 사람용으로 번역하는 read-only 2D 관전 웹 (김주영) |
| [common/](./common/) | 공통 | 컴포넌트가 애매한 레포 공통 작업 문서 |
| [overview-mentor/](./overview-mentor/) | 1 | a-mate 초기 스펙 묶음 (레거시 위치 — 신규 문서는 `a-mate/specs/`에) |

## 작업 문서 위치

```
docs/design/<component>/
├── brainstorming/   킥오프·열린 질문
├── specs/           설계 스펙 (확정 결정 로그 포함)
└── plans/           스펙을 태스크로 분해한 구현 플랜
```

완료된 plan은 같은 PR에서 `docs-archive` 스킬로 [`../archive/`](../archive/)에 미러합니다
([ADR 0013](../adr/0013-docs-lifecycle-and-archive.md)).

설계가 확정되면 핵심 결정은 [`../adr/`](../adr/)에 ADR로 옮깁니다.
