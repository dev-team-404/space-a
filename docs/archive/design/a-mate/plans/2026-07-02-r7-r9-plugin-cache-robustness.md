---
status: done
archived: 2026-07-19
---

# R7·R9 규칙 + 플러그인 캐시 읽기 견고성 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tier 0 규칙 R7(단순작업에 Opus)·R9(웹 도구 남용)을 추가하고, 인벤토리 reconciliation의 플러그인 캐시 읽기 한계(다중 버전 dir·중첩 `.mcp.json` 부분셋)를 닫는다.

**Architecture:** R7/R9는 기존 `Rule` trait 패턴(struct + `Default` + `evaluate` — 세션 단위 집계 SQL)을 따라 각 파일 하나로 추가하고 `cmd_rules`의 `RuleEngine`에 등록한다. 다이어리 서사는 `diary/mod.rs::finding_advice`에 arm을 추가해 렌더한다. C는 `inventory.rs`의 읽기 함수 3개에 completeness 신호를 전파하고 활성 버전(mtime 최신) 선택을 넣어, `cmd_inventory`가 불완전 호스트를 스킵하도록 한다.

**Tech Stack:** Rust 2021, rusqlite(bundled), serde_json, anyhow. 테스트: 표준 `#[cfg(test)]` + tempfile + (신규) filetime.

## Global Constraints

- **제품/스택:** Tauri v2 + Rust 백엔드 지향이나 **현재는 Tauri 셸 이전의 순수 백엔드 크레이트 단계.** 공개 함수는 나중에 `#[tauri::command]`로 배선 가능하게 유지. Tauri v1 API 금지.
- **플랫폼:** Windows 전용. macOS/Linux 분기 불필요.
- **에이전트 추상화:** 에이전트별 로직 하드코딩 금지 — SourceAdapter/Engine/Rule 인터페이스 뒤로. (R7/R9는 정규화 이벤트만 소비하므로 준수됨.)
- **빌드/테스트 환경:** `cargo` 명령은 **Git Bash에서 GNU 툴체인 + mingw를 PATH에 export한 뒤** 실행한다(비표준 레시피 — build-env 메모리 참조: `CARGO_HTTP_CHECK_REVOKE`, git `schannelCheckRevoke` 포함). 아래 각 스텝의 `cargo ...`는 그 환경에서 실행하는 것을 전제한다.
- **품질 기준:** 현재 `cargo test` 58개 통과·**무경고**. 신규 코드도 경고 0을 유지한다(미사용 변수/import 금지).
- **에러 철학(데이터 파운데이션):** 파싱/읽기 실패는 관대 처리(하드 실패 금지), DB/트랜잭션 오류는 `?`로 전파.
- **R7 v0 이탈(스펙 §1.1 근거):** `events`에 `uuid` 컬럼이 없어 턴↔도구 조인이 불가 → R7은 **세션 단위**로 구현(스펙 §5의 턴 단위 "출력<300 & 단일 trivial 도구"에서 의도적 이탈, 규칙 doc-comment에 명시). R1 v0 이탈과 동일한 관례.
- **est_tokens_saved 통화:** R7은 "비용-등가 토큰"(Opus↔Haiku 5:1 균일 가격비 → Haiku=20% 비용 → SAVINGS_FRACTION=0.8). evidence.note에 "비용-등가"임을 명시.

**참조 스펙:** `docs/specs/2026-07-02-r7-r9-plugin-cache-robustness-design.md`

---

## Task 1: R9 규칙 (웹 도구 남용, 세션 단위)

**Files:**
- Create: `src/rules/r9_web_overuse.rs`
- Modify: `src/rules/mod.rs` (모듈 등록)

**Interfaces:**
- Consumes: `crate::rules::Rule` trait (`fn id(&self)->&'static str`, `fn evaluate(&self, &SqliteStore)->Result<Vec<Finding>>`), `crate::finding::{Finding, Severity}`, `store.conn` (rusqlite `Connection`), `events` 테이블 컬럼 `session_id, host, project_id, kind, web_search, web_fetch`.
- Produces: `pub struct R9WebOveruse { pub threshold: u64, pub heuristic_tokens_per_request: u64 }` + `impl Default` + `impl Rule`. Finding: `rule_id="R9"`, `scope_kind="session"`, `scope_ref=session_id`, `prescription=None`, `dedup_key="R9|{session}"`, evidence `{web_search,web_fetch,total_requests,note}`.

- [ ] **Step 1: 모듈 등록**

`src/rules/mod.rs` 상단의 `pub mod` 목록에 R9를 추가한다.

```rust
pub mod r1_unused_mcp;
pub mod r5_repeated_read;
pub mod r9_web_overuse;
```

- [ ] **Step 2: 실패하는 테스트 작성**

`src/rules/r9_web_overuse.rs`를 생성하고 아래 전체 내용을 쓴다(구현 + 테스트). 이 스텝에서는 아직 컴파일이 안 될 수 있으니 다음 스텝에서 확인한다.

