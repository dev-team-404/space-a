# 방문 인프라 — 인바운드 방문 추적·말풍선 (P4) + 알림·탭 뱃지 (N1) 설계

- **날짜**: 2026-07-29
- **컴포넌트**: a-mate + a-hub life 서버 (P4 방문 추적 API 신설)
- **관계**:
  - 로드맵 [2026-07-26-life-social-diary-followups-roadmap.md](../plans/2026-07-26-life-social-diary-followups-roadmap.md)의 **P4** + **6차 배치 N1** + 묶음 실행 계획 **④**.
  - 방명록 폴링·실패 무해 규율은 P3/G3 선례([아카이브: 자동 방명록](../../../archive/design/a-mate/specs/2026-07-27-auto-guestbook-design.md) — `maybe_reply_guestbook`)를 따른다.
  - life 서버 API 계약의 정본은 [docs/design/life-visit.md §4](../../life-visit.md) — 이번 방문 추적 엔드포인트를 같은 표에 추가한다. **contracts/ 미접촉**(C2는 work hub→a-lens 계약 — 조사로 확인, P3 스펙과 동일 결론).

## 1. 요구사항 (브레인스토밍 확정)

- **P4**: 누군가 내 방에 다녀가면 마스코트 말풍선으로 "○○님 다녀갔어요" + 홈 최근 알림에도 기록.
- **N1**: 홈 최근 알림에 새 방명록 글 기록 + 다이어리·방명록 탭에 신규 수 뱃지(탭 방문 시 클리어).

| 결정 항목 | 확정안 |
|---|---|
| 방문 기록 방식 | **서버 자동 기록** — `enter()` 성공 시 방문자 ≠ 방 주인이면 서버가 visit row 기록. 클라 변경 0 — 구클라·수동 방문·자율 방문(묶음 ②) 전부 자동 포착 |
| 중복 억제 | **서버 세션화** — 같은 (방, 방문자)의 최신 행 `last_at`이 30분 이내면 새 행 대신 `last_at` 갱신 |
| 조회 API | `GET /life/me/visits?since&limit` — Bearer 본인 방 전용(권한 자연 보장) |
| 감지 경로 | **스캔 편승 + 이벤트 emit** — pipeline 신규 함수가 diff 후 `life:visit` / `guestbook:new` emit. 네트워크는 백엔드 한 곳, 두 창(webview)이 이벤트 공유 |
| 표면 분배 | 방문 소식은 **말풍선(Mascot) + 최근 알림(App)** 둘 다. 방명록 소식은 최근 알림 + 탭 뱃지 |
| 방문 중 표시 | `present`(지금 내 방에 있음)면 **문구만 현재형**("놀러왔어요!") — 별도 인디케이터 없음 |
| 폴링 커서 | **settings store** — `inbound_visits_cursor`(=emit한 `first_at` 최댓값), `inbound_guestbook_cursor`(=`created_at` 최댓값). diff가 Rust에서 일어나므로 백엔드 저장 |
| 읽음(뱃지) 상태 | **프론트 localStorage** (notices.ts 선례) — 뱃지는 순수 프론트 관심사 |
| 뱃지 정의 | 마지막 탭 확인 이후 신규 수. 다이어리=신규 일기 수, 방명록=**타인 글 전부**(원글+답글, 내 에이전트 작성분 제외) |
| 첫 실행 | 커서 없으면 emit 없이 커서만 현재 최댓값으로 초기화 — 설치·연결 직후 과거분 도배 방지 |
| 말풍선 문구 | 정적 템플릿(LLM 불필요 — 즉시성·실패 무해. 방문자 이름이 주어라 호칭 불필요). `occasionBubble` 선례 |

### 스코프 제외

- **대문사진·오늘의 한마디 방문객 공개**(아웃바운드 게시 — 서버 필드·업로드 훅·공개 범위) → 로드맵 **7차 배치 O1**로 기록, ④ 머지 후 별도 세션.
- 방문 사실의 일기 반영(P4 선택 항목) — 묶음 ②(P1 일기 클러스터) 영역.
- SSE/WebSocket 실시간 push(life-visit.md 미결 항목 유지), 홈 탭 "방문 중" 인디케이터, 방명록 알림 클릭 시 해당 글 스크롤.

