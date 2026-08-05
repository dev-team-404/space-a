---
status: done
archived: 2026-07-22
---

# B: 모델 적정화 (R7 v3) 구현 계획 — PR1

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** R7을 통계 발화에서 "세션 후보 채굴 → LLM 판정 → 프로젝트 롤업 카드"로 전환하고, 그 과정에서 R6 전용 판정 패스를 재사용 가능한 `CoachingJudge` 인프라로 일반화한다. R10·R11을 은퇴시킨다.

**Architecture:** R7 `Rule::evaluate()`가 Opus main-chain 세션마다 pending 후보 finding을 채굴한다. 일반화된 판정 드라이버 `run_coaching_judgments`가 pending 후보를 배치로 LLM 판정하고(락 밖), verdict를 findings 행에 캐시한다. B의 `rollup`이 confirmed 세션을 (host,project)별로 집계해 프로젝트 카드를 노출한다. R6은 같은 트레이트의 첫 소비자로 편입돼 추상화를 검증한다.

**Tech Stack:** Rust (Cargo workspace `agent_mentor` 코어 + `agent-mentor-app` Tauri 셸), rusqlite(SQLite), serde_json, anyhow. 테스트 = `cargo test` (in-memory SqliteStore + MockEngine).

## Global Constraints

- **플랫폼**: Windows 전용. 빌드·테스트는 네이티브 Windows PowerShell (`cargo test`), WSL 금지.
- **증거 경계**: 판정·집계는 `is_sidechain=0`(main-chain)만. 사용자 프롬프트 전문 + 객관 사실만 근거. 에이전트/서브에이전트 산출물·모델선택 채점 금지.
- **Fail-safe**: 엔진 미설정 → 판정 no-op(pending 잔류). 전송 실패 → attempts 미증가·pending 잔류. Malformed → attempts++, 3회면 rejected.
- **정밀도 우선**: 판정 프롬프트는 확신 없으면 부정(over_modeled=false).
- **근거 없는 수치 금지**: `est_tokens_saved=0` 관행 유지.
- **락 규율**: 엔진 해석·재료 수집·verdict 저장은 짧은 락, LLM 네트워크 I/O는 락 밖.
- **은퇴 관행**: 룰 제거 = 등록 해제 + findings purge + **코드·테스트 보존**.
- 커밋 메시지 영어 Conventional Commits. 각 커밋 끝에 `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

## Task 1: `collect_session_stats` main-chain 필터

**Files:**
- Modify: `a-mate/crates/core/src/rules/session_stats.rs` (`collect_session_stats` 쿼리)
- Test: 같은 파일 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `collect_session_stats(store) -> Result<Vec<SessionStat>>` — 이제 `is_sidechain=1` 이벤트를 집계에서 제외 (턴·토큰·도구 카운트 모두 main-chain only). 반환 타입 무변경.

- [ ] **Step 1: 실패 테스트 작성** — sidechain 턴이 집계에서 빠지는지

`session_stats.rs`의 `mod tests`에 추가:
```rust
#[test]
fn collect_stats_excludes_sidechain_turns() {
    let store = SqliteStore::open_in_memory().unwrap();
    // main-chain 턴 1개(출력 300) + sidechain 턴 1개(출력 9000)
    let mut main = turn_at("s1", "u1", "claude-opus-4-8", 300, "2026-07-06T09:00:00Z");
    let mut side = turn_at("s1", "u2", "claude-opus-4-8", 9000, "2026-07-06T09:05:00Z");
    side.is_sidechain = true;
    store.upsert_events(&[main, side]).unwrap();
    let stats = collect_session_stats(&store).unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].assistant_turns, 1, "sidechain 턴 제외");
    assert_eq!(stats[0].tok_output, 300, "sidechain 토큰 제외");
}
```
(참고: `turn_at`는 `mut` 바인딩 후 `is_sidechain` 설정. 기존 `turn_at`는 `is_sidechain:false`로 생성하므로 `let mut` 필요.)

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent_mentor collect_stats_excludes_sidechain -- --nocapture`
Expected: FAIL — `assistant_turns == 2`, `tok_output == 9300`.

- [ ] **Step 3: 쿼리에 필터 추가**

`collect_session_stats`의 SQL에서 `FROM events` 다음 줄에 WHERE를 추가하고 `GROUP BY`는 유지:
```rust
         FROM events
         WHERE is_sidechain = 0
         GROUP BY session_id",
```
(기존 쿼리는 `FROM events\n         GROUP BY session_id"` — 그 사이에 `WHERE is_sidechain = 0` 삽입.)

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p agent_mentor rules::session_stats -- --nocapture`
Expected: PASS (신규 + 기존 `collect_stats_aggregates_per_session` 등 전부).

- [ ] **Step 5: 커밋**
```bash
git add a-mate/crates/core/src/rules/session_stats.rs
git commit -m "fix(agent): exclude sidechain turns from session stats

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: 판정 패스 일반화 — `CoachingJudge` 트레이트 + 범용 store 조회

**Files:**
- Modify: `a-mate/crates/core/src/judge.rs` (트레이트·범용 파싱 추가, R6 impl)
- Modify: `a-mate/crates/core/src/store.rs` (`pending_for_judgment(rule_id, cap)` 추가)
- Test: 두 파일의 `mod tests`

**Interfaces:**
- Produces:
  - `struct PendingCandidate { dedup_key: String, scope_host: String, scope_project: Option<String>, evidence: serde_json::Value, prev_attempts: u32 }`
  - `trait CoachingJudge { fn rule_id(&self) -> &'static str; fn build_prompt(&self, store: &SqliteStore, c: &PendingCandidate) -> Result<(String, String)>; fn classify(&self, verdict: &serde_json::Value) -> &'static str; fn rollup(&self, store: &SqliteStore) -> Result<Vec<String>>; }`
  - `fn extract_verdict_json(text: &str) -> Result<serde_json::Value>` — 관대한 첫`{`~마지막`}` 추출.
  - `struct R6Judge;` impl CoachingJudge.
  - `SqliteStore::pending_for_judgment(&self, rule_id: &str, limit: usize) -> Result<Vec<PendingCandidate>>`
