# Agent Mentor — 3단계 미니홈피 대개편 설계

> **상태**: 브레인스토밍 합의 완료 (2026-07-05)
> **범위**: chat 창의 미니홈피 레이아웃 전환 + 파스텔 모던 스킨, 홈/코칭/다이어리/채팅 탭 완성,
> 마이룸, 마스코트 클릭·말풍선 정책 개편, triage 백로그. **updater는 제외**(서명 인프라 미확정).
> **선행**: 비전 스펙 `2026-07-03-frontend-vision-design.md`, 킥오프 시드 `docs/brainstroming/2026-07-05-minihompy-restyle-kickoff.md`.
> **제약**: Tauri v2 전용, Windows 전용, 외부 전송은 Engine 옵트인뿐(기존 경계 유지).

---

## 0. 합의 요약

| 항목 | 결정 |
|---|---|
| 미감 방향 | **레트로(픽셀 폰트·도트 프레임) 보류** → 파스텔 모던(둥근 모서리·부드러운 그림자). 미니홈피 **레이아웃 문법은 유지** |
| 레이아웃 | 이중 프레임, 타이틀바("OO님의 미니홈피" + TODAY/TOTAL), 좌측 컬럼(로봇 초상+기분), **우측 세로 책갈피 탭** |
| 홈 탭 | 위젯 4종(주간 추이·모델 분포·절약 top3·최근 알림) + 컴팩트 오늘 스트립 + **하단 마이룸**(128px 로봇) |
| 코칭 탭 | "무엇이→왜→어떻게" 카드. `finding_advice()` 노출 + **명령 복사**(R1/R2/R7) + **해결함/무시 상태 관리**. 파일 열기·자동 실행 제외 |
| 다이어리 탭 | 미니 달력(도트) + 날짜별 본문(경량 마크다운 렌더) |
| 채팅 탭 | Engine 재사용 + **코칭 컨텍스트 주입**. 트랜스크립트 원문 미전송 |
| 마스코트 | 클릭→chat 창 열기(4px 임계값 판별), 말풍선 **의미 있는 것만·지속 표시·X 닫기·파스텔 스타일** |
| triage | 마스코트 버그픽스, 진행률 바, 로그 관측성, UTC→로컬 "오늘", content_protected, allowScripts 전부 포함 |
| 실행 분할 | 스펙 1 + **PR 2개**: ①미니홈피 본체 ②채팅 탭+triage 잔여 |

## 1. 창 레이아웃 & 스킨

```
┌─────────────────────────────────────────────────────┐ ← 네이티브 창(장식·크기 기존 유지)
│  (파스텔 그라데이션 배경 — 프레임 바깥 여백)          │
│  ┌───────────────────────────────────────────┐      │
│  │  지빈님의 미니홈피        TODAY 12 · TOTAL 345│      │ ← 타이틀바
│  ├────────────┬──────────────────────────────┤──┐   │
│  │ ┌────────┐ │                              │홈│   │
│  │ │로봇 초상│ │                              ├──┤   │
│  │ └────────┘ │        (탭 콘텐츠)            │다 │   │ ← 우측 세로 책갈피 탭
│  │            │                              ├──┤   │
│  │ 오늘의 기분  │                              │코 │   │
│  │ "한마디"    │                              ├──┤   │
│  │            │                              │채 │   │
│  └────────────┴──────────────────────────────┘──┘   │
└─────────────────────────────────────────────────────┘
```

- **이중 프레임**: 네이티브 장식은 그대로. 안쪽에 파스텔 배경 여백 + 둥근 흰 콘텐츠 프레임(CSS만).
- **타이틀바**: `"{user_name}님의 미니홈피"` + TODAY(오늘 세션)/TOTAL(누적 세션). `user_name`은 `get_summary()`에 필드 추가 — 마스코트 시드와 동일한 유저명 소스 재사용.
- **우측 세로 탭**: 홈/다이어리/코칭/채팅. 활성 탭은 프레임과 같은 색으로 이어짐. 코칭 탭에 활성 finding 수 뱃지.
- **좌측 컬럼**: 로봇 초상(기존 `render.ts` 정적 1프레임, 소형 canvas) + "오늘의 기분" 한 줄(finding 상황 반영). BGM 위젯 없음.
- **스킨 토큰**: 전역 CSS 변수 — 파스텔 팔레트(연보라·민트·크림·코랄), radius 8–14px, 저채도 그림자. 도트 보더(3px+하드 섀도)·픽셀 폰트 전면 제거, 시스템 폰트(`Segoe UI`/`맑은 고딕`). 모든 탭과 마스코트 말풍선이 같은 토큰 사용.