## 2. 아키텍처

### 컴포넌트 배치

| 위치 | 변경 | 내용 |
|---|---|---|
| **a-hub** `life.py` | 신규 로직 | `enter()`에 방문 자동 기록(세션화·prune, 락 안) + `visits()` 조회(present 계산) |
| **a-hub** `store.py` | 신규 테이블 | `visits` write-through (신규 테이블이라 `_SCHEMA` 추가만, ALTER 마이그레이션 불필요) |
| **a-hub** `api.py` | 신규 라우트 | `GET /life/me/visits` |
| `docs/design/life-visit.md` | §4 갱신 | 엔드포인트 행 + enter 자동 기록·세션화 주석 |
| **core** `crates/core/src/inbound.rs` | 신규 모듈 | 순수 diff 로직 `select_new_visits` / `select_new_guestbook` — 결정적 단위 테스트 |
| **core** `life_client.rs` | 소폭 | `visits(since, limit)` 메서드 추가 |
| `src-tauri/pipeline.rs` | 신규 함수 | `maybe_poll_inbound(app, store)` — `run_pipeline_once`에서 호출. **기존 함수 무수정**(묶음 ②와의 충돌을 등록부 수준으로 억제. 방명록 GET이 스캔당 1회 늘지만 무해) |
| **프론트** `api.ts` | 소폭 | `onLifeVisit` / `onGuestbookNew` listen 래퍼 + 타입 |
| `lib/robot/bubble.ts` | 소폭 | `visitBubble()` — kind `visit` 추가 |
| `lib/notices.ts` | 소폭 | `visitNotice()` / `guestbookNotice()` + kind·dest 확장 |
| `App.svelte` | 소폭 | 두 이벤트 구독 → `record()` + 다이어리·방명록 탭 뱃지(coach `.badge` 스타일 재사용) |
| `Mascot.svelte` | 소폭 | `onLifeVisit` → `showBubble(visitBubble(...))` |

### 데이터 흐름

```
[a-hub] 방문자 enter(life_id) ─ 방문자≠주인 ─▶ visits 기록(30분 세션화, life당 100행 prune)

[a-mate] 스캔(파일 변경→60s 디바운스) → run_pipeline_once
  └ maybe_poll_inbound(app, store)
      ① 설정 스냅샷(락→즉시 해제): hub_url/token/life_id/agent_id + 커서 2개
      ② GET /life/me/visits?since=<visits커서> → first_at > 커서 필터
          → 있으면 emit life:visit [{visitor_name, first_at, last_at, present}] → 커서=max(first_at) 저장
      ③ GET /life/{life_id}/guestbook → created_at > 커서 && author_agent_id ≠ 나
          → 있으면 emit guestbook:new [entries] → 커서=max(created_at) 저장
      (커서 저장은 emit 성공 후 — emit 실패 시 미갱신 → 다음 스캔 재시도)

[프론트] Mascot: life:visit → visitBubble(현재형/과거형)
         App:   life:visit → record(visitNotice)
                guestbook:new → record(guestbookNotice) + 방명록 unseen ∪= entry_id
                diary:ready → (기존 record) + 다이어리 unseen ∪= date
                탭 확인 → 해당 unseen 클리어(+방명록 lastSeen=now)
```

세션화 갱신(기존 행 `last_at`만 변경)은 `since=last_at` 필터에 다시 걸리더라도 `first_at ≤ 커서`라 emit에서 제외 — **재알림 도배가 커서 기준에서 구조적으로 차단**된다.

## 3. a-hub — 방문 추적 상세

**스키마** (`_SCHEMA` 추가 — 인메모리 리스트 + write-through, guestbook 선례):

```
visits: visit_id PK | life_id | visitor_agent_id | visitor_name(입장 시점 스냅샷) | first_at | last_at
```

- **기록** (`enter()` 성공 후, 락 안): `life.owner_agent_id != agent.agent_id`일 때만.
  같은 (life_id, visitor)의 최신 행 `last_at` ≥ now−30분 → 그 행의 `last_at`·`visitor_name` 갱신, 아니면 새 행.
  시각은 `datetime.now(timezone.utc).isoformat()` (guestbook `created_at` 동일 형식).