```rust
use crate::finding::{Finding, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

/// R9 — 웹 도구 남용.
/// 세션당 서버측 웹 도구 호출(web_search+web_fetch) 과다를 지목.
/// advice-only(처방 없음): "캐싱/로컬"은 자동 적용 가능한 결정론적 액션이 아니므로
/// 정밀도의 선(코칭 설계 §1.4)상 처방 카드로 승격 부적합. R5처럼 evidence+finding_advice만.
pub struct R9WebOveruse {
    pub threshold: u64,
    pub heuristic_tokens_per_request: u64,
}

impl Default for R9WebOveruse {
    fn default() -> Self {
        R9WebOveruse { threshold: 15, heuristic_tokens_per_request: 2000 }
    }
}

impl Rule for R9WebOveruse {
    fn id(&self) -> &'static str {
        "R9"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id,
                    COALESCE(SUM(web_search),0) AS ws,
                    COALESCE(SUM(web_fetch),0) AS wf
             FROM events
             WHERE kind='assistant_turn'
             GROUP BY session_id
             HAVING ws + wf >= ?1
             ORDER BY ws + wf DESC",
        )?;
        let rows = stmt.query_map(params![self.threshold as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,                              // session
                r.get::<_, Option<String>>(1)?.unwrap_or_default(), // host
                r.get::<_, Option<String>>(2)?.unwrap_or_default(), // project
                r.get::<_, i64>(3)? as u64,                         // web_search
                r.get::<_, i64>(4)? as u64,                         // web_fetch
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session, host, project, ws, wf) = row?;
            let total = ws + wf;
            out.push(Finding {
                rule_id: "R9".into(),
                severity: Severity::Suggest,
                scope_host: Some(host),
                scope_project: Some(project),
                scope_kind: "session".into(),
                scope_ref: session.clone(),
                evidence: serde_json::json!({
                    "web_search": ws, "web_fetch": wf, "total_requests": total,
                    "note": "세션당 서버 웹 도구 호출 과다"
                }),
                est_tokens_saved: total * self.heuristic_tokens_per_request,
                prescription: None,
                dedup_key: format!("R9|{session}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn web_turn(session: &str, uuid: &str, ws: u32, wf: u32) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-opus-4-8"),
                usage: TokenUsage::default(), web_search: ws, web_fetch: wf,
            },
        }
    }

    #[test]
    fn r9_flags_session_over_threshold() {
        let store = SqliteStore::open_in_memory().unwrap();
        // s1: web_search 10 + web_fetch 8 = 18 (>=15)
        store.upsert_events(&[
            web_turn("s1", "u1", 10, 3),
            web_turn("s1", "u2", 0, 5),
        ]).unwrap();
        let findings = R9WebOveruse::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R9");
        assert_eq!(f.scope_ref, "s1");
        assert_eq!(f.scope_kind, "session");
        assert_eq!(f.evidence["total_requests"], 18);
        assert_eq!(f.evidence["web_search"], 10);
        assert_eq!(f.evidence["web_fetch"], 8);
        assert_eq!(f.est_tokens_saved, 18 * 2000);
        assert!(f.prescription.is_none(), "R9는 advice-only");
    }

    #[test]
    fn r9_silent_under_threshold() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[web_turn("s1", "u1", 5, 5)]).unwrap(); // 10 < 15
        assert!(R9WebOveruse::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r9_scoped_per_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        // s1: 8(<15), s2: 18(>=15) → s2만
        store.upsert_events(&[
            web_turn("s1", "u1", 8, 0),
            web_turn("s2", "u3", 18, 0),
        ]).unwrap();
        let findings = R9WebOveruse::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope_ref, "s2");
    }
}
```

- [ ] **Step 3: 테스트 실행(통과 확인)**

Run: `cargo test r9_ -- --nocapture`
Expected: 3 tests pass (`r9_flags_session_over_threshold`, `r9_silent_under_threshold`, `r9_scoped_per_session`).

> 만약 컴파일 에러(예: SQLite가 HAVING/ORDER BY에서 별칭 `ws`/`wf`를 못 씀)가 나면, HAVING/ORDER BY를 전체 식 `COALESCE(SUM(web_search),0)+COALESCE(SUM(web_fetch),0)`로 바꾼다. (SQLite는 별칭 허용이 일반적이나 방어책.)

- [ ] **Step 4: 커밋**

```bash
git add src/rules/mod.rs src/rules/r9_web_overuse.rs
git commit -m "feat: R9 웹 도구 남용 규칙(세션당 server_tool_use ≥15, advice-only)"
```

---

## Task 2: R7 규칙 (단순작업에 Opus, 세션 단위)

**Files:**
- Create: `src/rules/r7_opus_trivial.rs`
- Modify: `src/rules/mod.rs` (모듈 등록)

**Interfaces:**
- Consumes: `crate::rules::Rule`, `crate::finding::{Finding, Prescription, Severity}`, `events` 컬럼 `session_id, host, project_id, kind, model_family, tok_input, tok_output, tok_cache_read, tok_cache_create, web_search, web_fetch, tool_kind`.
- Produces: `pub struct R7OpusTrivial { pub max_output_tokens: u64, pub max_tool_calls: u64, pub savings_fraction_pct: u64 }` + `Default` + `Rule`. Finding: `rule_id="R7"`, `scope_kind="session"`, `prescription=Some(switch_model, {"from":"opus","to":"haiku"})`, `dedup_key="R7|{session}"`, evidence `{model,turns,tok_output,tool_calls,tools,billable_tokens,note}`.

- [ ] **Step 1: 모듈 등록**

`src/rules/mod.rs`에 R7을 추가(알파벳 순 유지).

```rust
pub mod r1_unused_mcp;
pub mod r5_repeated_read;
pub mod r7_opus_trivial;
pub mod r9_web_overuse;
```

- [ ] **Step 2: 구현 + 테스트 작성**

