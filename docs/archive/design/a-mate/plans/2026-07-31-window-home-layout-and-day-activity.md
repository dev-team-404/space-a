---
status: done
archived: 2026-07-31
---

# 묶음 ⑦ — 창 고정 + 홈 재배치 + 일별 활동 패널 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 미니홈피 창을 960×820으로 고정하고, 홈 배치를 "통계 위 · 방 아래"로 바꾸며, 다이어리 탭 달력 아래에 선택 날짜의 활동(세션·토큰·도구 호출)을 보여주는 패널을 추가한다.

**Architecture:** 창 고정은 `tauri.conf.json` 설정 변경만이다. 홈 재배치는 `HomeTab.svelte`의 마크업 순서만 바꾸되 **방을 `{#if !visiting}` 밖에 유지**한다(안으로 옮기면 방문 기능이 깨진다). D1은 Rust에 조회 함수 하나를 추가하고 기존 집계 둘과 묶어 커맨드 하나로 내보낸 뒤, 신규 Svelte 컴포넌트가 렌더한다 — 판정·포맷 로직은 순수 모듈로 분리해 Vitest로 덮는다.

**Tech Stack:** Tauri v2, Rust(rusqlite), Svelte 5(runes), TypeScript, Vitest.

**스펙:** [2026-07-31-window-home-layout-and-day-activity-design.md](../specs/2026-07-31-window-home-layout-and-day-activity-design.md)

## Global Constraints

- 실행 위치: `a-mate/`. 프론트 명령은 그 디렉터리에서.
- 플랫폼 **Windows 전용**. 빌드·실행은 네이티브 Windows PowerShell/cmd (WSL 금지).
- 스타일은 **전역 CSS 토큰(`var(--…)`)만** — 하드코딩 색 금지(`src/lib/no-hardcoded-colors.test.ts`가 강제).
- 모든 백엔드 호출은 `src/lib/api.ts`에 래핑하고 컴포넌트는 거기서 import.
- 무거운 데이터 처리는 Rust(`crates/core`), 프론트는 렌더링만 (a-mate/CLAUDE.md).
- 커밋: **Conventional Commits, 영어**. scope는 프론트 변경 `frontend`, Rust/커맨드 변경 `agent`.
  본문 끝에 `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- **`npm test` = `svelte-check --threshold error && vitest run`** (묶음 ⑥/PR #141에서 편입). 프론트 변경 후 이것만 돌리면 타입 체크까지 커버된다.
- a-hub·`contracts/` **미접촉**.
- 베이스라인 (2026-07-31 워크트리 실측): `npm test` svelte-check 0 errors + vitest **216 passed** / 26 files · `cargo test` **585 + 45 passed** exit 0.

---

### Task 1: 창 크기 고정 (960 × 820)

**Files:**
- Modify: `a-mate/src-tauri/tauri.conf.json` (`app.windows[]`의 `chat` 항목)

**Interfaces:**
- Produces: 고정된 뷰포트 960×820. Task 2·5의 레이아웃은 이 폭을 전제로 한다.

- [ ] **Step 1: chat 창 설정을 바꾼다**

`app.windows[]`에서 `label: "chat"` 항목을 아래로 만든다. `minWidth`/`minHeight`는 **삭제**한다 — `resizable: false` 아래에서는 의미가 없고, 남겨두면 다음 사람이 조절 가능한 줄 오해한다.

```jsonc
{
  "label": "chat",
  "url": "chat.html",
  "title": "agent mentor",
  "width": 960,
  "height": 820,
  "resizable": false,
  "visible": false
}
```

`mascot` 창은 **건드리지 않는다**(이미 `resizable: false`).

- [ ] **Step 2: 빌드가 통과하는지 확인**

Run: `cd a-mate && npm run build`
Expected: exit 0. (`tauri.conf.json`은 Rust 빌드 시 파싱되므로 형식 오류는 Step 3에서도 드러난다.)

Run: `cd a-mate && cargo test`
Expected: 585 + 45 passed, exit 0 (설정 변경이라 테스트 수 불변).

- [ ] **Step 3: 커밋**

```bash
git add a-mate/src-tauri/tauri.conf.json
git commit -F- <<'EOF'
feat(frontend): fix the minihompy window at 960x820

The window was resizable with a 860x760 default, and the layout is plain
flex with no max-width, so stretching it left the diary tab with the
calendar alone in the top-left corner and the rest empty.

The first plan was the actual Cyworld arrangement — resizable window,
fixed-width content, background margin — until the obvious question: if
the content does not grow, what is resizing for? Cyworld could afford it
because that was a browser window serving other tabs. Here the window is
the minihompy, so fixed content means a fixed window.

960 wide gives the two-column stat cards room they did not have at 860;
820 tall keeps the room visible without scrolling once the home tab is
reordered. minWidth/minHeight are dropped because they mean nothing under
resizable: false and would read as if the window still resized.