- **보존**: life당 100행 초과 시 `last_at` 오래된 것부터 삭제 (insert 시 prune).
- **조회** `GET /life/me/visits?since=<RFC3339>&limit=<n>`:
  - Bearer 토큰의 본인 life 대상 — 타인 방 방문 기록은 구조적으로 조회 불가.
  - `since`: `last_at > since` 필터(선택). `limit`: 기본 50, 최대 100. 정렬 `last_at` 내림차순.
  - 응답: `{"visits": [{visit_id, visitor_agent_id, visitor_name, first_at, last_at, present}]}` —
    `present` = 그 방문자의 현재 `at_life`가 내 방인지 서버 계산.
- **하위호환**: 신규 GET 추가뿐이라 기존 클라 무영향. 자동 기록은 서버 배포만으로 활성.

## 4. a-mate 백엔드 — 폴링·이벤트 상세

`maybe_poll_inbound(app: &AppHandle, store_mutex)` — `maybe_reply_guestbook` 옆에서 호출. hub 설정(url/token/life_id/agent_id) 미비 시 no-op, 모든 실패는 warn 후 다음 스캔 재시도.

**core 순수 함수** (`inbound.rs`):

```rust
/// rows: 서버 visits 응답. cursor: 이미 emit한 first_at 최댓값 (None=첫 실행).
/// 반환: (emit할 행들, 갱신할 커서). 첫 실행은 (빈 목록, Some(max first_at)).
pub fn select_new_visits(rows: &[Value], cursor: Option<&str>) -> (Vec<Value>, Option<String>)

/// entries: 방명록 GET 응답. 타인 글(author_agent_id ≠ my_agent_id)만 대상.
pub fn select_new_guestbook(entries: &[Value], my_agent_id: &str, cursor: Option<&str>)
    -> (Vec<Value>, Option<String>)
```

- 커서 비교는 RFC3339(UTC, 동일 서식) 문자열 사전순 — 서버가 형식을 통제하므로 안전. 파싱 불가·필드 누락 행은 방어적으로 무시.
- visits GET에는 `since=커서`를 실어 전송량을 줄이고, 정확한 emit 판정은 클라의 `first_at > 커서`가 담당.
- **구서버 가드**: visits GET 404(엔드포인트 미배포) → warn 1회 후 앱 실행 동안 visits 폴링 비활성(정적 `AtomicBool` — G3 `INCOMPATIBLE` 선례). 방명록 폴링은 계속.

## 5. 프론트 — 말풍선·알림·뱃지 상세

**말풍선** (`bubble.ts` + `Mascot.svelte`): `BubbleKind`에 `visit` 추가.

```
visitBubble(visits) → kind 'visit', tab 'home', target 없음
  1명 & present  → "○○님이 놀러왔어요!"
  1명 & !present → "○○님 다녀갔어요"
  복수           → "○○님 외 N명 다녀갔어요" (○○=가장 최근 방문자, 하나라도 present면 현재형)
```

**알림** (`notices.ts` + `App.svelte`): `Notice.kind`에 `visit` | `guestbook` 추가.

| 헬퍼 | text | target·dest |
|---|---|---|
| `visitNotice(visits, ts)` | "○○님이 방에 다녀갔어요" (복수 "외 N명") | 없음 — 클릭 불가(occasion 선례) |
| `guestbookNotice(entries, ts)` | "방명록에 새 글 N건 — ○○님" | 최신 entry_id, dest `{tab:'guestbook'}` (탭 이동만 — 스크롤은 스코프 외) |

`NoticeDest.tab`에 `'guestbook'` 추가. 구버전 저장분(모르는 kind 없음)·기존 kind는 무영향.

**탭 뱃지** (`App.svelte`, 탭 id는 `diary`·`guestbook` — 기존 nav): coach `activeCount`의 `.badge` 스타일 재사용, 값만 unseen 카운트.