`src/rules/r7_opus_trivial.rs`를 생성하고 아래 전체를 쓴다.

```rust
use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;

/// R7 — 단순 작업에 Opus (세션 단위).
///
/// v0 이탈(스펙 §5는 턴 단위): `events`에 uuid 컬럼이 없어 턴↔도구 조인이 불가하므로
/// 세션 단위로 근사한다. 조건 3개 모두 충족 시 지목:
///   1) Opus 전용(opus turn ≥1, non-opus turn 0)
///   2) 가벼운 출력(SUM(tok_output) < max_output_tokens)
///   3) 사소한 도구만(tool_call 1..=max, 무거운 도구 0, 서버 웹 0)
/// est_tokens_saved는 비용등가(Opus↔Haiku 5:1 균일 가격비 → 20% 비용 → 80% 절감).
pub struct R7OpusTrivial {
    pub max_output_tokens: u64,
    pub max_tool_calls: u64,
    pub savings_fraction_pct: u64,
}

impl Default for R7OpusTrivial {
    fn default() -> Self {
        R7OpusTrivial { max_output_tokens: 700, max_tool_calls: 5, savings_fraction_pct: 80 }
    }
}

impl Rule for R7OpusTrivial {
    fn id(&self) -> &'static str {
        "R7"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT session_id, host, project_id,
                    SUM(CASE WHEN kind='assistant_turn' AND model_family='opus' THEN 1 ELSE 0 END) AS opus_turns,
                    SUM(CASE WHEN kind='assistant_turn' AND model_family IS NOT NULL AND model_family<>'opus' THEN 1 ELSE 0 END) AS non_opus,
                    COALESCE(SUM(tok_output),0) AS out_tok,
                    COALESCE(SUM(tok_input+tok_output+tok_cache_read+tok_cache_create),0) AS billable,
                    COALESCE(SUM(web_search+web_fetch),0) AS web_reqs,
                    SUM(CASE WHEN kind='tool_call' THEN 1 ELSE 0 END) AS tool_calls,
                    SUM(CASE WHEN kind='tool_call' AND tool_kind IN ('sub_agent','mcp_call','web_search','web_fetch') THEN 1 ELSE 0 END) AS heavy,
                    GROUP_CONCAT(DISTINCT CASE WHEN kind='tool_call' THEN tool_kind END) AS tool_kinds
             FROM events
             GROUP BY session_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,                               // session
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),  // host
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),  // project
                r.get::<_, i64>(3)? as u64,                          // opus_turns
                r.get::<_, i64>(4)? as u64,                          // non_opus
                r.get::<_, i64>(5)? as u64,                          // out_tok
                r.get::<_, i64>(6)? as u64,                          // billable
                r.get::<_, i64>(7)? as u64,                          // web_reqs
                r.get::<_, i64>(8)? as u64,                          // tool_calls
                r.get::<_, i64>(9)? as u64,                          // heavy
                r.get::<_, Option<String>>(10)?.unwrap_or_default(), // tool_kinds csv
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (session, host, project, opus_turns, non_opus, out_tok,
                 billable, web_reqs, tool_calls, heavy, tool_kinds_csv) = row?;

            // 1) Opus 전용
            if opus_turns == 0 || non_opus > 0 {
                continue;
            }
            // 2) 가벼운 출력
            if out_tok >= self.max_output_tokens {
                continue;
            }
            // 3) 사소한 도구만
            if tool_calls < 1 || tool_calls > self.max_tool_calls {
                continue;
            }
            if heavy > 0 || web_reqs > 0 {
                continue;
            }

            let mut tools: Vec<String> = tool_kinds_csv
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            tools.sort();

            let est = (billable * self.savings_fraction_pct + 50) / 100;

            out.push(Finding {
                rule_id: "R7".into(),
                severity: Severity::Suggest,
                scope_host: Some(host),
                scope_project: Some(project),
                scope_kind: "session".into(),
                scope_ref: session.clone(),
                evidence: serde_json::json!({
                    "model": "opus",
                    "turns": opus_turns,
                    "tok_output": out_tok,
                    "tool_calls": tool_calls,
                    "tools": tools,
                    "billable_tokens": billable,
                    "note": "비용-등가 추정(Opus↔Haiku 5:1 가격비)"
                }),
                est_tokens_saved: est,
                prescription: Some(Prescription {
                    kind: "switch_model".into(),
                    payload: serde_json::json!({ "from": "opus", "to": "haiku" }),
                }),
                dedup_key: format!("R7|{session}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn turn(session: &str, uuid: &str, model: &str, output: u64, cache_create: u64) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage { output, cache_creation: cache_create, ..Default::default() },
                web_search: 0, web_fetch: 0,
            },
        }
    }

    fn tool(session: &str, uuid: &str, kind: ToolKind, raw: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-jibin".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::ToolCall { kind, raw_name: raw.into(), target: None },
        }
    }

    #[test]
    fn r7_flags_all_opus_trivial_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        // Opus 전용, 출력 400(<700), 상주 60000, 도구 2개(file_read/file_edit)
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 300, 60000),
            turn("s1", "u2", "claude-opus-4-8", 100, 0),
            tool("s1", "u3", ToolKind::FileRead, "Read"),
            tool("s1", "u4", ToolKind::FileEdit, "Edit"),
        ]).unwrap();

        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R7");
        assert_eq!(f.scope_ref, "s1");
        assert_eq!(f.scope_kind, "session");
        assert_eq!(f.prescription.as_ref().unwrap().kind, "switch_model");
        assert_eq!(f.evidence["model"], "opus");
        assert_eq!(f.evidence["tok_output"], 400);
        assert_eq!(f.evidence["tool_calls"], 2);
        assert_eq!(f.evidence["billable_tokens"], 60400);
        // billable 60400 × 0.8 = 48320
        assert_eq!(f.est_tokens_saved, 48320);
        let tools = f.evidence["tools"].as_array().unwrap();
        assert_eq!(tools[0], "file_edit"); // 정렬됨
        assert_eq!(tools[1], "file_read");
    }

    #[test]
    fn r7_silent_when_mixed_model() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 100, 0),
            turn("s1", "u2", "claude-sonnet-4-6", 100, 0),
            tool("s1", "u3", ToolKind::FileRead, "Read"),
        ]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_heavy_tool() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 100, 0),
            tool("s1", "u2", ToolKind::McpCall { server: "ctx".into(), tool: "x".into() }, "mcp__ctx__x"),
        ]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_server_web_used() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut t = turn("s1", "u1", "claude-opus-4-8", 100, 0);
        if let EventKind::AssistantTurn { web_search, .. } = &mut t.kind {
            *web_search = 2;
        }
        store.upsert_events(&[t, tool("s1", "u2", ToolKind::FileRead, "Read")]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_no_tool() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 순수 대화형 Opus 턴(도구 0) → 짧지만 깊은 답변일 수 있어 제외
        store.upsert_events(&[turn("s1", "u1", "claude-opus-4-8", 100, 60000)]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r7_silent_when_output_too_large() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8", 800, 0), // 800 >= 700
            tool("s1", "u2", ToolKind::FileRead, "Read"),
        ]).unwrap();
        assert!(R7OpusTrivial::default().evaluate(&store).unwrap().is_empty());
    }
}
```