Known cost: a 1366x768 screen now has no escape hatch, and this ships by
auto-update. Left unsolved on purpose — the fallbacks (auto-shrink, zoom
presets) are recorded in the spec if reports come in.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 2: 홈 재배치 — 통계 위 · 방 아래

**Files:**
- Modify: `a-mate/src/lib/ui/HomeTab.svelte:73-115` (마크업 순서만)

**Interfaces:**
- Consumes: Task 1의 고정 창.
- Produces: 없음 (마크업 순서 변경).

- [ ] **Step 1: 현재 구조를 확인한다**

Run: `cd a-mate && sed -n '73,96p' src/lib/ui/HomeTab.svelte`

Expected: 방(`LifeView`/`MiniLife`)이 **`{#if !visiting}` 블록보다 위**에 있고, `.strip` / `.grid` / `<footer class="status">`가 그 블록 안에 있다.

**이것이 이 태스크의 핵심 제약이다.** 방이 조건 블록 밖에 있는 이유는 남의 방을 방문 중일 때 방만 보이고 내 로컬 통계는 전부 숨겨야 하기 때문이다(코드 주석: "남의 것으로 오독 방지"). **방을 조건 블록 안으로 옮기면 방문 기능이 깨진다.**

- [ ] **Step 2: 순서를 바꾼다 — 조건 블록을 둘로 쪼갠다**

`<section class="home">`의 여는 태그부터 `{/if}` `</section>`까지를 아래 구조로 바꾼다. **각 블록의 내용(`.strip`·`.grid`·`footer`의 내부 마크업)은 한 글자도 바꾸지 말고 그대로 옮긴다** — 이 태스크는 순서 변경만이다.

```svelte
<section class="home">
  <!-- 아래는 전부 내 로컬 데이터 — 남의 방을 보는 동안엔 숨긴다 (남의 것으로 오독 방지) -->
  {#if !visiting}
  <div class="strip">
    … 기존 내용 그대로 …
  </div>

  <div class="grid">
    <WeekTrend {days} />
    <ModelMix />
    <SaveTop3 {findings} onGoto={onGotoCoach} />
    <NoticeLog {notices} onGoto={onGotoNotice} />
  </div>
  {/if}

  <!-- 방은 조건 밖에 유지한다 — 방문 중엔 이것만 보인다 -->
  {#if hubConnected || interiorPreview}
    <LifeView />
  {:else}
    <MiniLife advice={topAdvice} {honorific} />
  {/if}

  {#if !visiting}
  <footer class="status">
    … 기존 내용 그대로 …
  </footer>
  {/if}
</section>
```

- [ ] **Step 3: `.status`의 `margin-top: auto`를 확인한다**

`HomeTab.svelte:134`의 `.status { margin-top: auto; … }`는 footer를 바닥으로 밀어내는 규칙이다. 방이 footer 위로 오면서 여전히 의도대로 동작한다(`.home`이 `flex-direction: column`이고 `min-height: 100%`). **변경 불필요** — 확인만 하고 넘어간다.

- [ ] **Step 4: 검증**

Run: `cd a-mate && npm test`
Expected: svelte-check **0 errors** + vitest **216 passed** (마크업 순서 변경이라 테스트 수 불변).

Run: `cd a-mate && npm run build`
Expected: exit 0.

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src/lib/ui/HomeTab.svelte
git commit -F- <<'EOF'
feat(frontend): put the stats above the room on the home tab

Reorders the home tab toward the Cyworld arrangement the user asked for:
summary strip and stat cards first, the room below them, scan status last.

The room stays outside the `{#if !visiting}` block, which is why the block
is now split in two. That block hides local stats while visiting someone
else's room so they are not misread as theirs; folding the room into it to
get the ordering would have blanked the visiting view entirely. Visiting
renders exactly as before.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 3: `sessions_for_date` — 그날 세션 목록 조회 (Rust)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` (신규 `DaySession` 구조체 + `sessions_for_date` 메서드 + 테스트)

**Interfaces:**
- Produces:
  - `pub struct DaySession { pub session_id: String, pub project: String, pub first_ts: String, pub first_prompt: Option<String> }` — `Debug, Clone, serde::Serialize`
  - `pub fn sessions_for_date(&self, date: &str) -> Result<Vec<DaySession>>` — `first_ts` 오름차순
  - Task 4가 이 둘을 사용한다.

- [ ] **Step 1: 실패하는 테스트를 쓴다**

`crates/core/src/store.rs`의 `#[cfg(test)] mod tests` 안에 추가한다(파일 끝 근처, 기존 테스트들과 같은 모듈).

