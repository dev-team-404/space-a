# 프론트엔드 1단계(셸+데이터) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tauri v2 셸(트레이 상주 + autostart) 위에 기존 백엔드를 배선하고, notify 디바운스 파이프라인과 chat 창(홈·코칭 탭)까지 — 스펙 `docs/specs/2026-07-03-frontend-vision-design.md`의 1단계.

**Architecture:** 기존 크레이트를 `crates/core`로 이동(무수정)하고 cargo workspace화. `src-tauri` 앱 크레이트가 core를 path 의존으로 소비하며 `#[tauri::command]`·트레이·notify 파이프라인을 담당. 프론트는 Svelte 5 + Vite 멀티페이지(1단계는 `chat.html` 엔트리만).

**Tech Stack:** Tauri 2 (tray-icon feature), tauri-plugin-autostart 2, notify 6, Svelte 5, Vite 6, rusqlite(기존), sha2.

## Global Constraints

- **Tauri v2 전용. v1 API 금지** (`SystemTray`, `tauri::updater`, `WindowBuilder` 등 v1 심볼 금지).
- **Windows 전용.** macOS/Linux 분기 불필요 (autostart init의 MacosLauncher 인자는 API 시그니처상 필요할 뿐).
- **빌드 환경 (이 머신 비표준, cargo 실행 전 Git Bash에서 매번):**
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
  `git push`가 revocation 에러면 `git -c http.schannelCheckRevoke=false push`.
- **⚠ 선행 리스크:** Tauri는 Windows에서 MSVC 링커를 권장한다. 이 머신은 GNU 툴체인뿐이므로 Task 5의 `cargo check`가 조기 검증 게이트다. 링크 실패 시: VS Build Tools(C++ 워크로드) 설치 후 `rustup toolchain install stable-msvc; rustup default stable-msvc`로 전환하고 `RUSTUP_TOOLCHAIN` export를 제거 — 사용자에게 보고 후 진행.
- **기존 core 테스트 87개는 무수정 통과가 원칙.** core 변경은 이 플랜이 명시한 추가(Task 2·3·4)만.
- 관대한 수집 정책 유지: 파일/호스트 단위 실패는 스킵하고 계속. 에러를 UI 알림으로 띄우지 않는다.
- 앱 DB 경로는 `app_data_dir()/agent-mentor.db` (CLI의 `./agent-mentor.db`와 별개).
- 커밋 메시지는 기존 관례(`feat:`/`fix:`/`docs:` + 한국어 본문).

---

### Task 1: cargo workspace 재구성 (core 이동)

**Files:**
- Create: `Cargo.toml` (workspace 루트, 기존 것은 이동)
- Move: `Cargo.toml` → `crates/core/Cargo.toml`, `src/` → `crates/core/src/`

**Interfaces:**
- Consumes: 없음
- Produces: `crates/core` 크레이트 (패키지명 `agent-mentor`, lib명 `agent_mentor` 그대로) — 이후 모든 태스크의 기반

- [ ] **Step 1: 이동**

```bash
mkdir -p crates/core
git mv Cargo.toml crates/core/Cargo.toml
git mv src crates/core/src
```

- [ ] **Step 2: 워크스페이스 루트 Cargo.toml 생성**

```toml
[workspace]
resolver = "2"
members = ["crates/core"]
```

(`src-tauri`는 Task 5에서 members에 추가한다.)

- [ ] **Step 3: 전체 테스트로 무손상 확인**

Run: `cargo test 2>&1 | tail -5`
Expected: `test result: ok. 87 passed; 0 failed` (여러 타깃 합산 87) — 경고 0

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: cargo workspace화 — 기존 크레이트를 crates/core로 무수정 이동"
```

---

### Task 2: core 조회 표면 확장 (settings 테이블 + UI용 쿼리)

**Files:**
- Modify: `crates/core/src/store.rs`

**Interfaces:**
- Consumes: 기존 `SqliteStore`, `daily_rollup`/`findings`/`diary_index`/`sessions` 테이블
- Produces (Task 6·7이 사용):
  - `pub struct DaySummary { session_count, tok_input, tok_output, tok_cache_read, tok_cache_create: u64 }`
  - `pub struct FindingRow { rule_id, severity: String, scope_host, scope_project: Option<String>, scope_kind, scope_ref: String, evidence: Value, est_tokens_saved: u64, prescription: Option<Value>, dedup_key: String, last_seen: Option<String>, occurrences: u64 }` (`#[derive(Debug, Clone, Serialize)]`)
  - `summary_for_date(&self, date) -> Result<DaySummary>` / `total_sessions(&self) -> Result<u64>` / `sum_est_tokens_saved(&self) -> Result<u64>`
  - `list_findings_current(&self) -> Result<Vec<FindingRow>>` / `finding_severities(&self) -> Result<Vec<(String, String)>>`
  - `diary_dates(&self) -> Result<Vec<String>>` / `diary_path_for(&self, date) -> Result<Option<String>>`
  - `get_setting(&self, key) -> Result<Option<String>>` / `set_setting(&self, key, value) -> Result<()>` / `all_settings(&self) -> Result<Vec<(String, String)>>`

- [ ] **Step 1: 실패하는 테스트 작성** (`store.rs` 하단 `mod tests`에 추가)

```rust
#[test]
fn summary_and_settings_queries() {
    use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage};
    let store = SqliteStore::open_in_memory().unwrap();
    store.upsert_events(&[NormalizedEvent {
        source_agent: "claude-code".into(), schema_version: "t".into(),
        host: "Windows".into(), project_id: "p1".into(),
        session_id: "s1".into(), uuid: Some("u1".into()), parent_uuid: None,
        is_sidechain: false, ts: Some("2026-07-02T10:00:00Z".into()),
        source_file: "s.jsonl".into(), source_offset: 0,
        kind: EventKind::AssistantTurn {
            model: NormModel::from_raw_id("claude-opus-4-8"),
            usage: TokenUsage { input: 100, output: 50, cache_read: 10, cache_creation: 5, eph_1h: 0, eph_5m: 0 },
            web_search: 0, web_fetch: 0,
        },
    }]).unwrap();
    store.rebuild_rollup().unwrap();

    let day = store.summary_for_date("2026-07-02").unwrap();
    assert_eq!(day.session_count, 1);
    assert_eq!(day.tok_input, 100);
    assert_eq!(store.summary_for_date("2099-01-01").unwrap().session_count, 0);
    assert_eq!(store.total_sessions().unwrap(), 1);

    // settings 라운드트립
    assert_eq!(store.get_setting("k").unwrap(), None);
    store.set_setting("k", "v1").unwrap();
    store.set_setting("k", "v2").unwrap(); // upsert
    assert_eq!(store.get_setting("k").unwrap(), Some("v2".into()));
    assert_eq!(store.all_settings().unwrap(), vec![("k".to_string(), "v2".to_string())]);
}

#[test]
fn findings_and_diary_queries() {
    use crate::finding::{Finding, Severity};
    let store = SqliteStore::open_in_memory().unwrap();
    let base = Finding {
        rule_id: "R5".into(), severity: Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: Some("p".into()),
        scope_kind: "session".into(), scope_ref: "s1".into(),
        evidence: serde_json::json!({"path":"a.txt","count":5}),
        est_tokens_saved: 7200, prescription: None, dedup_key: "R5|s1|a.txt".into(),
    };
    store.upsert_finding(&base, "2026-07-01T10:00:00Z").unwrap();
    store.upsert_finding(&Finding {
        rule_id: "R1".into(), severity: Severity::Warn,
        est_tokens_saved: 99000, dedup_key: "R1|global|ctx".into(),
        ..base
    }, "2026-07-01T11:00:00Z").unwrap();

    let rows = store.list_findings_current().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].rule_id, "R1", "est_tokens_saved DESC 정렬");
    assert_eq!(rows[0].occurrences, 1);
    assert_eq!(store.sum_est_tokens_saved().unwrap(), 99000 + 7200);

    let sevs = store.finding_severities().unwrap();
    assert!(sevs.contains(&("R1|global|ctx".to_string(), "warn".to_string())));

    store.upsert_diary_index("2026-07-01", "Windows", "/tmp/d1.md", 100, "mock").unwrap();
    store.upsert_diary_index("2026-07-02", "Windows", "/tmp/d2.md", 100, "mock").unwrap();
    assert_eq!(store.diary_dates().unwrap(), vec!["2026-07-01", "2026-07-02"]);
    assert_eq!(store.diary_path_for("2026-07-02").unwrap(), Some("/tmp/d2.md".into()));
    assert_eq!(store.diary_path_for("2099-01-01").unwrap(), None);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor summary_and_settings 2>&1 | tail -3`
