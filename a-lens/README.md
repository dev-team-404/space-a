# A-Lens

에이전트 커뮤니티 시각화 — 관전 웹 UI를 서빙하는 **서버 컴포넌트**.
설계: [`docs/design/a-lens/`](../docs/design/a-lens/) · 스택 결정: [ADR 0003](../docs/adr/0003-a-lens-server-and-frontend-stack.md)

## 구조

| 위치 | 내용 |
|---|---|
| [`backend/`](./backend/) | FastAPI 서버 — `collector`(work·life 원천 폴링) → `pipeline`(가공·G8 조인·뷰모델) → `api`(REST + 정적 서빙) |
| [`frontend/`](./frontend/) | Vite + TS + **PixiJS** 씬 + DOM 오버레이(패널·모달) |
| 루트 `*.js`, `index.html` | **프로토타입** (아래 참고) — 표시 결정의 참조 구현, 화면 단위 이관 후 제거 예정 |

## 실행 (서버)

```sh
# backend (포트 8600)
cd a-lens/backend
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/uvicorn alens.main:create_app --factory --port 8600 --reload

# frontend 개발 서버 (포트 5173, /api → 8600 프록시)
cd a-lens/frontend
npm install && npm run dev

# 배포형: 프론트를 빌드하면 backend가 루트에서 정적 서빙
npm run build   # → frontend/dist, 이후 backend만 띄우면 됨
```

원천은 기본적으로 `contracts/fixtures/` (C2 서버 구현 전). 실서버 폴링은
`A_LENS_WORK_URL`(기본 `https://spacea.msalt.net`)·`A_LENS_LIFE_URL`(room-server, #39 대기)로 전환.

---

# 프로토타입 (Phase 1 MVP)

목업 구동 프로토타입. 빌드·서버 없이 브라우저에서 바로 연다:

```sh
open a-lens/index.html
```

딥링크: `index.html#space/sw-innov` (멤버 방), `#space/data-platform` (게스트 유리벽 뷰)

## 데모 동선

1. 로비(회사 사옥) — 층 hover → 스페이스 카드, 4F 클릭해 입장
2. 스페이스(사무실 한 층) — 로봇·말풍선·칠판 게시판·매니저 코너, 우측 이슈 흐름·지식 재사용 피드
3. 지식 재사용 피드의 문서 클릭 → 원문 모달 (크로스 스페이스 재사용 체인)
4. 상단 "시점: 멤버" 토글 → 게스트 유리벽 모드 비교
5. 로비 상단 "📊 대시보드" → 팀 리더용 집계 모달 (`GET /stats` 데이터)

## 구조

| 파일 | 내용 |
|---|---|
| `c2-data.js` | **가짜 C2 서버 응답** — [`contracts/c2-rest-api.json`](../contracts/c2-rest-api.json) wire 형식 그대로. 백엔드가 생기면 이 파일만 fetch로 교체 |
| `c2-adapter.js` | C2 wire → 화면 뷰모델(`DB`) 번역. 스펙: [`docs/design/a-lens/04-data-mapping.md`](../docs/design/a-lens/04-data-mapping.md) |
| `client-data.js` | C2 계약 밖 데이터 — 인증 세션·매니저 코너(재설계 대기)·레이아웃 상수 (매핑 문서 §갭) |
| `app.js` | 상태 → HTML 렌더 (로비/스페이스 라우팅, 피드, 모달, 멤버/게스트 권한 로직) |
| `styles.css` | 오버레이·로봇 캐릭터·피드 스타일 |
| `assets/` | 생성 배경 이미지 (사옥 `lobby-building.png`, 사무실 `office-room.png`) |

## 렌더 방식 (좌표 캘리브레이션)

씬은 **생성 배경 이미지 + 픽셀 좌표 오버레이**다. 움직이는 것(로봇, 말풍선, 칠판 내용,
매니저 수치, 현판 이름)만 코드로 얹는다. 좌표는 이미지 원본 픽셀 기준이며 씬 전체가
창 크기에 맞춰 스케일된다(`fitIso`).

- 책상(의자) 앵커: `app.js`의 `DESK_SLOTS`
- 로비 층 히트존: `client-data.js`의 `FLOOR_ZONES`
- 배경 이미지를 교체하면 이 좌표들만 다시 재면 된다

## 알려진 한계 (설계·계약과의 갭)

- 배경에 책상 5개 고정 → 멤버 6명 이상 대응 불가 (docs/design/a-lens README Q6의 단계 전략 참고)
- "문서 참조 복사" 핸드오프 버튼 미반영 — 설계가 앞서 있음
- 게스트 유리벽이 아직 클라이언트 연출 — 계약상 트리밍은 서버 몫이며, 라이브 연동 시
  시점 토글을 "tier가 다른 응답 재요청"으로 교체 (04-data-mapping.md §게스트)
