# 코칭 v3 PR① — 정리·수집 기반 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 코칭 v3 스펙(§3 처분 + §4 수집)의 기반 PR — 잔소리 룰 4종 은퇴·R10 강등·R7 확장과, 후속 룰(R13~R22)이 쓸 수집 신호를 깐다.

**Architecture:** 룰 은퇴는 ops.rs 등록 해제 + findings 일괄 삭제(코드·테스트 보존, R5 전례). 수집은 어댑터 매핑 확장(Agent 툴, permission-mode/compact_boundary 라인, 시크릿 플래그) + 인벤토리 스캔 2종(개인 스킬, 호스트 설정) + sessions.subagent_files. 신규 라인 신호는 전부 **events 행**으로 저장(내장 dedup으로 재수집 멱등).

**Tech Stack:** Rust (crates/core만 — 프론트 무변경), rusqlite, cargo test.

**스펙:** `docs/design/overview-mentor/specs/2026-07-19-coaching-v3-design.md`

## Global Constraints

- 실행 위치: `a-mate/` 루트. 테스트: `cargo test -p agent-mentor` (mac에서 실행 가능 — 경로 하드코딩 금지).
- **관대한 파싱**: `adapter::map`은 어떤 입력에도 하드 실패 금지. 마커 미인식 → 이벤트 미생성(침묵).
- **프로즈·시크릿 본문 DB 미저장** (스펙 §4.2): secret_flag는 pattern_id + 포인터만. 매칭 문자열을 어떤 컬럼·로그에도 쓰지 않는다.
- **룰 코드 보존**: R1·R2·R9·R12는 등록만 해제 — `rules/` 모듈·테스트 파일은 삭제하지 않는다 (R5 전례).
- v3 신규/변경 finding의 `est_tokens_saved`는 0 (스펙 원칙 9). 비용-등가 추정은 R7만 유지.
- 커밋: Conventional Commits, 영어, scope `agent` (예: `feat(agent): …`).
- 신규 의존성 금지 — 시크릿 매칭은 regex 크레이트 없이 수제 매처.
- 스펙 §4.1-4의 구현 정제(합의 필요 없음, 이 플랜의 결정): permission-mode는 `sessions` 카운터 컬럼이 아니라 **events 행**(kind `permission_mode`, `tool_target`=모드)으로 저장. 카운터 증분은 재수집에 멱등이 아니지만 events는 dedup_key가 있어 멱등. 룰(R16/R19, PR②·③)은 SQL 집계로 동일 정보를 얻는다.

---

### Task 1: model.rs — Agent 툴 매핑 + 신규 EventKind 2종

**Files:**
- Modify: `a-mate/crates/core/src/model.rs`

**Interfaces:**
- Produces: `ToolKind::from_raw_name("Agent") == ToolKind::SubAgent`,
  `EventKind::PermissionMode { mode: String }`, `EventKind::SecretFlag { pattern_id: String }` — Task 2(flatten)·Task 3·5(adapter)가 사용.

- [ ] **Step 1: 실패 테스트 작성** — `model.rs`의 기존 `tests` 모듈에 추가:

```rust
    #[test]
    fn agent_raw_name_maps_to_sub_agent() {
        // 신형 하네스는 서브에이전트 툴명이 Task → Agent로 바뀜 (스펙 §4.1-1)
        assert_eq!(ToolKind::from_raw_name("Agent"), ToolKind::SubAgent);
        assert_eq!(ToolKind::from_raw_name("Task"), ToolKind::SubAgent); // 구형 유지
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor agent_raw_name_maps -- --nocapture`
Expected: FAIL — `Other("Agent")` != `SubAgent`

- [ ] **Step 3: 구현** — `from_raw_name`의 match에 한 줄 추가 (`"Task" => ToolKind::SubAgent,` 아래):

```rust
            "Task" | "Agent" => ToolKind::SubAgent,
```

(기존 `"Task" => ToolKind::SubAgent,` 라인을 위 한 줄로 교체.)

- [ ] **Step 4: EventKind 확장** — `EventKind` enum(`model.rs:93` 부근)에 variant 2개 추가:

```rust
    /// permission-mode 라인 (plan·bypassPermissions 등) — R16/R19 재료 (코칭 v3 §4.1-4)
    PermissionMode { mode: String },
    /// 프롬프트/명령에서 시크릿 패턴 감지 — 본문 미저장, pattern_id만 (코칭 v3 §4.2)
    SecretFlag { pattern_id: String },
```

컴파일이 깨지는 match는 이 시점엔 없음(신규 variant 생성처가 아직 없고, `flatten`은 Task 2에서 처리 — 단 `store.rs::flatten`이 exhaustive match라 컴파일 에러 발생). **Task 2의 flatten 수정 전까지 컴파일이 깨지므로, Step 5에서 flatten에 임시가 아닌 최종 arm을 함께 넣는다** (Task 2와 같은 커밋으로 묶지 않기 위해 여기서 flatten arm까지 완성):

`store.rs::flatten`의 match에 arm 2개 추가 (`EventKind::SessionMeta` arm 앞):

```rust
        EventKind::PermissionMode { mode } => (
            "permission_mode".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, Some(mode.clone()), None,
        ),
        EventKind::SecretFlag { pattern_id } => (
            "secret_flag".into(), None, None, None, 0, 0, 0, 0, 0, 0, 0,
            None, None, None, Some(pattern_id.clone()), None,
        ),
```

- [ ] **Step 5: 저장 라운드트립 테스트** — `store.rs` tests 모듈에 추가:

```rust
    #[test]
    fn permission_mode_and_secret_flag_events_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |kind: EventKind, off: u64| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: None, parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-19T10:00:00Z".into()),
            source_file: "s1.jsonl".into(), source_offset: off, kind,
        };
        store.upsert_events(&[
            mk(EventKind::PermissionMode { mode: "plan".into() }, 0),
            mk(EventKind::SecretFlag { pattern_id: "github_token".into() }, 800),
        ]).unwrap();
        let (k1, t1): (String, String) = store.conn.query_row(
            "SELECT kind, tool_target FROM events WHERE kind='permission_mode'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((k1.as_str(), t1.as_str()), ("permission_mode", "plan"));
        let t2: String = store.conn.query_row(
            "SELECT tool_target FROM events WHERE kind='secret_flag'", [], |r| r.get(0)).unwrap();
        assert_eq!(t2, "github_token");
        // 멱등: 같은 이벤트 재삽입 시 dedup (uuid None → source_file:offset 키)
        let n = store.upsert_events(&[mk(EventKind::PermissionMode { mode: "plan".into() }, 0)]).unwrap();
        assert_eq!(n, 0);
    }
```

- [ ] **Step 6: 그린 확인**

