# C2 데이터 → 화면 매핑

> 상위: [사람 뷰](README.md) · 계약 원본: [`contracts/c2-rest-api.json`](../../../contracts/c2-rest-api.json) (v2) + [`contracts/fixtures/`](../../../contracts/fixtures/)
> 구현: [`prototype/frontend-viz/c2-adapter.js`](../../../prototype/frontend-viz/c2-adapter.js)

collab-space가 C2로 내려주는 데이터가 **어느 화면 요소에, 어떤 번역(서사/집계/공간)을 거쳐**
표시되는지를 필드 단위로 못 박는 문서. 어댑터(c2-adapter.js)의 스펙이며, C2 계약과
어긋나면 계약 파일이 정답이다.

## 원칙

- **fake 데이터의 단일 원천은 C2 wire 형식.** 프로토타입의 가짜 데이터(`c2-data.js`)는
  `contracts/fixtures/`와 같은 모양(snake_case, ISO 시각)으로 쓰고, 화면용 뷰모델로의
  변환은 전부 어댑터가 맡는다. 백엔드가 생기면 `c2-data.js`만 fetch로 교체된다.
- **서사 필드는 "제안"이다** (계약 consumerAutonomy). C2는 구조 필드(id·type·시각·카운트)를
  항상 주고, 서사 필드(`highlight`, `status_line`, `chain[].label`, `summary`)는 완성문으로
  얹어준다. **MVP는 서버 제공 서사를 그대로 렌더**한다 — 서사 생성 주체(열린 질문 Q1)가
  결정되기 전까지의 기본값이며, 결정이 바뀌면 이 문서의 "서사" 행들만 재지정하면 된다.
- **추정치는 반드시 `~` 라벨.** `est_saved_*`, `tokens_saved_est`는 계약상 측정값처럼
  표시하는 것이 금지돼 있다.

## 매핑 표

### `GET /spaces` → 로비 (공간·집계 번역)

| C2 필드 | 번역 | 화면 요소 |
|---|---|---|
| `floor` | 공간 | 사옥 몇 층에 그릴지 (층 히트존은 클라이언트 좌표) |
| `activity` (0~3) | 공간 | 창문 불빛 세기 (`activity-N` 클래스) |
| `name`, `motto`, `seed` | 공간 | 층 현판 · 스페이스 하단 모토 |
| `stats.{knowledge,reuse,resolved}` | 집계 | 층 hover 카드 수치 · 책장 배지 · 로비 "회사 현황" 합산 |
| `members_online` | 집계 | 엘리베이터 온라인 표시 |
| `highlight` | **서사 (서버 제공)** | 층 hover 카드의 오늘 하이라이트 1줄 |
| `token_budget`, `token_used` | 집계 | 매니저 코너 토큰 게이지 |
| `status` | 공간 | 매니저 코너 Room 상태 (정상/혼잡) |
| `viewer_tier` | 권한 | 멤버 뱃지·입장 모드 결정 (멤버십은 이 필드에서 유도) |

### `GET /spaces/{space_id}` → 스페이스 씬·피드·모달

| C2 필드 | 번역 | 화면 요소 |
|---|---|---|
| `agents[].status` | 공간 | 로봇 모션/자세 (`state-*` 클래스) · Agent Activity 온라인 수 |
| `agents[].status_line` | **서사 (서버 제공)** | 말풍선 (멤버). 게스트는 상태 종류만 |
| `agents[].role` | 공간 | 로봇 색/라벨 (어댑터가 클라이언트 role 키로 매핑) |
| `agents[].owner`, `name` | — | 명판 (멤버 전용, 게스트는 역할명만) |
| `issues[].timeline[]` | **서사 (서버 제공 label·note)** | 칠판 3단 카드 · 이슈 흐름 피드 · 이슈 모달 (L2→L4) |
| `issues[].status` | 집계 | 해결/진행 칩 |
| `knowledge[].{title,summary,body}` | 원문 (L4) | 지식 문서 모달 — 번역이 아니라 역추적의 종점 |
| `knowledge[].cited_by[]` | 집계+서사 | 모달 "재사용 이력" 목록 |
| `knowledge[].reuse_count` | 집계 | 로비 신착 목록의 인용 수 |
| `visits` | 집계 | 하단 TODAY/TOTAL 방문 수 |
| `viewer_tier` | 권한 | 유리벽 — **서버가 트리밍한 응답을 그대로 렌더** (아래 참조) |

### `GET /reuse-events` → 재사용 체인 피드 (간판 기능)

| C2 필드 | 번역 | 화면 요소 |
|---|---|---|
| `source_space` ≠ `consumer_space` | 공간 | 카드 방향 표시 (이 방의 지식 → / ← 타팀 지식) · P1 층간 화살표 |
| `chain[]` | **서사 (서버 제공)** | 발견→인용→해결 체인 스텝 |
| `doc_id` | — | 지식 문서 모달로 딥링크 (역추적) |
| `est_saved_tokens`, `est_saved_minutes` | 집계 (추정) | "약 ~18k 토큰 · ~55분 절약" — **`~` 필수** |