- [ ] **Step 3: 테스트 실행(통과 확인)**

Run: `cargo test r7_ -- --nocapture`
Expected: 6 tests pass.

- [ ] **Step 4: 커밋**

```bash
git add src/rules/mod.rs src/rules/r7_opus_trivial.rs
git commit -m "feat: R7 단순작업에 Opus 규칙(세션 단위, 비용등가 est, switch_model 처방)"
```

---

## Task 3: finding_advice R7·R9 arm (다이어리 서사)

**Files:**
- Modify: `src/diary/mod.rs` (`finding_advice` 함수의 match + tests 모듈)

**Interfaces:**
- Consumes: 기존 `pub fn finding_advice(rule_id: &str, evidence: &serde_json::Value, est_tokens_saved: u64) -> (String, String)`. R7 evidence `{tok_output, tool_calls}`, R9 evidence `{total_requests, web_search, web_fetch}`.
- Produces: R7/R9 rule_id에 대해 (detail, suggested_action) 반환.

- [ ] **Step 1: 실패하는 테스트 추가**

`src/diary/mod.rs`의 `mod tests` 안에 두 테스트를 추가한다(기존 `finding_advice_default_arm` 근처).

```rust
    #[test]
    fn finding_advice_r7() {
        let (detail, action) = super::finding_advice(
            "R7",
            &serde_json::json!({"model":"opus","tok_output":420,"tool_calls":2}),
            48320,
        );
        assert!(detail.contains("420"));
        assert!(detail.contains("2회"));
        assert!(detail.contains("48320"));
        assert!(action.contains("Haiku"));
    }

    #[test]
    fn finding_advice_r9() {
        let (detail, action) = super::finding_advice(
            "R9",
            &serde_json::json!({"web_search":12,"web_fetch":6,"total_requests":18}),
            36000,
        );
        assert!(detail.contains("검색 12"));
        assert!(detail.contains("페치 6"));
        assert!(detail.contains("18"));
        assert!(action.contains("캐싱"));
    }
```

- [ ] **Step 2: 테스트 실행(실패 확인)**

Run: `cargo test finding_advice_r7 finding_advice_r9`
Expected: FAIL — 현재 `finding_advice`는 R7/R9를 default arm(`_ =>`)으로 떨어뜨려 `action`이 빈 문자열이고 detail이 evidence 덤프라 assert 실패.

- [ ] **Step 3: R7·R9 arm 구현**

`src/diary/mod.rs::finding_advice`의 `match rule_id {` 안, `_ =>` 직전에 두 arm을 추가한다.

```rust
        "R7" => {
            let out = evidence.get("tok_output").and_then(|v| v.as_u64()).unwrap_or(0);
            let n = evidence.get("tool_calls").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("이 세션은 전부 Opus인데 출력 {out}토큰·도구 {n}회의 가벼운 작업이었어요 (~{est_tokens_saved}토큰 비용-등가)"),
                "이런 잔심부름은 Haiku로 전환하면 같은 결과를 훨씬 싸게 낼 수 있어요".to_string(),
            )
        }
        "R9" => {
            let total = evidence.get("total_requests").and_then(|v| v.as_u64()).unwrap_or(0);
            let s = evidence.get("web_search").and_then(|v| v.as_u64()).unwrap_or(0);
            let fetch = evidence.get("web_fetch").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("이 세션에서 웹 도구를 {total}회 호출했어요 (검색 {s}+페치 {fetch}, ~{est_tokens_saved}토큰)"),
                "반복 조회는 결과를 캐싱하거나 로컬 소스(예: 로컬 문서·context7 캐시)를 쓰면 웹 왕복 토큰을 아껴요".to_string(),
            )
        }
```