Run: `cargo test -p agent-mentor permission_mode_and_secret_flag && cargo test -p agent-mentor agent_raw_name_maps`
Expected: PASS (전체 스위트도 컴파일 그린: `cargo test -p agent-mentor` PASS)

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/model.rs crates/core/src/store.rs
git commit -m "feat(agent): map Agent tool to sub_agent, add permission-mode/secret-flag event kinds"
```

---

### Task 2: store.rs — 신규 테이블 2종 + sessions.subagent_files + v3 마이그레이션

**Files:**
- Modify: `a-mate/crates/core/src/store.rs`

**Interfaces:**
- Consumes: 없음 (독립)
- Produces: 테이블 `personal_skill_inventory(host, scope, name, path PK(host,path), body_chars)`,
  `host_settings(host PK, default_model, effort_level, scanned_at)`, 컬럼 `sessions.subagent_files INTEGER NOT NULL DEFAULT 0`.
  메서드 `replace_personal_skills(&mut self, host, &[PersonalSkill])`, `replace_host_settings(&self, host, Option<&str>, Option<&str>, now_ts)`,
  `session_cwds(&self, host) -> Result<Vec<String>>` — Task 6·7이 사용. `PersonalSkill`은 Task 7의 inventory.rs에 정의되므로,
  **이 태스크에서 inventory.rs에 struct만 먼저 추가**한다.

- [ ] **Step 1: 실패 테스트 작성** — `store.rs` tests 모듈에 추가:

```rust
    #[test]
    fn v3_migration_adds_subagent_files_and_wipes_derived_tables() {
        // 구버전 스키마(서브에이전트 컬럼 없음)를 시뮬레이션할 수 없으므로(open_in_memory는 항상 신 스키마),
        // 신 스키마에서 컬럼 존재 + 기본값 0만 검증한다. 와이프 경로는 기존 v2.1 전례와 동일 패턴.
        let store = SqliteStore::open_in_memory().unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='subagent_files'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "sessions.subagent_files 컬럼이 있어야 함");
    }

    #[test]
    fn replace_personal_skills_and_host_settings_roundtrip() {
        use crate::inventory::PersonalSkill;
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_personal_skills("Windows", &[
            PersonalSkill { name: "gh-commit".into(), path: "C:\\u\\.claude\\skills\\gh-commit\\SKILL.md".into(),
                            body_chars: 300, scope: "user".into() },
        ]).unwrap();
        // replace: 다시 부르면 이전 행 대체
        store.replace_personal_skills("Windows", &[
            PersonalSkill { name: "deploy".into(), path: "D:\\proj\\.claude\\skills\\deploy\\SKILL.md".into(),
                            body_chars: 2400, scope: "project".into() },
        ]).unwrap();
        let (name, scope, chars): (String, String, i64) = store.conn.query_row(
            "SELECT name, scope, body_chars FROM personal_skill_inventory WHERE host='Windows'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((name.as_str(), scope.as_str(), chars), ("deploy", "project", 2400));

        store.replace_host_settings("Windows", Some("claude-fable-5[1m]"), Some("xhigh"), "2026-07-19T10:00:00Z").unwrap();
        store.replace_host_settings("Windows", Some("claude-sonnet-5"), None, "2026-07-19T11:00:00Z").unwrap(); // upsert
        let (m, e): (String, Option<String>) = store.conn.query_row(
            "SELECT default_model, effort_level FROM host_settings WHERE host='Windows'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(m, "claude-sonnet-5");
        assert_eq!(e, None);
    }

    #[test]
    fn session_cwds_returns_distinct_local_host_cwds() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.conn.execute_batch(
            "INSERT INTO sessions (session_id, host, project_id, cwd) VALUES
             ('s1','Windows','p','D:\\proj'), ('s2','Windows','p','D:\\proj'),
             ('s3','wsl:U','p','/home/x/proj'), ('s4','Windows','p',NULL);").unwrap();
        let cwds = store.session_cwds("Windows").unwrap();
        assert_eq!(cwds, vec!["D:\\proj".to_string()]); // distinct + host 필터 + NULL 제외
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor replace_personal_skills -- --nocapture`
Expected: 컴파일 실패 (`PersonalSkill`·메서드 없음)

- [ ] **Step 3: 구현**

3a. `inventory.rs`에 struct 추가 (`PluginRecord` 정의 근처):

```rust
/// 개인(비플러그인) 스킬 1건 — ~/.claude/skills 또는 프로젝트 .claude/skills (코칭 v3 §4.2)
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalSkill {
    pub name: String,
    pub path: String,     // SKILL.md 절대 경로 (PK 성분)
    pub body_chars: u64,  // SKILL.md 전문 글자수 — R13 PersonalSkillHygiene 판정 재료
    pub scope: String,    // "user" | "project"
}
```

3b. `store.rs` SCHEMA 상수에 테이블 2개 추가 + sessions에 컬럼 추가:

`CREATE TABLE IF NOT EXISTS sessions (...)` 정의의 `first_prompt_offset INTEGER` 뒤에 `, subagent_files INTEGER NOT NULL DEFAULT 0` 추가. SCHEMA 끝(content_items 뒤)에:

```sql
CREATE TABLE IF NOT EXISTS personal_skill_inventory (
  host TEXT NOT NULL, scope TEXT NOT NULL, name TEXT NOT NULL,
  path TEXT NOT NULL, body_chars INTEGER DEFAULT 0,
  PRIMARY KEY (host, path)
);
CREATE TABLE IF NOT EXISTS host_settings (
  host TEXT PRIMARY KEY, default_model TEXT, effort_level TEXT, scanned_at TEXT
);
```

3c. `migrate()`에 v3 블록 추가 (v2.1 블록 뒤). 라인 파생 신호가 늘었으므로(Agent 매핑·permission-mode·secret_flag·first_prompt 오염 수정) 전체 재수집 필요:

```rust
    // v3 수집 마이그레이션 — subagent_files 부재 시 컬럼 추가 + 전체 재수집.
    // (Agent 툴 매핑·permission-mode·secret_flag·first_prompt 오염 수정이 라인 재해석을 요구 — 스펙 §4.4)
    let has_subagent_files = conn
        .prepare("SELECT 1 FROM pragma_table_info('sessions') WHERE name='subagent_files'")?
        .exists([])?;
    if !has_subagent_files {
        conn.execute_batch(
            "ALTER TABLE sessions ADD COLUMN subagent_files INTEGER NOT NULL DEFAULT 0;
             DELETE FROM events; DELETE FROM sessions; DELETE FROM ingest_state; DELETE FROM daily_rollup;",
        )?;
    }
```

3d. `store.rs`에 메서드 3개 추가 (`replace_plugin_inventory` 근처, 같은 스타일):

```rust
    /// 개인 스킬 인벤토리 전체 교체 (host 단위) — 코칭 v3 §4.2
    pub fn replace_personal_skills(
        &mut self,
        host: &str,
        skills: &[crate::inventory::PersonalSkill],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM personal_skill_inventory WHERE host=?1", params![host])?;
        for s in skills {
            tx.execute(
                "INSERT OR REPLACE INTO personal_skill_inventory (host, scope, name, path, body_chars)
                 VALUES (?1,?2,?3,?4,?5)",
                params![host, s.scope, s.name, s.path, s.body_chars as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 호스트 설정 스냅숏 (기본 모델·effort) — R7 확장·R13 OutdatedModel 재료 (코칭 v3 §4.2)
    pub fn replace_host_settings(
        &self,
        host: &str,
        default_model: Option<&str>,
        effort_level: Option<&str>,
        now_ts: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO host_settings (host, default_model, effort_level, scanned_at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(host) DO UPDATE SET
               default_model=?2, effort_level=?3, scanned_at=?4",
            params![host, default_model, effort_level, now_ts],
        )?;
        Ok(())
    }

    /// host의 distinct 세션 cwd 목록 (NULL 제외) — 프로젝트 스코프 개인 스킬 스캔용
    pub fn session_cwds(&self, host: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT cwd FROM sessions WHERE host=?1 AND cwd IS NOT NULL ORDER BY cwd",
        )?;
        let rows = stmt.query_map(params![host], |r| r.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }
```

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor -- store::tests`
Expected: 신규 3개 포함 전부 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/store.rs crates/core/src/inventory.rs
git commit -m "feat(agent): add personal-skill/host-settings tables, subagent_files column, v3 migration"
```

---

### Task 3: adapter.rs — permission-mode·compact_boundary 라인 + first_prompt 오염 수정

**Files:**
- Modify: `a-mate/crates/core/src/adapter.rs`

**Interfaces:**
- Consumes: `EventKind::PermissionMode` (Task 1)
- Produces: jsonl 라인 → 이벤트 매핑 3종. Task 5가 이 파일의 `content_text` 헬퍼를 재사용.

- [ ] **Step 1: 실패 테스트 작성** — `adapter.rs` tests 모듈에 추가:

```rust
    #[test]
    fn map_permission_mode_line_yields_event() {
        use crate::model::EventKind;
        // 실측 형태 (mac jsonl): {"type":"permission-mode","permissionMode":"plan","sessionId":"..."}
        let line = r#"{"type":"permission-mode","permissionMode":"plan","sessionId":"s1"}"#;
        let evs = adapter().map(line, "s1.jsonl", 42);
        assert_eq!(evs.len(), 1);
        match &evs[0].kind {
            EventKind::PermissionMode { mode } => assert_eq!(mode, "plan"),
            k => panic!("expected PermissionMode, got {k:?}"),
        }
        // permissionMode 키 부재 → 침묵 (fail-safe)
        let none = adapter().map(r#"{"type":"permission-mode","sessionId":"s1"}"#, "s1.jsonl", 0);
        assert!(none.is_empty());
    }

    #[test]
    fn map_system_compact_boundary_yields_compaction() {
        use crate::model::EventKind;
        // 신형 auto-compact 경계 (공식 문서 형태). trigger 구분은 Windows 실데이터 핀 후 후속 (스펙 §4.1-5)
        let line = r#"{"type":"system","subtype":"compact_boundary","sessionId":"s1",
            "compactMetadata":{"trigger":"auto","preCompactTokens":155000}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::Compaction)));
        // 다른 system subtype은 침묵 (fail-safe)
        let none = adapter().map(r#"{"type":"system","subtype":"turn_duration","sessionId":"s1"}"#, "s1.jsonl", 0);
        assert!(none.is_empty());
    }

    #[test]
    fn map_local_command_stdout_is_not_prompt() {
        // 실측: 세션 첫 user 라인이 "<local-command-stdout>Set model to ..." 로 오염됨 (스펙 §4.1-2)
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u9",
            "message":{"role":"user","content":"<local-command-stdout>Set model to Fable 5</local-command-stdout>"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })));
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor map_permission_mode map_system_compact map_local_command 2>&1 | tail -5`
Expected: 3건 FAIL (permission-mode/system 라인은 빈 결과, local-command는 UserPrompt 생성됨)

- [ ] **Step 3: 구현** — `map()`의 compaction 블록을 확장·교체하고 그 아래 분기 추가:

기존:
```rust
        // compaction 경계
        if v.get("isCompactSummary").and_then(|x| x.as_bool()).unwrap_or(false) {
            out.push(mk(EventKind::Compaction, 0));
            return out;
        }
```
교체:
```rust
        // compaction 경계 — 구형(isCompactSummary) + 신형(system/compact_boundary) 모두 인지.
        // trigger(auto/manual) 구분은 Windows 실데이터 핀 후 후속 (코칭 v3 §4.1-5, fail-safe: 미인식=침묵)
        let compact_boundary = ltype == "system"
            && v.get("subtype").and_then(|x| x.as_str()) == Some("compact_boundary");
        if v.get("isCompactSummary").and_then(|x| x.as_bool()).unwrap_or(false) || compact_boundary {
            out.push(mk(EventKind::Compaction, 0));
            return out;
        }
        // permission-mode 라인 → 이벤트 (plan=R16, bypassPermissions=R19 재료 — 코칭 v3 §4.1-4)
        if ltype == "permission-mode" {
            if let Some(mode) = v.get("permissionMode").and_then(|x| x.as_str()) {
                out.push(mk(EventKind::PermissionMode { mode: mode.to_string() }, 0));
            }
            return out;
        }
```

`extract_prompt_preview`의 스킵 조건 교체:

```rust
    if first_line.is_empty()
        || first_line.starts_with("<command")
        || first_line.starts_with("<local-command")
    {
        return None;
    }
```

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor -- adapter::tests`
Expected: 신규 3개 포함 전부 PASS (기존 `map_compact_summary_line_yields_compaction`도 그대로 PASS)

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/adapter.rs
git commit -m "feat(agent): parse permission-mode and compact-boundary lines, skip local-command stdout in first prompt"
```

---

### Task 4: curation.rs — 시크릿 패턴 수제 매처

**Files:**
- Modify: `a-mate/crates/core/src/curation.rs`

**Interfaces:**
- Produces: `pub fn find_secret_patterns(text: &str) -> Vec<&'static str>` — 정렬·중복 제거된 pattern_id 목록. Task 5(adapter)와 PR②의 R20이 사용.

- [ ] **Step 1: 실패 테스트 작성** — `curation.rs` tests 모듈에 추가:

```rust
    #[test]
    fn secret_patterns_hit_known_key_shapes() {
        assert_eq!(find_secret_patterns("here sk-ant-api03-AbCdEfGh123456 end"), vec!["anthropic_api_key"]);
        assert_eq!(find_secret_patterns("token=ghp_AbCdEf0123456789"), vec!["github_token"]);
        assert_eq!(find_secret_patterns("pat github_pat_11ABCDEFG_xyz123"), vec!["github_token"]);
        assert_eq!(find_secret_patterns("AKIAIOSFODNN7EXAMPLE"), vec!["aws_access_key"]);
        assert_eq!(find_secret_patterns("xoxb-123456789012-abcdef"), vec!["slack_token"]);
        assert_eq!(find_secret_patterns("-----BEGIN RSA PRIVATE KEY-----\nMII..."), vec!["private_key_block"]);
        // 복수 종류 → 정렬된 dedup 목록
        assert_eq!(
            find_secret_patterns("ghp_AbCdEf0123456789 and sk-ant-api03-AbCdEfGh123456"),
            vec!["anthropic_api_key", "github_token"]
        );
    }

    #[test]
    fn secret_patterns_suppress_short_or_prose_mentions() {
        // 접두 뒤 토큰이 짧으면(문서 언급 수준) 침묵 — 오탐 억제
        assert!(find_secret_patterns("환경변수 이름은 sk-ant- 로 시작해요").is_empty());
        assert!(find_secret_patterns("ghp_ 접두 토큰을 쓰세요").is_empty());
        assert!(find_secret_patterns("AKIA만 적으면 안 돼요").is_empty());
        assert!(find_secret_patterns("-----BEGIN CERTIFICATE-----").is_empty()); // PRIVATE KEY 아님
        assert!(find_secret_patterns("평범한 문장").is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor secret_patterns 2>&1 | tail -5`
Expected: 컴파일 실패 (`find_secret_patterns` 없음)

- [ ] **Step 3: 구현** — `curation.rs`에 추가 (regex 크레이트 금지 — Global Constraints):

```rust
/// R20 시크릿 패턴 큐레이션 (코칭 v3 §11.2). (pattern_id, 접두). 버전업 가능한 상수.
/// 주의: 매칭된 본문은 절대 저장·로그하지 않는다 — pattern_id만 반환.
const SECRET_PREFIXES: &[(&str, &str)] = &[
    ("anthropic_api_key", "sk-ant-"),
    ("github_token", "ghp_"),
    ("github_token", "gho_"),
    ("github_token", "github_pat_"),
    ("slack_token", "xoxb-"),
    ("slack_token", "xoxp-"),
];

/// 접두 뒤 토큰 문자([A-Za-z0-9_-]) 연속 길이 — 짧은 언급(문서 인용) 오탐 억제용.
fn token_len_after(text: &str, start: usize) -> usize {
    text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .count()
}

/// text에서 감지된 시크릿 pattern_id 목록 (정렬·dedup). 본문은 반환하지 않는다.
pub fn find_secret_patterns(text: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (id, prefix) in SECRET_PREFIXES {
        if let Some(pos) = text.find(prefix) {
            if token_len_after(text, pos + prefix.len()) >= 8 {
                out.push(id);
            }
        }
    }
    // 개인키 블록: BEGIN 헤더에 PRIVATE KEY 명시된 경우만
    if let Some(pos) = text.find("-----BEGIN ") {
        if text[pos..].contains("PRIVATE KEY-----") {
            out.push("private_key_block");
        }
    }
    // AWS Access Key ID: "AKIA" + 대문자/숫자 16자
    if let Some(pos) = text.find("AKIA") {
        let rest = text[pos + 4..].as_bytes();
        if rest.len() >= 16
            && rest[..16].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            out.push("aws_access_key");
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}
```

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor secret_patterns`
Expected: 2개 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/curation.rs
git commit -m "feat(agent): add curated secret-pattern matchers (id-only, no body capture)"
```

---

### Task 5: adapter.rs — 시크릿 플래그 이벤트 방출

**Files:**
- Modify: `a-mate/crates/core/src/adapter.rs`

**Interfaces:**
- Consumes: `find_secret_patterns` (Task 4), `EventKind::SecretFlag` (Task 1)
- Produces: user 프롬프트 전문·Bash command 인자에서 `SecretFlag` 이벤트. off_bump 800대(기존 SessionMeta 900·ToolCall i+1과 비충돌).

- [ ] **Step 1: 실패 테스트 작성** — `adapter.rs` tests 모듈에 추가:

```rust
    #[test]
    fn map_user_prompt_with_secret_emits_flag_without_body() {
        use crate::model::EventKind;
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u1",
            "message":{"role":"user","content":"이 키로 배포해줘\nghp_AbCdEf0123456789"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 100);
        let sf = evs.iter().find(|e| matches!(e.kind, EventKind::SecretFlag { .. })).unwrap();
        match &sf.kind {
            EventKind::SecretFlag { pattern_id } => assert_eq!(pattern_id, "github_token"),
            k => panic!("expected SecretFlag, got {k:?}"),
        }
        // 첫 줄이 평문이므로 UserPrompt도 함께 생성됨 (기능 독립)
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::UserPrompt { .. })));
    }

    #[test]
    fn map_bash_command_with_secret_emits_flag() {
        use crate::model::EventKind;
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u2",
            "message":{"model":"claude-opus-4-8","usage":{"input_tokens":1,"output_tokens":1},
            "content":[{"type":"tool_use","id":"t1","name":"Bash",
                        "input":{"command":"export ANTHROPIC_API_KEY=sk-ant-api03-AbCdEfGh123456"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        let flags: Vec<_> = evs.iter().filter(|e| matches!(e.kind, EventKind::SecretFlag { .. })).collect();
        assert_eq!(flags.len(), 1);
        match &flags[0].kind {
            EventKind::SecretFlag { pattern_id } => assert_eq!(pattern_id, "anthropic_api_key"),
            k => panic!("expected SecretFlag, got {k:?}"),
        }
        // ToolCall(Bash)도 평소대로 생성 (기능 독립)
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::ToolCall { .. })));
    }

    #[test]
    fn map_clean_lines_emit_no_secret_flag() {
        let clean_user = r#"{"type":"user","sessionId":"s1","uuid":"u3",
            "message":{"role":"user","content":"토큰 없이 평범한 요청"}}"#;
        assert!(!adapter().map(clean_user, "s.jsonl", 0).iter()
            .any(|e| matches!(e.kind, crate::model::EventKind::SecretFlag { .. })));
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor map_user_prompt_with_secret map_bash_command_with_secret 2>&1 | tail -5`
Expected: FAIL (SecretFlag 미생성)

- [ ] **Step 3: 구현**

3a. content 평탄화 헬퍼 추출 — `extract_prompt_preview` 위에 추가하고, `extract_prompt_preview`가 재사용하도록 수정:

```rust
/// user content(문자열 또는 블록 배열)의 텍스트 전문을 평탄화. 시크릿 스캔·미리보기 공용.
fn content_text(content: Option<&Value>) -> Option<String> {
    match content {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(arr)) => Some(
            arr.iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}
```

`extract_prompt_preview`의 `let raw = match content { ... }` 블록을 `let raw = content_text(content)?;`로 교체.

3b. user 분기(`if !had_tool_result { ... }` 블록)를 다음으로 교체:

```rust
            if !had_tool_result {
                if let Some(preview) = extract_prompt_preview(content) {
                    out.push(mk(EventKind::UserPrompt { preview }, 0)); // off_bump 0 = 라인 시작(deref 포인터)
                }
                // 시크릿 스캔은 프롬프트 전문 대상 (미리보기 스킵과 독립 — 코칭 v3 §4.2)
                if let Some(text) = content_text(content) {
                    for (i, pid) in crate::curation::find_secret_patterns(&text).iter().enumerate() {
                        out.push(mk(EventKind::SecretFlag { pattern_id: pid.to_string() }, 800 + i as u64));
                    }
                }
            }
```

3c. assistant 분기의 tool_use 루프 안, `out.push(mk(EventKind::ToolCall { ... }))` 직후에 추가:

```rust
                        // Bash command 인자 시크릿 스캔 (코칭 v3 §4.2). off_bump 800대 — 블록별 8칸.
                        if let Some(cmd) = input.get("command").and_then(|x| x.as_str()) {
                            for (j, pid) in
                                crate::curation::find_secret_patterns(cmd).iter().enumerate()
                            {
                                out.push(mk(
                                    EventKind::SecretFlag { pattern_id: pid.to_string() },
                                    800 + (i as u64) * 8 + j as u64,
                                ));
                            }
                        }
```

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor -- adapter::tests`
Expected: 신규 3개 포함 전부 PASS (기존 preview 테스트 회귀 없음)

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/adapter.rs
git commit -m "feat(agent): emit secret_flag events from user prompts and bash commands"
```

---

### Task 6: store.rs ingest_file — 서브에이전트 트랜스크립트 파일 카운트

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` (`ingest_file`)

**Interfaces:**
- Consumes: `sessions.subagent_files` 컬럼 (Task 2)
- Produces: 세션 jsonl 수집 시 `<세션id>/subagents/*.jsonl` 개수를 `sessions.subagent_files`에 기록 (절대값 UPDATE — 멱등).

- [ ] **Step 1: 실패 테스트 작성** — `store.rs` tests 모듈에 추가:

```rust
    #[test]
    fn ingest_file_counts_subagent_transcripts() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(proj.join("abc").join("subagents")).unwrap();
        // 세션 jsonl + 서브에이전트 파일 2개 (+ jsonl 아닌 파일 1개는 미집계)
        let file = proj.join("abc.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"type":"assistant","sessionId":"abc","uuid":"u1","timestamp":"2026-07-19T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#).unwrap();
        for name in ["agent-a.jsonl", "agent-b.jsonl"] {
            std::fs::File::create(proj.join("abc").join("subagents").join(name)).unwrap();
        }
        std::fs::File::create(proj.join("abc").join("subagents").join("note.txt")).unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        ingest_file(&store, &adapter, &file).unwrap();

        let n: i64 = store.conn.query_row(
            "SELECT subagent_files FROM sessions WHERE session_id='abc'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn ingest_file_without_subagent_dir_keeps_zero() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("solo.jsonl");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, r#"{{"type":"assistant","sessionId":"solo","uuid":"u1","timestamp":"2026-07-19T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":1}}}}}}"#).unwrap();

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        ingest_file(&store, &adapter, &file).unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT subagent_files FROM sessions WHERE session_id='solo'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor ingest_file_counts_subagent 2>&1 | tail -5`
Expected: FAIL (컬럼은 있으나 0)

- [ ] **Step 3: 구현** — `ingest_file`의 `store.set_offset(...)` 앞에 추가:

```rust
    // 서브에이전트 하위 트랜스크립트 수 — <세션id>/subagents/*.jsonl 존재 카운트만 (전문 파싱은 후속, 코칭 v3 §4.1-3)
    let sub_dir = file.with_extension("").join("subagents");
    if let Ok(entries) = std::fs::read_dir(&sub_dir) {
        let n = entries
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
            .count() as i64;
        if let Some(sid) = file.file_stem().and_then(|s| s.to_str()) {
            store.conn.execute(
                "UPDATE sessions SET subagent_files=?2 WHERE session_id=?1",
                rusqlite::params![sid, n],
            )?;
        }
    }
```

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor ingest_file`
Expected: 신규 2개 포함 ingest_file 관련 전부 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/store.rs
git commit -m "feat(agent): count subagent transcript files per session during ingest"
```

---

### Task 7: inventory.rs + ops.rs — 개인 스킬·호스트 설정 스캔

**Files:**
- Modify: `a-mate/crates/core/src/inventory.rs` (`scan_personal_skills` 추가)
- Modify: `a-mate/crates/core/src/ops.rs` (`run_inventory` 연결)

**Interfaces:**
- Consumes: `PersonalSkill`(Task 2), `replace_personal_skills`/`replace_host_settings`/`session_cwds`(Task 2)
- Produces: `pub fn scan_personal_skills(dir: &Path, scope: &str) -> Vec<PersonalSkill>`; `run_inventory`가 매 스캔마다 두 테이블 채움.

- [ ] **Step 1: 실패 테스트 작성** — `inventory.rs` tests 모듈에 추가:

```rust
    #[test]
    fn scan_personal_skills_reads_name_and_body_chars() {
        let dir = tempfile::tempdir().unwrap();
        let sk = dir.path().join("gh-commit");
        std::fs::create_dir_all(&sk).unwrap();
        std::fs::write(sk.join("SKILL.md"),
            "---\nname: gh-commit\ndescription: commit helper\n---\ngh commit wrapper body").unwrap();
        // frontmatter 없는 스킬 → 디렉터리명 폴백
        let raw = dir.path().join("raw-skill");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::write(raw.join("SKILL.md"), "no frontmatter body").unwrap();

        let skills = scan_personal_skills(dir.path(), "user");
        assert_eq!(skills.len(), 2);
        let gh = skills.iter().find(|s| s.name == "gh-commit").unwrap();
        assert_eq!(gh.scope, "user");
        assert!(gh.body_chars > 40);
        assert!(gh.path.ends_with("SKILL.md"));
        assert!(skills.iter().any(|s| s.name == "raw-skill")); // 폴백 이름
    }

    #[test]
    fn scan_personal_skills_missing_dir_is_empty() {
        assert!(scan_personal_skills(std::path::Path::new("/nonexistent/skills"), "user").is_empty());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor scan_personal_skills 2>&1 | tail -5`
Expected: 컴파일 실패 (함수 없음)

- [ ] **Step 3: 구현**

3a. `inventory.rs`에 추가 (`scan_skills_dir` 아래):

```rust
/// 개인 스킬 스캔 — <dir>/*/SKILL.md (코칭 v3 §4.2). 부재/접근 불가는 빈 목록(관대).
/// name은 frontmatter 우선, 없으면 디렉터리명 폴백. body_chars는 SKILL.md 전문 글자수.
pub fn scan_personal_skills(dir: &Path, scope: &str) -> Vec<PersonalSkill> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for v in entries.flatten() {
        let md = v.path().join("SKILL.md");
        if !md.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&md) else { continue };
        let name = parse_skill_frontmatter(&raw)
            .map(|(n, _)| n)
            .unwrap_or_else(|| v.file_name().to_string_lossy().to_string());
        out.push(PersonalSkill {
            name,
            path: md.to_string_lossy().to_string(),
            body_chars: raw.chars().count() as u64,
            scope: scope.to_string(),
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}
```

3b. `ops.rs::run_inventory`의 루프 끝(`store.replace_plugin_inventory(...)` 처리 뒤)에 추가:

```rust
        // v3: 호스트 설정 스냅숏 — R7 확장·R13 OutdatedModel 재료 (코칭 v3 §4.2)
        let default_model = settings.get("model").and_then(|v| v.as_str());
        let effort = settings.get("effortLevel").and_then(|v| v.as_str());
        if let Err(e) = store.replace_host_settings(
            &hs.host, default_model, effort, &chrono::Utc::now().to_rfc3339(),
        ) {
            warnings.push(format!("host {} 설정 스냅숏 실패: {e}", hs.host));
        }
        // v3: 개인 스킬 인벤토리 — 사용자 스코프 + 프로젝트 스코프(.claude/skills, 로컬 존재 cwd만)
        let mut personal =
            crate::inventory::scan_personal_skills(&hs.claude_root.join("skills"), "user");
        for cwd in store.session_cwds(&hs.host).unwrap_or_default() {
            let p = std::path::Path::new(&cwd).join(".claude").join("skills");
            personal.extend(crate::inventory::scan_personal_skills(&p, "project"));
        }
        if let Err(e) = store.replace_personal_skills(&hs.host, &personal) {
            warnings.push(format!("host {} 개인 스킬 스캔 실패: {e}", hs.host));
        }
```

(import: `ops.rs` 상단 `use crate::inventory::{collect_host_inventory, scan_plugin_inventory};`는 그대로 두고 위 코드처럼 전체 경로 호출 — 기존 스타일과 혼합을 피하려면 use에 `scan_personal_skills` 추가해도 무방.)

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor -- inventory::tests && cargo test -p agent-mentor -- ops::tests`
Expected: 전부 PASS (run_inventory는 실 env 의존이라 직접 테스트 없음 — 기존 관례)

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/inventory.rs crates/core/src/ops.rs
git commit -m "feat(agent): scan personal skills and host settings into inventory"
```

---

### Task 8: ops.rs — R1·R2·R9·R12 은퇴

**Files:**
- Modify: `a-mate/crates/core/src/ops.rs` (`run_rules` + 상단 use + 기존 테스트 1개 수정)

**Interfaces:**
- Consumes: `delete_findings_by_rule_and_scope` (기존)
- Produces: `run_rules`가 R7·R10·R11만 등록. R1(host·project)·R2(host)·R9(session)·R12(project) findings는 스캔 시 삭제.

주의(문서화된 결과): `store.rs::tip_personal_evidence`의 "mcp" 팁은 R1 finding으로 개인화 근거를 만들었다 —
R1 findings가 사라지면 그 한 줄이 자연 휴면(None)한다. 코드 무변경(분기 보존), PR③ R13이 재접지 예정.

- [ ] **Step 1: 기존 테스트를 새 계약으로 수정** — `ops.rs`의 `run_rules_purges_deprecated_session_findings`를 교체:

```rust
    #[test]
    fn run_rules_purges_retired_rule_findings() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |rule: &str, kind: &str, key: &str| Finding {
            rule_id: rule.into(), severity: Severity::Suggest,
            scope_host: None, scope_project: None,
            scope_kind: kind.into(), scope_ref: "x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        // v2 폐기분(R7 session·R5 전 스코프) + v3 은퇴분(R1·R2·R9·R12) + 생존 R11
        store.upsert_finding(&mk("R7", "session", "R7|s1"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "session", "R5|s1|a.md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "project", "R5|W|proj|cross_session_claude_md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "host", "R1|W|ctx"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "project", "R1|W|proj|pw"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R2", "host", "R2|W|superpowers@mp"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R9", "session", "R9|s9"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R12", "project", "R12|W|proj"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R11", "project", "R11|W|proj"), "2026-07-06T00:00:00Z").unwrap();

        run_rules(&store).unwrap();
        assert_eq!(store.count_findings().unwrap(), 1, "R11만 생존해야 함");
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor run_rules_purges 2>&1 | tail -5`
Expected: FAIL (은퇴 룰 findings가 생존)

- [ ] **Step 3: 구현** — `run_rules`를 교체:

```rust
pub fn run_rules(store: &SqliteStore) -> Result<Vec<Finding>> {
    // 코칭 v2 이행: 세션 스코프 R7은 폐기 — 프로젝트 집계(R7 v2)가 대체 (스펙 §3)
    store.delete_findings_by_rule_and_scope("R7", "session")?;
    // R5(반복 읽기) 발화 보류(2026-07-10 사용자 판정): 반복 Read는 에이전트 동작이라
    // 사용자가 행동할 레버가 없음 — 등록 해제·전 스코프 카드 정리, 룰 코드·테스트는 보존.
    store.delete_findings_by_rule_and_scope("R5", "session")?;
    store.delete_findings_by_rule_and_scope("R5", "project")?;
    // 코칭 v3 처분(스펙 §3.1): R1·R2·R9·R12 은퇴 — 잔소리 단독 카드 폐지,
    // 탐지 신호는 R13(환경 큐레이션, PR③)이 흡수. 룰 코드·테스트는 보존.
    store.delete_findings_by_rule_and_scope("R1", "host")?;
    store.delete_findings_by_rule_and_scope("R1", "project")?;
    store.delete_findings_by_rule_and_scope("R2", "host")?;
    store.delete_findings_by_rule_and_scope("R9", "session")?;
    store.delete_findings_by_rule_and_scope("R12", "project")?;
    let engine = RuleEngine::new(vec![
        Box::new(R7OpusTrivial::default()),
        Box::new(R10AutomationBurst::default()),
        Box::new(R11PermissionFriction::default()),
    ]);
    let findings = engine.run(store)?;
    let now = chrono::Utc::now().to_rfc3339();
    let tx = store.conn.unchecked_transaction()?;
    for f in &findings {
        store.upsert_finding(f, &now)?;
    }
    tx.commit()?;
    Ok(findings)
}
```

상단 use에서 이제 안 쓰는 4개 제거 (내 변경으로 고아가 된 import):

```rust
use crate::rules::r7_opus_trivial::R7OpusTrivial;
use crate::rules::r10_automation_burst::R10AutomationBurst;
use crate::rules::r11_permission_friction::R11PermissionFriction;
```

(`r1_unused_mcp`/`r2_unused_plugins`/`r9_web_overuse`/`r12_unused_skills` use 라인 삭제. `rules/mod.rs`의 `pub mod` 선언과 룰 파일·테스트는 그대로 보존.)

- [ ] **Step 4: 그린 확인**

Run: `cargo test -p agent-mentor 2>&1 | tail -10`
Expected: 전부 PASS, unused import 경고 0

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/ops.rs
git commit -m "feat(agent): retire R1/R2/R9/R12 rules and purge their findings"
```

---

### Task 9: R10 Info 강등 — 관찰 카드

**Files:**
- Modify: `a-mate/crates/core/src/rules/r10_automation_burst.rs`
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`finding_advice` R10 분기 + 테스트 4개)

**Interfaces:**
- Produces: R10 finding = `severity Info · est 0 · prescription None` + 관찰형 advice. 스펙 §3.2.

- [ ] **Step 1: 룰 테스트를 새 계약으로 수정** — `r10_automation_burst.rs` tests에서:

`r10_fires_single_project_card_for_opus_burst`의 아래 3줄을 교체:

```rust
        // 기존:
        // assert!(f.est_tokens_saved > 0);
        // assert_eq!(f.prescription.as_ref().unwrap().kind, "automation_model_config");
        // 교체 (v3 §3.2 — 관찰 카드):
        assert_eq!(f.severity, crate::finding::Severity::Info);
        assert_eq!(f.est_tokens_saved, 0);
        assert!(f.prescription.is_none(), "R10은 처방 없는 관찰 카드");
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor r10_fires 2>&1 | tail -5`
Expected: FAIL (Warn·est>0·prescription 있음)

- [ ] **Step 3: 룰 구현 수정** — `r10_automation_burst.rs`:

- struct에서 `savings_fraction_pct` 필드 제거: `#[derive(Default)] pub struct R10AutomationBurst {}` 로 교체 (기존 `impl Default` 블록 삭제).
- `evaluate`에서 billable/est 계산 2줄(`let billable = ...; let est = ...;`) 삭제.
- Finding 생성부 변경:
  - `severity: Severity::Warn` → `severity: Severity::Info`
  - evidence의 `"note": "비용-등가 추정(Opus↔Haiku 5:1 가격비)"` 키 삭제
  - `est_tokens_saved: est` → `est_tokens_saved: 0`
  - `prescription: Some(Prescription { kind: "automation_model_config", ... })` → `prescription: None`
- 이제 안 쓰는 `Prescription` import 제거 (`use crate::finding::{Finding, Severity};`로).

- [ ] **Step 4: finding_advice R10 문구 교체** — `diary/mod.rs`의 `"R10" => { ... }` 분기를 교체 (스펙 §3.2 문구):

```rust
        "R10" => {
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            let opus_n = evidence.get("opus_session_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let temp = evidence.get("temp_hit_ratio_pct").and_then(|v| v.as_u64()).unwrap_or(0);
            let opus_part =
                if opus_n == n { "전부".to_string() } else { format!("그중 {opus_n}건이") };
            let mut detail = format!(
                "자동화로 보이는 초단기 세션 {n}건이 짧은 간격으로 반복됐고 {opus_part} Opus 전용이었어요"
            );
            if temp > 0 {
                detail.push_str(&format!(" · temp 경로 흔적 {temp}%"));
            }
            if let Some(cwd) = evidence.get("rep_cwd").and_then(|v| v.as_str()) {
                detail.push_str(&format!(" · 경로 `{cwd}`"));
            }
            // v3 §3.2: 관찰만 — 수정 지시 없음. 자동화 소유 여부는 사용자가 판단.
            let mut action = "직접 만든 자동화라면 그 도구의 모델 설정을 낮출 수 있어요 — 아니라면 참고만 하세요".to_string();
            if let Some(p) = evidence.get("rep_first_prompt").and_then(|v| v.as_str()) {
                action.push_str(&format!("\n💬 이런 요청으로 시작해요: '{p}'"));
            }
            (detail, action)
        }
```

diary 테스트 수정 (`finding_advice_r10_burst`·`finding_advice_r10_inserts_real_path_and_first_prompt`):

```rust
        // finding_advice_r10_burst 의 마지막 3개 assert 교체:
        assert!(action.contains("직접 만든 자동화라면"));
        assert!(action.contains("참고만"));
        assert!(!action.contains("지정하세요"), "수정 지시 문구 금지 (v3 §3.2)");
```

(`finding_advice_r10_partial_opus_says_count`·`finding_advice_r10_omits_temp_when_zero`는 detail 형식 유지라 무변경.
`finding_advice_r10_inserts_real_path_and_first_prompt`의 `assert!(action.contains("이런 요청으로 시작해요"));`는 유지.)

- [ ] **Step 5: 그린 확인**

Run: `cargo test -p agent-mentor r10 && cargo test -p agent-mentor finding_advice_r10`
Expected: 전부 PASS

- [ ] **Step 6: coach.rs 회귀 확인** — `coach.rs` 기존 테스트 `v2_aggregate_rules_have_no_fix_command`가 R10 → None을 이미 검증. 무변경 확인만:

Run: `cargo test -p agent-mentor v2_aggregate_rules`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/rules/r10_automation_burst.rs crates/core/src/diary/mod.rs
git commit -m "feat(agent): demote R10 to observational info card without prescription"
```

---

### Task 10: R7 확장 — 기본 모델·effort 서사

**Files:**
- Modify: `a-mate/crates/core/src/rules/r7_opus_trivial.rs`
- Modify: `a-mate/crates/core/src/diary/mod.rs` (`finding_advice` R7 분기 + 테스트)

**Interfaces:**
- Consumes: `host_settings` 테이블 (Task 2)
- Produces: R7 evidence에 `default_model`·`effort_level`(없으면 null) 추가, advice에 effort 언급.

- [ ] **Step 1: 실패 테스트 작성** — `r7_opus_trivial.rs` tests에 추가:

```rust
    #[test]
    fn r7v2_evidence_carries_host_settings_when_present() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.replace_host_settings("Windows", Some("claude-fable-5[1m]"), Some("xhigh"), "2026-07-19T00:00:00Z").unwrap();
        let mut evs = Vec::new();
        evs.extend(light_opus_session("l1", "2026-07-06T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-06T11:00:00Z"));
        evs.extend(light_opus_session("l3", "2026-07-06T13:00:00Z"));
        store.upsert_events(&evs).unwrap();

        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].evidence["default_model"], "claude-fable-5[1m]");
        assert_eq!(findings[0].evidence["effort_level"], "xhigh");
    }

    #[test]
    fn r7v2_evidence_settings_null_when_absent() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut evs = Vec::new();
        evs.extend(light_opus_session("l1", "2026-07-06T09:00:00Z"));
        evs.extend(light_opus_session("l2", "2026-07-06T11:00:00Z"));
        evs.extend(light_opus_session("l3", "2026-07-06T13:00:00Z"));
        store.upsert_events(&evs).unwrap();
        let findings = R7OpusTrivial::default().evaluate(&store).unwrap();
        assert!(findings[0].evidence["default_model"].is_null());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p agent-mentor r7v2_evidence 2>&1 | tail -5`
Expected: FAIL (evidence에 키 없음)

- [ ] **Step 3: 룰 구현** — `r7_opus_trivial.rs::evaluate`의 프로젝트 루프 안, `out.push(Finding {...})` 앞에 추가:

```rust
            // v3 확장: 호스트 기본 설정을 서사 재료로 동봉 (코칭 v3 §3.3 — 판정에는 미사용)
            let (default_model, effort_level): (Option<String>, Option<String>) = store
                .conn
                .query_row(
                    "SELECT default_model, effort_level FROM host_settings WHERE host=?1",
                    rusqlite::params![host],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or((None, None));
```

evidence JSON에 두 키 추가 (`"note"` 라인 앞에):

```rust
                    "default_model": default_model,
                    "effort_level": effort_level,
```

- [ ] **Step 4: finding_advice R7 문구 확장** — `diary/mod.rs`의 `"R7" => { ... }` 분기를 교체:

```rust
        "R7" => {
            let ratio = evidence.get("ratio_pct").and_then(|v| v.as_u64()).unwrap_or(0);
            let n = evidence.get("total_sessions").and_then(|v| v.as_u64()).unwrap_or(0);
            let mut detail = format!(
                "이 프로젝트 세션의 {ratio}%({n}건)가 Opus로 처리한 가벼운 잔심부름이었어요 (~{est_tokens_saved}토큰 비용-등가)"
            );
            if let Some(m) = evidence.get("default_model").and_then(|v| v.as_str()) {
                detail.push_str(&format!(" · 기본 모델 {m}"));
            }
            let mut action =
                "다음엔 `claude --model sonnet`으로 시작하거나 settings.json에서 기본 모델을 낮춰보세요".to_string();
            if let Some(e) = evidence.get("effort_level").and_then(|v| v.as_str()) {
                action.push_str(&format!(" — effort({e})도 작업 난이도에 맞게 낮출 수 있어요"));
            }
            (detail, action)
        }
```

diary 테스트 추가 (기존 `finding_advice_r7_v2_project_aggregate`는 설정 키 없는 evidence라 그대로 PASS해야 함 — 회귀 확인):

```rust
    #[test]
    fn finding_advice_r7_mentions_default_model_and_effort_when_present() {
        let (detail, action) = super::finding_advice(
            "R7",
            &serde_json::json!({"ratio_pct": 75, "total_sessions": 3,
                "default_model": "claude-fable-5[1m]", "effort_level": "xhigh"}),
            48320,
        );
        assert!(detail.contains("claude-fable-5[1m]"));
        assert!(action.contains("effort(xhigh)"));
    }
```

- [ ] **Step 5: 그린 확인**

Run: `cargo test -p agent-mentor r7 && cargo test -p agent-mentor finding_advice_r7`
Expected: 전부 PASS (기존 R7 테스트 회귀 없음)

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/rules/r7_opus_trivial.rs crates/core/src/diary/mod.rs
git commit -m "feat(agent): enrich R7 narrative with host default model and effort"
```

---

### Task 11: 최종 검증

**Files:** 없음 (검증만; 발견된 회귀는 해당 파일에서 수정)

- [ ] **Step 1: 전체 스위트 그린**

Run: `cargo test -p agent-mentor 2>&1 | tail -15`
Expected: 전부 PASS, 경고 0 (unused import/variable 경고 포함 — 내 변경이 만든 고아만 정리, 무관 정리 금지)

- [ ] **Step 2: 실데이터 스모크 (이 mac)** — in-memory가 아닌 실제 jsonl로 어댑터 회귀 확인. 임시 테스트가 아니라 수동 확인:

```bash
cargo build -p agent-mentor 2>&1 | tail -3
```

Expected: 빌드 그린. (CLI 서브커맨드 구성은 main.rs 참조 — 스캔 커맨드가 있으면 `~/.claude` 대상 1회 실행해
`permission_mode`/`secret_flag` 이벤트와 first_prompt 오염 해소를 `sqlite3`로 눈검사. 없으면 생략 — Windows E2E에서 확인.)

- [ ] **Step 3: 마이그레이션 경로 확인 노트** — 기존 DB(v2.1 스키마)를 가진 환경에서 첫 실행 시
`subagent_files` 부재 → events/sessions/ingest_state/daily_rollup 와이프 + 재수집, findings·diary_index 보존.
코드 리뷰로 확인 (마이그레이션 블록이 v2.1 블록과 동일 패턴인지).

- [ ] **Step 4: 최종 커밋 (수정 있었던 경우만)**

```bash
git add -A && git commit -m "test(agent): fix regressions from coaching v3 pr1"
```

---

## Self-Review 체크 결과 (작성 시 수행)

- **스펙 커버리지**: §3.1(Task 8) §3.2(Task 9) §3.3(Task 10) §3.4(무변경) §4.1-1(Task 1) §4.1-2·4·5(Task 3) §4.1-3(Task 6) §4.2(Task 2·4·5·7) §4.4(Task 2). §4.3(카탈로그 JSON)은 R13과 함께 PR③ — 스펙 §15 분할표와 일치.
- **경계**: permission-mode를 events 행으로 정제한 결정은 Global Constraints에 명시 (스펙 §4.1-4 컬럼 방식과의 차이).
- **타입 일관성**: `PersonalSkill`(inventory.rs 정의, store가 참조) / `find_secret_patterns -> Vec<&'static str>` / `session_cwds(host)` 시그니처가 Task 2·5·7에서 동일.