Expected: 컴파일 에러 (`summary_for_date` 미정의)

- [ ] **Step 3: 구현**

`SCHEMA` 상수에 추가:

```sql
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY, value TEXT NOT NULL
);
```

`impl SqliteStore`에 추가 (RollupRow 정의 근처에 DaySummary·FindingRow struct):

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct DaySummary {
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FindingRow {
    pub rule_id: String,
    pub severity: String,
    pub scope_host: Option<String>,
    pub scope_project: Option<String>,
    pub scope_kind: String,
    pub scope_ref: String,
    pub evidence: serde_json::Value,
    pub est_tokens_saved: u64,
    pub prescription: Option<serde_json::Value>,
    pub dedup_key: String,
    pub last_seen: Option<String>,
    pub occurrences: u64,
}
```

```rust
pub fn summary_for_date(&self, date: &str) -> Result<DaySummary> {
    let row = self.conn.query_row(
        "SELECT COALESCE(SUM(session_count),0), COALESCE(SUM(tok_input),0),
                COALESCE(SUM(tok_output),0), COALESCE(SUM(tok_cache_read),0),
                COALESCE(SUM(tok_cache_create),0)
         FROM daily_rollup WHERE date=?1",
        params![date],
        |r| Ok(DaySummary {
            session_count: r.get::<_, i64>(0)? as u64,
            tok_input: r.get::<_, i64>(1)? as u64,
            tok_output: r.get::<_, i64>(2)? as u64,
            tok_cache_read: r.get::<_, i64>(3)? as u64,
            tok_cache_create: r.get::<_, i64>(4)? as u64,
        }),
    )?;
    Ok(row)
}

pub fn total_sessions(&self) -> Result<u64> {
    let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    Ok(n as u64)
}

pub fn sum_est_tokens_saved(&self) -> Result<u64> {
    let n: i64 = self.conn.query_row(
        "SELECT COALESCE(SUM(est_tokens_saved),0) FROM findings", [], |r| r.get(0))?;
    Ok(n as u64)
}

pub fn list_findings_current(&self) -> Result<Vec<FindingRow>> {
    let mut stmt = self.conn.prepare(
        "SELECT rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                evidence_json, est_tokens_saved, prescription_json, dedup_key,
                last_seen, occurrences
         FROM findings ORDER BY est_tokens_saved DESC, dedup_key",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?, r.get::<_, String>(5)?,
            r.get::<_, String>(6)?, r.get::<_, i64>(7)?,
            r.get::<_, Option<String>>(8)?, r.get::<_, String>(9)?,
            r.get::<_, Option<String>>(10)?, r.get::<_, i64>(11)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
             evidence_json, est, prescription_json, dedup_key, last_seen, occ) = row?;
        out.push(FindingRow {
            rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
            evidence: serde_json::from_str(&evidence_json).unwrap_or(serde_json::Value::Null),
            est_tokens_saved: est as u64,
            prescription: prescription_json
                .and_then(|s| serde_json::from_str(&s).ok()),
            dedup_key, last_seen,
            occurrences: occ as u64,
        });
    }
    Ok(out)
}

pub fn finding_severities(&self) -> Result<Vec<(String, String)>> {
    let mut stmt = self.conn.prepare("SELECT dedup_key, severity FROM findings")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn diary_dates(&self) -> Result<Vec<String>> {
    let mut stmt = self.conn.prepare("SELECT DISTINCT date FROM diary_index ORDER BY date")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn diary_path_for(&self, date: &str) -> Result<Option<String>> {
    let v: Option<String> = self
        .conn
        .query_row("SELECT path FROM diary_index WHERE date=?1 LIMIT 1", params![date], |r| r.get(0))
        .optional()?;
    Ok(v)
}

pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
    let v: Option<String> = self
        .conn
        .query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0))
        .optional()?;
    Ok(v)
}

pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
    self.conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=?2",
        params![key, value],
    )?;
    Ok(())
}

pub fn all_settings(&self) -> Result<Vec<(String, String)>> {
    let mut stmt = self.conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
}
```

(`optional()`은 `rusqlite::OptionalExtension` — 파일 상단 use에 이미 없으면 추가.)

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent-mentor 2>&1 | tail -3`
Expected: `test result: ok.` — 기존 87 + 신규 2, 경고 0

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/store.rs
git commit -m "feat: UI 조회 표면 — settings 테이블, DaySummary/FindingRow, diary 조회 쿼리"
```

---

### Task 3: core ops 모듈 (CLI 로직을 라이브러리로 추출)

**Files:**
- Create: `crates/core/src/ops.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod ops;` 추가), `crates/core/src/main.rs` (ops 호출로 재배선)

**Interfaces:**
- Consumes: `enumerate_hosts`, `ingest_file`, `collect_host_inventory`, `scan_plugin_inventory`, `RuleEngine`, 규칙 5종
- Produces (Task 7이 사용):
  - `pub struct IngestReport { pub files: usize, pub new_events: usize, pub warnings: Vec<String> }`
  - `pub fn run_ingest(store: &SqliteStore) -> anyhow::Result<IngestReport>` (rebuild_rollup 포함)
  - `pub fn run_inventory(store: &mut SqliteStore) -> anyhow::Result<Vec<String>>` (반환 = warnings)
  - `pub fn run_rules(store: &SqliteStore) -> anyhow::Result<Vec<Finding>>` (upsert_finding까지 수행)
  - `pub fn read_json_guarded(path: &Path) -> Option<serde_json::Value>` (main.rs에서 이동)

- [ ] **Step 1: 실패하는 테스트 작성** (`ops.rs`에 모듈과 함께 — main.rs의 `read_json_guarded_valid_absent_broken` 테스트를 그대로 이동 + 신규 스모크)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;

    #[test]
    fn run_rules_on_empty_store_is_ok() {
        let store = SqliteStore::open_in_memory().unwrap();
        let findings = run_rules(&store).unwrap();
        assert!(findings.is_empty());
        assert_eq!(store.count_findings().unwrap(), 0);
    }

    // main.rs에서 이동: read_json_guarded_valid_absent_broken (본문 동일)
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor run_rules_on_empty 2>&1 | tail -3`
Expected: 컴파일 에러 (`ops` 미정의)

- [ ] **Step 3: 구현** — `main.rs`의 `cmd_ingest`/`cmd_inventory`/`cmd_rules` 본문을 옮기되 `println!`/`eprintln!` 대신 값을 반환:

```rust
//! CLI와 Tauri 앱이 공유하는 상위 오퍼레이션. 출력(print) 없이 값을 반환한다.
use crate::adapter::SourceAdapter;
use crate::finding::Finding;
use crate::hosts::enumerate_hosts;
use crate::inventory::{collect_host_inventory, scan_plugin_inventory};
use crate::rules::r1_unused_mcp::R1UnusedMcp;
use crate::rules::r2_unused_plugins::R2UnusedPluginSkills;
use crate::rules::r5_repeated_read::R5RepeatedRead;
use crate::rules::r7_opus_trivial::R7OpusTrivial;
use crate::rules::r9_web_overuse::R9WebOveruse;
use crate::rules::RuleEngine;
use crate::store::{ingest_file, SqliteStore};
use anyhow::Result;
use std::path::Path;

pub struct IngestReport {
    pub files: usize,
    pub new_events: usize,
    pub warnings: Vec<String>,
}

pub fn run_ingest(store: &SqliteStore) -> Result<IngestReport> {
    let mut report = IngestReport { files: 0, new_events: 0, warnings: Vec::new() };
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        let files = match adapter.discover() {
            Ok(f) => f,
            Err(e) => {
                report.warnings.push(format!("host {} 파일 열거 실패: {e}", hs.host));
                Vec::new()
            }
        };
        report.files += files.len();
        for f in &files {
            match ingest_file(store, &adapter, f) {
                Ok(n) => report.new_events += n,
                Err(e) => report.warnings.push(format!("{} 수집 실패(건너뜀): {e}", f.display())),
            }
        }
    }
    store.rebuild_rollup()?;
    Ok(report)
}

pub fn read_json_guarded(path: &Path) -> Option<serde_json::Value> {
    // main.rs 본문 그대로 이동
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Some(serde_json::Value::Null),
        Ok(s) => serde_json::from_str(&s).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(serde_json::Value::Null),
        Err(_) => None,
    }
}

pub fn run_inventory(store: &mut SqliteStore) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    for hs in enumerate_hosts() {
        let Some(claude_json) = read_json_guarded(&hs.claude_json()) else {
            warnings.push(format!("host {} claude.json 읽기 실패 — reconcile 스킵", hs.host));
            continue;
        };
        let Some(settings) = read_json_guarded(&hs.settings_json()) else {
            warnings.push(format!("host {} settings.json 읽기 실패 — reconcile 스킵", hs.host));
            continue;
        };
        let cache = hs.claude_root.join("plugins").join("cache");
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        if !inv.complete {
            warnings.push(format!("host {} 중첩 .mcp.json 읽기 실패 — reconcile 스킵", hs.host));
            continue;
        }
        store.replace_host_inventory(&hs.host, &inv.entries)?;
        let (plugins, plugins_complete) = scan_plugin_inventory(&settings, &cache);
        if !plugins_complete {
            warnings.push(format!("host {} 플러그인 스킬 스캔 실패 — R2 스킵", hs.host));
        } else {
            store.replace_plugin_inventory(&hs.host, &plugins)?;
        }
    }
    Ok(warnings)
}

pub fn run_rules(store: &SqliteStore) -> Result<Vec<Finding>> {
    let engine = RuleEngine::new(vec![
        Box::new(R5RepeatedRead::default()),
        Box::new(R1UnusedMcp::default()),
        Box::new(R2UnusedPluginSkills::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(R9WebOveruse::default()),
    ]);
    let findings = engine.run(store)?;
    let now = chrono::Utc::now().to_rfc3339();
    for f in &findings {
        store.upsert_finding(f, &now)?;
    }
    Ok(findings)
}
```

`main.rs`는 `cmd_*` 본문을 `ops::run_*` 호출 + 기존과 동등한 `println!`으로 교체 (예: `cmd_ingest`는 `let r = ops::run_ingest(&store)?;` 후 warnings를 `eprintln!`, 요약을 `println!`). `read_json_guarded`와 그 테스트는 main.rs에서 삭제(ops로 이동했으므로).

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent-mentor 2>&1 | tail -3`
Expected: `test result: ok.` — 총 테스트 수는 Task 2 시점 + 1 (이동 1, 신규 1, 삭제 1), 경고 0

- [ ] **Step 5: CLI 동작 확인 (수동 스모크)**

Run: `cargo run -p agent-mentor -- rules 2>&1 | tail -3`
Expected: 기존과 동일한 `[severity] R# ...` 출력과 `total N findings`

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/ops.rs crates/core/src/lib.rs crates/core/src/main.rs
git commit -m "refactor: ingest/inventory/rules 오퍼레이션을 ops 모듈로 추출 — CLI·Tauri 공용"
```

---

### Task 4: core mascot 모듈 (RobotSpec 절차 생성)

**Files:**
- Create: `crates/core/src/mascot.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod mascot;`), `crates/core/Cargo.toml` (`sha2 = "0.10"` 추가)

**Interfaces:**
- Consumes: 없음 (env `COMPUTERNAME`/`USERNAME`)
- Produces (Task 6이 사용):
  - `pub struct RobotSpec { pub antenna: u8, pub head: u8, pub eyes: u8, pub body: u8, pub arms: u8, pub palette: u8 }` (`Serialize`)
  - `pub fn stable_identity() -> String`
  - `pub fn robot_spec_for(identity: &str) -> RobotSpec`

- [ ] **Step 1: 실패하는 테스트 작성** (`mascot.rs`에 모듈과 함께)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_deterministic_and_in_range() {
        let a = robot_spec_for("HOSTA|alice");
        let b = robot_spec_for("HOSTA|alice");
        assert_eq!(a, b, "같은 identity → 같은 스펙");
        assert!(a.antenna < ANTENNA_VARIANTS && a.head < HEAD_VARIANTS
            && a.eyes < EYES_VARIANTS && a.body < BODY_VARIANTS
            && a.arms < ARMS_VARIANTS && a.palette < PALETTE_VARIANTS);
    }

    #[test]
    fn different_identity_differs() {
        // SHA-256 기반이므로 이 두 입력은 최소 한 슬롯이 다르다 (사전 확인된 페어).
        assert_ne!(robot_spec_for("HOSTA|alice"), robot_spec_for("HOSTB|bob"));
    }

    #[test]
    fn identity_uses_env_or_fallback() {
        let id = stable_identity();
        assert!(id.contains('|'));
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor mascot 2>&1 | tail -3`
Expected: 컴파일 에러 (`mascot` 미정의)

- [ ] **Step 3: 구현**

