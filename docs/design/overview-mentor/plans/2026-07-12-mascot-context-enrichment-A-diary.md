# 맥락 인지 마스코트 — 묶음 A(다이어리) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 다이어리 브리프에 그날그날 달라지는 사실(상시 finding 여부·도구 사용량·근무 맥락)을 신호로 넣고 프롬프트를 재조정해, 매일 context7만 반복하던 일기를 그날의 실제 이야기 + 위로/응원으로 바꾼다. 이모지도 상향.

**Architecture:** 변경은 `crates/core/src/diary/mod.rs` 한 파일에 집중. `assemble_brief`가 이미 `store.conn`으로 totals를 직접 조회하는 선례를 따라, 도구·근무 집계도 diary 모듈 내 헬퍼가 `store.conn`으로 조회한다(brief 타입 소유를 diary에 유지, store 공개 API 무증설). recently_covered는 기존 `store.findings_for_date`를 최근 일기 날짜에 대해 호출해 dedup_key 집합으로 계산.

**Tech Stack:** Rust(core diary), rusqlite, cargo test. 프런트 변경 없음.

**Spec:** `docs/specs/2026-07-12-mascot-context-enrichment-design.md`

## Global Constraints

- **매 cargo 명령 전 (Git Bash) 필수:**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- `LONG_WORK_HOURS = 5.0`(하루 세션 지속시간 합 임계). 이모지 지시 verbatim: "문단마다 1~2개".
- `voice_guidance()` 무변경(3표면 공유). 신규 색·프런트 변경 없음.
- 브랜치 `feat/diary-variety-ui-fixes`(PR #22, HEAD 시점 스펙 커밋 a8d5634)에 **이어서**. 테스트·빌드 경고 0. 커밋 태스크 단위.
- 도구 집계·근무 집계 쿼리는 `date(ts,'localtime')` / `date(first_ts,'localtime')` 버킷(기존 rollup 관례 일치).

---

## File Structure

- Modify: `crates/core/src/diary/mod.rs`
  - `Brief`에 `tool_usage: ToolUsage`, `work_context: WorkContext` 필드; `BriefFinding`에 `recently_covered: bool`.
  - 신규 타입 `ToolUsage`, `WorkContext`(둘 다 `Serialize + Default`).
  - 헬퍼 `collect_tool_usage`, `collect_work_context`; `assemble_brief`에서 recently_covered 계산.
  - `build_system_prompt` 지시 재조정 + 기존 테스트 보정.

각 태스크는 이 한 파일만 수정한다.

---

### Task A0: 테스트 헬퍼 추가 (turn_event)

**Files:** Modify `crates/core/src/diary/mod.rs` `mod tests`

기존 `seed_diary`(PR #22 도입) 옆에, 세션 활동 이벤트를 만드는 헬퍼를 추가한다. 이후 태스크의 테스트가 재사용한다.

- [ ] **Step 1: 헬퍼 추가**

`mod tests` 안, `seed_diary` 함수 바로 뒤에 추가:

```rust
    /// 특정 host/project/session/ts의 assistant turn 이벤트 하나(sessions.first_ts/last_ts·rollup 채움용).
    fn turn_event(host: &str, project: &str, session: &str, uuid: &str, ts: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: project.into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }
    }

    /// 특정 host/session/ts의 도구 호출 이벤트(tool_kind/서버/타깃 적재용).
    fn tool_event(host: &str, session: &str, ts: &str, kind: ToolKind, raw: &str, target: Option<&str>) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: "p".into(),
            session_id: session.into(), uuid: None, parent_uuid: None,
            is_sidechain: false, ts: Some(ts.into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::ToolCall {
                kind, raw_name: raw.into(),
                target: target.map(|s| s.to_string()), tool_use_id: None,
            },
        }
    }
```

- [ ] **Step 2: 컴파일 확인(경고 무시 — 아직 미사용)**

Run (빌드 레시피 export 후):
```bash
cargo test -p agent-mentor turn_event 2>&1 | tail -5
```
Expected: 컴파일 성공(테스트 0개 매칭). `dead_code` 경고는 다음 태스크에서 소비하므로 이 태스크 단독 커밋은 하지 않는다 — Task A1과 함께 커밋.

*(A0는 A1의 준비 단계 — 별도 커밋 없이 A1 커밋에 포함)*

---

### Task A1: `recently_covered` — 상시 finding 표시

**Files:** Modify `crates/core/src/diary/mod.rs`

**Interfaces:**
- Produces: `BriefFinding.recently_covered: bool`
- Consumes: 기존 `store.findings_for_date(host, date) -> Result<Vec<Finding>>`, `Finding.dedup_key`

- [ ] **Step 1: 실패하는 테스트 작성**

`mod tests`에 추가:

```rust
    #[test]
    fn assemble_brief_marks_recently_covered_findings() {
        use crate::finding::{Finding, Severity};
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };

        // 07-09·07-10 각각 세션(host 스코프 finding이 두 날 모두 활성이 되도록 별개 세션)
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-09T10:00:00Z"),
            turn_event("Windows", "p", "s2", "u2", "2026-07-10T10:00:00Z"),
        ]).unwrap();
        store.rebuild_rollup().unwrap();

        // 상시(host) finding — 두 날 모두 브리프에 포함됨
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Warn,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "Windows".into(),
            evidence: serde_json::json!({"server":"context7"}),
            est_tokens_saved: 2500, prescription: None,
            dedup_key: "R1|Windows|Windows|context7".into(),
        }, "2026-07-09T10:00:00Z").unwrap();
        // 오늘(s2)만의 세션 스코프 finding — 어제 일기엔 없던 새것
        store.upsert_finding(&Finding {
            rule_id: "R9".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: "s2".into(),
            evidence: serde_json::json!({"web_search":20,"web_fetch":0,"total_requests":20}),
            est_tokens_saved: 40000, prescription: None, dedup_key: "R9|s2".into(),
        }, "2026-07-10T10:00:00Z").unwrap();

        // 어제(07-09) 일기 존재 → recent_diaries에 포함 → 그날 finding(R1)이 "이미 다룸"
        seed_diary(&store, &cfg, "2026-07-09", "어제도 context7 얘기");

        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        let r1 = brief.findings.iter().find(|f| f.rule_id == "R1").unwrap();
        let r9 = brief.findings.iter().find(|f| f.rule_id == "R9").unwrap();
        assert!(r1.recently_covered, "어제 일기 날짜에도 있던 상시 finding → true");
        assert!(!r9.recently_covered, "오늘만의 새 finding → false");
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor recently_covered 2>&1 | tail -15
```
Expected: 컴파일 실패 — `BriefFinding`에 `recently_covered` 없음.

- [ ] **Step 3: 구현**

`BriefFinding`에 필드 추가(struct 정의):

```rust
#[derive(Debug, Clone, Serialize)]
pub struct BriefFinding {
    pub rule_id: String,
    pub severity: String,
    pub evidence: serde_json::Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<serde_json::Value>,
    pub detail: String,
    pub suggested_action: String,
    pub recently_covered: bool,
}
```

`assemble_brief`를 재구성한다. 현재는 `findings`를 totals 직후에 만들지만, recent_diaries 날짜가 필요하므로 **recent_diaries → recent_keys → findings** 순으로 옮긴다. `assemble_brief` 본문을 다음으로 교체:

```rust
    let locale = resolve_locale(cfg);
    let today = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();

    // 직전 며칠 일기(서사 반복 방지) — 먼저 계산해야 recently_covered 판정에 쓸 수 있다.
    let recent_diaries = match today {
        Some(d) => collect_recent_diaries(store, host, d),
        None => Vec::new(),
    };
    // 최근 일기가 있던 날들의 finding dedup_key 집합 — 오늘 finding이 여기 있으면 "이미 다룬 상시 이슈".
    let recent_keys: std::collections::HashSet<String> = recent_diaries
        .iter()
        .filter_map(|rd| store.findings_for_date(host, &rd.date).ok())
        .flatten()
        .map(|f| f.dedup_key)
        .collect();

    let findings = store
        .findings_for_date(host, date)?
        .into_iter()
        .map(|f| {
            let (detail, suggested_action) = finding_advice(&f.rule_id, &f.evidence, f.est_tokens_saved);
            let recently_covered = recent_keys.contains(&f.dedup_key);
            BriefFinding {
                rule_id: f.rule_id,
                severity: f.severity.as_str().to_string(),
                evidence: f.evidence,
                est_tokens_saved: f.est_tokens_saved,
                prescription: f.prescription.map(|p| serde_json::json!({
                    "kind": p.kind, "payload": p.payload
                })),
                detail,
                suggested_action,
                recently_covered,
            }
        })
        .collect();

    let anchor = store
        .earliest_session_ts()?
        .and_then(|ts| local_date_of(&ts));
    let occasions = match today {
        Some(d) => compute_occasions(d, anchor, &locale, cfg.include_dev_days),
        None => Vec::new(),
    };

    Ok(Brief {
        date: date.to_string(),
        host: host.to_string(),
        totals,
        findings,
        occasions,
        recent_diaries,
    })
```

(위 블록은 기존 `let findings = ...` 부터 `Ok(Brief { ... })` 까지 전체를 대체한다. `totals` 계산부는 그대로 위에 남는다.)

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor recently_covered 2>&1 | tail -8
```
Expected: PASS.

- [ ] **Step 5: 커밋** (A0 헬퍼 포함)

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): finding recently_covered — 최근 일기에서 다룬 상시 이슈 표시"
```

---

### Task A2: `tool_usage` — 그날 도구 사용량 신호

**Files:** Modify `crates/core/src/diary/mod.rs`

**Interfaces:**
- Produces: `Brief.tool_usage: ToolUsage`, `pub struct ToolUsage`, `fn collect_tool_usage(&SqliteStore, host, date) -> ToolUsage`

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
    #[test]
    fn assemble_brief_collects_tool_usage() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 07-10: 스킬 2종(brainstorming×2, writing-plans×1), 파일읽기×3, MCP(context7)×1
        store.upsert_events(&[
            tool_event("Windows", "s1", "2026-07-10T10:00:00Z", ToolKind::Skill { name: "brainstorming".into() }, "Skill", Some("superpowers:brainstorming")),
            tool_event("Windows", "s1", "2026-07-10T10:01:00Z", ToolKind::Skill { name: "brainstorming".into() }, "Skill", Some("superpowers:brainstorming")),
            tool_event("Windows", "s1", "2026-07-10T10:02:00Z", ToolKind::Skill { name: "writing-plans".into() }, "Skill", Some("superpowers:writing-plans")),
            tool_event("Windows", "s1", "2026-07-10T10:03:00Z", ToolKind::FileRead, "Read", Some("a.rs")),
            tool_event("Windows", "s1", "2026-07-10T10:04:00Z", ToolKind::FileRead, "Read", Some("b.rs")),
            tool_event("Windows", "s1", "2026-07-10T10:05:00Z", ToolKind::FileRead, "Read", Some("c.rs")),
            tool_event("Windows", "s1", "2026-07-10T10:06:00Z", ToolKind::McpCall { server: "context7".into(), tool: "query".into() }, "mcp__context7__query", None),
        ]).unwrap();

        let brief = assemble_brief(&store, "Windows", "2026-07-10", &cfg).unwrap();
        let tu = &brief.tool_usage;
        assert_eq!(tu.total_calls, 7);
        // by_kind는 count 내림차순 — file_read(3)가 skill(3)와 함께 상위
        assert_eq!(tu.by_kind.iter().find(|(k, _)| k == "skill").unwrap().1, 3);
        assert_eq!(tu.by_kind.iter().find(|(k, _)| k == "file_read").unwrap().1, 3);
        assert_eq!(tu.by_kind.iter().find(|(k, _)| k == "mcp_call").unwrap().1, 1);
        // distinct 스킬 2종·MCP 서버 1종
        assert_eq!(tu.skills.len(), 2);
        assert!(tu.skills.contains(&"superpowers:brainstorming".to_string()));
        assert_eq!(tu.mcp_servers, vec!["context7".to_string()]);
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor collects_tool_usage 2>&1 | tail -15
```
Expected: 컴파일 실패 — `ToolUsage`/`tool_usage` 없음.

- [ ] **Step 3: 구현**

`BriefTotals` 정의 아래에 타입 추가:

```rust
/// 그날 (host,date) 도구 사용 집계 — 일기의 "그날 리듬" 소재.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ToolUsage {
    pub total_calls: u64,
    pub by_kind: Vec<(String, u64)>, // 0 아닌 kind만, count 내림차순
    pub skills: Vec<String>,         // distinct 스킬 타깃, 최대 8
    pub mcp_servers: Vec<String>,    // distinct MCP 서버, 최대 8
}
```

`Brief`에 필드 추가:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    pub date: String,
    pub host: String,
    pub totals: BriefTotals,
    pub findings: Vec<BriefFinding>,
    pub occasions: Vec<Occasion>,
    pub recent_diaries: Vec<RecentDiary>,
    pub tool_usage: ToolUsage,
}
```

`collect_recent_diaries` 근처(assemble_brief 뒤 헬퍼 영역)에 헬퍼 추가:

```rust
/// 그날 (host,date)의 도구 호출을 집계한다. tool_kind별 카운트 + distinct 스킬/서버.
/// 실패(쿼리 오류)는 빈 집계로 처리 — 브리프 조립을 막지 않는다.
fn collect_tool_usage(store: &SqliteStore, host: &str, date: &str) -> ToolUsage {
    let by_kind: Vec<(String, u64)> = store
        .conn
        .prepare(
            "SELECT tool_kind, COUNT(*) FROM events
             WHERE host=?1 AND date(ts,'localtime')=?2 AND tool_kind IS NOT NULL AND tool_kind <> ''
             GROUP BY tool_kind ORDER BY COUNT(*) DESC, tool_kind",
        )
        .and_then(|mut s| {
            let rows = s.query_map(params![host, date], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    let total_calls: u64 = by_kind.iter().map(|(_, n)| *n).sum();

    let distinct = |col: &str, kind: &str| -> Vec<String> {
        store
            .conn
            .prepare(&format!(
                "SELECT {col} FROM events
                 WHERE host=?1 AND date(ts,'localtime')=?2 AND tool_kind=?3 AND {col} IS NOT NULL
                 GROUP BY {col} ORDER BY COUNT(*) DESC, {col} LIMIT 8"
            ))
            .and_then(|mut s| {
                let rows = s.query_map(params![host, date, kind], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap_or_default()
    };
    ToolUsage {
        total_calls,
        by_kind,
        skills: distinct("tool_target", "skill"),
        mcp_servers: distinct("tool_server", "mcp_call"),
    }
}
```

`assemble_brief`의 `Ok(Brief { ... })`에 `tool_usage` 추가(occasions 계산 뒤):

```rust
    let tool_usage = collect_tool_usage(store, host, date);

    Ok(Brief {
        date: date.to_string(),
        host: host.to_string(),
        totals,
        findings,
        occasions,
        recent_diaries,
        tool_usage,
    })
```

수동 `Brief` 리터럴(`generate_diary_writes_md_with_token_footer_and_index` 테스트)에 `tool_usage: ToolUsage::default(),` 추가.

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor collects_tool_usage 2>&1 | tail -8
```
Expected: PASS.

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 브리프 tool_usage — 그날 도구 사용량(kind별·스킬·MCP) 집계"
```

---

### Task A3: `work_context` — 주말·근무시간 신호

**Files:** Modify `crates/core/src/diary/mod.rs`

**Interfaces:**
- Produces: `Brief.work_context: WorkContext`, `pub struct WorkContext`, `const LONG_WORK_HOURS: f64`, `fn collect_work_context(&SqliteStore, host, date, today: NaiveDate) -> WorkContext`

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
    #[test]
    fn assemble_brief_work_context_weekend_and_long_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 2026-07-11 = 토요일. 한 세션이 10:00~15:30 (5.5h)
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-11T10:00:00Z"),
            turn_event("Windows", "p", "s1", "u2", "2026-07-11T15:30:00Z"),
        ]).unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-11", &cfg).unwrap();
        assert!(brief.work_context.is_weekend, "07-11은 토요일");
        assert!((brief.work_context.active_hours - 5.5).abs() < 0.01);
        assert!(brief.work_context.long_work, "5.5h >= 5.0 임계");
    }

    #[test]
    fn assemble_brief_work_context_weekday_short() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        let cfg = DiaryConfig { vault_dir: tmp.path().to_path_buf(), ..DiaryConfig::default() };
        // 2026-07-08 = 수요일. 한 세션 10:00~11:00 (1h)
        store.upsert_events(&[
            turn_event("Windows", "p", "s1", "u1", "2026-07-08T10:00:00Z"),
            turn_event("Windows", "p", "s1", "u2", "2026-07-08T11:00:00Z"),
        ]).unwrap();
        let brief = assemble_brief(&store, "Windows", "2026-07-08", &cfg).unwrap();
        assert!(!brief.work_context.is_weekend);
        assert!(!brief.work_context.long_work);
    }
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor work_context 2>&1 | tail -15
```
Expected: 컴파일 실패 — `WorkContext`/`work_context` 없음.

- [ ] **Step 3: 구현**

`ToolUsage` 정의 아래에 추가:

```rust
/// 근무 맥락 — 위로/응원 트리거 신호.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkContext {
    pub is_weekend: bool,
    pub active_hours: f64, // Σ 세션 지속시간(시간, 소수 1자리)
    pub long_work: bool,   // active_hours >= LONG_WORK_HOURS
}

