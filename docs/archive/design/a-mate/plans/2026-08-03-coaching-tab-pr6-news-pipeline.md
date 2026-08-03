---
status: done
archived: 2026-08-03
---

# 코칭 탭 통일 PR⑥ — 소식 파이프라인 구현 계획

**Goal:** Claude Code가 `~/.claude.json`에 캐시해 둔 시작 공지를 코칭 탭 「배움 · 소식」에 모아두고,
영어 소식을 아이템당 1회 번역·캐시하며, 유효 기한 공지를 최상단 고정 슬롯에 띄운다.
덤으로 홈 알림 박스의 타임스탬프가 어제/오늘을 구분하게 한다(단일 스트림 스펙 §5의 C).

**참조 스펙**
- `docs/archive/design/a-mate/specs/2026-08-02-coaching-tab-unification-design.md` — §6 전체(6.1~6.6), §9(에러 처리), §10(테스트), §11(PR⑥)
- `docs/design/a-mate/specs/2026-08-02-coaching-tab-single-stream-design.md` — §5의 **C**(홈 알림 타임스탬프)

**Architecture:** 이 저장소엔 컴포넌트 테스트 라이브러리가 없다(`package.json`에 vitest + svelte-check뿐).
따라서 **판정 로직은 전부 `.ts` 순수 함수**로 빼고 Svelte는 그리기만 한다(PR②가 세운 규약).
Rust 쪽도 같은 규율: 파싱·프롬프트 조립·판정은 순수 함수, 파일 읽기·LLM 호출은 가장자리(파이프라인).

---

## Global Constraints

- **작업 위치**: 워크트리 `.claude/worktrees/coaching-news-pipeline` (base `7275f5c` = origin/main).
- **플랫폼**: Windows 전용. 빌드·테스트는 네이티브 PowerShell. WSL 안에서 금지.
- **베이스라인(워크트리 실측, `7275f5c`)**: `cargo test` **663건**(core 615 + app 48), `npm test` **260건** / svelte-check 0 errors.
  ⚠ `node_modules` 없이 `npm test`를 돌리면 종료 코드 0으로 거짓 통과한다. 출력에 `Tests  N passed`가 있는지 눈으로 본다.