- Consumes: 기존 `judgment_prompt`, `gather_context`(skill_draft), `SqliteStore::set_judgment`.

- [ ] **Step 1: `extract_verdict_json` 실패 테스트** (judge.rs)
```rust
#[test]
fn extract_verdict_json_tolerates_prose_and_fence() {
    let v = extract_verdict_json("결과:\n```json\n{\"over_modeled\":true,\"reason\":\"x\"}\n```").unwrap();
    assert_eq!(v["over_modeled"], serde_json::json!(true));
    assert!(extract_verdict_json("판정 불가").is_err());
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent_mentor extract_verdict_json -- --nocapture`
Expected: FAIL — `extract_verdict_json` 미정의.

- [ ] **Step 3: `extract_verdict_json` 구현** (judge.rs — 기존 `parse_judgment`의 경계 추출을 범용화)
```rust
/// 엔진 응답에서 JSON 객체를 관대히 추출(첫 '{'~마지막 '}'). 코드펜스·사족 허용.
pub fn extract_verdict_json(text: &str) -> anyhow::Result<serde_json::Value> {
    let start = text.find('{').ok_or_else(|| anyhow!("응답에 JSON 객체 없음"))?;
    let end = text.rfind('}').ok_or_else(|| anyhow!("응답에 JSON 객체 없음"))?;
    if end < start {
        return Err(anyhow!("JSON 경계 불량"));
    }
    Ok(serde_json::from_str(&text[start..=end])?)
}
```
그리고 기존 `parse_judgment`를 이걸로 재구현(DRY):
```rust
pub fn parse_judgment(text: &str) -> Result<Judgment> {
    let v = extract_verdict_json(text)?;
    Ok(serde_json::from_value(v)?)
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent_mentor judge:: -- --nocapture`
Expected: PASS (신규 + 기존 `parse_*` 전부).

- [ ] **Step 5: `pending_for_judgment` 실패 테스트** (store.rs mod tests)
```rust
#[test]
fn pending_for_judgment_returns_generic_candidates() {
    let store = SqliteStore::open_in_memory().unwrap();
    // pending R7 세션 후보 2개 시드 (evidence에 session_id)
    let mk = |key: &str, sid: &str| crate::finding::Finding {
        rule_id: "R7".into(), severity: crate::finding::Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: Some("p".into()),
        scope_kind: "session".into(), scope_ref: sid.into(),
        evidence: serde_json::json!({"session_id": sid}),
        est_tokens_saved: 0, prescription: None, dedup_key: key.into(),
    };
    store.upsert_finding(&mk("R7|sess|Windows|s1", "s1"), "2026-07-06T10:00:00Z").unwrap();
    store.upsert_finding(&mk("R7|sess|Windows|s2", "s2"), "2026-07-06T11:00:00Z").unwrap();
    let batch = store.pending_for_judgment("R7", 10).unwrap();
    assert_eq!(batch.len(), 2);
    assert!(batch.iter().all(|c| c.prev_attempts == 0));
    assert!(batch.iter().any(|c| c.evidence["session_id"] == "s2"));
    // R6 후보는 안 섞임
    assert!(store.pending_for_judgment("R6", 10).unwrap().is_empty());
}
```
(전제: Task 3에서 `upsert_finding`이 `(R7,session)`을 pending으로 넣도록 일반화됨. **Task 순서상 Task 3을 먼저 하거나**, 이 테스트는 Task 3 완료 후 통과. 실행 순서: Task 3 → Task 2 Step 5~8. 아래 Step 6 참조.)

- [ ] **Step 6: `pending_for_judgment` 구현** (store.rs — `pending_r6_for_judgment` 옆에)
```rust
/// 범용 판정 배치 대상 — 주어진 rule_id의 pending & 시도 3회 미만, 최근 활동 순 상한.
pub fn pending_for_judgment(&self, rule_id: &str, limit: usize) -> Result<Vec<crate::judge::PendingCandidate>> {
    let mut stmt = self.conn.prepare(
        "SELECT dedup_key, COALESCE(scope_host,''), scope_project, evidence_json,
                COALESCE(json_extract(judgment_json,'$.attempts'), 0)
         FROM findings
         WHERE rule_id=?1 AND status='pending'
           AND COALESCE(json_extract(judgment_json,'$.attempts'), 0) < 3
         ORDER BY last_seen DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![rule_id, limit as i64], |r| {
        let ev: String = r.get(3)?;
        Ok(crate::judge::PendingCandidate {
            dedup_key: r.get(0)?,
            scope_host: r.get(1)?,
            scope_project: r.get::<_, Option<String>>(2)?,
            evidence: serde_json::from_str(&ev).unwrap_or(serde_json::Value::Null),
            prev_attempts: r.get::<_, i64>(4)? as u32,
        })
    })?;
    rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
}
```

- [ ] **Step 7: `CoachingJudge` 트레이트 + `R6Judge` 구현** (judge.rs)
```rust
use crate::store::SqliteStore;

pub struct PendingCandidate {
    pub dedup_key: String,
    pub scope_host: String,
    pub scope_project: Option<String>,
    pub evidence: serde_json::Value,
    pub prev_attempts: u32,
}

/// 결정론 채굴(findings pending) + LLM 판정 + 롤업. 각 아이템이 구현.
pub trait CoachingJudge {
    fn rule_id(&self) -> &'static str;
    /// 후보 1건의 (system, user) 판정 프롬프트. 재료 수집 실패는 Err(영구 실패).
    fn build_prompt(&self, store: &SqliteStore, c: &PendingCandidate) -> Result<(String, String)>;
    /// 파싱된 verdict → 상태 전이("new"|"confirmed"|"rejected").
    fn classify(&self, verdict: &serde_json::Value) -> &'static str;
    /// verdict 저장 후 노출 finding (재)구성. 반환 = 새로 노출된 dedup_key.
    fn rollup(&self, store: &SqliteStore) -> Result<Vec<String>>;
}

pub struct R6Judge;
impl CoachingJudge for R6Judge {
    fn rule_id(&self) -> &'static str { "R6" }
    fn build_prompt(&self, store: &SqliteStore, c: &PendingCandidate) -> Result<(String, String)> {
        let rep = c.evidence.get("repeated_prompt").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("R6 evidence에 repeated_prompt 없음"))?;
        let ctx = crate::skill_draft::gather_context(store, &c.scope_host, rep)?;
        Ok(judgment_prompt(&ctx))
    }
    fn classify(&self, verdict: &serde_json::Value) -> &'static str {
        if verdict.get("worthy").and_then(|v| v.as_bool()).unwrap_or(false) { "new" } else { "rejected" }
    }
    fn rollup(&self, _store: &SqliteStore) -> Result<Vec<String>> { Ok(vec![]) } // classify→"new"가 곧 노출, 롤업 없음
}
```

- [ ] **Step 8: R6Judge 단위 테스트 + Task 2 테스트 통과** (judge.rs)
```rust
#[test]
fn r6_judge_classifies_worthy_and_unworthy() {
    let j = R6Judge;
    assert_eq!(j.classify(&serde_json::json!({"worthy": true})), "new");
    assert_eq!(j.classify(&serde_json::json!({"worthy": false})), "rejected");
    assert_eq!(j.classify(&serde_json::json!({})), "rejected"); // 필드 없음 = 보수적 rejected
    assert_eq!(j.rule_id(), "R6");
}
```
Run: `cargo test -p agent_mentor judge:: && cargo test -p agent_mentor pending_for_judgment`
Expected: PASS (단, `pending_for_judgment` 테스트는 Task 3 완료 후 — Step 5 주석 참조).

- [ ] **Step 9: 커밋**
```bash
git add a-mate/crates/core/src/judge.rs a-mate/crates/core/src/store.rs
git commit -m "feat(agent): generalize LLM judgment into CoachingJudge trait

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `upsert_finding` init_status 일반화 + `confirmed` 상태

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` (`upsert_finding` init_status)
- Test: `store.rs` mod tests

**Interfaces:**
- Produces: `upsert_finding`이 `(rule_id="R6", *)` 및 `(rule_id="R7", scope_kind="session")` 신규 행을 `status='pending'`으로 넣는다. 그 외는 `'new'`. ON CONFLICT status 불변(기존과 동일).

- [ ] **Step 1: 실패 테스트** (store.rs mod tests)
```rust
#[test]
fn r7_session_finding_starts_pending_project_starts_new() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mk = |kind: &str, key: &str| crate::finding::Finding {
        rule_id: "R7".into(), severity: crate::finding::Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: Some("p".into()),
        scope_kind: kind.into(), scope_ref: "r".into(),
        evidence: serde_json::json!({}), est_tokens_saved: 0,
        prescription: None, dedup_key: key.into(),
    };
    store.upsert_finding(&mk("session", "R7|sess|Windows|s1"), "2026-07-06T10:00:00Z").unwrap();
    store.upsert_finding(&mk("project", "R7|Windows|p"), "2026-07-06T10:00:00Z").unwrap();
    let status = |key: &str| -> String {
        store.conn.query_row("SELECT status FROM findings WHERE dedup_key=?1", [key], |r| r.get(0)).unwrap()
    };
    assert_eq!(status("R7|sess|Windows|s1"), "pending", "세션 후보는 판정 전 비노출");
    assert_eq!(status("R7|Windows|p"), "new", "프로젝트 롤업 카드는 즉시 노출");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent_mentor r7_session_finding_starts_pending -- --nocapture`
Expected: FAIL — 세션 후보가 'new'로 들어감(현재 R7은 'new').

- [ ] **Step 3: init_status 일반화** — `upsert_finding`의 기존 줄 교체

기존:
```rust
        let init_status = if f.rule_id == "R6" { "pending" } else { "new" };