const LONG_WORK_HOURS: f64 = 5.0;
```

`use chrono::NaiveDate;`는 이미 있음. `Datelike`가 필요하므로 파일 상단 `use chrono::NaiveDate;`를 `use chrono::{Datelike, NaiveDate};`로 변경.

헬퍼 추가(`collect_tool_usage` 뒤):

```rust
/// 근무 맥락: 요일(주말)과 그날 세션 지속시간 합. 지속시간은 세션 first_ts 날짜 기준 버킷.
fn collect_work_context(store: &SqliteStore, host: &str, date: &str, today: NaiveDate) -> WorkContext {
    let hours: f64 = store
        .conn
        .query_row(
            "SELECT COALESCE(SUM((julianday(last_ts)-julianday(first_ts))*24.0), 0.0)
             FROM sessions WHERE host=?1 AND date(first_ts,'localtime')=?2",
            params![host, date],
            |r| r.get::<_, f64>(0),
        )
        .unwrap_or(0.0);
    let active_hours = (hours * 10.0).round() / 10.0; // 소수 1자리
    WorkContext {
        is_weekend: matches!(today.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun),
        active_hours,
        long_work: active_hours >= LONG_WORK_HOURS,
    }
}
```

`assemble_brief`: `today`가 `Some`일 때만 근무맥락 계산(파싱 실패 시 기본값):

```rust
    let tool_usage = collect_tool_usage(store, host, date);
    let work_context = match today {
        Some(d) => collect_work_context(store, host, date, d),
        None => WorkContext::default(),
    };

    Ok(Brief {
        date: date.to_string(),
        host: host.to_string(),
        totals,
        findings,
        occasions,
        recent_diaries,
        tool_usage,
        work_context,
    })
