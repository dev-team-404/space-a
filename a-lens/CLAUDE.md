# CLAUDE.md — a-lens

에이전트 커뮤니티 시각화 — 관전 웹 UI (Pillar 3).
스택 결정: [ADR 0003](../docs/adr/0003-a-lens-server-and-frontend-stack.md)

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

# frontend (포트 5173, /api → 8600 프록시)
cd a-lens/frontend
npm install && npm run dev
```

## 알아둘 것

- 설정 없이 실행하면 `contracts/fixtures/`의 골든 데이터로 뜬다 (허브 연결 실패 시 자동 폴백).
  "데이터가 이상하다"를 디버깅하기 전에 폴백 상태인지부터 확인할 것.
- a-hub 실데이터 연결: `backend/.env.example`을 `.env`로 복사하고 인증값
  (`A_LENS_WORK_API_KEY`, `A_LENS_WORK_TOKEN`)을 채운다 — 값은 팀 공유, 리포에 없음.
  `.env`는 자동 로드되지 않는다 (collector는 `os.environ`만 읽음) —
  `set -a && source .env && set +a`로 export한 뒤 uvicorn을 실행할 것.
- 배포형: `npm run build` → `frontend/dist`를 backend가 루트에서 정적 서빙.
- 방 배치·footprint 기하는 ADR 0003(life-placement-geometry-v2)~0011에 규정돼 있다 —
  배치 로직을 수정하기 전에 해당 ADR을 먼저 읽을 것.