## 2. 홈 탭 + 마이룸

- **오늘 스트립**(1행): 세션·입력·출력·절약가능 — 기존 카드 5장을 축소 통합.
- **위젯 2×2**:
  - **주간 추이**: 최근 7일 토큰/세션 미니 바 차트(CSS/SVG, 차트 라이브러리 없음). `get_week_summary()`.
  - **모델 분포**: 오늘 tier별(opus/sonnet/haiku/…) 토큰 비율 가로 스택 바. `get_model_mix()`.
  - **절약 실천 top3**: 활성 findings 중 절약 토큰 상위 3건을 행동 문장(suggested_action)으로. 클릭 → 코칭 탭 해당 카드로 스크롤.
  - **최근 알림**: chat 창이 수신하는 `coach:finding`/`diary:ready`/`occasion:today` 이벤트를 localStorage에 최근 20건 기록·표시(창 숨김 상태에서도 수신됨. 백엔드 스키마 불변).
- **마이룸**(하단): 파스텔 방 배경(CSS/SVG 벽지·바닥·소품 2~3개, 이미지 자산 없음) + 128px 로봇(기존 `render.ts`+`anim.ts` idle 재사용). **클릭 시 happy 모션+랜덤 대사**. 말풍선(방 안 코믹 한마디)엔 기분/최근 advice/잡담 순환.
- **상태줄**: 마지막 스캔 + "지금 스캔" 버튼. 진행률 바 자리 확보(구현은 PR②).

## 3. 코칭 탭

### 백엔드
- `list_findings` 확장(신규 커맨드 없음): core가 `finding_advice()`로 `detail`·`suggested_action`을 동봉하고, `fix_command`(Option) 필드 신설.
  - `fix_command` 매핑(core 결정론 함수, 단위 테스트): `remove_mcp` → `claude mcp remove {server}` / `disable_plugin` → `claude plugin disable {plugin}` / `switch_model` → `/model haiku` / R5·R9 → `None`.
- **상태 관리**: 신규 커맨드 `set_finding_status(dedup_key, "new"|"resolved"|"dismissed")`. `list_findings(include_hidden: bool)` — 기본은 `new`만.
  - findings upsert(`store.rs`)는 ON CONFLICT 시 status를 건드리지 않으므로 숨김은 스캔 후에도 유지(확인됨).
  - **자동 부활 없음**: 규칙이 과거 데이터를 매 스캔 재평가해 동일 finding을 재방출하므로 자동 부활은 '해결함'을 즉시 무효화함. 복원은 "숨긴 항목 N개" 토글에서 수동.

### 카드 구조 (evidence JSON 노출 폐지)
```
⚠ 안 쓰는 MCP 서버가 토큰을 먹고 있어요        ~4.2k   ← 무엇이 (사람말 제목)
playwright가 상주하는데 호출 0회 · 12회 관측          ← 왜 (detail + 관측 횟수·범위)
➜ 설정에서 제거하면 매 세션 상주 토큰을 아껴요          ← 어떻게 (suggested_action)
[📋 claude mcp remove playwright] [해결함] [무시]     ← 액션
▸ 원본 데이터 보기                                   ← evidence (접힘, 격하)
```
- 절약 토큰 큰 순 정렬, severity 아이콘 유지. 복사는 `navigator.clipboard` + "복사됨" 토스트.
- 파일 열기는 제외 — MCP/플러그인 정의 위치(프로젝트 `.mcp.json`/전역/마켓플레이스)를 수집 데이터로 특정할 수 없어 오히려 혼란. 명령 복사가 위치 무관으로 정확.

## 4. 다이어리 탭

- 미니 달력(월 그리드, ←/→ 월 이동): `list_diary_dates()`(기존)로 도트 표시. 날짜 클릭 → `get_diary(date)`(기존) 본문.
- 본문은 경량 마크다운 렌더(`marked`) — 일기에 제목/강조가 섞이므로 raw 노출 금지.
- occasions 뱃지는 생략(일기 본문이 이미 서술) — YAGNI.

## 5. 채팅 탭