```

`Brief` struct에 `pub work_context: WorkContext,` 추가. 수동 `Brief` 리터럴에 `work_context: WorkContext::default(),` 추가.

- [ ] **Step 4: 테스트 통과 확인**

```bash
cargo test -p agent-mentor work_context 2>&1 | tail -8
```
Expected: 두 테스트 PASS.

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 브리프 work_context — 주말·하루 근무시간 합·장시간 여부"
```

---

### Task A4: 프롬프트 재조정 (반복 억제·도구·위로·이모지)

**Files:** Modify `crates/core/src/diary/mod.rs` `build_system_prompt` + 테스트

- [ ] **Step 1: 실패하는 테스트 작성 + 기존 테스트 보정**

`mod tests`에 신규 테스트 추가:

```rust
    #[test]
    fn system_prompt_directs_context_signals_and_comfort() {
        let p = build_system_prompt(&DiaryConfig::default());
        assert!(p.contains("recently_covered")); // 상시 이슈 억제 지시
        assert!(p.contains("상시 이슈"));
        assert!(p.contains("tool_usage"));        // 도구 텍스처 지시
        assert!(p.contains("work_context"));      // 근무 맥락
        assert!(p.contains("위로"));              // 주말/공휴일/장시간 위로
        assert!(p.contains("쉬엄쉬엄"));          // long_work 챙김
    }
```

