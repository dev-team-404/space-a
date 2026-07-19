---
status: done
archived: 2026-07-19
---

# 홈 모델 분포 기간 선택 구현 플랜

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 홈 "모델 분포" 위젯에 기간 세그먼트 토글(오늘/주간/월간/전체)을 추가한다.

**Architecture:** store에 날짜 범위 집계 `model_mix_for_range` 추가(기존 `model_mix_for_date`는 위임 래퍼로),
커맨드 `get_model_mix`에 `period` 옵션 파라미터(기간→범위 계산은 백엔드 순수 함수 `period_range`),
프론트는 ModelMix.svelte가 period 상태·fetch·scan:done 재로드를 자체 소유(HomeTab에서 mix 로딩 제거).

**Tech Stack:** Rust(rusqlite, chrono) + Tauri v2 커맨드, Svelte 5(runes), TypeScript.

**스펙:** `docs/specs/2026-07-12-model-mix-period-design.md`

## Global Constraints

- 브랜치 `feat/model-mix-period` (생성됨). **main 직접 커밋 금지.** 커밋은 태스크 단위.
- 매 cargo 명령 전 (Git Bash):
  `export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"; export CARGO_HTTP_CHECK_REVOKE=false; export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`
- push revocation 에러 시: `git -c http.schannelCheckRevoke=false push`
- 와이어 필드명 `tier` 유지(실제 값은 raw model id — 기존 계약). serde rename 금지.
- 기간 의미론: 주간 = today−6~today, 월간 = today−29~today, 전체 = 하한 없음. 알 수 없는 period 값은 today 취급.
- 날짜 버킷은 `date(ts,'localtime')`, 범위 양끝 포함.

---

### Task 1: store `model_mix_for_range` (crates/core)

**Files:**
- Modify: `crates/core/src/store.rs` (함수 588행 부근 `model_mix_for_date`, 테스트는 같은 파일 `mod tests`)

**Interfaces:**
- Produces: `pub fn model_mix_for_range(&self, from: Option<&str>, to: &str) -> Result<Vec<(String, u64)>>`
  — Task 2가 호출. `from=None`이면 하한 없음. 반환은 (모델 raw id, 입력+출력 토큰 합) 내림차순.
- 기존 `model_mix_for_date(&self, date: &str)`는 시그니처 불변(위임 래퍼로 축소) — 기존 테스트 무수정 통과.

- [ ] **Step 1: 실패하는 테스트 작성** — `crates/core/src/store.rs`의 `mod tests`에 (기존 `model_mix_for_date_groups_by_family` 테스트 아래) 추가:

```rust
    #[test]
    fn model_mix_for_range_bounds_inclusive_and_open_start() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let ev = |uuid: &str, ts: &str, inp: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some(ts.into()),
            source_file: "f.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: inp, output: 0, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        };
        store.upsert_events(&[
            ev("u1", "2026-07-01T10:00:00Z", 1),
            ev("u2", "2026-07-03T10:00:00Z", 10),
            ev("u3", "2026-07-05T10:00:00Z", 100),
        ]).unwrap();

        // 양끝 포함: 03~05 → 10+100 (여러 날짜가 한 모델로 합산)
        assert_eq!(store.model_mix_for_range(Some("2026-07-03"), "2026-07-05").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 110)]);
        // 단일일(from=to) — model_mix_for_date와 동치
        assert_eq!(store.model_mix_for_range(Some("2026-07-03"), "2026-07-03").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 10)]);
        // from=None → 하한 없음(전체)
        assert_eq!(store.model_mix_for_range(None, "2026-07-05").unwrap(),
                   vec![("claude-opus-4-8".to_string(), 111)]);
        // 범위 밖 → 빈 벡터
        assert!(store.model_mix_for_range(Some("2026-08-01"), "2026-08-31").unwrap().is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

Run (Git Bash, env 세팅 후): `cargo test -p agent-mentor model_mix`
Expected: FAIL — `model_mix_for_range` 함수 없음(컴파일 에러 `no method named model_mix_for_range`)

- [ ] **Step 3: 최소 구현** — `crates/core/src/store.rs`의 기존 `model_mix_for_date`(주석 포함 588~601행)를 다음으로 교체:

```rust
    /// 특정 하루의 모델 분포 — model_mix_for_range의 단일일 특수형.
    pub fn model_mix_for_date(&self, date: &str) -> Result<Vec<(String, u64)>> {
        self.model_mix_for_range(Some(date), date)
    }

    /// 기간 내 모델별(raw id 기준, 구 데이터는 family 폴백) 토큰(입력+출력) 합. 내림차순.
    /// from=None이면 하한 없음(전체). 로컬 날짜 버킷(date(ts,'localtime')), 양끝 포함.
    pub fn model_mix_for_range(&self, from: Option<&str>, to: &str) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model_raw, model_family) AS m,
                    COALESCE(SUM(tok_input),0) + COALESCE(SUM(tok_output),0) AS toks
             FROM events
             WHERE date(ts, 'localtime') <= ?2
               AND (?1 IS NULL OR date(ts, 'localtime') >= ?1)
               AND COALESCE(model_raw, model_family) IS NOT NULL
             GROUP BY m ORDER BY toks DESC",
        )?;
        let rows = stmt.query_map(params![from, to], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
```

- [ ] **Step 4: 통과 확인 (신규 + 기존 회귀)**

Run: `cargo test -p agent-mentor model_mix`
Expected: PASS 3건 — `model_mix_for_range_bounds_inclusive_and_open_start`,
`model_mix_for_date_groups_by_family`(래퍼 검증), `rollup_and_model_mix_bucket_by_local_date`(로컬 버킷 회귀)

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/store.rs
git commit -m "feat(store): model_mix_for_range — 날짜 범위 모델 분포 집계 (date는 위임 래퍼로)"
```

---

### Task 2: 커맨드 `period_range` + `get_model_mix(period)` (src-tauri)

**Files:**
- Modify: `src-tauri/src/commands.rs` (`ModelMixEntry` 정의 118~122행 인근에 inner 추가,
  `get_model_mix` 커맨드 246~252행 교체, 테스트는 같은 파일 `mod tests`)

**Interfaces:**
- Consumes: Task 1의 `store.model_mix_for_range(from: Option<&str>, to: &str)`
- Produces: 커맨드 `get_model_mix(period: Option<String>)` — 와이어 응답 `[{ tier, tokens }]` 불변.
  period 값: `"today" | "week" | "month" | "all"`, 미지정·미지의 값은 today.
  (Task 3의 `invoke('get_model_mix', { period })`가 소비)

- [ ] **Step 1: 실패하는 테스트 작성** — `src-tauri/src/commands.rs`의 `mod tests`에 (`week_summary_is_7_days_oldest_first` 아래) 추가:

```rust
    #[test]
    fn period_range_maps_periods() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        assert_eq!(period_range("today", today), (Some("2026-07-12".into()), "2026-07-12".into()));
        assert_eq!(period_range("week", today), (Some("2026-07-06".into()), "2026-07-12".into()));
        assert_eq!(period_range("month", today), (Some("2026-06-13".into()), "2026-07-12".into()));
        assert_eq!(period_range("all", today), (None, "2026-07-12".into()));
        // 알 수 없는 값은 today 취급
        assert_eq!(period_range("yolo", today), period_range("today", today));
    }

    #[test]
    fn model_mix_inner_today_vs_all() {
        use agent_mentor::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "1".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some("u1".into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2020-01-01T10:00:00Z".into()), // 확실한 과거 — today엔 안 걸린다
            source_file: "f.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage { input: 5, output: 5, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
        let all = model_mix_inner(&store, "all").unwrap();
        assert_eq!(all[0].tier, "claude-opus-4-8");
        assert_eq!(all[0].tokens, 10);
        assert!(model_mix_inner(&store, "today").unwrap().is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor-app`
Expected: FAIL — `period_range`/`model_mix_inner` 없음(컴파일 에러)

- [ ] **Step 3: 최소 구현** — `commands.rs`의 `ModelMixEntry` 정의(118~122행) 바로 아래에 추가:

```rust
/// period("today"|"week"|"month"|"all") → (from, to) 로컬 날짜 범위(양끝 포함).
/// week/month는 오늘 포함 rolling 7/30일, all은 하한 없음. 알 수 없는 값은 today 취급.
fn period_range(period: &str, today: chrono::NaiveDate) -> (Option<String>, String) {
    let d = |n: i64| (today - chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
    let to = d(0);
    let from = match period {
        "week" => Some(d(6)),
        "month" => Some(d(29)),
        "all" => None,
        _ => Some(to.clone()),
    };
    (from, to)
}

pub fn model_mix_inner(store: &SqliteStore, period: &str) -> anyhow::Result<Vec<ModelMixEntry>> {
    let (from, to) = period_range(period, chrono::Local::now().date_naive());
    Ok(store.model_mix_for_range(from.as_deref(), &to)?
        .into_iter().map(|(tier, tokens)| ModelMixEntry { tier, tokens }).collect())
}
```

기존 `get_model_mix` 커맨드(246~252행)를 교체:

```rust
#[tauri::command(async)]
pub fn get_model_mix(state: State<AppState>, period: Option<String>) -> Result<Vec<ModelMixEntry>, String> {
    let guard = lock(&state)?;
    model_mix_inner(&*guard, period.as_deref().unwrap_or("today")).map_err(|e| e.to_string())
}
```

- [ ] **Step 4: 통과 확인 (신규 + app 전체 회귀)**

Run: `cargo test -p agent-mentor-app`
Expected: PASS (신규 2건 포함 전체 녹색)

- [ ] **Step 5: 커밋**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): get_model_mix에 period 파라미터 — rolling 주간/월간·전체 범위"
```

---

### Task 3: 프론트 — api.ts + ModelMix 자체 소유 + HomeTab 정리

**Files:**
- Modify: `src/lib/api.ts` (57~60행 `ModelMixEntry` 인근 + 86행 `getModelMix`)
- Modify: `src/lib/ui/home/ModelMix.svelte` (전면 개정 — 아래 전체 코드)
- Modify: `src/lib/ui/HomeTab.svelte` (mix 로딩 제거)

**Interfaces:**
- Consumes: Task 2의 커맨드 `get_model_mix(period)`, api.ts 기존 `onScanDone(cb): Promise<UnlistenFn>`
- Produces: `export type ModelMixPeriod = 'today' | 'week' | 'month' | 'all'`,
  `getModelMix(period: ModelMixPeriod = 'today')`. `<ModelMix />`는 prop 없음.

- [ ] **Step 1: api.ts 수정** — `ModelMixEntry` 인터페이스(57~60행) 아래에 타입 추가, `getModelMix`(86행) 교체:

```ts
export type ModelMixPeriod = 'today' | 'week' | 'month' | 'all';
```

```ts
export const getModelMix = (period: ModelMixPeriod = 'today') =>
  invoke<ModelMixEntry[]>('get_model_mix', { period });