```rust
//! 마스코트 로봇 절차 생성 — 안정적 ID 해시로 파츠 조합을 결정한다.
//! 파츠 도트 데이터 자체는 프론트(TS)에 있고, 여기선 "어떤 파츠 조합인지"만 결정.
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const ANTENNA_VARIANTS: u8 = 6;
pub const HEAD_VARIANTS: u8 = 6;
pub const EYES_VARIANTS: u8 = 6;
pub const BODY_VARIANTS: u8 = 6;
pub const ARMS_VARIANTS: u8 = 6;
pub const PALETTE_VARIANTS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RobotSpec {
    pub antenna: u8,
    pub head: u8,
    pub eyes: u8,
    pub body: u8,
    pub arms: u8,
    pub palette: u8,
}

/// 호스트명+사용자명 — 계정 없이 사용자마다 안정적인 시드.
pub fn stable_identity() -> String {
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".into());
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    format!("{host}|{user}")
}

pub fn robot_spec_for(identity: &str) -> RobotSpec {
    let d = Sha256::digest(identity.as_bytes());
    RobotSpec {
        antenna: d[0] % ANTENNA_VARIANTS,
        head: d[1] % HEAD_VARIANTS,
        eyes: d[2] % EYES_VARIANTS,
        body: d[3] % BODY_VARIANTS,
        arms: d[4] % ARMS_VARIANTS,
        palette: d[5] % PALETTE_VARIANTS,
    }
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent-mentor mascot 2>&1 | tail -3`
Expected: `3 passed` (만약 `different_identity_differs`가 실패하면 두 identity 문자열을 다른 페어로 교체 — 해시 충돌이 아니라 6/8 모듈로 공간에서의 우연 일치이므로)

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/mascot.rs crates/core/src/lib.rs crates/core/Cargo.toml Cargo.lock
git commit -m "feat: 마스코트 RobotSpec 절차 생성 — SHA-256(호스트|사용자) 시드 파츠 조합"
```

---

### Task 5: Tauri v2 앱 + Svelte 5 스캐폴드

**Files:**
- Create: `package.json`, `vite.config.ts`, `svelte.config.js`, `tsconfig.json`
- Create: `src/chat.html`, `src/chat.ts`, `src/App.svelte`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Modify: `Cargo.toml` (workspace members에 `src-tauri` 추가), `.gitignore` (`node_modules/`, `dist/` 추가)

**Interfaces:**
- Consumes: `crates/core` (path 의존)
- Produces: 실행 가능한 Tauri 셸. 창 라벨 `"chat"`, dev 포트 1420, `agent_mentor_app::run()` 엔트리. Task 6~9가 이 크레이트를 확장.

- [ ] **Step 1: Node 확인**

Run: `node --version && npm --version`
Expected: v20+ / 10+. 없으면 `winget install OpenJS.NodeJS.LTS` 후 새 셸에서 재확인.

- [ ] **Step 2: 프론트 스캐폴드 파일 작성**

`package.json`:

```json
{
  "name": "agent-mentor-ui",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^5.0.0",
    "@tauri-apps/cli": "^2.0.0",
    "svelte": "^5.0.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0"
  }
}
```

`vite.config.ts`:

```ts
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [svelte()],
  root: 'src',
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    // 멀티페이지: 2단계에서 mascot.html 엔트리 추가 (ESM 설정파일 — __dirname 없음)
    rollupOptions: {
      input: { chat: fileURLToPath(new URL('./src/chat.html', import.meta.url)) },
    },
  },
});
```

`svelte.config.js`:

```js
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
export default { preprocess: vitePreprocess() };
```

`tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "verbatimModuleSyntax": true,
    "types": ["vite/client"]
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`src/chat.html`:

```html
<!doctype html>
<html lang="ko">
  <head>
    <meta charset="UTF-8" />
    <title>agent mentor</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/chat.ts"></script>
  </body>
</html>
```

`src/chat.ts`:

```ts
import { mount } from 'svelte';
import App from './App.svelte';

mount(App, { target: document.getElementById('app')! });
```

`src/App.svelte` (Task 9에서 교체될 최소 확인용):

```svelte
<main>agent mentor — shell OK</main>
```

- [ ] **Step 3: src-tauri 스캐폴드 작성**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "agent-mentor-app"
version = "0.1.0"
edition = "2021"