- [ ] **Step 4: 테스트 실행(통과 확인)**

Run: `cargo test finding_advice`
Expected: PASS — `finding_advice_r5_and_r1`, `finding_advice_default_arm`, `finding_advice_r7`, `finding_advice_r9` 모두 통과.

- [ ] **Step 5: 커밋**

```bash
git add src/diary/mod.rs
git commit -m "feat: finding_advice R7·R9 arm(다이어리 근거·개선방향 서사)"
```

---

## Task 4: cmd_rules 배선

**Files:**
- Modify: `src/main.rs` (import + `cmd_rules`의 `RuleEngine::new` 벡터)

**Interfaces:**
- Consumes: `R7OpusTrivial`, `R9WebOveruse` (Task 1·2).
- Produces: `agent-mentor rules`가 R5·R1·R7·R9를 모두 실행.

- [ ] **Step 1: import 추가**

`src/main.rs` 상단 use 블록에 두 줄 추가(기존 R1/R5 import 옆).

```rust
use agent_mentor::rules::r7_opus_trivial::R7OpusTrivial;
use agent_mentor::rules::r9_web_overuse::R9WebOveruse;
```

- [ ] **Step 2: RuleEngine 벡터에 등록**

`cmd_rules` 안의 `RuleEngine::new(vec![...])`를 아래로 교체한다.

```rust
    let engine = RuleEngine::new(vec![
        Box::new(R5RepeatedRead::default()),
        Box::new(R1UnusedMcp::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(R9WebOveruse::default()),
    ]);
```

- [ ] **Step 3: 빌드 + 전체 테스트(통과 확인)**

Run: `cargo build && cargo test`
Expected: 빌드 성공(무경고), 전체 테스트 통과(기존 58 + Task1~3 신규 11 = 69).

- [ ] **Step 4: 커밋**

```bash
git add src/main.rs
git commit -m "feat: cmd_rules에 R7·R9 등록"
```

---

## Task 5: C — 플러그인 캐시 completeness 전파 + cmd_inventory 스킵

**Files:**
- Modify: `src/inventory.rs` (`find_plugin_mcp_files`, `plugin_servers`, `resolve_project_servers`, `collect_host_inventory` 시그니처 + `HostInventory` 추가 + 기존/신규 테스트)
- Modify: `src/main.rs` (`cmd_inventory`)

**Interfaces:**
- Consumes: 기존 `parse_claude_json`, `parse_mcp_json`, `ProjectMcpConfig`, `McpServer`, `store.replace_host_inventory(&str, &[(String, Vec<McpServer>)])`.
- Produces:
  - `fn find_plugin_mcp_files(&Path) -> (Vec<PathBuf>, bool)` — (활성 후보 목록, complete). 이 태스크에서는 **전체 버전 읽기 유지**(활성 선택은 Task 6).
  - `pub fn plugin_servers(&Value, &Path) -> (Vec<McpServer>, bool)`
  - `pub fn resolve_project_servers(&ProjectMcpConfig) -> (Vec<McpServer>, bool)`
  - `pub struct HostInventory { pub entries: Vec<(String, Vec<McpServer>)>, pub complete: bool }`
  - `pub fn collect_host_inventory(&Value, &Value, &Path) -> HostInventory`

- [ ] **Step 1: 신규 completeness 테스트 추가**

`src/inventory.rs`의 `mod tests`에 아래 4개 테스트를 추가한다.

```rust
    #[test]
    fn plugin_servers_incomplete_on_corrupt_mcp_json() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let p = cache.join("mp").join("bad").join("1.0.0");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(".mcp.json"), "{not json").unwrap();
        let settings = serde_json::json!({ "enabledPlugins": { "bad@mp": true } });
        let (servers, complete) = plugin_servers(&settings, cache);
        assert!(servers.is_empty());
        assert!(!complete, "손상된 .mcp.json → incomplete");
    }

    #[test]
    fn plugin_servers_complete_when_cache_dir_absent() {
        let dir = tempfile::tempdir().unwrap();
        let settings = serde_json::json!({ "enabledPlugins": { "ghost@mp": true } });
        let (servers, complete) = plugin_servers(&settings, &dir.path().join("nonexistent"));
        assert!(servers.is_empty());
        assert!(complete, "캐시 dir 부재는 정당(complete)");
    }

    #[test]
    fn resolve_project_servers_incomplete_on_corrupt_when_enable_all() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".mcp.json"), "{broken").unwrap();
        let cfg = ProjectMcpConfig {
            key: "k".into(), real_path: dir.path().to_string_lossy().to_string(),
            servers: vec![McpServer { name: "explicit".into(), source: "project".into() }],
            enable_all_project: true, disabled: vec![],
        };
        let (servers, complete) = resolve_project_servers(&cfg);
        assert_eq!(servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["explicit"]);
        assert!(!complete, "enable_all + 손상 .mcp.json → incomplete");
    }

    #[test]
    fn collect_host_inventory_incomplete_when_plugin_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("plugins").join("cache");
        let p = cache.join("mp").join("bad").join("1.0.0");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(".mcp.json"), "{bad").unwrap();
        let claude_json = serde_json::json!({ "projects": { "C:\\proj": { "mcpServers": { "local1": {} } } } });
        let settings = serde_json::json!({ "enabledPlugins": { "bad@mp": true } });
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        assert!(!inv.complete);
        assert!(inv.entries.iter().any(|(k, _)| k == "c--proj"), "부분셋이라도 프로젝트 항목은 수집");
    }
```