```rust
    #[test]
    fn sessions_for_date_buckets_by_local_date_and_names_by_cwd_basename() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        let ts = now.to_rfc3339();

        let ev = |sid: &str, off: u64, ts: &str, kind: EventKind| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "win:d:\\project\\space-a".into(),
            session_id: sid.into(), uuid: Some(format!("{sid}-{off}")), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "C:\\proj\\s.jsonl".into(), source_offset: off, msg_id: None, kind,
        };

        store.upsert_events(&[
            // cwd 있는 세션 — 표시명은 basename
            ev("s1", 0, &ts, EventKind::SessionMeta {
                cwd: "D:\\Project\\space-a".into(), git_branch: None }),
            ev("s1", 10, &ts, EventKind::UserPrompt {
                preview: "PR138까지 머지했다".into(), is_command: false }),
            // cwd 없는 세션 — project_id로 폴백. 프롬프트도 없음
            ev("s2", 0, &ts, EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0 }),
            // 다른 날짜 세션 — 오늘 버킷에 안 잡혀야 한다
            ev("s3", 0, "2026-01-02T03:04:05Z", EventKind::SessionMeta {
                cwd: "D:\\Project\\other".into(), git_branch: None }),
        ]).unwrap();

        let rows = store.sessions_for_date(&today).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"s1") && ids.contains(&"s2"), "오늘 세션 2건: {ids:?}");
        assert!(!ids.contains(&"s3"), "다른 날짜 세션이 섞였다: {ids:?}");

        let s1 = rows.iter().find(|r| r.session_id == "s1").unwrap();
        assert_eq!(s1.project, "space-a", "cwd basename을 표시명으로");
        assert_eq!(s1.first_prompt.as_deref(), Some("PR138까지 머지했다"));

        let s2 = rows.iter().find(|r| r.session_id == "s2").unwrap();
        assert_eq!(s2.project, "win:d:\\project\\space-a", "cwd 없으면 project_id 폴백");
        assert!(s2.first_prompt.is_none());
    }
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

Run: `cd a-mate && cargo test -p agent-mentor sessions_for_date`
Expected: FAIL — 컴파일 오류 `no method named 'sessions_for_date' found`.

- [ ] **Step 3: `DaySession` 구조체를 추가한다**

`crates/core/src/store.rs`의 `pub struct DaySummary` 정의(파일 내 `#[derive(Debug, Clone, serde::Serialize)] pub struct DaySummary`) **바로 아래**에 붙인다.

```rust
/// 그날 시작한 세션 1건 — 다이어리 일별 활동 패널용.
/// `project`는 **표시명**이다(`cwd`의 basename, 없으면 `project_id` 폴백) —
/// `project_id`는 `win:d:\project\space-a` 형태의 정규화 키라 그대로 보여줄 값이 아니다.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DaySession {
    pub session_id: String,
    pub project: String,
    pub first_ts: String,
    pub first_prompt: Option<String>,
}
```

- [ ] **Step 4: 조회 메서드를 구현한다**

`impl SqliteStore` 블록 안, `summary_for_date` 메서드 **바로 아래**에 붙인다.

```rust
    /// 그날(로컬 날짜) 시작한 세션 목록 — `first_ts` 오름차순.
    /// 날짜 버킷 규약은 나머지 일별 집계와 동일한 `date(...,'localtime')`이다
    /// (다르게 하면 요약의 세션 수와 이 목록의 길이가 어긋난다).
    pub fn sessions_for_date(&self, date: &str) -> Result<Vec<DaySession>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, project_id, cwd, first_ts, first_prompt_preview
               FROM sessions
              WHERE date(first_ts,'localtime')=?1
              ORDER BY first_ts",
        )?;
        let rows = stmt.query_map(params![date], |r| {
            let project_id: String = r.get(1)?;
            let cwd: Option<String> = r.get(2)?;
            Ok(DaySession {
                session_id: r.get(0)?,
                project: cwd
                    .as_deref()
                    .map(crate::hosts::path_basename)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(project_id),
                first_ts: r.get(3)?,
                first_prompt: r.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
```

`WHERE date(first_ts,…)`가 `first_ts IS NULL`인 행을 걸러내므로 `first_ts`는 `String`으로 받아도 안전하다.

- [ ] **Step 5: 테스트 통과 확인**

Run: `cd a-mate && cargo test -p agent-mentor sessions_for_date`
Expected: PASS (1 test).

Run: `cd a-mate && cargo test`
Expected: **586 + 45 passed** (신규 1), exit 0.

- [ ] **Step 6: 커밋**