[lib]
name = "agent_mentor_app"
# Windows 데스크톱 전용: cdylib/staticlib(모바일용)을 넣으면 GNU ld의
# DLL export ordinal 한계(~65535)에 걸려 링크가 실패한다. rlib만 사용.
crate-type = ["rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-autostart = "2"
agent-mentor = { path = "../crates/core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
chrono = "0.4"
notify = "6"
```

`src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Agent Mentor",
  "version": "0.1.0",
  "identifier": "dev.agentmentor.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "chat",
        "url": "chat.html",
        "title": "agent mentor",
        "width": 900,
        "height": 640,
        "visible": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": { "active": false, "icon": [] }
}
```

(주: `visible: true`는 스캐폴드 확인용. Task 8에서 트레이 상주로 바꾸며 `false`로 변경. 아이콘도 Task 8에서 추가.)

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "chat 창 기본 권한",
  "windows": ["chat"],
  "permissions": ["core:default"]
}
```

`src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agent_mentor_app::run()
}
```

`src-tauri/src/lib.rs`:

```rust
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("tauri 실행 실패");
}
```

루트 `Cargo.toml`의 members를 `["crates/core", "src-tauri"]`로. `.gitignore`에 `node_modules/`, `dist/` 추가.

- [ ] **Step 4: 빌드 검증 (⚠ GNU 툴체인 게이트)**

Run: `npm install && npm run build`
Expected: `dist/chat.html` 생성

Run: `cargo check -p agent-mentor-app 2>&1 | tail -5`
Expected: 성공. **링커 에러(WebView2Loader 등) 발생 시 여기서 중단하고 사용자에게 보고** — Global Constraints의 MSVC 전환 절차 협의.

- [ ] **Step 5: 수동 스모크**

Run: `npm run tauri dev`
Expected: "agent mentor — shell OK" 창 표시. Ctrl+C로 종료.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: Tauri v2 앱 + Svelte 5 스캐폴드 — chat 창 셸, workspace 편입"
```

---

### Task 6: AppState + 커맨드 배선

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 2의 store 쿼리, Task 4의 mascot
- Produces (Task 7·9가 사용):
  - `pub struct AppState { pub store: std::sync::Mutex<SqliteStore> }` (lib.rs)
  - 커맨드: `get_summary() -> Summary`, `list_findings() -> Vec<FindingRow>`, `list_diary_dates() -> Vec<String>`, `get_diary(date) -> Option<String>`, `get_mascot_seed() -> RobotSpec`, `get_settings() -> HashMap<String,String>`, `set_setting(key, value)`
  - `pub struct Summary { date, session_count, tok_input, tok_output, tok_cache_read, tok_cache_create, total_sessions, est_tokens_saved_total: u64…, last_scan: Option<String> }` (Serialize)

- [ ] **Step 1: 실패하는 테스트 작성** (`commands.rs` 하단 — 커맨드 본체는 `*_inner(&SqliteStore)` 순수 함수로 두고 그것을 테스트)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_mentor::store::SqliteStore;

    #[test]
    fn summary_inner_on_empty_store() {
        let store = SqliteStore::open_in_memory().unwrap();
        let s = summary_inner(&store).unwrap();
        assert_eq!(s.session_count, 0);
        assert_eq!(s.total_sessions, 0);
        assert_eq!(s.last_scan, None);
        assert_eq!(s.date.len(), 10); // YYYY-MM-DD
    }

    #[test]
    fn diary_inner_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("d.md");
        std::fs::write(&p, "일기 본문").unwrap();
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_diary_index("2026-07-02", "Windows", p.to_str().unwrap(), 10, "mock").unwrap();
        assert_eq!(diary_inner(&store, "2026-07-02").unwrap(), Some("일기 본문".into()));
        assert_eq!(diary_inner(&store, "2099-01-01").unwrap(), None);
    }
}
```

(`src-tauri/Cargo.toml`에 `[dev-dependencies] tempfile = "3"` 추가.)

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor-app 2>&1 | tail -3`
Expected: 컴파일 에러 (`commands` 미정의)

- [ ] **Step 3: 구현**

`src-tauri/src/commands.rs`:

```rust
use crate::AppState;
use agent_mentor::mascot::{robot_spec_for, stable_identity, RobotSpec};
use agent_mentor::store::{FindingRow, SqliteStore};
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct Summary {
    pub date: String,
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub tok_cache_read: u64,
    pub tok_cache_create: u64,
    pub total_sessions: u64,
    pub est_tokens_saved_total: u64,
    pub last_scan: Option<String>,
}

pub fn summary_inner(store: &SqliteStore) -> anyhow::Result<Summary> {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let day = store.summary_for_date(&date)?;
    Ok(Summary {
        date,
        session_count: day.session_count,
        tok_input: day.tok_input,
        tok_output: day.tok_output,
        tok_cache_read: day.tok_cache_read,
        tok_cache_create: day.tok_cache_create,
        total_sessions: store.total_sessions()?,
        est_tokens_saved_total: store.sum_est_tokens_saved()?,
        last_scan: store.get_setting("last_scan_ts")?,
    })
}

pub fn diary_inner(store: &SqliteStore, date: &str) -> anyhow::Result<Option<String>> {
    match store.diary_path_for(date)? {
        Some(path) => Ok(Some(std::fs::read_to_string(path)?)),
        None => Ok(None),
    }
}

fn lock<'a>(state: &'a State<AppState>) -> Result<std::sync::MutexGuard<'a, SqliteStore>, String> {
    state.store.lock().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_summary(state: State<AppState>) -> Result<Summary, String> {
    summary_inner(&lock(&state)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_findings(state: State<AppState>) -> Result<Vec<FindingRow>, String> {
    lock(&state)?.list_findings_current().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_diary_dates(state: State<AppState>) -> Result<Vec<String>, String> {
    lock(&state)?.diary_dates().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_diary(state: State<AppState>, date: String) -> Result<Option<String>, String> {
    diary_inner(&lock(&state)?, &date).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_mascot_seed() -> RobotSpec {
    robot_spec_for(&stable_identity())
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<HashMap<String, String>, String> {
    Ok(lock(&state)?
        .all_settings()
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect())
}

#[tauri::command]
pub fn set_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    const ALLOWED: &[&str] = &["mascot_visible", "chatter_level", "content_protected"];
    if !ALLOWED.contains(&key.as_str()) {
        return Err(format!("허용되지 않은 설정 키: {key}"));
    }
    lock(&state)?.set_setting(&key, &value).map_err(|e| e.to_string())
}
```

`src-tauri/src/lib.rs` 교체:

```rust
mod commands;

use agent_mentor::store::SqliteStore;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub store: Mutex<SqliteStore>,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = SqliteStore::open(&dir.join("agent-mentor.db"))?;
            app.manage(AppState { store: Mutex::new(store) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_summary,
            commands::list_findings,
            commands::list_diary_dates,
            commands::get_diary,
            commands::get_mascot_seed,
            commands::get_settings,
            commands::set_setting,
        ])
        .run(tauri::generate_context!())
        .expect("tauri 실행 실패");
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p agent-mentor-app 2>&1 | tail -3`
Expected: `2 passed`, 경고 0

- [ ] **Step 5: Commit**

```bash
git add src-tauri
git commit -m "feat: AppState + 커맨드 배선 — summary/findings/diary/mascot/settings"
```

---

### Task 7: notify 파이프라인 (디바운스·diff·다이어리 스케줄)

**Files:**
- Create: `src-tauri/src/pipeline.rs`
- Modify: `src-tauri/src/lib.rs` (채널·스레드 기동, `run_scan_now` 커맨드 등록), `src-tauri/src/commands.rs` (`run_scan_now` 추가)

**Interfaces:**
- Consumes: Task 3 `ops::{run_ingest, run_inventory, run_rules}`, Task 2 쿼리, core `diary::{assemble_brief, generate_diary, DiaryConfig}`, `diary::engine::OpenAiCompatEngine`, `hosts::enumerate_hosts`
- Produces:
  - `pub enum PipelineMsg { FileChanged, RunNow }` — `AppState.scan_tx: std::sync::mpsc::Sender<PipelineMsg>` 필드 추가
  - 이벤트: `"coach:finding"` (payload `Vec<FindingRow>` — 신규/악화만), `"diary:ready"` (payload `String` date), `"scan:done"` (payload `String` rfc3339)
  - 순수 함수: `diff_findings`, `missing_diary_dates`, `debounce_loop` (단위 테스트 대상)

- [ ] **Step 1: 실패하는 테스트 작성** (`pipeline.rs` 하단)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn diff_detects_new_and_escalated_only() {
        let before: HashMap<String, String> = [
            ("a".to_string(), "suggest".to_string()),
            ("b".to_string(), "warn".to_string()),
        ].into();
        let after = vec![
            ("a".to_string(), "warn".to_string()),    // 악화 → 포함
            ("b".to_string(), "warn".to_string()),    // 동일 → 제외
            ("c".to_string(), "info".to_string()),    // 신규 → 포함
        ];
        let mut got = diff_findings(&before, &after);
        got.sort();
        assert_eq!(got, vec!["a", "c"]);
    }

    #[test]
    fn missing_dates_up_to_yesterday_with_lookback() {
        let existing = vec!["2026-07-01".to_string()];
        let got = missing_diary_dates(&existing, "2026-07-03", 3);
        assert_eq!(got, vec!["2026-06-30", "2026-07-02"]); // 오늘(03)은 제외, 01은 존재
    }

    #[test]
    fn debounce_coalesces_bursts_and_runs_once() {
        let (tx, rx) = mpsc::channel();
        for _ in 0..5 { tx.send(PipelineMsg::FileChanged).unwrap(); }
        drop(tx); // 채널 닫힘 → 디바운스 창 소진 후 1회 실행하고 루프 종료
        let mut runs = 0;
        debounce_loop(rx, Duration::from_millis(20), || runs += 1);
        assert_eq!(runs, 1, "burst 5건 → 1회 실행");
    }

    #[test]
    fn run_now_bypasses_debounce() {
        let (tx, rx) = mpsc::channel();
        tx.send(PipelineMsg::RunNow).unwrap();
        drop(tx);
        let mut runs = 0;
        debounce_loop(rx, Duration::from_secs(60), || runs += 1);
        assert_eq!(runs, 1);
    }
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor-app pipeline 2>&1 | tail -3`
Expected: 컴파일 에러

- [ ] **Step 3: 순수 로직 구현**

```rust
//! notify 감시 → 디바운스 배치 → ingest→inventory→rules → finding diff → 이벤트.
//! 이 파일의 순수 함수(diff/debounce/missing_dates)는 Tauri 무관 — 단위 테스트 대상.
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

pub enum PipelineMsg {
    FileChanged,
    RunNow,
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "warn" => 2,
        "suggest" => 1,
        _ => 0,
    }
}

/// before: dedup_key→severity 스냅샷. after의 각 항목 중 신규이거나 심각도가 오른 것만.
pub fn diff_findings(before: &HashMap<String, String>, after: &[(String, String)]) -> Vec<String> {
    after
        .iter()
        .filter(|(k, sev)| match before.get(k) {
            None => true,
            Some(prev) => severity_rank(sev) > severity_rank(prev),
        })
        .map(|(k, _)| k.clone())
        .collect()
}

/// 어제까지 중 다이어리가 없는 날짜들 (오래된 것부터, 최대 lookback일 소급).
pub fn missing_diary_dates(existing: &[String], today: &str, lookback: i64) -> Vec<String> {
    let Ok(today) = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return Vec::new();
    };
    (1..=lookback)
        .rev()
        .map(|i| (today - chrono::Duration::days(i)).format("%Y-%m-%d").to_string())
        .filter(|d| !existing.contains(d))
        .collect()
}

/// FileChanged는 window만큼 조용해질 때까지 모았다가 1회 실행. RunNow는 즉시 실행.
/// 채널이 닫히면 (대기 중 burst가 있으면 마저 실행 후) 종료.
pub fn debounce_loop(rx: Receiver<PipelineMsg>, window: Duration, mut run: impl FnMut()) {
    loop {
        match rx.recv() {
            Ok(PipelineMsg::RunNow) => run(),
            Ok(PipelineMsg::FileChanged) => {
                let mut deadline = Instant::now() + window;
                let disconnected = loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    match rx.recv_timeout(left) {
                        Ok(PipelineMsg::FileChanged) => deadline = Instant::now() + window,
                        Ok(PipelineMsg::RunNow) => break false,
                        Err(RecvTimeoutError::Timeout) => break false,
                        Err(RecvTimeoutError::Disconnected) => break true,
                    }
                };
                run();
                if disconnected {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}
```

- [ ] **Step 4: 순수 로직 테스트 통과 확인**

Run: `cargo test -p agent-mentor-app pipeline 2>&1 | tail -3`
Expected: `4 passed`

- [ ] **Step 5: 파이프라인 실행부·watcher·다이어리 스케줄 구현** (`pipeline.rs`에 이어서)

```rust
use crate::AppState;
use agent_mentor::diary::engine::OpenAiCompatEngine;
use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
use agent_mentor::hosts::enumerate_hosts;
use agent_mentor::store::SqliteStore;
use notify::Watcher;
use tauri::{AppHandle, Emitter, Manager};

/// 시작 시 풀 스캔 1회 후, 파일 변경을 디바운스하며 상주 감시.
pub fn start(app: AppHandle, rx: Receiver<PipelineMsg>, tx: std::sync::mpsc::Sender<PipelineMsg>) {
    std::thread::spawn(move || {
        let _watchers = spawn_watchers(tx); // Vec을 스레드가 소유 — drop되면 감시 중단되므로 유지
        run_pipeline_once(&app);
        let app2 = app.clone();
        debounce_loop(rx, Duration::from_secs(60), move || run_pipeline_once(&app2));
    });
}

/// 호스트별 projects/ 감시. Windows는 RecommendedWatcher, WSL(UNC)은 30초 PollWatcher.
/// 주: 스펙 §7의 "watcher 사망 시 자동 재시작"은 1단계에선 유예 — 불안정한 쪽(WSL UNC)은
/// 처음부터 PollWatcher라 사망 모드가 사실상 없고, 재시작 로직은 2단계에서 필요 시 추가.
fn spawn_watchers(tx: std::sync::mpsc::Sender<PipelineMsg>) -> Vec<Box<dyn Watcher + Send>> {
    let mut out: Vec<Box<dyn Watcher + Send>> = Vec::new();
    for hs in enumerate_hosts() {
        let projects = hs.claude_root.join("projects");
        if !projects.is_dir() {
            continue;
        }
        let tx2 = tx.clone();
        let handler = move |res: notify::Result<notify::Event>| {
            if res.is_ok() {
                let _ = tx2.send(PipelineMsg::FileChanged);
            }
        };
        let watcher: notify::Result<Box<dyn Watcher + Send>> = if hs.host == "Windows" {
            notify::recommended_watcher(handler).map(|w| Box::new(w) as _)
        } else {
            notify::PollWatcher::new(
                handler,
                notify::Config::default().with_poll_interval(Duration::from_secs(30)),
            )
            .map(|w| Box::new(w) as _)
        };
        match watcher {
            Ok(mut w) => {
                if let Err(e) = w.watch(&projects, notify::RecursiveMode::Recursive) {
                    eprintln!("warn: {} 감시 실패: {e}", projects.display());
                } else {
                    out.push(w);
                }
            }
            Err(e) => eprintln!("warn: {} watcher 생성 실패: {e}", hs.host),
        }
    }
    out
}

pub fn run_pipeline_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    let result = (|| -> anyhow::Result<()> {
        let mut store = state
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        let before: HashMap<String, String> = store.finding_severities()?.into_iter().collect();

        let report = agent_mentor::ops::run_ingest(&store)?;
        for w in &report.warnings {
            eprintln!("warn: {w}");
        }
        for w in agent_mentor::ops::run_inventory(&mut store)? {
            eprintln!("warn: {w}");
        }
        agent_mentor::ops::run_rules(&store)?;

        let after = store.finding_severities()?;
        let fresh = diff_findings(&before, &after);
        let now = chrono::Utc::now().to_rfc3339();
        store.set_setting("last_scan_ts", &now)?;

        if !fresh.is_empty() {
            let rows: Vec<_> = store
                .list_findings_current()?
                .into_iter()
                .filter(|f| fresh.contains(&f.dedup_key))
                .collect();
            app.emit("coach:finding", &rows)?;
        }

        maybe_generate_diaries(app, &store)?;
        app.emit("scan:done", &now)?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("pipeline error: {e}"); // UI 알림 금지 — 로그만 (스펙 §7)
    }
}

/// 자정 이후 첫 완료 시 전날치(최대 7일 소급) 생성. 엔진 미설정이면 스킵(mock 일기 방지).
fn maybe_generate_diaries(app: &AppHandle, store: &SqliteStore) -> anyhow::Result<()> {
    let Some(engine) = OpenAiCompatEngine::from_env() else {
        return Ok(());
    };
    let vault = app.path().app_data_dir()?.join("diary");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let existing = store.diary_dates()?;
    for date in missing_diary_dates(&existing, &today, 7) {
        let cfg = DiaryConfig { vault_dir: vault.clone(), ..DiaryConfig::default() };
        let brief = assemble_brief(store, "Windows", &date, &cfg)?;
        if brief.totals.session_count == 0 {
            continue; // 활동 없는 날은 일기 없음
        }
        generate_diary(store, &engine, &brief, &cfg)?;
        app.emit("diary:ready", &date)?;
    }
    Ok(())
}
```

`commands.rs`에 추가:

```rust
#[tauri::command]
pub fn run_scan_now(state: State<AppState>) -> Result<(), String> {
    state
        .scan_tx
        .send(crate::pipeline::PipelineMsg::RunNow)
        .map_err(|e| e.to_string())
}
```

`lib.rs` 수정 — `mod pipeline;`, AppState에 `scan_tx` 추가, setup에서 기동, 핸들러 등록:

```rust
mod commands;
mod pipeline;

use agent_mentor::store::SqliteStore;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub store: Mutex<SqliteStore>,
    pub scan_tx: std::sync::mpsc::Sender<pipeline::PipelineMsg>,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = SqliteStore::open(&dir.join("agent-mentor.db"))?;
            let (tx, rx) = std::sync::mpsc::channel();
            app.manage(AppState { store: Mutex::new(store), scan_tx: tx.clone() });
            pipeline::start(app.handle().clone(), rx, tx);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_summary,
            commands::list_findings,
            commands::list_diary_dates,
            commands::get_diary,
            commands::get_mascot_seed,
            commands::get_settings,
            commands::set_setting,
            commands::run_scan_now,
        ])
        .run(tauri::generate_context!())
        .expect("tauri 실행 실패");
}
```

- [ ] **Step 6: 전체 테스트 + 수동 스모크**

Run: `cargo test -p agent-mentor-app 2>&1 | tail -3`
Expected: `6 passed` (commands 2 + pipeline 4), 경고 0

Run: `npm run tauri dev` — 콘솔에 파이프라인 경고 로그(있다면)와 함께 창 표시. Claude Code 세션 하나를 잠깐 굴린 뒤 ~60초 내 재스캔되는지 콘솔로 확인. Ctrl+C.

- [ ] **Step 7: Commit**

```bash
git add src-tauri
git commit -m "feat: notify 디바운스 파이프라인 — ingest→rules→finding diff 이벤트, 다이어리 자정 스케줄"
```

---

### Task 8: 트레이 상주 + autostart + 닫기=숨김

**Files:**
- Create: `src-tauri/src/tray.rs`, `src-tauri/icons/` (생성물), `app-icon.png` (원본, 루트)
- Modify: `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: Task 7 `AppState.scan_tx`
- Produces: 트레이 아이콘(좌클릭=chat 토글, 메뉴: 열기/지금 스캔/시작 시 실행/종료), 시작 시 창 숨김 상주

- [ ] **Step 1: 플레이스홀더 아이콘 생성** (PowerShell — 정식 로봇 아이콘은 2단계에서 교체)

```powershell
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap 1024,1024
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::FromArgb(255,40,42,64))
$b = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255,122,220,232))
$g.FillRectangle($b, 288, 384, 448, 384)   # 몸통
$g.FillRectangle($b, 384, 224, 256, 128)   # 머리
$g.FillRectangle($b, 496, 128, 32, 96)     # 안테나
$g.Dispose(); $bmp.Save("app-icon.png"); $bmp.Dispose()
```

Run: `npx tauri icon app-icon.png`
Expected: `src-tauri/icons/` 아래 `icon.ico`, `128x128.png` 등 생성

- [ ] **Step 2: tauri.conf.json 수정** — 트레이 상주 전환

`app.windows[0].visible`을 `false`로, `bundle`을 다음으로:

```json
"bundle": { "active": false, "icon": ["icons/icon.ico", "icons/128x128.png"] }
```

- [ ] **Step 3: tray.rs 구현**

```rust
use crate::{pipeline::PipelineMsg, AppState};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tauri_plugin_autostart::ManagerExt;

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "지금 스캔", true, None::<&str>)?;
    let auto_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "시작 시 실행", true, auto_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &scan, &autostart, &quit])?;

    let autostart_item = autostart.clone();
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("window icon"))
        .tooltip("Agent Mentor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id().as_ref() {
            "open" => show_chat(app),
            "scan" => {
                let _ = app.state::<AppState>().scan_tx.send(PipelineMsg::RunNow);
            }
            "autostart" => {
                let al = app.autolaunch();
                let cur = al.is_enabled().unwrap_or(false);
                let _ = if cur { al.disable() } else { al.enable() };
                let _ = autostart_item.set_checked(al.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_chat(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_chat(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
```

- [ ] **Step 4: lib.rs 배선** — autostart 플러그인, 트레이, 닫기=숨김

`run()`의 Builder 체인에 추가:

```rust
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent, // Windows에선 무시되는 인자
            None,
        ))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "chat" {
                    let _ = window.hide(); // 상주: destroy 대신 hide (스펙 §3)
                    api.prevent_close();
                }
            }
        })