- [ ] **Step 2: 테스트 실행(실패 확인)**

Run: `cargo test`
Expected: FAIL(컴파일 에러) — `plugin_servers`/`resolve_project_servers`가 아직 `Vec`을 반환하고 `HostInventory`가 없어 타입 불일치. 이는 다음 스텝에서 시그니처를 바꾸면 해소된다.

- [ ] **Step 3: 시그니처 변경 구현**

`src/inventory.rs`에서 4개 함수와 struct를 아래로 교체한다.

`find_plugin_mcp_files`(read_dir 에러 → incomplete 신호 추가, 선택은 아직 전체):

```rust
/// <marketplace>/<name>/*/.mcp.json 후보. (files, complete).
/// complete=false 는 버전 dir 열거(read_dir) IO 에러일 때만. 부재(NotFound)는 complete.
/// (활성 버전 mtime 선택은 Task 6에서. 이 단계는 전체 반환 유지.)
fn find_plugin_mcp_files(plugin_dir: &Path) -> (Vec<PathBuf>, bool) {
    let entries = match std::fs::read_dir(plugin_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), true),
        Err(_) => return (Vec::new(), false),
    };
    let mut out = Vec::new();
    for v in entries.flatten() {
        let candidate = v.path().join(".mcp.json");
        if candidate.is_file() {
            out.push(candidate);
        }
    }
    (out, true)
}
```

`plugin_servers`:

```rust
/// settings.json 의 enabledPlugins(값 true) → 각 플러그인 캐시의 .mcp.json 서버.
/// 반환: (서버 목록, complete). 활성 .mcp.json 이 존재하나 read/parse 실패면 incomplete.
pub fn plugin_servers(settings: &Value, plugins_cache_dir: &Path) -> (Vec<McpServer>, bool) {
    let mut out: Vec<McpServer> = Vec::new();
    let mut complete = true;
    let Some(plugins) = settings.get("enabledPlugins").and_then(|p| p.as_object()) else {
        return (out, true);
    };
    for (id, enabled) in plugins {
        if enabled.as_bool() != Some(true) {
            continue;
        }
        let mut parts = id.splitn(2, '@');
        let name = parts.next().unwrap_or("");
        let marketplace = parts.next().unwrap_or("");
        if name.is_empty() || marketplace.is_empty() {
            continue;
        }
        let plugin_dir = plugins_cache_dir.join(marketplace).join(name);
        let (mcp_files, dir_complete) = find_plugin_mcp_files(&plugin_dir);
        if !dir_complete {
            complete = false;
        }
        for mcp in mcp_files {
            let Ok(raw) = std::fs::read_to_string(&mcp) else {
                complete = false; // is_file()로 확인된 파일의 읽기 실패 → incomplete
                continue;
            };
            let Ok(json) = serde_json::from_str::<Value>(&raw) else {
                complete = false; // 파싱 실패 → incomplete
                continue;
            };
            for sname in parse_mcp_json(&json) {
                if !out.iter().any(|s| s.name == sname) {
                    out.push(McpServer { name: sname, source: "plugin".into() });
                }
            }
        }
    }
    (out, complete)
}
```

`collect_host_inventory` + `HostInventory`(기존 `collect_host_inventory`를 교체하고 struct 추가):

```rust
/// 한 호스트 인벤토리 + 완전성 신호.
pub struct HostInventory {
    pub entries: Vec<(String, Vec<McpServer>)>,
    pub complete: bool,
}

/// 한 호스트의 전체 인벤토리를 조립. "*" 키 = host-global 플러그인 MCP.
/// complete = 모든 하위(프로젝트 .mcp.json, 플러그인 캐시) 완전성의 AND.
pub fn collect_host_inventory(
    claude_json: &Value,
    settings: &Value,
    plugins_cache_dir: &Path,
) -> HostInventory {
    let mut entries = Vec::new();
    let mut complete = true;
    for cfg in parse_claude_json(claude_json) {
        let (servers, ok) = resolve_project_servers(&cfg);
        if !ok {
            complete = false;
        }
        entries.push((cfg.key.clone(), servers));
    }
    let (plugins, ok) = plugin_servers(settings, plugins_cache_dir);
    if !ok {
        complete = false;
    }
    if !plugins.is_empty() {
        entries.push(("*".to_string(), plugins));
    }
    HostInventory { entries, complete }
}
```

`resolve_project_servers`:

```rust
/// enableAllProjectMcpServers=true 면 프로젝트 .mcp.json 의 모든 서버를 활성으로 합친다.
/// 반환: (서버 목록, complete). 부재(NotFound)는 complete; 존재하나 IO/파싱 실패면 incomplete.
pub fn resolve_project_servers(cfg: &ProjectMcpConfig) -> (Vec<McpServer>, bool) {
    let mut servers = cfg.servers.clone();
    if !cfg.enable_all_project {
        return (servers, true);
    }
    let mcp_path = std::path::Path::new(&cfg.real_path).join(".mcp.json");
    let raw = match std::fs::read_to_string(&mcp_path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (servers, true),
        Err(_) => return (servers, false),
    };
    let json = match serde_json::from_str::<Value>(&raw) {
        Ok(j) => j,
        Err(_) => return (servers, false),
    };
    for name in parse_mcp_json(&json) {
        if !cfg.disabled.contains(&name) && !servers.iter().any(|s| s.name == name) {
            servers.push(McpServer { name, source: "mcpjson".into() });
        }
    }
    (servers, true)
}
```

