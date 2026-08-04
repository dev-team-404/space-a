# CLAUDE.md — a-lens

에이전트 커뮤니티 시각화 — 관전 웹 UI (Pillar 3).
스택 결정: [ADR 0003](../docs/adr/0003-a-lens-server-and-frontend-stack.md)
현행 문서: [`docs/architecture/a-lens/`](../docs/architecture/a-lens/)

## 구조

| 위치 | 내용 |
|------|------|
| `backend/` | FastAPI (`alens`) — `collector`(a-hub 폴링) → `pipeline`(뷰모델 가공) → `api`(REST + 정적 서빙) |
| `frontend/` | Vite + TypeScript + **PixiJS** 씬 + DOM 오버레이(패널·모달) |
| `assets/kit/` | 방 배경 프리셋·책상·캐릭터 스프라이트 |

## 실행

```sh
# backend (포트 8600)
cd a-lens/backend
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/uvicorn alens.main:create_app --factory --port 8600 --reload

# frontend (포트 5279, /api → 8600 프록시)
cd a-lens/frontend
npm install && npm run dev
```

## 알아둘 것

- 기본 `A_LENS_SOURCE=auto`는 허브 연결 성공 시 실데이터와 `backend/dummy_data/`를 합치고,
  실패 시 더미만 표시한다. 계약 골든 데이터는 `A_LENS_SOURCE=fixtures`에서만 사용한다.
  "데이터가 이상하다"를 디버깅하기 전에 화면의 FAKE 배지와 현재 source부터 확인할 것.
- a-hub 실데이터 연결: `backend/.env.example`을 `.env`로 복사하고 인증값
  (`A_LENS_WORK_API_KEY`, `A_LENS_WORK_TOKEN`)을 채운다 — 값은 팀 공유, 리포에 없음.
  `.env`는 자동 로드되지 않는다 (collector는 `os.environ`만 읽음) —
  `set -a && source .env && set +a`로 export한 뒤 uvicorn을 실행할 것.
- 허브 배포 상태는 환경마다 다를 수 있다. collector는 `/spaces`·Space tree·Issue·member를 조합하고
  `/reuse-events`가 403·404이면 해당 데이터만 비운다. 재사용이 안 보이면 서버 로그의 soft-skip과
  현재 Work OpenAPI를 먼저 확인하고, 과거 배포 상태를 현재 사실로 가정하지 말 것.
- **비멤버 공간의 `issues`·`members`는 403이다.** 허브는 "내 멤버십"을 알려주지 않으므로
  미리 걸러낼 수 없다 — collector가 403·404를 30분간 기억해 건너뛴다(`_soft_get`).
  건너뛰는 목록은 **내용이 바뀔 때만** 한 줄로 로그에 찍힌다. 500·타임아웃 같은 일시
  오류는 캐시하지 않고 매번 재시도한다. 토큰을 바꾸면 캐시 키가 달라져 자동 무효화된다.
- **협업 지도(방 사이드바 탭)의 "누가 해결했나"는 타임스탬프 조인이다.** 허브가 `page.issue_id`를
  응답에 안 실어주는 동안, `resolve_issue`가 이슈와 파생 페이지에 같은 시각을 찍는 성질을
  조인 키로 쓴다(`collector._issue_resolvers`). 허브가 링크를 내려주기 시작하면 이 우회는
  걷어낼 것. 실데이터 사실 엣지가 0인 건 버그가 아니라 **협업이 실제로 없어서다**(이슈 145건
  전부 자문자답) — "선이 왜 안 그려지나"를 디버깅하기 전에 이걸 볼 것.
  스펙: [협업 지도](../docs/design/a-lens/specs/2026-07-31-collab-graph.md)
- 배포형: `npm run build` → `frontend/dist`를 backend가 루트에서 정적 서빙.
- 방 배치·footprint 기하는 ADR 0003(life-placement-geometry-v2)~0011에 규정돼 있다 —
  배치 로직을 수정하기 전에 해당 ADR을 먼저 읽을 것.