- **마이그레이션에 `DELETE FROM events/sessions/ingest_state/daily_rollup` 금지.** 릴리스마다 콜드 스캔이
  되돌아온 사고(#146)의 원인. `judgment_json` 패턴(`store.rs` migrate, `pragma_table_info` 확인 후 ALTER만)을 따른다.
- **락 규율**: `claude.json` 읽기·LLM 호출은 **락 밖**, persist만 짧은 락 (`maybe_curate_content` 기존 3단 규율).
- **CSS 하드코딩 hex 금지**, 전경 `color`에 `var(--accent)` 금지(`--accent-strong` 사용). `~0 tok`을 다시 렌더하지 않는다.
- **커밋**: Conventional Commits, 영어.

### ③(#153)과의 충돌 지점

③(처분·수명 모델)이 병렬로 돌고 **③이 먼저 머지되는 것을 목표로 한다 — 리베이스는 이쪽 몫이다.**

| 위치 | ③ | ⑥ | 대응 |
|------|----|----|------|
| `store.rs` `migrate()` | findings에 `status_evidence_n`·`status_ts` | content_items에 `summary_ko`·`title_ko`·`deadline` | 각자 `if !has_x { ALTER }` 블록 — 둘 다 남기면 해소 |
| `store.rs` `replace_content_items()` | 프룬에 resolved+미방출 삭제 | **손대지 않는다**(아래 설계 결정 D3) | 충돌 없음 |
| `CoachTab.svelte` | 처분 접힌 줄·실행취소 | 기한 공지 고정 슬롯(섹션 **위**) | 위치가 달라 수동 병합 가능 |
| `notices.ts`·`NoticeLog.svelte` | 안 건드림 | §6.5 새 공지 알림 + C 타임스탬프 | 충돌 없음 |

---

## 설계 결정 (스펙이 정하지 않아 이 계획에서 정한 것)

| # | 결정 | 근거 |
|---|------|------|
| **D1** | 🤖 `coachTip` 라이브 호출을 **제거**한다(캐시로 대체가 아니라 폐지) | §6.3 표가 소스별 LLM 처리를 확정했고 **내장 팁·팀 지식은 "그대로"** 다 — 한국어 소스에 LLM을 쓰지 않는다는 뜻. §4.2 문법 B 슬롯 표에도 🤖 줄이 없다. D7(매 렌더 호출)은 호출 자체를 없애 해소한다. 죽은 `coach_tip` 커맨드·`coach_prompt`도 같이 정리(내 변경이 만든 고아) |
| **D2** | `content_items`에 컬럼 **3개**(`summary_ko`·`title_ko`·`deadline`) | §6.4의 LLM 출력이 3필드다. 하나의 `if !has_summary_ko` 블록에서 ALTER 3회 — `judgment_json` 패턴 |
| **D3** | `[확인]` 처분은 **localStorage** | 고정 슬롯은 표시 층 상태다. 백엔드 컬럼·커맨드를 늘리지 않고, ③이 건드리는 `replace_content_items`를 피한다. 기한 경과 자동 강등이 안전망이라 영속성 손실이 무해 |
| **D4** | 로컬 공지 점수 `SCORE_ANNOUNCEMENT = 320` | 태그 게이트(`SCORE_TAG_MISS = -600`)에 걸리면 `list_content`의 `score >= 0`에서 통째로 사라진다. changelog(25)를 쓰면 상한에 밀려 영영 안 보인다 — §6.1의 존재 이유(노출 소진돼 더는 못 보는 공지를 모아두기)가 무너진다. 프론티어 팁(500)·개인 레슨(550) 아래, boris/팀/plugin-reco(≈300~306) 위 |
| **D5** | `CONTENT_CARD_LIMIT` 4 → **6** | 소스가 하나 늘었다. 상한을 그대로 두면 공지 2건이 배움 카드를 전부 밀어낸다. 자르는 지점은 여전히 한 곳(`partitionCoachItems` 앞) |
| **D6** | 공지 `text`의 `Learn more: <url>`을 뜯어 `source_url`로 | 문법 B의 주 CTA가 「전문 보기 →」다. URL이 없으면 CTA가 없는 카드가 된다. 결정론·순수 함수 |

---

## File Structure

| 파일 | 책임 | 변경 |
|------|------|------|
| `crates/core/src/announcements.rs` | **신규** — 로컬 공지 파싱·병합·ContentItem 변환 | 생성 |
| `crates/core/src/lib.rs` | 모듈 등록 | `pub mod announcements;` |
| `crates/core/src/content.rs` | 번역 프롬프트·응답 파싱·점수 | `translate_*` 추가, `coach_prompt` 삭제, `SCORE_ANNOUNCEMENT` |
| `crates/core/src/store.rs` | 스키마·마이그레이션·조회 | 3컬럼 + 번역 조회/쓰기 + 공지 통지 기록 |
| `crates/core/src/main.rs` | CLI `curate` | 로컬 공지를 피드에 합류 |
| `src-tauri/src/pipeline.rs` | 스캔 편승 스텝 | 공지 수집·번역·새 공지 emit |
| `src-tauri/src/commands.rs`·`lib.rs` | 커맨드 등록 | `coach_tip` 제거 |
| `src/lib/api.ts` | 타입·커맨드 바인딩 | 3필드 추가, `coachTip` 제거, `onAnnouncementNew` |
| `src/lib/ui/coach-helpers.ts` | 순수 헬퍼 | 고정 슬롯 판정·번역 우선 뷰모델·ack 저장 |
| `src/lib/ui/coach/LearnCard.svelte` | 문법 B 카드 | 기한 칩·고정 스타일·`[확인]`·`data-key` |
| `src/lib/ui/CoachTab.svelte` | 탭 조립 | 고정 슬롯, coachTip 제거 |
| `src/lib/notices.ts` | 알림 헬퍼 | `announcementNotice` + **`noticeStamp`(C)** |
| `src/lib/ui/home/NoticeLog.svelte` | 알림 박스 | 아이콘 + **`noticeStamp` 사용(C)** |
| `src/lib/robot/bubble.ts` | 말풍선 | `announcementBubble` |
| `src/App.svelte`·`src/Mascot.svelte` | 구독 배선 | `onAnnouncementNew` |

---

## Task 1 — 로컬 공지 파싱 (§6.1·§6.6)

**Files:** `crates/core/src/announcements.rs`(신규), `crates/core/src/lib.rs`

```rust
pub struct LocalAnnouncement { id, title: Option<String>, text, priority: i64, impressions: Option<i64>, emergency: bool }
pub fn parse_announcements(claude_json: &Value) -> Vec<LocalAnnouncement>
pub fn parse_emergency_tip(claude_json: &Value) -> Option<LocalAnnouncement>
pub fn merge_announcements(per_host: Vec<Vec<LocalAnnouncement>>) -> Vec<LocalAnnouncement>  // id dedup + priority desc
pub fn split_learn_more(text: &str) -> (String, Option<String>)                              // (본문, url)
pub fn announcement_items(anns: &[LocalAnnouncement]) -> Vec<ContentItem>
pub fn collect_local_announcements() -> Vec<LocalAnnouncement>                               // 가장자리(파일 읽기)
```

관대한 파싱(§6.6): `id`·`text`가 문자열이고 비어있지 않은 항목만 취한다. 그 외는 **그 항목만 skip**.
경로 부재·타입 불일치 → 빈 벡터. 하드 에러 없음.

**테스트**: 정상 배열 N개 / 필드 누락 skip / 타입 불일치 skip / 경로 부재 → 빈 벡터 /
호스트 간 id dedup + priority 정렬 / `Learn more:` URL 분리 / 긴급 팁은 `notice` 태그.

## Task 2 — 마이그레이션 3컬럼 (§5.3·§6.3)

**Files:** `crates/core/src/store.rs`

SCHEMA의 `content_items`에 `summary_ko TEXT, title_ko TEXT, deadline TEXT` 추가 +
`if !has_summary_ko { ALTER ×3 }`. `ContentRow`·`content_row_from`·`list_content` SELECT 확장.
`replace_content_items`의 `ON CONFLICT DO UPDATE SET`에는 **넣지 않는다**(재큐레이션이 캐시를 지우면 안 된다).

**테스트**: 구 스키마 DB(컬럼 없음) + events/sessions/ingest_state/daily_rollup 각 1행 →
open 후 **행 수 보존**을 테이블별로 명시 검증 + `summary_ko`·`title_ko`·`deadline` 존재.

## Task 3 — 번역·기한 추출 (§6.3·§6.4)

**Files:** `crates/core/src/content.rs`, `crates/core/src/store.rs`

```rust
pub enum TranslateKind { Announcement, Changelog, Boris }
pub fn translate_kind_for(tags: &[String]) -> Option<TranslateKind>   // 한국어 소스는 None
pub fn translate_prompt(kind, title, body) -> (String, String)
pub struct Translation { title_ko: Option<String>, summary_ko: String, deadline: Option<String> }
pub fn parse_translation(text: &str) -> Result<Translation>           // extract_verdict_json 재사용
```
`deadline`은 `YYYY-MM-DD`로 파싱되는 값만 채택 — **확신 없으면 null**(기본 폐쇄).

store: `content_needing_translation(limit)`(status='new' AND summary_ko IS NULL) /
`set_content_translation(id, title_ko, summary_ko, deadline)`.

**테스트**: 소스별 프롬프트 분기 / 코드펜스·사족 섞인 응답 파싱 / `deadline` 형식 불량 → null /
같은 id 재큐레이션 시 `content_needing_translation`가 그 행을 다시 주지 않음(= Engine 호출 0회, MockEngine 카운트).

## Task 4 — 파이프라인 배선 (§9 락 규율)

**Files:** `src-tauri/src/pipeline.rs`, `crates/core/src/main.rs`

- `maybe_curate_content`: 락 **밖**에서 `collect_local_announcements()` → `announcement_items()`를 `feed`에 합류.
  로컬 파일이라 TTL 대상이 아니다 — `FEED_SOURCES`는 손대지 않는다.
- `maybe_translate_content`(신규): ①짧은 락 = 엔진 해석 + 대상 조회 ②락 밖 = LLM ③짧은 락 = persist.
  바뀐 게 있으면 `content:ready` 재emit(번역 전 목록이 이미 나갔으므로).
- `maybe_notify_announcements`(신규): 짧은 락으로 미통지 공지 id를 집계·기록하고 `announcement:new` emit.
  **번역 뒤**에 둔다 — 앞에 두면 말풍선에 영어 제목이 나간다.

## Task 5 — 프론트 고정 슬롯·번역 표시 (§6.4·§4.2)

**Files:** `src/lib/ui/coach-helpers.ts`(+test), `LearnCard.svelte`, `CoachTab.svelte`, `api.ts`

```ts
export function pinnedNewsItem(items: ContentItem[], acked: string[], today: string): ContentItem | null
export const CONTENT_CARD_LIMIT = 6;   // D5
```
`toLearnCardView`가 `title_ko`·`summary_ko`를 우선하고 `deadline`을 뷰모델에 싣는다.
배지: `notice` 태그 → 「공지」, 그 외 news/changelog → 「소식」(기존 분기 앞에 추가).

**테스트(순수 함수만)**: 유효 기한+미확인 → 고정 / 기한 경과 → null / `deadline: null` → null /
확인 처분 → null / 후보 2건이면 상위 1건 / 번역 있으면 title_ko·summary_ko 우선, 없으면 원문 폴백.
`~0 tok` 회귀 가드는 기존 `no-savings-display.test.ts`가 계속 강제한다.

## Task 6 — 새 공지 알림 (§6.5)

**Files:** `notices.ts`(+test), `NoticeLog.svelte`, `bubble.ts`(+test), `App.svelte`, `Mascot.svelte`

`announcementNotice(rows, ts)` / `announcementBubble(rows, honorific)`. kind `'announcement'`,
아이콘 `📣`, `noticeDest` → `{tab:'coach', target:id}`. `LearnCard`에 `data-key`를 달아 딥링크가 실제로 스크롤되게 한다.

## Task 7 — C: 홈 알림 타임스탬프 (단일 스트림 스펙 §5 C)

**Files:** `src/lib/notices.ts`(+test), `src/lib/ui/home/NoticeLog.svelte`

```ts
export function noticeStamp(ts: string, now: Date): { label: string; full: string }
```
오늘 → `HH:MM`, 그 외 → `MM-DD`. `full`은 항상 `YYYY-MM-DD HH:MM`(`title` 속성).
**`now`를 인자로 받는다** — 함수 안에서 `new Date()`를 만들면 자정 근처에서 테스트가 간헐 실패한다.

**테스트**: 같은 날 → `HH:MM` / 어제 → `MM-DD` / 해가 바뀐 과거 → `MM-DD` /
`full`은 두 경우 모두 `YYYY-MM-DD HH:MM` / 자정 직전·직후 경계.

---

## 뮤테이션 규율

「확신 없으면 null」·「경과 기한은 고정 안 함」 같은 **negative-space 테스트는 RED를 못 본다.**
구현 후 조건을 일부러 뒤집어(예: `deadline >= today` → `deadline <= today`) 실패하는지 확인하고 되돌린다.

## DoD

- `cargo test`·`npm test` 녹색(베이스라인 663 / 260 이상)
- `docs-archive` 스킬로 이 계획 문서 + `docs/archive/design/a-mate/specs/2026-08-02-coaching-tab-unification-design.md` 아카이브.
  단일 스트림 스펙이 unification 스펙의 §4.1·§5를 계속 참조하므로 **링크가 깨지지 않는지 확인**한다.
  `specs/2026-08-02-coaching-tab-single-stream-design.md`는 착수 전이라 **아카이브하지 않는다**.