```
교체:
```rust
        // 판정 패스를 거치는 후보는 비노출(pending)로 시작 — R6 패턴, R7 세션 후보.
        // R7 프로젝트 카드는 롤업이 만드는 노출물이므로 'new'.
        let init_status = match (f.rule_id.as_str(), f.scope_kind.as_str()) {
            ("R6", _) => "pending",
            ("R7", "session") => "pending",
            _ => "new",
        };
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent_mentor store:: -- --nocapture`
Expected: PASS (신규 + 기존 upsert 테스트 전부). 이제 Task 2 Step 5의 `pending_for_judgment` 테스트도 통과.

- [ ] **Step 5: 커밋**
```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): start R7 session-candidate findings as pending

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: R7 `evaluate()` → 세션 후보 채굴

**Files:**
- Modify: `a-mate/crates/core/src/rules/r7_opus_trivial.rs` (`evaluate` 전면 교체)
- Test: 같은 파일 mod tests (기존 통계 테스트 대체)

**Interfaces:**
- Produces: `R7OpusTrivial::evaluate(store)`가 관찰창(14일) 내 **main-chain Opus 세션마다** scope_kind `session`·dedup `R7|sess|{host}|{session_id}`·evidence `{session_id, host, project, opus_turns, tok_output, tool_calls, first_ts}` finding을 반환. 제외: sub_agent 쓴 세션(`heavy_tools`엔 sub_agent 포함) · 버스트(`detect_bursts`) · main-chain 턴 0.
- Consumes: `collect_session_stats`(Task 1로 main-chain), `detect_bursts`.

- [ ] **Step 1: 실패 테스트** — 후보 채굴 (r7_opus_trivial.rs mod tests, 기존 통계 테스트는 삭제)