- [ ] **Step 4: 기존 테스트 destructuring 업데이트**

같은 파일의 기존 테스트 6개를 튜플/struct 반환에 맞게 고친다.

`plugin_servers_reads_enabled_plugins_only` — 마지막 두 줄을:

```rust
        let (servers, complete) = plugin_servers(&settings, cache);
        assert!(complete);
        let mut names: Vec<_> = servers.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["context7", "vercel"]);
        assert!(servers.iter().all(|s| s.source == "plugin"));
```

`plugin_servers_no_enabled_plugins_is_empty`:

```rust
        let dir = tempfile::tempdir().unwrap();
        let (servers, complete) = plugin_servers(&serde_json::json!({}), dir.path());
        assert!(servers.is_empty());
        assert!(complete);
```

`resolve_project_servers_reads_mcp_json_when_enable_all` — 마지막 세 줄:

```rust
        let (servers, complete) = resolve_project_servers(&cfg);
        let mut names: Vec<_> = servers.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["explicit", "local_a"]);
        assert!(complete);
```

`resolve_project_servers_noop_when_not_enable_all`:

```rust
        let (servers, complete) = resolve_project_servers(&cfg);
        let names: Vec<_> = servers.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["only"], "enableAll=false 면 .mcp.json 안 읽음");
        assert!(complete);
```

`collect_host_inventory_merges_project_and_global`:

```rust
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        assert!(inv.complete);
        let proj = inv.entries.iter().find(|(k, _)| k == "c--proj").unwrap();
        assert_eq!(proj.1.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["local1"]);
        let glob = inv.entries.iter().find(|(k, _)| k == "*").unwrap();
        assert_eq!(glob.1.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["context7"]);
```

`collect_host_inventory_omits_global_when_no_plugins`:

```rust
        let inv = collect_host_inventory(
            &serde_json::json!({ "projects": { "C:\\p": {} } }),
            &serde_json::json!({}),
            std::path::Path::new(r"Z:\none"),
        );
        assert!(inv.entries.iter().all(|(k, _)| k != "*"), "플러그인 서버 없으면 '*' 항목 없음");
```

- [ ] **Step 5: main.rs cmd_inventory 업데이트**

`src/main.rs::cmd_inventory`의 `let cache = ...` 이후 3줄을 아래로 교체한다.

```rust
        let cache = hs.claude_root.join("plugins").join("cache");
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        if !inv.complete {
            eprintln!("warn: host {} 중첩 .mcp.json 읽기 실패 — 인벤토리 유지, reconcile 스킵", hs.host);
            continue;
        }
        total_servers += inv.entries.iter().map(|(_, servers)| servers.len()).sum::<usize>();
        // 원자 교체: 이 호스트의 기존 행 삭제 후 현재셋 삽입 → stale 제거.
        store.replace_host_inventory(&hs.host, &inv.entries)?;
```

- [ ] **Step 6: 빌드 + 전체 테스트(통과 확인)**

Run: `cargo build && cargo test`
Expected: 빌드 성공(무경고), 전체 통과(기존/수정 테스트 + 신규 4개).

- [ ] **Step 7: 커밋**

```bash
git add src/inventory.rs src/main.rs
git commit -m "fix: 중첩 .mcp.json completeness 전파 + cmd_inventory 불완전 호스트 스킵 (R1 false-negative 방지)"
```

---

## Task 6: C — 활성 버전(mtime 최신) 선택

**Files:**
- Modify: `Cargo.toml` (`[dev-dependencies]`에 `filetime`)
- Modify: `src/inventory.rs` (`pick_active_version` 추가, `find_plugin_mcp_files` 활성 선택)

**Interfaces:**
- Consumes: Task 5의 `find_plugin_mcp_files(&Path) -> (Vec<PathBuf>, bool)`.
- Produces: `fn pick_active_version(Vec<(PathBuf, std::time::SystemTime)>) -> Option<PathBuf>` (최대 mtime). `find_plugin_mcp_files`는 정상 시 활성 1개만, mtime 전부 실패 시 전체(폴백) 반환.

- [ ] **Step 1: filetime dev-dependency 추가**

`Cargo.toml`의 `[dev-dependencies]`에 한 줄 추가.

```toml
[dev-dependencies]
tempfile = "3"
filetime = "0.2"
```

- [ ] **Step 2: pick_active_version 테스트 작성(실패 확인)**

`src/inventory.rs`의 `mod tests`에 추가.

```rust
    #[test]
    fn pick_active_version_picks_latest_mtime() {
        use std::time::{Duration, UNIX_EPOCH};
        let older = UNIX_EPOCH + Duration::from_secs(1000);
        let newer = UNIX_EPOCH + Duration::from_secs(2000);
        let chosen = pick_active_version(vec![
            (PathBuf::from("/a/old/.mcp.json"), older),
            (PathBuf::from("/a/new/.mcp.json"), newer),
        ]);
        assert_eq!(chosen, Some(PathBuf::from("/a/new/.mcp.json")));
        assert_eq!(pick_active_version(vec![]), None);
    }
```

Run: `cargo test pick_active_version`
Expected: FAIL(컴파일) — `pick_active_version` 미정의.

- [ ] **Step 3: pick_active_version 구현(통과 확인)**

`src/inventory.rs`의 `find_plugin_mcp_files` 근처에 추가.