기존 `system_prompt_directs_short_length_and_moderate_emoji`의 이모지 단언을 교체:

```rust
        assert!(p.contains("이모지"));    // 이모지 지시
        assert!(p.contains("문단마다 1~2개")); // 사용량 상향(1개 정도 → 1~2개)
```

- [ ] **Step 2: 테스트 실패 확인**

```bash
cargo test -p agent-mentor system_prompt_directs 2>&1 | tail -15
```
Expected: `context_signals_and_comfort` FAIL(마커 없음), `short_length_and_moderate_emoji` FAIL(문단마다 1~2개 없음).

- [ ] **Step 3: 구현**

`build_system_prompt`의 포맷 문자열에서, recent_diaries 문단과 "형식:" 문단 사이에 신호/위로 지시를 삽입하고 이모지 줄을 교체한다. 아래 블록으로 교체:

기존:
```rust
         비어있으면 신경 쓰지 마세요. \
         \
         형식: 일기는 짧게 — 2~3문단, 전체 350자 이내로 쓰세요. \
         그날의 핵심 한두 가지만 골라 쓰고 나머지 사실은 과감히 버리세요. \
         이모지는 문단마다 1개 정도, 감정이 실리는 자연스러운 자리에 넣되 같은 이모지를 반복하지 마세요.",
```