기존 `r7v2_*` 테스트 6개를 삭제하고 아래로 대체:
```rust
#[test]
fn r7_mines_session_candidates_for_main_chain_opus() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut evs = Vec::new();
    // Opus 세션 2개(1시간 간격 → 버스트 아님)
    evs.extend(light_opus_session("s1", "2026-07-06T09:00:00Z"));
    evs.extend(light_opus_session("s2", "2026-07-06T11:00:00Z"));
    store.upsert_events(&evs).unwrap();
    let f = R7OpusTrivial::default().evaluate(&store).unwrap();
    assert_eq!(f.len(), 2, "Opus 세션마다 후보 1건");
    assert!(f.iter().all(|x| x.scope_kind == "session"));
    assert!(f.iter().any(|x| x.dedup_key == "R7|sess|Windows|s1"));
    assert_eq!(f[0].est_tokens_saved, 0);
    assert!(f.iter().all(|x| x.evidence.get("session_id").is_some()));
}

#[test]
fn r7_excludes_subagent_and_burst_sessions() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut evs = Vec::new();
    // sub_agent 쓴 Opus 세션 → 제외
    evs.push(turn_at("sa", "sa-u", "claude-opus-4-8", 300, "2026-07-06T09:00:00Z"));
    evs.push(tool_at("sa", "sa-t", ToolKind::SubAgent, "Task", None, "2026-07-06T09:01:00Z"));
    // 비Opus 세션 → 제외 (opus_turns 0)
    evs.push(turn_at("hk", "hk-u", "claude-haiku-4-5", 100, "2026-07-06T12:00:00Z"));
    store.upsert_events(&evs).unwrap();
    let f = R7OpusTrivial::default().evaluate(&store).unwrap();
    assert!(f.iter().all(|x| x.evidence["session_id"] != "sa"), "sub_agent 세션 제외");
    assert!(f.iter().all(|x| x.evidence["session_id"] != "hk"), "비Opus 세션 제외");
}
```
(참고: `ToolKind::SubAgent` 실제 배리언트명은 `model.rs`에서 확인 — `sub_agent`로 매핑되는 배리언트. `heavy_tools`가 세는 `tool_kind IN ('sub_agent',...)`와 일치시킬 것.)

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent_mentor r7_mines_session_candidates -- --nocapture`
Expected: FAIL — 기존 evaluate는 프로젝트 집계 반환.

- [ ] **Step 3: `evaluate` 교체** (r7_opus_trivial.rs — 구조체·`Default`는 유지하되 필드 중 통계 문턱은 제거 가능, `evaluate` 본문 교체)
```rust
fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339();
    let stats = collect_session_stats(store)?; // Task 1: main-chain only
    let burst_ids: HashSet<String> = detect_bursts(&stats)
        .into_iter()
        .flat_map(|g| g.sessions.into_iter().map(|s| s.session_id))
        .collect();

    let mut out = Vec::new();
    for s in &stats {
        // 후보 조건: main-chain Opus 사용 + subagent/버스트 아님 + 관찰창 내
        if s.opus_turns == 0 { continue; }
        if s.heavy_tools > 0 { continue; }          // sub_agent 등 무거운 도구 = 실질 작업
        if burst_ids.contains(&s.session_id) { continue; }
        match s.first_ts.as_deref() {
            Some(ts) if ts >= cutoff.as_str() => {}
            _ => continue,                            // 관찰창 밖 or ts 없음
        }
        out.push(Finding {
            rule_id: "R7".into(),
            severity: Severity::Suggest,
            scope_host: Some(s.host.clone()),
            scope_project: Some(s.project_id.clone()),
            scope_kind: "session".into(),
            scope_ref: s.session_id.clone(),
            evidence: serde_json::json!({
                "session_id": s.session_id,
                "host": s.host,
                "project": s.project_id,
                "opus_turns": s.opus_turns,
                "tok_output": s.tok_output,
                "tool_calls": s.tool_calls,
                "first_ts": s.first_ts,
            }),
            est_tokens_saved: 0,
            prescription: None, // 처방은 프로젝트 롤업 카드에만
            dedup_key: format!("R7|sess|{}|{}", s.host, s.session_id),
        });
    }
    Ok(out)
}
```
불필요해진 import(`BTreeMap`, `Prescription` 등)·필드는 컴파일 경고 따라 정리. `heavy_tools`가 sub_agent를 포함하는지 `session_stats.rs`의 `heavy` 집계(`tool_kind IN ('sub_agent','mcp_call','web_search','web_fetch')`)로 확인 — 포함됨.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent_mentor r7 -- --nocapture`
Expected: PASS (신규 2개). 기존 `run_rules_purges_retired_rule_findings`(ops.rs)의 R7 관련 단언이 깨지면 Task 6에서 함께 조정.

- [ ] **Step 5: 커밋**
```bash
git add a-mate/crates/core/src/rules/r7_opus_trivial.rs
git commit -m "feat(agent): R7 mines per-session model-rightsizing candidates

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: B 판정 impl (`R7Judge`)

**Files:**
- Create: `a-mate/crates/core/src/rules/r7_judge.rs` (B의 `CoachingJudge` impl — 프롬프트·verdict·classify)
- Modify: `a-mate/crates/core/src/rules/mod.rs` (`pub mod r7_judge;`)
- Test: `r7_judge.rs` mod tests

**Interfaces:**
- Consumes: `CoachingJudge`, `PendingCandidate`(judge.rs), `deref_jsonl_line`(ops.rs)로 프롬프트 전문, `SqliteStore`.
- Produces: `struct R7Judge;` impl CoachingJudge (rule_id "R7"). `classify`: `over_modeled==true → "confirmed"`, else `"rejected"`. `build_prompt`: 세션 프롬프트 전문 + main-chain 사실. `rollup`: Task 6에서 구현(여기선 `Ok(vec![])` 스텁 후 Task 6에서 채움).

- [ ] **Step 1: classify 실패 테스트** (r7_judge.rs)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::CoachingJudge;
    #[test]
    fn r7_judge_classify_maps_over_modeled() {
        let j = R7Judge;
        assert_eq!(j.classify(&serde_json::json!({"over_modeled": true})), "confirmed");
        assert_eq!(j.classify(&serde_json::json!({"over_modeled": false})), "rejected");
        assert_eq!(j.classify(&serde_json::json!({})), "rejected");
        assert_eq!(j.rule_id(), "R7");
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent_mentor r7_judge_classify -- --nocapture`
Expected: FAIL — `r7_judge` 모듈/`R7Judge` 미정의.