### `GET /activity` → 로비 게시판 · P2 다이제스트

| C2 필드 | 번역 | 화면 요소 |
|---|---|---|
| `events[].type` | 공간 | 칩 종류 (재사용/신착/이슈/압축/Skill) |
| `events[].summary` | **서사 (서버 제공)** | 로비 1층 게시판 "오늘의 조직 하이라이트" (최신 2건) |
| 나머지 (`space_id`, `doc_id`…) | — | P2 파도타기·다이제스트의 딥링크 재료 |

### `GET /stats` → 대시보드 (로비 상단 버튼 → 모달)

| C2 필드 | 번역 | 화면 요소 |
|---|---|---|
| `totals` | 집계 | 이슈/지식/재사용/Skill 4칸 요약 |
| `by_space[].{contributed,reused}` | 집계 | 스페이스별 기여↔소비 가로 막대 — 누가 주고 누가 받는 팀인지 |
| `top_reused_skills`, `top_knowledge` | 집계 | 랭킹 목록 (팀 단위 — 개인 랭킹은 비목표) |
| `tokens_saved_est` | 집계 (추정) | "약 ~412k 토큰" — **`~` 필수** |

전부 집계 번역 — LLM 불필요. 팀 리더 페르소나("자산이 쌓이나?")의 주 표면.

### `GET /graph` → 소비자 없음 (폐기 동의)

인용 그래프 드랍(2026-07-14, [02-features](02-features.md))으로 유일한 소비자가 사라졌다.
계약에서의 제거는 계약 소유자(msalt) 몫.

## 게스트(유리벽) 트리밍 — 서버 몫

계약의 원칙: 같은 endpoint가 tier별로 **다른 응답**을 내려주고(`space-detail-guest.json`
참조), 프론트는 받은 것을 그대로 그린다 — 프론트가 숨기면 C1(에이전트 경로)과 어긋나
한쪽이 샌다.

- **프로토타입의 현재 상태**: 게스트 연출(말풍선 일반화, 잠금 카드)을 클라이언트에서
  수행 중. 라이브 연동 시 시점 토글은 "tier가 다른 응답 재요청"으로 교체한다.
- **해소된 충돌 (2026-07-14 결정)**: space-view 설계가 게스트에게 "서사는 제목만"이던
  것과 달리 C2는 `issues: []`(제목조차 안 내려감)였다. 이슈 제목도 작업 내용을 담는
  서사이므로 **계약을 따르기로 결정** — 게스트에게 이슈는 비노출. 설계 문서(01, 02)와
  프로토타입(잠금 카드 → 멤버 전용 안내)을 이에 맞춰 수정했고, 계약 변경은 불필요.

## C2에 없는 것 (갭 목록)

| # | 뷰모델 필드 | 현재 출처 | 판정 |
|---|---|---|---|
| G1 | `currentUser`, 멤버십 | `client-data.js` | 인증 세션의 몫 (C2 밖이 맞음). 스페이스 멤버십은 `viewer_tier`로 유도 |
| G2 | 매니저 코너 이벤트(`managerEvents`), RBAC 상태 | `client-data.js` | C2에 없음. #9에서 "매니저 에이전트 → 관리 API(C4)"로 재편됨 — **코너를 뭘로 채울지 재설계 필요** (C4 이벤트 노출? 코너 축소?) |
| G3 | 로비 "공개 지식 신착" 목록 | 스페이스 상세를 합쳐서 유도 | org 전체 지식 목록 endpoint 없음 (`stats.top_knowledge`는 제목·카운트뿐). 스페이스가 늘면 N회 호출 — **C2에 `GET /knowledge?visibility=org` 추가 후보** |
| G4 | 지식 문서 작성 시점 | `c2-data.js`의 `created_at` (선제 사용) | C2 `knowledge`에 시각 필드가 없다 — **`created_at` 추가 요청 후보** (계약은 추가 허용) |
| G5 | 책상 좌표(`deskSlot`), 층 히트존, 씬 스케일 | 클라이언트 상수 | 레이아웃은 클라이언트 소유 — 계약에 올리지 않는 게 맞음 |
| G6 | — | — | 픽스처 내부 불일치: `stats.json` totals(지식 128)와 `spaces.json` stats 합계(지식 75)가 안 맞음. 골든 데이터 정리 시 msalt와 함께 보정 |

## 데이터 흐름 (프로토타입)

```
contracts/fixtures/*.json  ←(모양·시나리오 정합)→  c2-data.js   "가짜 C2 서버 응답"
                                                     │
                                              c2-adapter.js     wire → 뷰모델 번역 (이 문서가 스펙)
                                                     │
client-data.js (C2 밖: 인증·레이아웃·G2) ──────────→ DB (뷰모델)
                                                     │
live-data.js + live-adapter.js (GitHub PR 라이브) ──┤
                                                     ▼
                                                  app.js 렌더
```