```

- [ ] **Step 2: ModelMix.svelte 전면 개정** — 파일 전체를 다음으로 교체:

```svelte
<script lang="ts">
  import { getModelMix, onScanDone, type ModelMixEntry, type ModelMixPeriod } from '../../api';

  const PERIODS: { key: ModelMixPeriod; label: string }[] = [
    { key: 'today', label: '오늘' }, { key: 'week', label: '주간' },
    { key: 'month', label: '월간' }, { key: 'all', label: '전체' },
  ];
  let period = $state<ModelMixPeriod>('today');
  let mix = $state<ModelMixEntry[]>([]);

  let seq = 0; // 토글 연타 시 마지막 요청 응답만 반영
  async function load() {
    const my = ++seq;
    const rows = await getModelMix(period).catch(() => [] as ModelMixEntry[]);
    if (my === seq) mix = rows;
  }
  load();
  $effect(() => {
    const sub = onScanDone(() => load());
    return () => { sub.then((u) => u()); };
  });
  function pick(p: ModelMixPeriod) {
    if (p === period) return;
    period = p;
    load();
  }

  const total = $derived(Math.max(mix.reduce((a, m) => a + m.tokens, 0), 1));
  // tier 필드에 raw 모델 id가 담긴다(구 데이터만 family 폴백). 계열별 고정색 + 나머지는 순환 팔레트.
  const FAMILY_COLOR: [string, string][] = [
    ['opus', 'var(--pastel-coral)'], ['sonnet', 'var(--pastel-lav)'],
    ['haiku', 'var(--pastel-mint)'], ['fable', 'var(--pastel-cream)'],
  ];
  const FALLBACK = ['#e3d3ec', '#cfe3d3', '#ecdccf', '#d3d9ec'];
  const color = (model: string, i: number) =>
    FAMILY_COLOR.find(([k]) => model.includes(k))?.[1] ?? FALLBACK[i % FALLBACK.length];
  const label = (model: string) => model.replace(/^claude-/, '');
  const pct = (t: number) => Math.round((t / total) * 100);
</script>