- [ ] **Step 3: `R7Judge` 구현** (r7_judge.rs 신규)
```rust
//! B — 모델 적정화(R7 v3)의 LLM 판정. 세션 후보의 사용자 요청 + main-chain 사실로
//! "이 작업이 Opus가 필요했나"를 판정한다. 근거는 사용자 프롬프트·객관 사실뿐 —
//! 에이전트/서브에이전트 산출물·모델선택은 판단 대상 아님(증거 경계).

use crate::judge::{CoachingJudge, PendingCandidate};
use crate::store::SqliteStore;
use anyhow::{anyhow, Result};

pub struct R7Judge;

impl CoachingJudge for R7Judge {
    fn rule_id(&self) -> &'static str { "R7" }

    fn build_prompt(&self, store: &SqliteStore, c: &PendingCandidate) -> Result<(String, String)> {
        let sid = c.evidence.get("session_id").and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("R7 evidence에 session_id 없음"))?;
        // 세션의 사용자 프롬프트 전문(최대 5개) — main-chain만 (prompt_events는 이미 sidechain 제외)
        let prompts = store.session_user_prompts(sid, 5)?;
        if prompts.is_empty() {
            return Err(anyhow!("세션 {sid}에 사용자 프롬프트 없음 — 판정 근거 부족"));
        }
        let facts = format!(
            "모델: Opus / 어시스턴트 턴 {} / 도구 호출 {} / 출력 토큰 {}",
            c.evidence.get("opus_turns").and_then(|v| v.as_u64()).unwrap_or(0),
            c.evidence.get("tool_calls").and_then(|v| v.as_u64()).unwrap_or(0),
            c.evidence.get("tok_output").and_then(|v| v.as_u64()).unwrap_or(0),
        );
        let system = "당신은 Claude Code 사용 습관을 코칭하는 판정자입니다. \
사용자가 이 세션에서 시킨 작업이 Opus가 꼭 필요했는지, Sonnet으로 충분했는지 판정하세요.\n\
작업의 '무게'만 보세요 — 에이전트가 잘했는지·서브에이전트 모델 선택은 판단 대상이 아닙니다.\n\
over_modeled=true: 사소·정형 작업(단순 조회·이름변경·짧은 수정 등)이라 Sonnet으로 충분.\n\
over_modeled=false: 복잡한 추론·설계·큰 구현 등 Opus가 정당. **확신이 없으면 false.**\n\
아래 JSON 객체 하나만 출력(코드펜스·사족 금지):\n\
{\"over_modeled\": true 또는 false, \"reason\": \"한국어 한 문장\", \"suggested_model\": \"sonnet\"}"
            .to_string();
        let joined = prompts.iter()
            .map(|p| format!("- \"{}\"", p.replace('"', "'")))
            .collect::<Vec<_>>().join("\n");
        let user = format!("사용자 요청:\n{joined}\n\n객관적 작업량: {facts}");
        Ok((system, user))
    }

    fn classify(&self, verdict: &serde_json::Value) -> &'static str {
        if verdict.get("over_modeled").and_then(|v| v.as_bool()).unwrap_or(false) {
            "confirmed"
        } else {
            "rejected"
        }
    }

    fn rollup(&self, store: &SqliteStore) -> Result<Vec<String>> {
        crate::rules::r7_judge::rollup_project_cards(store) // Task 6에서 구현
    }
}

// Task 6에서 실제 구현으로 교체. 지금은 스텁.
pub(crate) fn rollup_project_cards(_store: &SqliteStore) -> Result<Vec<String>> {
    Ok(vec![])
}
```
`mod.rs`에 `pub mod r7_judge;` 추가. `store.session_user_prompts`는 Step 4에서 추가.

- [ ] **Step 4: `session_user_prompts` 추가 + 테스트** (store.rs)

먼저 실패 테스트 (store.rs mod tests):
```rust
#[test]
fn session_user_prompts_returns_full_text_main_chain() {
    let store = SqliteStore::open_in_memory().unwrap();
    // 실질 프롬프트 2개 (deref는 source_file+offset 필요 없이 preview로 폴백 확인)
    store.upsert_events(&[
        crate::model::NormalizedEvent {
            source_agent:"claude-code".into(), schema_version:"t".into(), host:"Windows".into(),
            project_id:"p".into(), session_id:"s1".into(), uuid:Some("u1".into()), parent_uuid:None,
            is_sidechain:false, ts:Some("2026-07-06T10:00:00Z".into()),
            source_file:"s.jsonl".into(), source_offset:0, msg_id:None,
            kind: crate::model::EventKind::UserPrompt { preview:"이 파일 이름만 바꿔줘".into() },
        },
    ]).unwrap();
    let ps = store.session_user_prompts("s1", 5).unwrap();
    assert_eq!(ps.len(), 1);
    assert!(ps[0].contains("이름만 바꿔줘"));
}
```
구현 (store.rs — `prompt_events`에서 세션별 프리뷰; 전문 deref는 후속 최적화, v1은 preview 사용):
```rust
/// 한 세션의 사용자 프롬프트(최신순 상한). prompt_events는 이미 sidechain·meta·도구결과 제외.
pub fn session_user_prompts(&self, session_id: &str, limit: usize) -> Result<Vec<String>> {
    let mut stmt = self.conn.prepare(
        "SELECT preview FROM prompt_events WHERE session_id=?1
         ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![session_id, limit as i64], |r| r.get::<_, String>(0))?;
    let mut out: Vec<String> = rows.collect::<std::result::Result<_, _>>()?;
    out.retain(|p| !p.trim().is_empty());
    Ok(out)
}
```
(전제: `prompt_events`에 `preview`·`session_id`·`id` 컬럼 존재 — store.rs 스키마 확인. 없으면 실제 컬럼명으로 조정.)

- [ ] **Step 5: build_prompt 근거 경계 테스트** (r7_judge.rs)
```rust
#[test]
fn r7_judge_prompt_carries_user_request_and_forbids_agent_grading() {
    let store = SqliteStore::open_in_memory().unwrap();
    store.upsert_events(&[/* s1에 UserPrompt "로그 파일 개수만 세줘" 시드 (위 헬퍼 형태) */]).unwrap();
    let c = PendingCandidate {
        dedup_key: "R7|sess|Windows|s1".into(), scope_host: "Windows".into(),
        scope_project: Some("p".into()),
        evidence: serde_json::json!({"session_id":"s1","opus_turns":1,"tool_calls":1,"tok_output":120}),
        prev_attempts: 0,
    };
    let (system, user) = R7Judge.build_prompt(&store, &c).unwrap();
    assert!(system.contains("over_modeled"));
    assert!(system.contains("무게"), "작업 무게만 보라는 경계 명시");
    assert!(system.contains("서브에이전트"), "subagent 모델 판단 금지 명시");
    assert!(user.contains("개수만 세줘"), "사용자 요청 전문 포함");
}
```