```

그리고 setup 끝에 (pipeline::start 다음):

```rust
            tray::setup_tray(app.handle())?;
```

파일 상단에 `mod tray;` 추가.

- [ ] **Step 5: 수동 검증**

Run: `npm run tauri dev`
Expected 체크리스트:
- 시작 시 창 없음, 트레이에 아이콘.
- 좌클릭 → chat 창 표시. X 클릭 → 숨김(프로세스 유지). 좌클릭 → 다시 표시.
- 우클릭 메뉴 4항목. "시작 시 실행" 체크 → PowerShell `Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'`에 Agent Mentor 항목 등장, 체크 해제 → 사라짐.
- "지금 스캔" → 콘솔에 파이프라인 로그. "종료" → 프로세스 종료.

- [ ] **Step 6: 회귀 확인 + Commit**

Run: `cargo test 2>&1 | tail -3` (워크스페이스 전체)
Expected: 전부 통과, 경고 0

```bash
git add -A
git commit -m "feat: 트레이 상주(좌클릭 토글·메뉴) + autostart 토글 + 닫기=숨김"
```

---

### Task 9: chat 창 UI — 홈·코칭 탭

**Files:**
- Create: `src/lib/api.ts`, `src/lib/ui/HomeTab.svelte`, `src/lib/ui/CoachTab.svelte`
- Modify: `src/App.svelte` (전면 교체)