<div class="widget">
  <div class="head">
    <h3>모델 분포</h3>
    <div class="segs" role="group" aria-label="기간 선택">
      {#each PERIODS as p (p.key)}
        <button class:active={period === p.key} onclick={() => pick(p.key)}>{p.label}</button>
      {/each}
    </div>
  </div>
  {#if mix.length === 0}
    <p class="empty">{period === 'today' ? '아직 오늘 기록이 없어요' : '이 기간엔 기록이 없어요'}</p>
  {:else}
    <div class="stack">
      {#each mix as m, i (m.tier)}
        <div class="seg" style:width={`${pct(m.tokens)}%`} style:background={color(m.tier, i)}></div>
      {/each}
    </div>
    <ul class="legend">
      {#each mix as m, i (m.tier)}
        <li>
          <span class="chip" style:background={color(m.tier, i)}></span>
          {label(m.tier)} {pct(m.tokens)}% <small>({m.tokens.toLocaleString()})</small>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  .head { display: flex; align-items: center; justify-content: space-between; margin: 0 0 10px; }
  h3 { margin: 0; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .segs { display: flex; gap: 2px; }
  .segs button {
    border: none; cursor: pointer; font: inherit; font-size: 10px; color: var(--ink-soft);
    background: transparent; border-radius: 999px; padding: 2px 7px;
  }
  .segs button.active { background: var(--pastel-lav); color: var(--ink); }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  .stack { display: flex; height: 14px; border-radius: 999px; overflow: hidden; }
  .legend { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 8px; font-size: 11px; color: var(--ink-soft); }
  .legend li { display: flex; align-items: center; gap: 4px; }
  .chip { width: 10px; height: 10px; border-radius: 3px; display: inline-block; }
</style>
```

(변경점: props 제거→자체 상태·fetch·onScanDone 구독, 헤더 flex + 세그먼트 pill,
빈 상태 기간별 문구. `.stack`의 `.seg` div·legend·색 로직은 기존 그대로. h3의 기존
`margin: 0 0 10px`은 `.head`로 이동.)

- [ ] **Step 3: HomeTab.svelte에서 mix 로딩 제거** — 4개 편집:

import에서 `getModelMix`, `type ModelMixEntry` 제거 (2~5행):

```ts
  import {
    getWeekSummary, listFindings, onScanDone, onScanProgress, runScanNow,
    type CoachFinding, type DayStat, type ScanProgress, type Summary,
  } from '../api';
```

상태 선언에서 `let mix = ...`(19행) 삭제. `load()`(24~31행)를 다음으로 교체:

```ts
  async function load() {
    [days, findings] = await Promise.all([
      getWeekSummary().catch(() => [] as DayStat[]),
      listFindings(false).catch(() => [] as CoachFinding[]),
    ]);
    notices = loadNotices();
  }
```

마크업(62행): `<ModelMix {mix} />` → `<ModelMix />`

- [ ] **Step 4: 프론트 검증**

Run: `npx vitest run`
Expected: 기존 스위트 전체 PASS (신규 vitest 없음 — 컴포넌트 테스트 관례 없음)

Run: `npm run build`
Expected: 빌드 성공, `svelte-check`/vite 에러 0 (스크립트가 check 포함이 아니면 빌드 성공만 확인)

- [ ] **Step 5: 커밋**

```bash
git add src/lib/api.ts src/lib/ui/home/ModelMix.svelte src/lib/ui/HomeTab.svelte
git commit -m "feat(home): 모델 분포 기간 세그먼트(오늘/주간/월간/전체) — ModelMix 자체 fetch로 전환"
```

---

### Task 4: 전체 검증 + PR

**Files:** 없음 (검증·배포만)

- [ ] **Step 1: 백엔드 전체 테스트**

Run: `cargo test -p agent-mentor -p agent-mentor-app`
Expected: 전체 PASS

- [ ] **Step 2: 앱 컴파일 확인**

Run: `cargo build -p agent-mentor-app`
Expected: 빌드 성공 (Tauri 커맨드 시그니처 배선 확인)

- [ ] **Step 3: push + PR 생성**

```bash
git push -u origin feat/model-mix-period   # revocation 에러 시 -c http.schannelCheckRevoke=false
gh pr create --base main --title "feat(home): 모델 분포 기간 선택 — 오늘/주간/월간/전체 세그먼트" --body "..."
```

PR 본문: 스펙 링크(`docs/specs/2026-07-12-model-mix-period-design.md`), 변경 요약(store 범위 집계 /
period 파라미터 / ModelMix 자체 소유), 테스트 결과. 끝에
`🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

- [ ] **Step 4: 육안 판정 안내 (사용자)**

앱 실행 → 홈 위젯에서 토글 4개 클릭: 기간별 수치 변화, 빈 기간 문구("이 기간엔 기록이 없어요"),
스캔 완료 시 현재 선택 기간으로 재로드 확인.