```bash
git add a-mate/crates/core/src/store.rs
git commit -F- <<'EOF'
feat(agent): add sessions_for_date for the diary activity panel

The panel needs the day's sessions, and nothing returned them — session_ctx
takes one id at a time. Buckets on date(first_ts,'localtime') like every
other per-day aggregate, so the count in the summary and the length of this
list cannot disagree.

`project` is a display name, not `project_id`. The stored id is a
normalized key (`win:d:\project\space-a`) that merges WSL and Windows-UNC
sessions for the same directory; `path_basename(cwd)` is what yields
"space-a". Falls back to the id when cwd is missing.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 4: `day_activity` 커맨드 — 세 집계를 한 왕복으로

**Files:**
- Modify: `a-mate/crates/core/src/diary/mod.rs:529` (`collect_tool_usage`를 `pub`으로)
- Modify: `a-mate/src-tauri/src/commands.rs` (신규 `DayActivity` + `day_activity_inner` + `day_activity` 커맨드)
- Modify: `a-mate/src-tauri/src/lib.rs` (커맨드 등록)
- Modify: `a-mate/src/lib/api.ts` (타입 + 래퍼)

**Interfaces:**
- Consumes: Task 3의 `DaySession`·`sessions_for_date`.
- Produces:
  - Rust: `pub struct DayActivity { pub summary: DaySummary, pub tools: ToolUsage, pub sessions: Vec<DaySession> }`
  - TS: `dayActivity(date: string): Promise<DayActivityData>` — Task 5가 사용한다.

- [ ] **Step 1: `collect_tool_usage`를 공개한다**

`crates/core/src/diary/mod.rs`에서 함수 시그니처 한 줄만 바꾼다.

```rust
pub fn collect_tool_usage(store: &SqliteStore, date: &str) -> ToolUsage {
```

`ToolUsage`(같은 파일 `:23`)는 이미 `pub` + `Serialize`이고, `crates/core/src/lib.rs:6`이 `pub mod diary`라 외부에서 `agent_mentor::diary::collect_tool_usage`로 닿는다.

- [ ] **Step 2: 커맨드를 추가한다**

먼저 `src-tauri/src/commands.rs:4`의 import를 넓힌다. 이 파일은 타입을 **짧은 이름으로 쓰는 스타일**이다(`use agent_mentor::store::SqliteStore;`).

```rust
use agent_mentor::diary::{collect_tool_usage, ToolUsage};
use agent_mentor::store::{DaySession, DaySummary, SqliteStore};
```

그다음 본문을 `sessions_ctx_inner`/`sessions_ctx` 근처(같은 "조회" 계열)에 붙인다.

```rust
/// 다이어리 일별 활동 패널 — 요약·도구 집계·세션 목록을 한 왕복으로.
#[derive(Debug, Serialize)]
pub struct DayActivity {
    pub summary: DaySummary,
    pub tools: ToolUsage,
    pub sessions: Vec<DaySession>,
}

pub fn day_activity_inner(store: &SqliteStore, date: &str) -> anyhow::Result<DayActivity> {
    Ok(DayActivity {
        summary: store.summary_for_date(date)?,
        // 쿼리 실패는 빈 집계로 흡수된다(원래 계약) — 패널이 통째로 사라지지 않게
        tools: collect_tool_usage(store, date),
        sessions: store.sessions_for_date(date)?,
    })
}

#[tauri::command(async)]
pub fn day_activity(state: State<AppState>, date: String) -> Result<DayActivity, String> {
    let guard = lock(&state)?;
    day_activity_inner(&*guard, &date).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: 커맨드를 등록한다**

`src-tauri/src/lib.rs`의 `invoke_handler![…]` 목록에서 `commands::sessions_ctx,` 다음 줄에 추가한다.

```rust
                commands::day_activity,
```

- [ ] **Step 4: Rust 빌드 확인**

Run: `cd a-mate && cargo test`
Expected: **586 + 45 passed**, exit 0 (이 태스크는 테스트를 추가하지 않는다 — 조립 계층이고 재료 셋은 각각 이미 덮여 있다).

- [ ] **Step 5: api.ts에 타입과 래퍼를 추가한다**

`src/lib/api.ts`의 `SessionCtxItem` 인터페이스 근처에 타입을, `sessionsCtx` 근처에 래퍼를 넣는다.

```ts
export interface DaySessionItem {
  session_id: string;
  project: string;
  first_ts: string;
  first_prompt: string | null;
}

export interface DayActivityData {
  summary: {
    session_count: number;
    tok_input: number;
    tok_output: number;
    tok_cache_read: number;
    tok_cache_create: number;
  };
  tools: {
    total_calls: number;
    by_kind: [string, number][];
    skills: string[];
    mcp_servers: string[];
  };
  sessions: DaySessionItem[];
}
```

```ts
export const dayActivity = (date: string) => invoke<DayActivityData>('day_activity', { date });
```

`by_kind`가 `[string, number][]`인 것은 Rust `Vec<(String, u64)>`가 JSON 배열의 배열로 직렬화되기 때문이다.

- [ ] **Step 6: 타입 체크 확인**

Run: `cd a-mate && npm test`
Expected: svelte-check **0 errors** + vitest **216 passed** (아직 소비처가 없다).

- [ ] **Step 7: 커밋**

```bash
git add a-mate/crates/core/src/diary/mod.rs a-mate/src-tauri/src/commands.rs a-mate/src-tauri/src/lib.rs a-mate/src/lib/api.ts
git commit -F- <<'EOF'
feat(agent): expose a day_activity command for the diary panel

Bundles the three per-day aggregates the panel needs into one round trip:
the existing summary, the tool-usage rollup the diary pipeline already
computes, and the new session list.

`collect_tool_usage` only had to become pub — it was private to the diary
module but computes exactly what the panel wants, and duplicating the
tool_kind rollup would give two places to drift. Its contract of absorbing
query failures into an empty aggregate is kept, so a broken tool query
costs the panel one section rather than the whole thing.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 5: `DayActivity.svelte` — 달력 아래 패널

**Files:**
- Create: `a-mate/src/lib/ui/diary/day-activity.ts` (순수 포맷·계산)
- Create: `a-mate/src/lib/ui/diary/day-activity.test.ts`
- Create: `a-mate/src/lib/ui/diary/DayActivity.svelte`
- Modify: `a-mate/src/lib/ui/DiaryTab.svelte` (패널 배선)

**Interfaces:**
- Consumes: Task 4의 `dayActivity(date)` · `DayActivityData` · `DaySessionItem`.
- Produces: 없음 (최종 소비자).

- [ ] **Step 1: 순수 함수의 실패하는 테스트를 쓴다**

`a-mate/src/lib/ui/diary/day-activity.test.ts`를 만든다.

```ts
import { describe, expect, it } from 'vitest';
import { compactTokens, hhmm, toolBars } from './day-activity';

describe('compactTokens', () => {
  it('백만 미만은 천 단위 구분만 — 좁은 폭에서도 읽히는 자릿수', () => {
    expect(compactTokens(0)).toBe('0');
    expect(compactTokens(3367)).toBe('3,367');
    expect(compactTokens(429577)).toBe('429,577');
  });
  it('백만 이상은 M으로 줄인다 — 240px 컬럼에 9자리가 안 들어간다', () => {
    expect(compactTokens(1_200_000)).toBe('1.2M');
    expect(compactTokens(12_345_678)).toBe('12.3M');
  });
});

describe('hhmm', () => {
  it('ISO 시각에서 시:분만 뽑는다', () => {
    expect(hhmm('2026-07-30T09:12:34+09:00')).toBe('09:12');
  });
  it('파싱 불가면 빈 문자열 — 라벨이 "Invalid Date"가 되지 않게', () => {
    expect(hhmm('nope')).toBe('');
  });
});

describe('toolBars', () => {
  const kinds: [string, number][] = [
    ['bash', 210], ['edit', 142], ['read', 98], ['write', 55], ['skill', 12], ['mcp_call', 3],
  ];
  it('상위 N개만, 최대값 기준 폭 비율', () => {
    const bars = toolBars(kinds, 5);
    expect(bars.map((b) => b.kind)).toEqual(['bash', 'edit', 'read', 'write', 'skill']);
    expect(bars[0].pct).toBe(100);
    expect(bars[1].pct).toBe(Math.round((142 / 210) * 100));
  });
  it('빈 입력은 빈 배열 — 0 나눗셈 없음', () => {
    expect(toolBars([], 5)).toEqual([]);
  });
  it('전부 0이어도 NaN이 되지 않는다', () => {
    expect(toolBars([['bash', 0]], 5)).toEqual([{ kind: 'bash', count: 0, pct: 0 }]);
  });
});
```

- [ ] **Step 2: 테스트가 실패하는지 확인**

Run: `cd a-mate && npx vitest run src/lib/ui/diary/day-activity.test.ts`
Expected: FAIL — `Failed to resolve import "./day-activity"`.

- [ ] **Step 3: 순수 모듈을 구현한다**

`a-mate/src/lib/ui/diary/day-activity.ts`를 만든다.

```ts
/** 다이어리 일별 활동 패널의 포맷·계산 — 컴포넌트에서 분리해 테스트 가능하게 둔다. */

/** 폭 240px 컬럼용 토큰 표기. 백만 이상만 줄인다(그 아래는 자릿수가 들어간다). */
export function compactTokens(n: number): string {
  if (!Number.isFinite(n)) return '0';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  return n.toLocaleString();
}

/** ISO 시각 → "09:12". 파싱 불가면 빈 문자열(라벨에 "Invalid Date"를 흘리지 않는다). */
export function hhmm(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

export interface ToolBar {
  kind: string;
  count: number;
  pct: number;
}

/** 상위 `limit`종만, 최대값을 100%로 한 막대 폭. 빈 입력·전부 0에서도 NaN이 없다. */
export function toolBars(byKind: [string, number][], limit: number): ToolBar[] {
  const top = byKind.slice(0, limit);
  const max = Math.max(...top.map(([, n]) => n), 0);
  return top.map(([kind, count]) => ({
    kind,
    count,
    pct: max > 0 ? Math.round((count / max) * 100) : 0,
  }));
}
```

`by_kind`는 Rust에서 이미 count 내림차순이므로(`diary/mod.rs`의 `ORDER BY COUNT(*) DESC`) `slice`만으로 상위 N이 된다.

- [ ] **Step 4: 테스트 통과 확인**

Run: `cd a-mate && npx vitest run src/lib/ui/diary/day-activity.test.ts`
Expected: PASS (7 tests).

- [ ] **Step 5: 컴포넌트를 만든다**

`a-mate/src/lib/ui/diary/DayActivity.svelte`를 만든다. 색은 **전역 토큰만** 쓴다(`no-hardcoded-colors.test.ts`가 강제).

```svelte
<script lang="ts">
  import { dayActivity, type DayActivityData } from '../../api';
  import { compactTokens, hhmm, toolBars } from './day-activity';

  let { date }: { date: string } = $props();

  let data = $state<DayActivityData | null>(null);
  let expanded = $state(false);
  let loadSequence = 0;

  const SESSION_HEAD = 3;

  $effect(() => {
    const sequence = ++loadSequence;
    const target = date;
    expanded = false;
    data = null;
    dayActivity(target)
      .then((d) => { if (sequence === loadSequence) data = d; })
      .catch(() => { if (sequence === loadSequence) data = null; });
  });

  const bars = $derived(data ? toolBars(data.tools.by_kind, 5) : []);
  const cache = $derived(data ? data.summary.tok_cache_read + data.summary.tok_cache_create : 0);
  const chips = $derived(data ? [...data.tools.skills, ...data.tools.mcp_servers] : []);
  const shown = $derived(
    !data ? [] : expanded ? data.sessions : data.sessions.slice(0, SESSION_HEAD),
  );
  const rest = $derived(data ? Math.max(0, data.sessions.length - SESSION_HEAD) : 0);
</script>

{#if data}
  <div class="activity">
    <h4>{date}</h4>
    <p class="head">세션 <b>{data.summary.session_count}</b> · 도구 <b>{data.tools.total_calls}</b>회</p>

    <section>
      <h5>토큰</h5>
      <dl>
        <dt>입력</dt><dd>{compactTokens(data.summary.tok_input)}</dd>
        <dt>출력</dt><dd>{compactTokens(data.summary.tok_output)}</dd>
        <dt>캐시</dt><dd>{compactTokens(cache)}</dd>
      </dl>
    </section>

    {#if bars.length}
      <section>
        <h5>도구 호출</h5>
        {#each bars as b (b.kind)}
          <div class="bar-row">
            <span class="kind">{b.kind}</span>
            <span class="track"><span class="fill" style:width={`${b.pct}%`}></span></span>
            <span class="count">{b.count}</span>
          </div>
        {/each}
      </section>
    {/if}

    {#if chips.length}
      <section>
        <h5>스킬 · MCP</h5>
        <div class="chips">{#each chips as c (c)}<span class="chip">{c}</span>{/each}</div>
      </section>
    {/if}

    {#if shown.length}
      <section>
        <h5>세션</h5>
        {#each shown as s (s.session_id)}
          <div class="session">
            <span class="when">{hhmm(s.first_ts)}</span>
            <span class="proj">{s.project}</span>
            {#if s.first_prompt}<p class="prompt">{s.first_prompt}</p>{/if}
          </div>
        {/each}
        {#if rest > 0 && !expanded}
          <button class="more" onclick={() => (expanded = true)}>… {rest}개 더 ▾</button>
        {/if}
      </section>
    {/if}
  </div>
{/if}

<style>
  .activity {
    margin-top: 12px; background: var(--frame-bg); border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft); padding: 12px; font-size: 11px; color: var(--ink);
  }
  .activity h4 { margin: 0 0 6px; font-size: 12px; }
  .head { margin: 0 0 10px; color: var(--ink-soft); }
  .head b { color: var(--ink); }
  section { border-top: 1px solid var(--line); padding-top: 8px; margin-top: 8px; }
  h5 { margin: 0 0 6px; font-size: 11px; color: var(--ink-soft); font-weight: normal; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 2px 8px; margin: 0; }
  dt { color: var(--ink-soft); }
  dd { margin: 0; text-align: right; }
  .bar-row { display: grid; grid-template-columns: 44px 1fr auto; gap: 6px; align-items: center; margin-bottom: 3px; }
  .kind { color: var(--ink-soft); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .track { height: 6px; border-radius: 999px; background: var(--pastel-lav); overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--accent); border-radius: 999px; }
  .count { color: var(--ink-soft); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip {
    background: var(--accent-tint); border-radius: var(--radius-s); padding: 1px 6px;
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .session { margin-bottom: 6px; }
  .when { color: var(--ink-soft); margin-right: 5px; }
  .proj { font-weight: 600; }
  .prompt {
    margin: 1px 0 0; color: var(--ink-soft); overflow: hidden;
    text-overflow: ellipsis; white-space: nowrap;
  }
  .more {
    border: none; background: none; font: inherit; font-size: 11px;
    color: var(--accent-strong); cursor: pointer; padding: 2px 0;
  }
</style>
```

`--accent-tint`·`--line`·`--frame-bg`·`--pastel-lav`는 기존 컴포넌트가 쓰는 토큰이다(`GuestbookTab`·`HomeTab` 선례).

- [ ] **Step 6: DiaryTab에 배선한다**

`a-mate/src/lib/ui/DiaryTab.svelte`의 import에 한 줄을 더하고,

```ts
  import DayActivity from './diary/DayActivity.svelte';
```

`</div>`(`.cal` 닫는 태그) **바로 뒤**, 즉 `.cal` 블록과 `.body` 사이에 넣는다.

```svelte
    {#if !visiting && selected}<DayActivity date={selected} />{/if}
```

결과 구조:

```svelte
  <div class="cal"> … 달력 … </div>
  {#if !visiting && selected}<DayActivity date={selected} />{/if}
  <div class="body"> … 일기 본문 … </div>
```

**`!visiting` 가드가 필수다** — 남의 방 일기를 보는 중에 내 세션 기록·토큰이 보이면 남의 것으로 오독되거나 내 활동이 새어 나간다.

`.diary`가 `display:flex`(가로 배치)이므로 패널이 달력 **옆**이 아니라 **아래**로 가려면 달력과 패널을 한 컬럼으로 묶어야 한다. `.cal`의 `width: 238px`는 그대로 두고, 감싸는 요소를 추가한다:

```svelte
  <div class="side">
    <div class="cal"> … 달력 … </div>
    {#if !visiting && selected}<DayActivity date={selected} />{/if}
  </div>
```

그리고 `<style>`에서 `.cal`의 `width: 238px; align-self: flex-start;`를 `.side`로 옮긴다:

```css
.side { width: 238px; flex-shrink: 0; align-self: flex-start; }
.cal { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px; }
```

(원래 `.cal`에 있던 `width: 238px; flex-shrink: 0; align-self: flex-start;` 세 속성만 `.side`로 이동하고 나머지는 유지한다.)

- [ ] **Step 7: 전체 검증**

Run: `cd a-mate && npm test`
Expected: svelte-check **0 errors** + vitest **223 passed** (216 + 신규 7).

Run: `cd a-mate && npm run build`
Expected: exit 0.

- [ ] **Step 8: 커밋**

```bash
git add a-mate/src/lib/ui/diary/ a-mate/src/lib/ui/DiaryTab.svelte
git commit -F- <<'EOF'
feat(frontend): show the day's activity under the diary calendar

The diary tab put the calendar in a 238px column and left everything below
it empty, which got worse the taller the window. It now carries the day's
numbers: sessions and tool calls, token split, the top five tool kinds as
bars, skill/MCP chips, and the first three sessions with their opening
prompt.

Hidden while visiting. The panel is local data, and the diary tab doubles
as the viewer for other people's shared entries — the same rule the home
tab already applies to its stats.

Formatting and bar maths live in a plain module so they are testable:
tokens stay grouped until a million and only then collapse to M, since a
240px column fits the digits but not nine of them, and the bar helper
returns 0% rather than NaN when a day has no calls.

The calendar and the panel are wrapped in one column, so the fixed width
moved from .cal to that wrapper — otherwise the flex row would have put the
panel beside the calendar instead of under it.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

---

### Task 6: 실화면 확인 + DoD 아카이브

**Files:**
- Modify: `docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md` (5차 배치 D1 완료 표시 + 구현 결과)
- Move: 이 계획과 스펙을 `docs/archive/` 미러로 (docs-archive 스킬이 수행)

**Interfaces:**
- Consumes: Task 1–5의 커밋.
- Produces: 없음 (문서 전용).

- [ ] **Step 1: 실화면을 확인한다**

Run: `cd a-mate && npm run tauri dev`

확인 항목 — 하나라도 어긋나면 **멈추고 보고한다**:

| 확인 | 기대 |
|---|---|
| 창 크기 | 960×820으로 뜨고, 가장자리를 끌어도 **크기가 안 바뀐다** |
| 홈 탭 | 위에서부터 요약 strip → 통계 카드 4개 → 방 → 마지막 스캔. **세로 스크롤바가 없다** |
| 다이어리 탭 | 도트 찍힌 날짜를 누르면 달력 **아래**에 활동 패널이 뜬다(옆이 아니라) |
| 세션 펼치기 | 세션이 4건 이상인 날에 "… N개 더 ▾"가 뜨고 누르면 전부 나온다 |
| 방문 중 | 다른 사람 미니홈피로 이동 → 다이어리 탭에 **패널이 없다**, 홈 탭엔 방만 보인다 |

- [ ] **Step 2: 로드맵에 완료를 기록한다**

`docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md`의 5차 배치 **D1** 제목 줄 끝에 `— ✅ 완료(2026-07-31, 묶음 ⑦)`를 붙이고, D1 항목 끝에 구현 결과를 적는다. **아래는 계획 시점의 예상이므로, 실제 착지 내용과 다르면 실제를 따른다.**

```markdown
**묶음 ⑦ 구현 결과(2026-07-31)** — 설계: [아카이브 스펙](../specs/2026-07-31-window-home-layout-and-day-activity-design.md) · [아카이브 계획](2026-07-31-window-home-layout-and-day-activity.md)

- **창을 960×820으로 고정**(`resizable: false`, `minWidth`/`minHeight` 제거). 처음엔 "창은 조절 가능 +
  콘텐츠 `max-width` 고정"(실제 싸이월드 방식)을 제안했으나 **"창을 늘려도 콘텐츠가 그대로면 늘리는 의미가
  있나"** 라는 반문으로 뒤집혔다 — 싸이월드는 브라우저 창이라 다른 탭도 담았지만 이 앱은 **창 자체가
  미니홈피**다. 알려진 비용: 1366×768 화면에 탈출구가 없다(자동 축소·배율 프리셋은 보고가 오면 도입).
- **홈 배치**를 통계(strip·grid) → **방** → footer 순으로. 방은 `{#if !visiting}` **밖에 유지**해야 해서
  조건 블록이 둘로 쪼개졌다 — 안으로 옮기면 방문 화면이 통째로 비어 방문 기능이 깨진다.
- **D1**: 신규 커맨드 `day_activity(date)`가 세 집계를 한 왕복으로 — 기존 `summary_for_date` +
  `collect_tool_usage`(diary 모듈 private → **pub 공개화**, 중복 구현 대신) + 신규
  `sessions_for_date`(`store.rs`). 날짜 버킷은 기존과 같은 `date(...,'localtime')` — 다르게 하면
  요약의 세션 수와 목록 길이가 어긋난다. **`project_id`는 표시명이 아니다**(`win:d:\project\space-a`
  형태의 정규화 키) → `path_basename(cwd)`, 없으면 id 폴백.
- 패널은 신규 `DayActivity.svelte`(41줄짜리 압축된 `DiaryTab`에 넣지 않음) + 순수 모듈
  `day-activity.ts`(포맷·막대 폭, Vitest 7건). **`!visiting && selected`일 때만 표시** — 다이어리 탭은
  남의 공개 일기 뷰어도 겸한다. 달력과 패널을 한 컬럼(`.side`)으로 묶어 고정폭을 `.cal`에서 옮겼다
  (안 그러면 flex row가 패널을 달력 **옆**에 놓는다).
- **검증**: `npm test` = svelte-check 0 errors + vitest **223 passed**(신규 7) · `npm run build` exit 0 ·
  `cargo test` **586 + 45**(신규 1). 실화면 확인 5종은 PR 체크리스트.
```

- [ ] **Step 3: 로드맵 커밋**

```bash
git add docs/design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md
git commit -F- <<'EOF'
docs(docs): record bundle 7 completion in the roadmap

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

- [ ] **Step 4: docs-archive 스킬로 작업 문서를 아카이브한다**

`docs-archive` 스킬을 호출해 이 묶음의 스펙·계획을 `docs/archive/` 미러로 옮긴다(ADR 0013의 DoD 규칙). 로드맵 문서는 **아카이브하지 않는다** — X1이 남아 있다.

스킬이 커밋을 만들지 않으므로 이동 결과를 직접 커밋한다.

```bash
git add -A docs/
git commit -F- <<'EOF'
docs(docs): archive bundle 7 working docs

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
```

- [ ] **Step 5: 최종 검증**

Run: `cd a-mate && npm test && npm run build`
Expected: 0 errors + 223 passed + exit 0.

Run: `cd a-mate && cargo test`
Expected: 586 + 45 passed, exit 0.

---

## 검증 요약

| 시점 | 명령 | 기대 |
|---|---|---|
| 베이스라인 | `npm test` / `cargo test` | 216 passed / 585 + 45 (2026-07-31 실측) |
| Task 1 후 | `npm run build` · `cargo test` | exit 0 / 585 + 45 |
| Task 2 후 | `npm test` | 0 errors + 216 passed |
| Task 3 후 | `cargo test` | **586** + 45 |
| Task 4 후 | `npm test` · `cargo test` | 0 errors + 216 / 586 + 45 |
| Task 5 후 | `npm test` | 0 errors + **223 passed** |
| 최종 | `npm run build` · `cargo test` | exit 0 |

## PR 체크리스트 (사용자 확인)

- [ ] 창이 960×820으로 뜨고 크기 조절이 막혀 있다
- [ ] 홈 탭이 통계 → 방 → 스캔 순이고 세로 스크롤바가 없다
- [ ] 다이어리에서 날짜를 고르면 달력 **아래**에 활동 패널이 뜬다
- [ ] 세션 4건 이상인 날에 "… N개 더" 펼치기가 동작한다
- [ ] 남의 미니홈피 방문 중에는 활동 패널이 보이지 않는다
