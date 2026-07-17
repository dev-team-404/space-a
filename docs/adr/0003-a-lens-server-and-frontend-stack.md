# 0003. a-lens 서버·프론트 스택 (FastAPI + PixiJS)

- 상태: 채택(Accepted)
- 날짜: 2026-07-17

## 배경

목표 아키텍처(2026-07-17 합의, [docs/highlevel/architecture.html](../highlevel/architecture.html))에서
a-lens는 관전 웹 UI를 서빙하는 **서버 컴포넌트**로 확정됐다 — 브라우저는 접근 수단이지
컴포넌트가 아니다. 지금까지는 정적 vanilla JS 프로토타입(`a-lens/` 루트)으로 화면·표시
결정을 검증했고, 이제 실제 구현을 시작한다.

요구는 두 축이다:

- **backend** — a-hub-work(C2)·a-hub-life(프레즌스)의 raw data를 가져와 가공(집계·
  인덱싱·뷰모델 변환)해서 프론트에 내려준다. G8 결정(a-lens가 두 소스를 직접 조인)의
  조인 지점이기도 하다.
- **frontend** — 게임형 UI. 이후 캐릭터 이동·클릭 시 정보 카드 같은 인터랙션이 예정돼
  있다 (설계의 열린 질문 Q2 프레임워크 / Q3 씬 렌더가 이 결정으로 닫힌다).

## 결정

1. **폴더 구조** — `a-lens/backend/`(서버) + `a-lens/frontend/`(웹 클라이언트).
   기존 프로토타입 파일은 이관이 끝날 때까지 루트에 유지한다.
2. **backend = Python FastAPI.** a-hub-work·room-server와 동일한 팀 표준 스택
   (uvicorn·Docker 패턴 재사용). 구조는 `collector`(원천 폴링) → `pipeline`(가공·
   조인·인덱스) → `api`(뷰모델 REST + 정적 서빙). 지금 브라우저 어댑터
   (`c2-adapter.js`)가 하던 번역은 pipeline으로 이동한다 — **프론트는 완성된
   뷰모델만 받는다** (BFF).
3. **frontend = Vite + TypeScript + PixiJS(씬) + DOM 오버레이(패널·모달).**
   - Q3을 닫는다: 씬(방·캐릭터·이동·클릭)은 PixiJS 캔버스.
   - a-lens의 본질은 데이터 인포그래픽이므로 피드·모달·대시보드는 HTML/CSS —
     프로토타입의 CSS·연출 자산을 재사용한다.
   - Q2는 "당분간 프레임워크 미도입"으로 닫는다. HUD 상태 관리가 아파지면 재검토.
   - Phaser(풀 게임 엔진)는 충돌·카메라 추적 수준의 본격 게임플레이가 필요해질 때
     재검토한다.
4. **데이터 원천 전략** — collector는 실서버(work: `spacea.msalt.net`, life: room-server)
   폴링을 기본으로 하되, C2 미구현·미배포 구간은 `contracts/fixtures/`로 대체한다
   (원천만 바뀌고 pipeline·api는 동일).

## 결과

- 번역 로직의 소유가 서버로 올라가 "프론트는 받은 것을 그대로 그린다" 원칙이
  구조로 강제된다.
- 팀 백엔드 3개(work·life·a-lens)가 전부 FastAPI — 서로의 코드를 읽고 고칠 수 있다.
- 프론트 갱신은 MVP에서 폴링, 이후 SSE/WebSocket 여지 (03-architecture "데이터
  리듬 두 축"과 일치).
- 프로토타입은 표시 결정의 참조 구현으로 남고, 화면 단위로 frontend에 이관 후 제거한다.