**Interfaces:**
- Consumes: Task 6·7 커맨드(`get_summary`, `list_findings`, `run_scan_now`)와 이벤트(`scan:done`, `coach:finding`)
- Produces: 사용자에게 보이는 1단계 완성 화면 (다이어리·채팅 탭은 "준비 중" 자리)

- [ ] **Step 1: api.ts 작성**

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface Summary {
  date: string;
  session_count: number;
  tok_input: number;
  tok_output: number;
  tok_cache_read: number;
  tok_cache_create: number;
  total_sessions: number;
  est_tokens_saved_total: number;
  last_scan: string | null;
}

export interface Finding {
  rule_id: string;
  severity: 'info' | 'suggest' | 'warn';
  scope_host: string | null;
  scope_project: string | null;
  scope_kind: string;
  scope_ref: string;
  evidence: unknown;
  est_tokens_saved: number;
  prescription: { kind: string; payload: unknown } | null;
  dedup_key: string;
  last_seen: string | null;
  occurrences: number;
}

export const getSummary = () => invoke<Summary>('get_summary');
export const listFindings = () => invoke<Finding[]>('list_findings');
export const runScanNow = () => invoke<void>('run_scan_now');

export const onScanDone = (cb: (ts: string) => void): Promise<UnlistenFn> =>
  listen<string>('scan:done', (e) => cb(e.payload));
export const onNewFindings = (cb: (rows: Finding[]) => void): Promise<UnlistenFn> =>
  listen<Finding[]>('coach:finding', (e) => cb(e.payload));
```

- [ ] **Step 2: App.svelte 교체** (Svelte 5 runes)

```svelte
<script lang="ts">
  import HomeTab from './lib/ui/HomeTab.svelte';
  import CoachTab from './lib/ui/CoachTab.svelte';
  import { getSummary, onScanDone, type Summary } from './lib/api';

  let tab = $state<'home' | 'diary' | 'coach' | 'chat'>('home');
  let summary = $state<Summary | null>(null);

  async function refresh() {
    summary = await getSummary();
  }
  refresh();
  onScanDone(() => refresh());

  const mood = $derived(
    summary && summary.est_tokens_saved_total > 0 ? '절약할 게 보여요…' : '평화로워요'
  );
</script>