- [ ] **Step 6: 통과 확인**

Run: `cargo test -p agent_mentor r7_judge -- --nocapture`
Expected: PASS.

- [ ] **Step 7: 커밋**
```bash
git add a-mate/crates/core/src/rules/r7_judge.rs a-mate/crates/core/src/rules/mod.rs a-mate/crates/core/src/store.rs
git commit -m "feat(agent): add R7Judge (model right-sizing verdict)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: B 롤업 + 판정 드라이버 배선

**Files:**
- Modify: `a-mate/crates/core/src/rules/r7_judge.rs` (`rollup_project_cards` 실제 구현)
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`maybe_judge_repeats` → `run_coaching_judgments`)
- Test: `r7_judge.rs` mod tests (롤업), 판정 드라이버는 코어 순수 함수로 뽑아 MockEngine 테스트

**Interfaces:**
- Produces:
  - `rollup_project_cards(store)` — confirmed 세션 ≥3 & 과반인 (host,project)마다 `R7|{host}|{project}` 프로젝트 카드('new', 처방 `start_with_lighter_model`)를 upsert, 비자격 stale 'new' 카드 삭제. 반환 = 새로 만든 dedup_key.
  - `judge::run_judgments_core(engine, store, judges) -> Vec<String>` (순수 로직, 락 밖 호출 가정) 또는 pipeline 내 오케스트레이션.
- Consumes: `set_judgment`, `pending_for_judgment`, `extract_verdict_json`, `resolve_engine`.

- [ ] **Step 1: 롤업 실패 테스트** (r7_judge.rs)
```rust
#[test]
fn rollup_emits_project_card_when_enough_confirmed() {
    let store = SqliteStore::open_in_memory().unwrap();
    // confirmed 세션 3개 + rejected 1개 (같은 host/project) → 3 >= 3, 3*2 > 4 과반
    let mk = |sid: &str, status: &str| {
        let f = crate::finding::Finding {
            rule_id:"R7".into(), severity:crate::finding::Severity::Suggest,
            scope_host:Some("Windows".into()), scope_project:Some("p".into()),
            scope_kind:"session".into(), scope_ref:sid.into(),
            evidence: serde_json::json!({"session_id":sid}), est_tokens_saved:0,
            prescription:None, dedup_key: format!("R7|sess|Windows|{sid}"),
        };
        store.upsert_finding(&f, "2026-07-06T10:00:00Z").unwrap();
        store.set_judgment(&f.dedup_key, Some(status), &serde_json::json!({"attempts":1})).unwrap();
    };
    mk("s1","confirmed"); mk("s2","confirmed"); mk("s3","confirmed"); mk("s4","rejected");
    let fresh = rollup_project_cards(&store).unwrap();
    assert_eq!(fresh, vec!["R7|Windows|p".to_string()]);
    let status: String = store.conn.query_row(
        "SELECT status FROM findings WHERE dedup_key='R7|Windows|p'", [], |r| r.get(0)).unwrap();
    assert_eq!(status, "new");
}