교체:
```rust
         비어있으면 신경 쓰지 마세요. \
         \
         finding 중 `recently_covered`가 true인 것은 요 며칠 일기에서 이미 다룬 상시 이슈입니다 — \
         오늘은 그걸로 시작하지 말고, 정 필요하면 맨 뒤에 한 줄로만 스치세요. \
         `recently_covered`가 false인(새로운) finding을 우선 소재로 삼고, \
         새 코칭거리가 없으면 억지로 지적을 만들지 말고 그날의 흐름을 편하게 적으세요. \
         \
         브리프의 `tool_usage`는 오늘 쓴 도구 집계입니다 — 그날의 리듬을 살리는 데 쓰세요 \
         (예: '오늘은 스킬을 열 번 넘게 불러서 정신없었네', '온종일 파일만 뒤졌다'). \
         \
         `work_context.is_weekend`가 true이거나 `occasions`에 명절·공휴일이 있는데도 일했다면, \
         쉬는 날에도 함께해줘 고맙다는 위로·응원을 한마디 건네세요 \
         (단 발렌타인·파이데이 같은 재미 기념일은 위로 대상이 아니니 상식으로 가려서). \
         `work_context.long_work`가 true면 '오래 붙어 있었네, 무리하지 말고 쉬엄쉬엄' 하고 챙기세요. \
         \
         형식: 일기는 짧게 — 2~3문단, 전체 350자 이내로 쓰세요. \
         그날의 핵심 한두 가지만 골라 쓰고 나머지 사실은 과감히 버리세요. \
         이모지는 문단마다 1~2개, 감정이 실리는 자연스러운 자리에 넣되 같은 이모지를 반복하지 마세요.",
```