<div class="shell">
  <aside class="side">
    <div class="counter">
      TODAY <b>{summary?.session_count ?? '–'}</b> · TOTAL <b>{summary?.total_sessions ?? '–'}</b>
    </div>
    <div class="miniroom">미니룸<br /><span class="robot">🤖</span><br /><small>(2단계 입주 예정)</small></div>
    <div class="mood">오늘의 기분: {mood}</div>
  </aside>
  <main class="main">
    <nav class="tabs">
      <button class:active={tab === 'home'} onclick={() => (tab = 'home')}>홈</button>
      <button class:active={tab === 'diary'} onclick={() => (tab = 'diary')}>다이어리</button>
      <button class:active={tab === 'coach'} onclick={() => (tab = 'coach')}>코칭</button>
      <button class:active={tab === 'chat'} onclick={() => (tab = 'chat')}>채팅</button>
    </nav>
    {#if tab === 'home'}
      <HomeTab {summary} />
    {:else if tab === 'coach'}
      <CoachTab />
    {:else}
      <section class="placeholder">준비 중이에요, 주인. (다음 단계에서 열려요)</section>
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #f3f0e6;
    color: #33325a;
    font-family: 'Galmuri11', 'DungGeunMo', 'Courier New', monospace;
    font-size: 14px;
  }
  .shell { display: flex; height: 100vh; }
  .side {
    width: 200px; padding: 12px; background: #e8e4f0;
    border-right: 3px solid #33325a; display: flex; flex-direction: column; gap: 12px;
  }
  .counter { font-size: 12px; }
  .miniroom {
    border: 3px solid #33325a; background: #fffdf5; text-align: center;
    padding: 16px 8px; box-shadow: 4px 4px 0 #c9c3dd;
  }
  .robot { font-size: 40px; }
  .mood { font-size: 12px; margin-top: auto; }
  .main { flex: 1; display: flex; flex-direction: column; }
  .tabs { display: flex; gap: 4px; padding: 8px 8px 0; border-bottom: 3px solid #33325a; }
  .tabs button {
    border: 3px solid #33325a; border-bottom: none; background: #d9d4e8;
    padding: 6px 14px; font: inherit; cursor: pointer;
  }
  .tabs button.active { background: #fffdf5; }
  .placeholder { padding: 24px; }
</style>
```

- [ ] **Step 3: HomeTab.svelte 작성**

```svelte
<script lang="ts">
  import { runScanNow, type Summary } from '../api';

  let { summary }: { summary: Summary | null } = $props();

  const fmt = (n: number | undefined) => (n ?? 0).toLocaleString();
  let scanning = $state(false);

  async function scan() {
    scanning = true;
    await runScanNow();
    setTimeout(() => (scanning = false), 3000); // 갱신 자체는 App의 scan:done 리스너가 수행
  }
</script>

<section class="home">
  <h2>{summary?.date ?? ''} 오늘 요약</h2>
  <div class="cards">
    <div class="card">세션 <b>{fmt(summary?.session_count)}</b></div>
    <div class="card">입력 <b>{fmt(summary?.tok_input)}</b> tok</div>
    <div class="card">출력 <b>{fmt(summary?.tok_output)}</b> tok</div>
    <div class="card">캐시 읽기 <b>{fmt(summary?.tok_cache_read)}</b> tok</div>
    <div class="card save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</div>
  </div>
  <footer class="status">
    마지막 스캔: {summary?.last_scan ? new Date(summary.last_scan).toLocaleString() : '아직 없음'}
    <button onclick={scan} disabled={scanning}>{scanning ? '스캔 중…' : '지금 스캔'}</button>
  </footer>
</section>

<style>
  .home { padding: 16px; display: flex; flex-direction: column; flex: 1; }
  .cards { display: flex; flex-wrap: wrap; gap: 10px; }
  .card {
    border: 3px solid #33325a; background: #fffdf5; padding: 12px 16px;
    box-shadow: 4px 4px 0 #c9c3dd;
  }
  .card.save { background: #fdf1c7; }
  .status { margin-top: auto; display: flex; justify-content: space-between; align-items: center; font-size: 12px; }
  .status button { border: 3px solid #33325a; background: #d9d4e8; font: inherit; padding: 4px 10px; cursor: pointer; }
</style>
```

- [ ] **Step 4: CoachTab.svelte 작성**

```svelte
<script lang="ts">
  import { listFindings, onNewFindings, type Finding } from '../api';

  let findings = $state<Finding[]>([]);
  let open = $state<string | null>(null);

  async function refresh() {
    findings = await listFindings();
  }
  refresh();
  onNewFindings(() => refresh());

  const icon = (s: Finding['severity']) => (s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ');
</script>

<section class="coach">
  {#if findings.length === 0}
    <p>지적할 게 없어요, 주인. 완벽해요!</p>
  {:else}
    {#each findings as f (f.dedup_key)}
      <article class="finding" class:warn={f.severity === 'warn'}>
        <button class="head" onclick={() => (open = open === f.dedup_key ? null : f.dedup_key)}>
          <span>{icon(f.severity)} [{f.rule_id}] {f.scope_kind}: {f.scope_ref}</span>
          <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
        </button>
        {#if open === f.dedup_key}
          <div class="detail">
            <div>범위: {f.scope_host ?? '-'} / {f.scope_project ?? '-'} · {f.occurrences}회 관측</div>
            <pre>{JSON.stringify(f.evidence, null, 2)}</pre>
            {#if f.prescription}
              <div class="rx">처방: {f.prescription.kind}</div>
              <pre>{JSON.stringify(f.prescription.payload, null, 2)}</pre>
            {/if}
          </div>
        {/if}
      </article>
    {/each}
  {/if}
</section>

<style>
  .coach { padding: 16px; overflow-y: auto; }
  .finding { border: 3px solid #33325a; background: #fffdf5; margin-bottom: 10px; box-shadow: 4px 4px 0 #c9c3dd; }
  .finding.warn { background: #fde8e0; }
  .head {
    width: 100%; display: flex; justify-content: space-between; gap: 8px;
    border: none; background: none; font: inherit; padding: 10px 12px; cursor: pointer; text-align: left;
  }
  .save { white-space: nowrap; }
  .detail { border-top: 2px dashed #33325a; padding: 10px 12px; font-size: 12px; }
  pre { background: #efece2; padding: 8px; overflow-x: auto; }
  .rx { margin-top: 6px; font-weight: bold; }
</style>
```

- [ ] **Step 5: 빌드 + 수동 검증**

Run: `npm run build`
Expected: 에러 없이 dist 생성

Run: `npm run tauri dev` 후 체크리스트:
- 트레이 좌클릭 → 미니홈피 창. 사이드바 TODAY/TOTAL에 실데이터.
- 홈 탭: 카드 수치 = CLI `cargo run -p agent-mentor -- ingest` 후와 일관.
- 코칭 탭: 실제 finding 목록 표시, 클릭 시 evidence/처방 펼침.
- "지금 스캔" → 잠시 후 마지막 스캔 시각 갱신.
- 다이어리·채팅 탭 → "준비 중" 자리.

- [ ] **Step 6: Commit**

```bash
git add src package.json
git commit -m "feat: chat 창 미니홈피 1단계 — 홈·코칭 탭, 사이드바, 이벤트 갱신"
```

---

### Task 10: 최종 검증

**Files:** 없음 (검증만)

- [ ] **Step 1: 전체 테스트**

Run: `cargo test 2>&1 | tail -5`
Expected: workspace 전체(core 93 + app 6 = 99) 통과, 경고 0

- [ ] **Step 2: 수동 E2E 체크리스트** (`npm run tauri dev`)

- [ ] 시작 → 창 없이 트레이만, ~수 초 내 초기 풀 스캔 로그
- [ ] 좌클릭 → 홈 탭 실데이터 / 코칭 탭 finding / X → 숨김 → 재오픈
- [ ] 실제 Claude Code 세션 활동 후 60초 내 자동 재스캔(콘솔), 새 finding 있으면 코칭 탭 자동 갱신
- [ ] autostart 토글 ↔ HKCU Run 레지스트리 반영
- [ ] `AGENT_MENTOR_ENGINE_URL` 설정 상태에서 재시작 → 전날 일기 생성 + `diary:ready` (미설정이면 스킵 확인)
- [ ] "종료" → 프로세스 완전 종료

- [ ] **Step 3: 이슈 없으면 브랜치 마무리** — superpowers:finishing-a-development-branch 스킬로 병합/PR 결정