#[test]
fn rollup_silent_below_threshold_and_clears_stale() {
    let store = SqliteStore::open_in_memory().unwrap();
    // 기존(구 통계) 프로젝트 카드가 'new'로 남아있으나 confirmed 세션 부족 → 제거돼야
    let card = crate::finding::Finding {
        rule_id:"R7".into(), severity:crate::finding::Severity::Suggest,
        scope_host:Some("Windows".into()), scope_project:Some("p".into()),
        scope_kind:"project".into(), scope_ref:"p".into(),
        evidence: serde_json::json!({}), est_tokens_saved:0, prescription:None,
        dedup_key:"R7|Windows|p".into(),
    };
    store.upsert_finding(&card, "2026-07-06T10:00:00Z").unwrap(); // 'new'
    let fresh = rollup_project_cards(&store).unwrap();
    assert!(fresh.is_empty());
    let n: i64 = store.conn.query_row(
        "SELECT COUNT(*) FROM findings WHERE dedup_key='R7|Windows|p' AND status='new'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0, "자격 없는 stale 카드 제거");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent_mentor rollup_ -- --nocapture`
Expected: FAIL — `rollup_project_cards`가 스텁(`Ok(vec![])`).

- [ ] **Step 3: `rollup_project_cards` 구현** (r7_judge.rs — 스텁 교체)
```rust
pub(crate) fn rollup_project_cards(store: &SqliteStore) -> Result<Vec<String>> {
    use crate::finding::{Finding, Prescription, Severity};
    // (host, project)별 confirmed/판정합 집계
    let mut stmt = store.conn.prepare(
        "SELECT COALESCE(scope_host,''), COALESCE(scope_project,''),
                SUM(CASE WHEN status='confirmed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status IN ('confirmed','rejected') THEN 1 ELSE 0 END),
                GROUP_CONCAT(CASE WHEN status='confirmed'
                    THEN json_extract(evidence_json,'$.session_id') END)
         FROM findings
         WHERE rule_id='R7' AND scope_kind='session'
         GROUP BY scope_host, scope_project",
    )?;
    let rows: Vec<(String, String, i64, i64, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut qualifying: Vec<(String, String, i64, Vec<String>)> = Vec::new();
    for (host, proj, confirmed, judged, sids) in rows {
        if confirmed >= 3 && confirmed * 2 > judged {
            let examples: Vec<String> = sids.unwrap_or_default()
                .split(',').filter(|s| !s.is_empty()).take(5).map(String::from).collect();
            qualifying.push((host, proj, confirmed, examples));
        }
    }

    // 비자격 stale 'new' 프로젝트 카드 제거 (구 통계 카드 마이그레이션 포함).
    // 자격 세트는 소규모라 개별 확인.
    let qualifying_keys: std::collections::HashSet<String> =
        qualifying.iter().map(|(h, p, _, _)| format!("R7|{h}|{p}")).collect();
    let mut stale = store.conn.prepare(
        "SELECT dedup_key FROM findings WHERE rule_id='R7' AND scope_kind='project' AND status='new'",
    )?;
    let existing: Vec<String> = stale.query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    for key in &existing {
        if !qualifying_keys.contains(key) {
            store.conn.execute("DELETE FROM findings WHERE dedup_key=?1 AND status='new'", [key])?;
        }
    }

    // 자격 프로젝트 카드 upsert. 신규 노출만 fresh로 반환.
    let existing_set: std::collections::HashSet<&String> = existing.iter().collect();
    let now = chrono::Utc::now().to_rfc3339();
    let mut fresh = Vec::new();
    for (host, proj, confirmed, examples) in qualifying {
        let key = format!("R7|{host}|{proj}");
        let card = Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some(host.clone()), scope_project: Some(proj.clone()),
            scope_kind: "project".into(), scope_ref: proj.clone(),
            evidence: serde_json::json!({
                "over_modeled_sessions": confirmed,
                "example_session_ids": examples,
                "note": "LLM 판정: 이 프로젝트의 Opus 세션 상당수가 Sonnet으로 충분",
            }),
            est_tokens_saved: 0,
            prescription: Some(Prescription {
                kind: "start_with_lighter_model".into(),
                payload: serde_json::json!({ "to": "sonnet" }),
            }),
            dedup_key: key.clone(),
        };
        store.upsert_finding(&card, &now)?; // (R7,project) → init 'new'
        if !existing_set.contains(&key) {
            fresh.push(key); // 새로 노출된 것만 알림
        }
    }
    Ok(fresh)
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent_mentor r7_judge -- --nocapture`
Expected: PASS (롤업 2개 포함).

- [ ] **Step 5: 판정 드라이버 배선** — `pipeline.rs`의 `maybe_judge_repeats`를 `run_coaching_judgments`로 교체

`maybe_judge_repeats`(현 177-256)의 로직을 유지하되 judges 벡터를 순회하도록 일반화. 락 규율(①엔진 짧은락 ②pending+build_prompt 짧은락 ③generate 락밖 ④set_judgment 짧은락 ⑤rollup 짧은락 ⑥emit)은 그대로. 교체 함수:
```rust
fn run_coaching_judgments(app: &AppHandle, store_mutex: &std::sync::Mutex<SqliteStore>) {
    use agent_mentor::judge::{CoachingJudge, extract_verdict_json};
    let judges: Vec<Box<dyn CoachingJudge>> = vec![
        Box::new(agent_mentor::judge::R6Judge),
        Box::new(agent_mentor::rules::r7_judge::R7Judge),
    ];
    // ① 엔진 (짧은 락)
    let engine = match store_mutex.lock() {
        Ok(store) => crate::resolve_engine(&store),
        Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
    };
    let Some(engine) = engine else { return; };

    let mut fresh_all: Vec<String> = Vec::new();
    for judge in &judges {
        // ② pending + 프롬프트 (짧은 락)
        let batch: Vec<(String, u32, String, String)> = match store_mutex.lock() {
            Ok(store) => {
                let mut b = Vec::new();
                for c in store.pending_for_judgment(judge.rule_id(), 10).unwrap_or_default() {
                    match judge.build_prompt(&store, &c) {
                        Ok((sys, usr)) => b.push((c.dedup_key, c.prev_attempts, sys, usr)),
                        Err(e) => {
                            log::warn!("{} 재료 수집 실패({}): {e}", judge.rule_id(), c.dedup_key);
                            let attempts = c.prev_attempts + 1;
                            let status = if attempts >= 3 { Some("rejected") } else { None };
                            let _ = store.set_judgment(&c.dedup_key, status,
                                &serde_json::json!({"attempts":attempts,"error":e.to_string()}));
                        }
                    }
                }
                b
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        // ③ 락 밖: 판정
        let mut results = Vec::new();
        for (key, prev, sys, usr) in batch {
            match engine.generate(&sys, &usr) {
                Ok(out) => results.push((key, prev, out.text, out.tokens_used)),
                Err(e) => log::warn!("{} 판정 전송 실패({key}): {e}", judge.rule_id()),
            }
        }
        // ④ verdict 저장 (짧은 락)
        match store_mutex.lock() {
            Ok(store) => {
                for (key, prev, text, tokens) in results {
                    let attempts = prev + 1;
                    match extract_verdict_json(&text) {
                        Ok(v) => {
                            let status = judge.classify(&v);
                            let mut rec = v.clone();
                            rec["attempts"] = serde_json::json!(attempts);
                            rec["tokens"] = serde_json::json!(tokens);
                            let _ = store.set_judgment(&key, Some(status), &rec);
                        }
                        Err(e) => {
                            let status = if attempts >= 3 { Some("rejected") } else { None };
                            let _ = store.set_judgment(&key, status,
                                &serde_json::json!({"attempts":attempts,"error":e.to_string(),"tokens":tokens}));
                        }
                    }
                }
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }
        // ⑤ 롤업 (짧은 락)
        match store_mutex.lock() {
            Ok(store) => match judge.rollup(&store) {
                Ok(keys) => fresh_all.extend(keys),
                Err(e) => log::warn!("{} rollup 실패: {e}", judge.rule_id()),
            },
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        }
    }
    // ⑥ 새 노출 finding 재발행 (R6 worthy 'new' 포함 — classify가 'new'로 바꾼 것도 fresh로 모을지
    //    검토: R6은 classify→'new'라 rollup=[]이므로, ④에서 'new' 전환된 key도 fresh에 추가할 것)
    // → ④ 루프에서 status=="new"인 key를 fresh_all에 push하도록 보강.
    if !fresh_all.is_empty() {
        if let Ok(store) = store_mutex.lock() {
            let rows: Vec<_> = store.list_findings_current(false).unwrap_or_default()
                .into_iter().filter(|f| fresh_all.contains(&f.dedup_key)).collect();
            drop(store);
            if !rows.is_empty() {
                let _ = app.emit("coach:finding", &rows);
                let _ = app.emit("scan:done", &chrono::Utc::now().to_rfc3339());
            }
        }
    }
}
```
그리고 호출부(현 110줄 `maybe_judge_repeats(app, &state.store);`)를 `run_coaching_judgments(app, &state.store);`로 교체. ④ 루프에서 `if status == "new" { fresh_all.push(key.clone()); }` 추가(R6 worthy 즉시 노출 재발행 보존). 기존 `maybe_judge_repeats`·`judge_one`·`judgment_record` 및 그 R6 전용 경로는 제거(또는 judge_one/judgment_record는 R6 테스트가 참조하면 유지). 컴파일 경고 따라 정리.

- [ ] **Step 6: 코어 판정 루프 단위 테스트** (선택 — pipeline은 Tauri 의존이라, 판정 순수 로직을 코어에서 MockEngine으로 검증)

`judge.rs`에 순수 헬퍼가 있으면 그것을, 없으면 이 배선은 통합 성격 → `cargo build`로 컴파일 검증 + 기존 R6 판정 코어 테스트(judge.rs) 통과로 회귀 확인.
Run: `cargo test -p agent_mentor && cargo build -p agent-mentor-app`
Expected: PASS / 빌드 성공.

- [ ] **Step 7: 커밋**
```bash
git add a-mate/crates/core/src/rules/r7_judge.rs a-mate/src-tauri/src/pipeline.rs
git commit -m "feat(agent): roll up R7 verdicts into project cards; generalize judge driver

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: R10 · R11 은퇴

**Files:**
- Modify: `a-mate/crates/core/src/ops.rs` (`run_rules` 등록 해제 + purge)
- Test: `ops.rs` mod tests (`run_rules_purges_retired_rule_findings` 확장)

**Interfaces:**
- Produces: `run_rules`가 R10·R11을 더는 등록하지 않고, 각 findings를 purge. `R10AutomationBurst`·`R11PermissionFriction` 코드·테스트는 보존(등록만 해제).

- [ ] **Step 1: 실패 테스트** — `run_rules_purges_retired_rule_findings`에 R10·R11 추가

`ops.rs`의 기존 테스트에서, 시드 목록에 추가:
```rust
        store.upsert_finding(&mk("R10", "project", "R10|W|proj"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R11", "project", "R11|W|proj"), "2026-07-06T00:00:00Z").unwrap();
```
그리고 마지막 단언을 조정 — 이제 R11도 purge되므로 "R11만 생존" 단언을 제거하고:
```rust
        run_rules(&store).unwrap();
        // 은퇴 룰(R10·R11 포함)·구폐기분 전부 purge. R7 세션 후보는 이 테스트에 시드 안 함.
        for key in ["R10|W|proj", "R11|W|proj"] {
            let n: i64 = store.conn.query_row(
                "SELECT COUNT(*) FROM findings WHERE dedup_key=?1", [key], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{key} purge되어야");
        }
```
(기존 "R11만 생존" 단언은 삭제 — R11이 이제 은퇴하므로.)

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent_mentor run_rules_purges -- --nocapture`
Expected: FAIL — R10·R11 findings가 아직 purge 안 됨(R11은 여전히 등록됨).

- [ ] **Step 3: `run_rules` 수정** (ops.rs)

purge 블록에 추가 (기존 `delete_findings_by_rule_and_scope` 줄들 옆):
```rust
    // 코칭 가치 재설계: R10(관찰 카드)·R11(권한 마찰) 은퇴 — 코드·테스트는 보존.
    store.delete_findings_by_rule_and_scope("R10", "project")?;
    store.delete_findings_by_rule_and_scope("R11", "project")?;
```
RuleEngine 벡터에서 R10·R11 등록 제거:
```rust
    let engine = RuleEngine::new(vec![
        Box::new(crate::rules::r6_repeated_prompts::R6RepeatedPrompts::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(crate::rules::r8_mcp_large_result::R8McpLargeResult::default()),
        // R10·R11 은퇴 (코칭 가치 재설계) — detect_bursts는 R7 후보 제외에 계속 사용
    ]);
```
`use` 문의 `R10AutomationBurst`·`R11PermissionFriction` import 제거(경고 정리). 파일 상단 `use crate::rules::r10_automation_burst::R10AutomationBurst;`·`r11_permission_friction::R11PermissionFriction;` 삭제.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent_mentor -- --nocapture`
Expected: PASS (전체). R10·R11 룰 파일 자체의 테스트는 보존돼 계속 통과(등록만 해제).

- [ ] **Step 5: 커밋**
```bash
git add a-mate/crates/core/src/ops.rs
git commit -m "feat(agent): retire R10 and R11 coaching rules

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## 자기검토 결과 (스펙 대비)

- **§2 계약**: main-chain 필터(Task 1·B), 프롬프트 근거(Task 5 build_prompt), subagent 배제(Task 4·5), fail-safe(Task 6 드라이버), 세션카운트 미사용(B는 세션 단위·예외로 스펙 명시) — 커버.
- **§3① 세그먼터**: 이 PR 범위 아님(F/C/A에서). B는 세그먼터 안 씀 — 스펙 일치.
- **§3② 판정 패스**: Task 2(트레이트·드라이버)·Task 6(배선), R6 편입(Task 2 R6Judge + Task 6 판정 루프에 R6 포함) — 커버.
- **§4 B**: Task 4(채굴)·5(판정)·6(롤업). 롤업 문턱 ≥3&과반, 처방 sonnet — 일치.
- **§5 R10/R11**: Task 7 — 커버. `detect_bursts` 유지 확인(Task 4·7 주석).
- **미커버(의도적)**: F·C·A·E는 후속 PR(스펙 §7). effort 독립 판정·Haiku 세분화 후속.

**미해결 확인 필요(구현 중)**: `ToolKind`의 sub_agent 배리언트 정확명(Task 4 Step 1 주석), `prompt_events` 실제 컬럼명(Task 5 Step 4), `deref_jsonl_line` 전문 사용은 v1에서 preview 폴백으로 단순화(후속 최적화 가능).