- [ ] **Step 4: 테스트 통과 확인(전체 회귀 포함)**

```bash
cargo test -p agent-mentor system_prompt 2>&1 | tail -12
cargo test -p agent-mentor 2>&1 | tail -6
```
Expected: 신규 + 기존 `system_prompt_*` 전부 PASS, 전체 PASS 경고 0.

- [ ] **Step 5: 커밋**

```bash
git add crates/core/src/diary/mod.rs
git commit -m "feat(diary): 프롬프트 재조정 — 상시 finding 억제·도구 텍스처·위로/응원·이모지 상향"
```

---

## 최종 게이트 + 마무리

- [ ] 전체 그린: `cargo test -p agent-mentor && cargo build -p agent-mentor-app` (빌드 레시피 export 후), 경고 0
- [ ] push (`git -c http.schannelCheckRevoke=false push`) — PR #22에 자동 반영
- [ ] (머지 후, 코드 밖) 재생성 — 스펙 §4: 앱 종료 → `diary_index` 창(−7~−1) 삭제(**findings 유지**) → 실엔진 앱 실행 → backfill 재생성 → 육안(날마다 다른 이야기·도구 언급·주말/장시간 위로·이모지↑)

## 후속: 묶음 B(잡담)

별도 브랜치·별도 PR·별도 계획(`docs/plans/2026-07-12-...-B-chatter.md`). 묶음 A 착지 후 작성 — B의 chatter 컨텍스트가 A의 `work_context`/세션수 신호를 재사용하므로 A 확정 후 계획이 정확해진다. 스펙 §4 묶음 B 참조(주말·연속세션≥5 코믹 위로, PR #18 chatter 구조 재사용).