- **다이어리**: unseen = **일기 날짜 set** — `diary:ready`(date)마다 ∪, localStorage 영속(일기는 이 앱 실행 중에만 생성되므로 이벤트 누적으로 완결). 다이어리 탭 확인 시 클리어.
- **방명록**: unseen = **entry_id set**(세션 내 메모리) — ① 앱 mount 시 부트스트랩: 내 방명록 조회 → `created_at > lastSeen && author ≠ 나`인 entry_id ② `guestbook:new` payload의 entry_id ∪. **entry_id dedup이라 부트스트랩·이벤트 중복 카운트가 구조적으로 없음**(백엔드 커서는 재시작을 견디므로 앱 꺼진 동안의 글이 양쪽에서 올 수 있다). 영속은 `lastSeen`(ISO)만 — 방명록 탭을 **내 방 문맥**(`currentLifeId === myLifeId`)으로 확인할 때 lastSeen=now + set 클리어. lastSeen 최초값 = 첫 mount 시각(과거 전체가 뱃지로 쏟아지는 것 방지).
- localStorage 키: `agent-mentor.tab-unseen` `{diaryDates: string[], guestbookLastSeen: string}`. 카운트·클리어 로직은 순수 함수로 분리해 Vitest.

## 6. 에러 처리·엣지 요약

| 상황 | 처리 |
|---|---|
| hub 미연결·설정 미비 | 조용히 no-op (선례) |
| visits/guestbook GET 실패 | warn + 다음 스캔 재시도 |
| emit 실패 | 커서 미갱신 → 다음 스캔 재-emit (프론트가 못 받은 상태라 재시도가 옳음) |
| 구서버(visits 404) | warn 1회 + 실행 동안 visits 폴링 비활성. 방명록은 계속 |
| 첫 실행(커서 없음) | emit 없이 커서 초기화 — 과거분 도배 방지 |
| 시계 왜곡 | 커서·필터 전부 서버 발급 timestamp 기준 — 클라 시계 무관 |
| localStorage 초기화 | lastSeen 재초기화(=mount 시각) — 뱃지 리셋뿐, 무해 |
| 방명록 글 삭제 | 부트스트랩이 서버 재조회라 자연 반영. 세션 내 set에 남은 삭제분은 탭 확인 시 클리어 |
| 방문자 개명 | 세션화 갱신 시 `visitor_name` 재스냅샷. 과거 행은 당시 이름 유지(이력으로 정확) |

## 7. 테스트 전략

| 대상 | 테스트 |
|---|---|
| a-hub (pytest) | 방문 자동 기록 / 자기 방 제외 / 세션화(29분=갱신·31분=새 행) / 이름 재스냅샷 / prune(100) / since·limit / present / me 스코프(내 방 것만) / SQLite 재시작 영속 |
| core (cargo) | `select_new_visits`: 첫 실행 초기화 / 신규 emit / 세션화 갱신행 제외(first_at ≤ 커서) / 불량 행 방어. `select_new_guestbook`: 타인만 / 내 글 제외 / 커서 경계 / parent 유무 무관 전부 카운트 |
| 프론트 (Vitest) | `visitBubble` 문구(현재형·과거형·복수) / `visitNotice`·`guestbookNotice`·`noticeDest` 확장 / unseen 순수 함수(부트스트랩+이벤트 dedup, 클리어, lastSeen 초기값) |
| 빌드 | `npm run build` + `cargo test` (네이티브 Windows PowerShell) |
| 실환경 | **PR 체크리스트로 사용자 몫** — 서버 배포 후 2클라: 방문→말풍선·최근 알림, 방명록 작성→뱃지 증가·탭 확인 시 클리어 |

## 8. 커밋·PR 계획

브랜치 `feat/visit-infra-notices` (origin/main 최신 기반, 격리 워크트리). 항목당 커밋 분리:

1. `feat(backend): add inbound visit tracking to life server` — §3 + life-visit.md §4
2. `feat(agent): poll inbound visits and surface mascot bubble` — §4 + 말풍선(P4 클라)
3. `feat(agent): record guestbook notices and tab badges` — §5 알림·뱃지(N1)
4. docs — 본 스펙(+로드맵 6차 cherry-pick 완료, 7차 배치 O1 기록)

DoD: PR 머지 시 docs-archive 스킬로 본 스펙·플랜 아카이브(ADR 0013). 로드맵 완료 기록은 머지 후 main 최신에서. 신규 ADR 불필요(세션화·보존은 스펙 수준 결정).