```rust
/// 버전 후보 (.mcp.json 경로, 버전 dir mtime) 중 mtime 최신 하나를 고른다.
fn pick_active_version(
    candidates: Vec<(PathBuf, std::time::SystemTime)>,
) -> Option<PathBuf> {
    candidates.into_iter().max_by_key(|(_, t)| *t).map(|(p, _)| p)
}
```

Run: `cargo test pick_active_version`
Expected: PASS.

- [ ] **Step 4: find_plugin_mcp_files 활성 선택 테스트 작성(실패 확인)**

`src/inventory.rs`의 `mod tests`에 추가.

```rust
    #[test]
    fn find_plugin_mcp_files_reads_only_active_version() {
        use filetime::{set_file_mtime, FileTime};
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path();
        let old = plugin.join("0.1.0");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join(".mcp.json"), r#"{"old_server":{"command":"x"}}"#).unwrap();
        let new = plugin.join("0.2.0");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join(".mcp.json"), r#"{"new_server":{"command":"x"}}"#).unwrap();
        // 버전 dir mtime 명시: old < new
        set_file_mtime(&old, FileTime::from_unix_time(1000, 0)).unwrap();
        set_file_mtime(&new, FileTime::from_unix_time(2000, 0)).unwrap();

        let (files, complete) = find_plugin_mcp_files(plugin);
        assert!(complete);
        assert_eq!(files.len(), 1, "활성(최신 mtime) 버전 하나만");
        assert!(files[0].starts_with(&new));
    }
```

Run: `cargo test find_plugin_mcp_files_reads_only_active_version`
Expected: FAIL — 현재(Task 5) find_plugin_mcp_files는 전체 반환 → `files.len()==2`.

- [ ] **Step 5: find_plugin_mcp_files 활성 선택 구현(통과 확인)**

Task 5의 `find_plugin_mcp_files`를 아래로 교체한다.

```rust
/// <marketplace>/<name>/*/.mcp.json 중 **활성 버전(mtime 최신)** 하나. (files, complete).
/// complete=false 는 버전 dir 열거(read_dir) IO 에러일 때만; 부재(NotFound)는 complete.
/// mtime을 하나도 못 얻으면 폴백으로 전체 반환(안전한 상위집합, 극히 드묾).
fn find_plugin_mcp_files(plugin_dir: &Path) -> (Vec<PathBuf>, bool) {
    let entries = match std::fs::read_dir(plugin_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), true),
        Err(_) => return (Vec::new(), false),
    };
    let mut with_mtime: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    let mut all: Vec<PathBuf> = Vec::new();
    for v in entries.flatten() {
        let mcp = v.path().join(".mcp.json");
        if !mcp.is_file() {
            continue;
        }
        all.push(mcp.clone());
        if let Ok(mtime) = v.path().metadata().and_then(|m| m.modified()) {
            with_mtime.push((mcp, mtime));
        }
    }
    if all.is_empty() {
        return (Vec::new(), true);
    }
    match pick_active_version(with_mtime) {
        Some(active) => (vec![active], true),
        None => (all, true), // mtime 전부 실패 폴백
    }
}
```

Run: `cargo test find_plugin_mcp_files_reads_only_active_version`
Expected: PASS.

- [ ] **Step 6: 전체 테스트 + 무경고 확인**

Run: `cargo test && cargo build`
Expected: 전체 통과, 빌드 무경고. (Task 5의 `plugin_servers_reads_enabled_plugins_only`는 단일 버전 dir이라 활성 선택 후에도 각 1개 → 값 불변, 계속 통과.)

- [ ] **Step 7: 커밋**

```bash
git add Cargo.toml src/inventory.rs
git commit -m "fix: find_plugin_mcp_files 다중 버전 중 mtime 최신(활성) 버전만 읽기 (R1 오탐 방지)"
```

---

## 최종 검증

- [ ] `cargo test` 전체 통과(기존 58 + R9 3 + R7 6 + finding_advice 2 + C completeness 4 + C mtime 2 ≈ **75개**), 무경고.
- [ ] (선택, 실 히스토리) `.env.ref` 실엔진으로 `agent-mentor rules` → R7/R9가 참으로 뜨는지, `agent-mentor diary <date>` 서사에 R7/R9 근거·개선방향이 반영되는지 확인(LLM_*→AGENT_MENTOR_ENGINE_*, host.docker.internal→localhost).
- [ ] (선택, 스모크) 설정의 미사용 서버 제거/플러그인 버전 dir 조작 후 `agent-mentor inventory` 재실행 → 불완전 호스트 스킵·활성 버전만 반영 확인.

---

## Self-Review 결과 (작성 시점)

- **스펙 커버리지:** §2.1 R7(Task 2), §2.2 R9(Task 1), §2.3 finding_advice(Task 3), §2.4 배선(Task 4), §3.1 활성버전(Task 6), §3.2 completeness(Task 5), §5 테스트(각 태스크 내), §6 non-goals(범위 밖 유지). 전부 매핑됨.
- **타입 일관성:** `HostInventory{entries,complete}`, `(Vec<McpServer>,bool)`, `(Vec<PathBuf>,bool)`, `pick_active_version(Vec<(PathBuf,SystemTime)>)->Option<PathBuf>` — Task 5·6 간 일치. `find_plugin_mcp_files`는 Task 5(전체)→Task 6(활성)로 내부만 바뀌고 시그니처 동일.
- **플레이스홀더:** 없음(모든 스텝에 실제 코드/명령/기대출력).