- 신규 커맨드 `chat_send(messages: [{role, content}]) -> String` — 다이어리와 같은 `Engine` 추상화 재사용. 엔진 미설정 시 채팅 UI에 설정 안내 문구(에러 아님).
- **시스템 프롬프트 주입**: ①마스코트 페르소나(다이어리와 동일 인물, 1인칭 다마고치풍) ②오늘 요약 수치 ③활성 findings의 detail/suggested_action 요약. → 코칭 문답 가능.
- **전송 경계**: 트랜스크립트 원문 미전송, findings 요약·집계 수치만. 기존 Engine 옵트인 경계와 동일.
- 대화 이력: 창 수명 동안 프론트 메모리(영속화는 후속). 스트리밍 없음(동기 호출) — 로딩 인디케이터.

## 6. 마스코트: 클릭 열기 + 말풍선 정책

- **클릭 vs 드래그**: canvas의 `data-tauri-drag-region` 제거 → JS 수동 판별. pointerdown 좌표 기록, 이동 > **4px** 시 `getCurrentWindow().startDragging()`, 미만에서 pointerup이면 클릭 → `open_chat_tab('home')`(기존 커맨드 재사용). 판별 로직은 순수 함수 분리(vitest).
- **트리거 정리**: "수집 완료/세션 갱신" 류 대사 **제거**. 말 거는 경우: ①새/악화 finding advice ②다이어리 완성 ③occasion ④가끔 잡담(기존 보수적 빈도). `realtime_advice` 옵트인도 "스캔 요약"이 아닌 **새 advice 있을 때만**으로 변경.
- **지속 표시**: 자동 소멸(5–8초) 제거 → 본문 클릭(관련 탭 열기)·**X 버튼**·다음 말풍선 도착까지 유지. X로 닫으면 창 축소 복귀.
- **스타일**: 픽셀 보더 → §1 파스텔 토큰(둥근 모서리+그림자).
- **버그픽스**: ①occasion 시작 레이스 — 신규 pull 커맨드 `get_today_occasions()` + 마운트 시 호출 ②위치 복원 시 모니터 경계 클램핑.

## 7. 나머지 triage (PR②)

- **스캔 진행률 바**: 백엔드 파일 단위 `scan:progress` emit → 홈 상태줄 바.
- **로그 관측성**: 공식 `tauri-plugin-log`(v2) 파일 로그 — `windows_subsystem` eprintln 소실 해소.
- **"오늘" 정책**: UTC → **로컬 타임존 자정 기준** 통일. rollup은 매 스캔 전체 재산정이라 소급 적용.
- **content_protected**: 설정값을 창에 실제 적용. **allowScripts**: package.json 비표준 필드 정리.

## 8. 커맨드 표면 변경 요약

| 커맨드 | 변경 | 용도 |
|---|---|---|
| `get_summary()` | `user_name` 필드 추가 | 타이틀바 |
| `list_findings(include_hidden)` | advice(`detail`·`suggested_action`)·`fix_command`·`status` 동봉, 기본 `new`만 | 코칭·절약 top3 |
| `set_finding_status(dedup_key, status)` | 신규 | 해결함/무시 |
| `get_week_summary()` | 신규 — `daily_rollup` 7일 합산 | 주간 추이 |
| `get_model_mix()` | 신규 — `events` 오늘자 tier별 집계 | 모델 분포 |
| `get_today_occasions()` | 신규 — occasion pull | 레이스 픽스 |
| `chat_send(messages)` | 신규 — Engine 재사용 (PR②) | 채팅 탭 |

## 9. 에러 처리 · 테스트

- **에러 철학 유지**: 말풍선으로 에러를 알리지 않음. 상태줄·토스트만. `chat_send` 실패는 채팅 UI 인라인.
- **테스트**:
  - core: `week_summary`·`model_mix` 쿼리, `fix_command` 매핑, `set_finding_status`+upsert status 보존 단위 테스트.
  - 프론트 vitest: 달력 생성 로직, 말풍선 정책(트리거 필터·지속·교체), 클릭/드래그 판별 순수 함수.
  - E2E: 수동 체크리스트(관례).

## 10. 실행 분할

| PR | 내용 |
|---|---|
| **① 미니홈피 본체** | §1 레이아웃·스킨, §2 홈·마이룸, §3 코칭, §4 다이어리, §6 마스코트 + 백엔드 커맨드(§8 중 chat_send 제외) |
| **② 채팅+마감** | §5 채팅 탭, §7 triage 잔여(진행률 바·로그·오늘 정책·content_protected·allowScripts) |

각 PR: 플랜(`docs/plans/`) → SDD(태스크별 서브에이전트+리뷰) → 봇 리뷰 대응 → 사용자 E2E.
