# PR2 — R6 의미 판정 레이어 + R23 폐기 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** R6(반복 지시) finding을 스캔 후 비동기 LLM 판정으로 걸러 "스킬화할 가치가 있는" 카드만 코치 탭에 노출하고, 전제가 약한 R23(도구 시퀀스) 룰을 코드째 폐기한다.

**Architecture:** 채굴(SQL, 결정론)은 그대로 두되 R6 finding을 `status='pending'`(비노출)으로 저장한다. 스캔 완료 후 락 밖에서 다이어리와 같은 Engine으로 배치 판정을 돌려 `worthy→'new'`(노출) / `unworthy→'rejected'`(영구 캐시)로 전환한다. 판정 결과는 evidence를 덮지 않도록 신규 컬럼 `judgment_json`에 저장하고, 엔진 미설정이면 pending은 영원히 비노출(fail-safe 침묵)이다.

**Tech Stack:** Rust (`crates/core` = 순수 도메인, `src-tauri` = Tauri v2 런타임), SQLite(rusqlite), Svelte 5 + TypeScript 프론트엔드, Vitest / cargo test.

**스펙 기준:** [docs/design/a-mate/specs/2026-07-21-repeat-coaching-judgment-redesign.md](../specs/2026-07-21-repeat-coaching-judgment-redesign.md) §4. PR1(#79 데이터 위생)은 머지 완료, 체크포인트 관찰 결과 재보정 불필요(R6 junk 소멸, R23 카드는 룰 전제 결함으로 재생성 → 폐기 결정 재확인).

## Global Constraints

- **플랫폼: Windows 전용.** macOS/Linux 분기 불필요. 빌드·실행·`cargo test`는 네이티브 Windows PowerShell에서 (`a-mate/`). **WSL 안에서 빌드/실행 금지.**
- **무거운 데이터 처리는 `crates/core`에서.** 판정 순수 로직(프롬프트·파싱·결과 변환)은 core에, 락 관리·네트워크 호출만 `src-tauri`에. 프론트엔드는 렌더링만.
- **프라이버시.** R6 evidence·판정 프롬프트에는 프롬프트 원문 미리보기가 들어간다 → 외부 전송은 Engine(사내 on-prem 기본)으로만. 허브 공유 화이트리스트 제외 유지(이번 PR에서 R6를 공유 대상에 넣지 않는다).
- **정밀도 우선(precision > recall).** 판정 프롬프트는 "확신이 없으면 `worthy=false`"를 명시한다.
- **에이전트 추상화 유지.** 엔진은 `diary::engine::Engine` trait 뒤에서만 호출한다(하드코딩 금지).
- **커밋: Conventional Commits, 영어.** `<type>(<scope>): <subject>` (예: `feat(agent): add r6 judgment pass`). scope는 `agent`(core/src-tauri Rust) 또는 `frontend`(Svelte/TS).
- **DoD:** 이 plan 완료 PR에서 `docs-archive` 스킬로 본 문서를 `docs/archive/`로 이관한다.

---

## File Structure

**신규 파일**

| 파일 | 책임 |
|------|------|
| `a-mate/crates/core/src/judge.rs` | R6 판정 순수 로직 — 판정 프롬프트 구성, 엄격 JSON 파싱, `judge_one`(엔진 1회 호출), 결과→(status, judgment_json) 변환. `skill_draft::DraftContext`를 재료로 재사용. |

**수정 파일**

| 파일 | 변경 |
|------|------|
| `a-mate/crates/core/src/store.rs` | `findings.judgment_json` 컬럼(CREATE + ALTER), `upsert_finding` 룰별 초기 status, v7 마이그레이션, `FindingRow.judgment`, `list_findings_current` 반영, 배치 조회 `pending_r6_for_judgment`, 결과 적용 `set_judgment`. |
| `a-mate/crates/core/src/lib.rs` | `pub mod judge;` 등록. |
| `a-mate/crates/core/src/ops.rs` | `run_rules`에서 R23 등록·prune 제거 + 관련 테스트 삭제. |
| `a-mate/crates/core/src/rules/mod.rs` | `pub mod r23_tool_sequences;` 제거. |
| `a-mate/crates/core/src/rules/r23_tool_sequences.rs` | **삭제.** |
| `a-mate/crates/core/src/skill_draft.rs` | `gather_context_for_sequence` + 그 테스트 삭제(R23 전용, 유일 소비자와 함께 폐기). |
| `a-mate/src-tauri/src/pipeline.rs` | `maybe_judge_repeats` 런타임 훅 추가 + `run_pipeline_once`에서 호출. |
| `a-mate/src-tauri/src/commands.rs` | `generate_skill_draft`에서 `sequence` 파라미터 제거, `suggested_name`(optional) 추가. |
| `a-mate/src/lib/api.ts` | `Finding.judgment` 필드 추가, `generateSkillDraft` 시그니처 변경. |
| `a-mate/src/lib/ui/coach-helpers.ts` | `COACH_TITLE`에서 R23 제거. |
| `a-mate/src/lib/ui/coach-helpers.test.ts` | R23 제목 기대 제거. |
| `a-mate/src/lib/ui/CoachTab.svelte` | R23 분기(`sequenceOf`·`skillifiable`의 R23·`makeDraft`의 seq) 제거, R6 판정 사유 1줄 표시, `suggested_name`을 초안 기본 이름으로 전달. |

**의존 순서:** 스키마(1) → upsert 초기 status(2) → v7 마이그레이션(3) → store 판정 메서드(4) → judge 모듈(5,6) → 런타임 훅(7) → R23 폐기(8) → UI(9).

---

## Task 1: `findings.judgment_json` 컬럼 + `FindingRow.judgment` 노출

판정 결과를 evidence와 분리 저장할 컬럼을 만들고, 조회가 이를 프론트로 실어 나르게 한다. (스펙 §4.1)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` (SCHEMA `findings`, `migrate`, `list_findings_current`, `FindingRow`)
- Test: `a-mate/crates/core/src/store.rs` (기존 `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `FindingRow.judgment: Option<serde_json::Value>` (Task 4·9가 소비), `findings` 테이블에 `judgment_json TEXT` 컬럼.

- [ ] **Step 1: 실패 테스트 작성** — `store.rs` 테스트 모듈에 추가

```rust
#[test]
fn judgment_json_column_roundtrips_through_finding_row() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f = Finding {
        rule_id: "R6".into(), severity: Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: None,
        scope_kind: "pattern".into(), scope_ref: "pattern:abcd1234".into(),
        evidence: serde_json::json!({"repeated_prompt": "판매 리포트 뽑아줘"}),
        est_tokens_saved: 0, prescription: None, dedup_key: "R6|Windows|abcd1234".into(),
    };
    store.upsert_finding(&f, "2026-07-21T00:00:00Z").unwrap();
    // 신규 컬럼에 직접 판정 결과를 써 넣고, 조회가 이를 실어오는지 검증
    store.conn.execute(
        "UPDATE findings SET judgment_json=?2 WHERE dedup_key=?1",
        rusqlite::params!["R6|Windows|abcd1234", r#"{"worthy":true,"reason":"매일 반복되는 절차"}"#],
    ).unwrap();
    let rows = store.list_findings_current(true).unwrap();
    let row = rows.iter().find(|r| r.dedup_key == "R6|Windows|abcd1234").unwrap();
    let j = row.judgment.as_ref().expect("judgment_json이 FindingRow로 실려야 함");
    assert_eq!(j["worthy"], serde_json::json!(true));
    assert_eq!(j["reason"], serde_json::json!("매일 반복되는 절차"));
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent_mentor judgment_json_column_roundtrips`
Expected: FAIL — `no field 'judgment' on type FindingRow` (컴파일 에러)

- [ ] **Step 3: 스키마·구조체·조회에 컬럼 반영**

`store.rs`의 `SCHEMA` 상수에서 `findings` 테이블 마지막 컬럼을 확장 (현재 `store.rs:48`):

```rust
  prescription_json TEXT, status TEXT NOT NULL DEFAULT 'new',
  first_seen TEXT, last_seen TEXT, occurrences INTEGER DEFAULT 1,
  judgment_json TEXT
```

`migrate` 함수의 ALTER 그룹(user_version 블록보다 **앞**, `store.rs:160` 직전)에 존재-검사 기반 컬럼 추가 — 신규 DB는 CREATE TABLE에 이미 있어 스킵된다:

```rust
    // R6 판정 레이어(PR2) — findings.judgment_json 부재 시 컬럼만 추가(재수집 불필요, 판정은 새로 채워짐).
    let has_judgment = conn
        .prepare("SELECT 1 FROM pragma_table_info('findings') WHERE name='judgment_json'")?
        .exists([])?;
    if !has_judgment {
        conn.execute_batch("ALTER TABLE findings ADD COLUMN judgment_json TEXT;")?;
    }
```

`FindingRow` 구조체(`store.rs:1502`)에 필드 추가 (마지막 `status` 아래):

```rust
    pub status: String,
    /// R6 판정 결과({worthy,reason,suggested_name,attempts,tokens}). 미판정이면 None.
    pub judgment: Option<serde_json::Value>,
```

`list_findings_current`(`store.rs:737`)의 SELECT·매핑에 `judgment_json` 추가 — SELECT 목록 끝(`status` 뒤)에 `, judgment_json`, `query_map` 클로저에 인덱스 13 추가, 구조체 채우기에 반영:

```rust
    // SELECT 목록: ... last_seen, occurrences, status, judgment_json
    // 클로저 끝에 추가:
                r.get::<_, String>(12)?,
                r.get::<_, Option<String>>(13)?,
    // 구조체 destructure: ..., status, judgment_raw
    // FindingRow 채우기 끝에:
                status,
                judgment: judgment_raw.and_then(|s| serde_json::from_str(&s).ok()),
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p agent_mentor judgment_json_column_roundtrips`
Expected: PASS

- [ ] **Step 5: 전체 store 테스트 회귀 확인** — `FindingRow` 필드 추가로 다른 생성부가 깨지지 않는지 (전부 `list_findings_current` 경유라 영향 없어야 함)

Run: `cargo test -p agent_mentor --lib store`
Expected: PASS (모든 store 테스트)

- [ ] **Step 6: 커밋**

```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): add findings.judgment_json column and expose on FindingRow"
```

---

## Task 2: `upsert_finding` 룰별 초기 status (R6 → pending)

신규 R6 finding은 판정 전이므로 `pending`(비노출)으로 들어가고, 나머지 룰은 기존대로 `new`. ON CONFLICT는 status를 건드리지 않아 판정 캐시가 재스캔에도 유지된다. (스펙 §4.1)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` (`upsert_finding`)
- Test: `a-mate/crates/core/src/store.rs`

**Interfaces:**
- Consumes: `Finding.rule_id` (Task 없음, 기존).
- Produces: 신규 R6 = `pending`, 그 외 = `new`. 재관측 시 status 불변.

- [ ] **Step 1: 실패 테스트 작성**

```rust
#[test]
fn upsert_seeds_r6_as_pending_others_as_new() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mk = |rule: &str, key: &str| Finding {
        rule_id: rule.into(), severity: Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: None,
        scope_kind: "pattern".into(), scope_ref: "x".into(),
        evidence: serde_json::json!({"repeated_prompt": "rep"}), est_tokens_saved: 0,
        prescription: None, dedup_key: key.into(),
    };
    store.upsert_finding(&mk("R6", "R6|W|a"), "2026-07-21T00:00:00Z").unwrap();
    store.upsert_finding(&mk("R11", "R11|W|b"), "2026-07-21T00:00:00Z").unwrap();
    let status = |k: &str| -> String {
        store.conn.query_row("SELECT status FROM findings WHERE dedup_key=?1",
            rusqlite::params![k], |r| r.get(0)).unwrap()
    };
    assert_eq!(status("R6|W|a"), "pending", "신규 R6은 판정 전 pending");
    assert_eq!(status("R11|W|b"), "new", "다른 룰은 기존대로 new");

    // 이미 판정돼 rejected가 된 R6은 재관측(upsert)돼도 status 불변 — 판정 캐시 유지
    store.set_finding_status("R6|W|a", "rejected").unwrap();
    store.upsert_finding(&mk("R6", "R6|W|a"), "2026-07-21T01:00:00Z").unwrap();
    assert_eq!(status("R6|W|a"), "rejected", "ON CONFLICT는 status를 덮지 않아야 함");
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent_mentor upsert_seeds_r6_as_pending`
Expected: FAIL — `assertion failed: status("R6|W|a") == "pending"` (현재 항상 `new`)

- [ ] **Step 3: `upsert_finding`에 룰별 초기 status 분기** (`store.rs:438`)

INSERT의 `status` 리터럴 `'new'`를 파라미터로 교체. 함수 상단에 추가하고 VALUES/params 반영:

```rust
    pub fn upsert_finding(&self, f: &Finding, now_ts: &str) -> Result<()> {
        let evidence = serde_json::to_string(&f.evidence)?;
        let presc = match &f.prescription {
            Some(p) => Some(serde_json::to_string(p)?),
            None => None,
        };
        // R6은 판정 전 비노출(pending), 나머지는 기존대로 즉시 노출(new). ON CONFLICT는 status 불변.
        let init_status = if f.rule_id == "R6" { "pending" } else { "new" };
        self.conn.execute(
            "INSERT INTO findings
                (dedup_key, rule_id, severity, scope_host, scope_project, scope_kind, scope_ref,
                 evidence_json, est_tokens_saved, prescription_json, status,
                 first_seen, last_seen, occurrences)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?12,?11,?11,1)
             ON CONFLICT(dedup_key) DO UPDATE SET
                last_seen = ?11,
                occurrences = occurrences + 1,
                est_tokens_saved = ?9,
                evidence_json = ?8,
                severity = ?3,
                prescription_json = ?10",
            params![
                f.dedup_key, f.rule_id, f.severity.as_str(), f.scope_host, f.scope_project,
                f.scope_kind, f.scope_ref, evidence, f.est_tokens_saved as i64, presc, now_ts,
                init_status
            ],
        )?;
        Ok(())
    }
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p agent_mentor upsert_seeds_r6_as_pending`
Expected: PASS

- [ ] **Step 5: R6를 `new`로 가정하던 기존 마이그레이션 테스트 보정**

`upsert_finding`이 R6를 pending으로 넣으므로, R6를 **활성(new)** 상태로 시드해 삭제/전환을 검증하던 기존 테스트가 깨진다. 먼저 전체를 돌려 실패 테스트를 특정:

Run: `cargo test -p agent_mentor --lib`
Expected: `migrate_v6_recollects_and_purges_repeat_junk` 등에서 FAIL 가능

깨진 테스트에서 R6 finding 시드 **직후** status를 명시적으로 되돌린다. 예 — `migrate_v6_recollects_and_purges_repeat_junk`(`store.rs:2608` 부근) junk 시드 뒤:

```rust
            store.upsert_finding(&f("R6", "R6|Windows|junk"), "2026-07-21T00:00:00Z").unwrap();
            // PR2: upsert가 R6을 pending으로 넣으므로, v6(오염된 'new' 카드 정화) 검증을 위해 new로 복원
            store.set_finding_status("R6|Windows|junk", "new").unwrap();
```

`migrate_v3_purges_r23_findings_without_recollect`·`migrate_synthetic_prompt_purges_and_forces_recollect_once` 등에서 R6를 dismissed로 만드는 시드는 pending→dismissed로 여전히 dismissed라 보정 불필요. **new 상태 유지를 전제하는 시드에만** 위 한 줄을 추가한다.

- [ ] **Step 6: 전체 회귀 통과 확인**

Run: `cargo test -p agent_mentor --lib`
Expected: PASS

- [ ] **Step 7: 커밋**

```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): seed R6 findings as pending for judgment gating"
```

---

## Task 3: v7 마이그레이션 — R23 전량 삭제 + R6 new→pending

R23 룰이 사라지므로 dismissed 포함 전량 삭제(쿨다운 기록도 무의미). PR1 배포로 노출됐던 R6 `new` 카드는 판정을 거치도록 `pending`으로 되돌린다. (스펙 §4.6)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs` (`migrate`)
- Test: `a-mate/crates/core/src/store.rs`

**Interfaces:**
- Consumes: 기존 `PRAGMA user_version` 체인(v6까지).
- Produces: `user_version = 7`.

- [ ] **Step 1: 실패 테스트 작성**

```rust
#[test]
fn migrate_v7_purges_r23_and_repends_r6() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("m7.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch("PRAGMA user_version = 6;").unwrap();
        let store = SqliteStore { conn };
        let f = |rule: &str, key: &str| Finding {
            rule_id: rule.into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "pattern".into(), scope_ref: "x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        store.upsert_finding(&f("R23", "R23|Windows|a"), "2026-07-21T00:00:00Z").unwrap();
        store.upsert_finding(&f("R23", "R23|Windows|muted"), "2026-07-21T00:00:00Z").unwrap();
        store.set_finding_status("R23|Windows|muted", "dismissed").unwrap();
        store.upsert_finding(&f("R6", "R6|Windows|active"), "2026-07-21T00:00:00Z").unwrap();
        store.set_finding_status("R6|Windows|active", "new").unwrap(); // PR1 시대 노출 카드
        store.upsert_finding(&f("R6", "R6|Windows|kept"), "2026-07-21T00:00:00Z").unwrap();
        store.set_finding_status("R6|Windows|kept", "dismissed").unwrap();
        store.upsert_finding(&f("R11", "R11|Windows|keep"), "2026-07-21T00:00:00Z").unwrap();
    }
    // 재오픈 → migrate 실행
    let store = SqliteStore::open(&path).unwrap();
    let count = |sql: &str| -> i64 { store.conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT COUNT(*) FROM findings WHERE rule_id='R23'"), 0, "R23 전량 삭제(dismissed 포함)");
    let status = |k: &str| -> String {
        store.conn.query_row("SELECT status FROM findings WHERE dedup_key=?1",
            rusqlite::params![k], |r| r.get(0)).unwrap()
    };
    assert_eq!(status("R6|Windows|active"), "pending", "노출 R6은 판정 대상으로 되돌림");
    assert_eq!(status("R6|Windows|kept"), "dismissed", "R6 dismissed는 보존");
    assert_eq!(status("R11|Windows|keep"), "new", "무관 룰은 불변");
    let uv: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(uv, 7);
    // 멱등 — 재오픈해도 안전
    drop(store);
    let store2 = SqliteStore::open(&path).unwrap();
    let uv2: i64 = store2.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(uv2, 7);
}
```

> 참고: 이 테스트는 기존 마이그레이션 테스트(`migrate_v6_*` 등)와 동일한 `SqliteStore::open` / `SCHEMA` / 직접 `PRAGMA` 시드 패턴을 따른다(`store.rs:2586` 참조). `SqliteStore { conn }` 필드 접근·`SqliteStore::open`은 테스트 모듈에서 이미 사용 중이다.

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent_mentor migrate_v7_purges_r23`
Expected: FAIL — R23 잔존 / `user_version == 6`

- [ ] **Step 3: v7 블록 추가** — `migrate` 함수의 v6 블록(`store.rs:206-213`) **뒤**, `Ok(())` 앞

```rust
    // v7 R6 판정 레이어(PR2 스펙 §4.6) — R23 룰 폐기: finding 전량 삭제(dismissed 포함,
    // 룰이 사라져 쿨다운 기록도 무의미). PR1 배포로 노출됐던 R6 'new' 카드는 판정을 거치도록
    // pending으로 되돌린다. dismissed/resolved는 사용자 기록이라 보존. 재수집 불필요.
    if user_version < 7 {
        conn.execute_batch(
            "DELETE FROM findings WHERE rule_id='R23';
             UPDATE findings SET status='pending' WHERE rule_id='R6' AND status='new';
             PRAGMA user_version = 7;",
        )?;
    }
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p agent_mentor migrate_v7_purges_r23`
Expected: PASS

- [ ] **Step 5: 기존 v6 테스트의 `user_version` 단언 완화**

`migrate()`는 항상 최신(v7)까지 연쇄 실행되므로, 특정 버전을 **엄격히** 단언하던 기존 테스트가 깨진다. `migrate_v6_recollects_and_purges_repeat_junk`(`store.rs:2629`·`2634`)의 `assert_eq!(uv, 6)` 두 곳을 완화한다 (`migrate_v5` 테스트의 `assert!(uv >= 6)` 선례):

```rust
    assert!(uv >= 6, "v6 분기 통과 (v7 연쇄로 최종 7)");
```

(다른 마이그레이션 테스트에 `assert_eq!(uv, N)`이 더 있으면 Step 6 회귀에서 드러난다 — 동일하게 `>=`로 완화한다.)

- [ ] **Step 6: 마이그레이션 전체 회귀 확인**

Run: `cargo test -p agent_mentor migrate`
Expected: PASS (v1~v7 전 체인)

- [ ] **Step 7: 커밋**

```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): add v7 migration to retire R23 and repend R6 findings"
```

---

## Task 4: 판정 배치 조회·적용 store 메서드

판정 패스가 후보를 뽑고 결과를 쓰는 두 메서드. (스펙 §4.2)

**Files:**
- Modify: `a-mate/crates/core/src/store.rs`
- Test: `a-mate/crates/core/src/store.rs`

**Interfaces:**
- Produces:
  - `pub struct JudgmentTarget { pub dedup_key: String, pub host: String, pub representative: String, pub prev_attempts: u32 }`
  - `pub fn pending_r6_for_judgment(&self, limit: usize) -> Result<Vec<JudgmentTarget>>` — Task 7이 소비
  - `pub fn set_judgment(&self, dedup_key: &str, new_status: Option<&str>, judgment: &serde_json::Value) -> Result<()>` — Task 7이 소비

- [ ] **Step 1: 실패 테스트 작성**

```rust
#[test]
fn pending_r6_batch_filters_attempts_and_limits() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mk = |key: &str, rep: &str| Finding {
        rule_id: "R6".into(), severity: Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: None,
        scope_kind: "pattern".into(), scope_ref: "x".into(),
        evidence: serde_json::json!({"repeated_prompt": rep}), est_tokens_saved: 0,
        prescription: None, dedup_key: key.into(),
    };
    // 12개 pending 시드 (last_seen 순서 구분)
    for i in 0..12 {
        let ts = format!("2026-07-21T00:{:02}:00Z", i);
        store.upsert_finding(&mk(&format!("R6|W|{i}"), &format!("반복 지시 {i}번")), &ts).unwrap();
    }
    // 하나는 attempts=3 도달 → 제외
    store.set_judgment("R6|W|0", None, &serde_json::json!({"attempts": 3, "error": "malformed"})).unwrap();
    // 하나는 이미 판정돼 new → pending 아님 → 제외
    store.set_judgment("R6|W|1", Some("new"), &serde_json::json!({"worthy": true, "attempts": 1})).unwrap();

    let batch = store.pending_r6_for_judgment(10).unwrap();
    assert_eq!(batch.len(), 10, "배치 상한 10");
    assert!(batch.iter().all(|t| t.dedup_key != "R6|W|0"), "attempts 3 도달분 제외");
    assert!(batch.iter().all(|t| t.dedup_key != "R6|W|1"), "판정 완료(new) 제외");
    // last_seen DESC → 가장 최근(11번)이 먼저
    assert_eq!(batch[0].dedup_key, "R6|W|11");
    assert_eq!(batch[0].representative, "반복 지시 11번");
    assert_eq!(batch[0].prev_attempts, 0, "미시도는 attempts 0");
}

#[test]
fn set_judgment_transitions_status_or_keeps_pending() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f = Finding {
        rule_id: "R6".into(), severity: Severity::Suggest,
        scope_host: Some("Windows".into()), scope_project: None,
        scope_kind: "pattern".into(), scope_ref: "x".into(),
        evidence: serde_json::json!({"repeated_prompt": "r"}), est_tokens_saved: 0,
        prescription: None, dedup_key: "R6|W|k".into(),
    };
    store.upsert_finding(&f, "2026-07-21T00:00:00Z").unwrap();
    // status 유지(파싱 실패) — judgment_json만 갱신
    store.set_judgment("R6|W|k", None, &serde_json::json!({"attempts": 1, "error": "bad"})).unwrap();
    let (st, j): (String, String) = store.conn.query_row(
        "SELECT status, judgment_json FROM findings WHERE dedup_key='R6|W|k'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(st, "pending");
    assert!(j.contains("\"attempts\":1"));
    // status 전환(판정 완료)
    store.set_judgment("R6|W|k", Some("rejected"), &serde_json::json!({"worthy": false, "attempts": 1})).unwrap();
    let st2: String = store.conn.query_row(
        "SELECT status FROM findings WHERE dedup_key='R6|W|k'", [], |r| r.get(0)).unwrap();
    assert_eq!(st2, "rejected");
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent_mentor pending_r6_batch_filters`
Expected: FAIL — `no method 'pending_r6_for_judgment'`

- [ ] **Step 3: `JudgmentTarget` 구조체 + 두 메서드 구현** — `store.rs`의 `impl SqliteStore` 안(예: `set_finding_status` 근처, `store.rs:782` 뒤)

```rust
    /// R6 판정 배치 대상 — pending & 시도 3회 미만, 최근 활동 순 상한 LIMIT.
    /// attempts는 judgment_json.$.attempts (NULL=0). (스펙 §4.2)
    pub fn pending_r6_for_judgment(&self, limit: usize) -> Result<Vec<JudgmentTarget>> {
        let mut stmt = self.conn.prepare(
            "SELECT dedup_key, scope_host,
                    json_extract(evidence_json,'$.repeated_prompt'),
                    COALESCE(json_extract(judgment_json,'$.attempts'), 0)
             FROM findings
             WHERE rule_id='R6' AND status='pending'
               AND COALESCE(json_extract(judgment_json,'$.attempts'), 0) < 3
             ORDER BY last_seen DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(JudgmentTarget {
                dedup_key: r.get(0)?,
                host: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                representative: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                prev_attempts: r.get::<_, i64>(3)? as u32,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// 판정 결과 저장. new_status=Some → status 전환(worthy→'new', unworthy→'rejected'),
    /// None → status 불변(파싱 실패 시 pending 잔류). judgment_json은 항상 갱신.
    pub fn set_judgment(
        &self,
        dedup_key: &str,
        new_status: Option<&str>,
        judgment: &serde_json::Value,
    ) -> Result<()> {
        let j = serde_json::to_string(judgment)?;
        match new_status {
            Some(s) => self.conn.execute(
                "UPDATE findings SET status=?2, judgment_json=?3 WHERE dedup_key=?1",
                params![dedup_key, s, j],
            )?,
            None => self.conn.execute(
                "UPDATE findings SET judgment_json=?2 WHERE dedup_key=?1",
                params![dedup_key, j],
            )?,
        };
        Ok(())
    }
```

`FindingRow` 구조체 근처(`store.rs:1502` 부근)에 공개 구조체 추가:

```rust
/// R6 판정 배치 후보 — pending finding에서 뽑은 판정 재료 참조.
#[derive(Debug, Clone)]
pub struct JudgmentTarget {
    pub dedup_key: String,
    pub host: String,
    pub representative: String,
    pub prev_attempts: u32,
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p agent_mentor pending_r6_batch_filters set_judgment_transitions`
Expected: PASS

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/store.rs
git commit -m "feat(agent): add R6 judgment batch query and result persistence"
```

---

## Task 5: `judge` 모듈 — 판정 프롬프트 + 엄격 JSON 파싱

판정의 순수 로직. 엔진 없이 테스트되는 프롬프트 구성과 파싱. (스펙 §4.3)

**Files:**
- Create: `a-mate/crates/core/src/judge.rs`
- Modify: `a-mate/crates/core/src/lib.rs` (`pub mod judge;`)
- Test: `a-mate/crates/core/src/judge.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `skill_draft::DraftContext` (기존, `representative`·`session_count`·`sample_prompts`·`top_tools`).
- Produces:
  - `pub struct Judgment { pub worthy: bool, pub reason: String, pub suggested_name: String }`
  - `pub fn judgment_prompt(ctx: &DraftContext) -> (String, String)`
  - `pub fn parse_judgment(text: &str) -> anyhow::Result<Judgment>`

- [ ] **Step 1: 모듈 등록** — `lib.rs`의 `inventory` 선언(`lib.rs:10`) 뒤

```rust
pub mod inventory;
pub mod judge;
pub mod mascot;
```

- [ ] **Step 2: 실패 테스트를 담은 모듈 뼈대 작성** — `judge.rs` 생성

```rust
//! R6 반복 지시 → "스킬화할 가치가 있는가" 의미 판정 (PR2 스펙 §4).
//! 채굴(SQL)이 잡은 pending R6를 Engine으로 걸러 precision을 확보한다.
//! 순수 로직(프롬프트·파싱·결과 변환)만 여기 있고, 락·네트워크는 src-tauri가 조립한다.

use crate::skill_draft::DraftContext;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgment {
    pub worthy: bool,
    pub reason: String,
    pub suggested_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> DraftContext {
        DraftContext {
            representative: "PR 리뷰 코멘트 종합 검토해서 조치해줘".into(),
            session_count: 4,
            sample_prompts: vec!["PR 리뷰 코멘트 정리".into(), "리뷰 코멘트 반영해줘".into()],
            top_tools: vec![("Bash".into(), 12), ("Read".into(), 8)],
        }
    }

    #[test]
    fn prompt_carries_criteria_and_evidence() {
        let (system, user) = judgment_prompt(&ctx());
        assert!(system.contains("worthy"));
        assert!(system.contains("확신"), "정밀도 우선 기준 명시");
        assert!(system.contains("JSON"));
        assert!(user.contains("PR 리뷰 코멘트"));
        assert!(user.contains("4"), "세션 수 포함");
        assert!(user.contains("Bash"), "주요 도구 포함");
    }

    #[test]
    fn parse_accepts_strict_json() {
        let j = parse_judgment(r#"{"worthy":true,"reason":"매번 같은 리뷰 절차","suggested_name":"pr-review-triage"}"#).unwrap();
        assert!(j.worthy);
        assert_eq!(j.suggested_name, "pr-review-triage");
    }

    #[test]
    fn parse_tolerates_codefence_and_prose() {
        let j = parse_judgment("판정 결과:\n```json\n{\"worthy\":false,\"reason\":\"대화 접착제\",\"suggested_name\":\"none\"}\n```").unwrap();
        assert!(!j.worthy);
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(parse_judgment("죄송하지만 판정할 수 없습니다").is_err());
        assert!(parse_judgment(r#"{"worthy":true}"#).is_err(), "필수 필드 누락은 실패");
    }
}
```

- [ ] **Step 3: 테스트 실패 확인**

Run: `cargo test -p agent_mentor --lib judge::`
Expected: FAIL — `cannot find function 'judgment_prompt'`

- [ ] **Step 4: `judgment_prompt` + `parse_judgment` 구현** — `judge.rs`의 `Judgment` 아래(`#[cfg(test)]` 위)

```rust
/// 판정 프롬프트 (정밀도 우선, 한국어). system=기준, user=재료. (스펙 §4.3)
pub fn judgment_prompt(ctx: &DraftContext) -> (String, String) {
    let system = "당신은 Claude Code 사용 습관을 코칭하는 심사관입니다. \
사용자가 여러 세션에서 반복한 지시가 '재사용 가능한 스킬/커맨드로 묶을 가치'가 있는지 판정하세요.\n\
worthy=true: 재사용 가능한 절차·규칙·체크리스트를 담은 지시 — 매번 같은 다단계 작업, 정해진 형식이나 규칙을 요구하는 지시.\n\
worthy=false: 대화 접착제('진행해줘','계속','ㅇㅋ' 등), 일회성·문맥 의존 지시, 단순 질문·피드백, 인사.\n\
정밀도가 최우선입니다 — 확신이 없으면 worthy=false로 판정하세요.\n\
반드시 아래 형태의 JSON 객체 하나만 출력하고, 그 밖의 설명·코드펜스는 붙이지 마세요:\n\
{\"worthy\": true 또는 false, \"reason\": \"판정 이유를 담은 한국어 한 문장\", \"suggested_name\": \"kebab-case 슬러그\"}"
        .to_string();

    let samples = if ctx.sample_prompts.is_empty() {
        "- (변형 표본 없음)".to_string()
    } else {
        ctx.sample_prompts
            .iter()
            .map(|p| format!("- \"{}\"", p.replace('"', "'")))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let tools = if ctx.top_tools.is_empty() {
        "(도구 기록 없음)".to_string()
    } else {
        ctx.top_tools
            .iter()
            .take(6)
            .map(|(t, n)| format!("{t}({n})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let user = format!(
        "대표 지시: \"{rep}\"\n반복된 세션 수: {count}\n변형 표본:\n{samples}\n주요 도구: {tools}",
        rep = ctx.representative.replace('"', "'"),
        count = ctx.session_count,
        samples = samples,
        tools = tools,
    );
    (system, user)
}

/// 엔진 응답에서 엄격 JSON 판정을 추출. 코드펜스·사족을 관대히 벗기되(첫 '{'~마지막 '}'),
/// 필수 필드(worthy·reason·suggested_name)가 없으면 실패로 본다.
pub fn parse_judgment(text: &str) -> Result<Judgment> {
    let start = text.find('{').ok_or_else(|| anyhow!("판정 응답에 JSON 객체 없음"))?;
    let end = text.rfind('}').ok_or_else(|| anyhow!("판정 응답에 JSON 객체 없음"))?;
    if end < start {
        return Err(anyhow!("판정 응답 JSON 경계 불량"));
    }
    let j: Judgment = serde_json::from_str(&text[start..=end])?;
    Ok(j)
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test -p agent_mentor --lib judge::`
Expected: PASS

- [ ] **Step 6: 커밋**

```bash
git add a-mate/crates/core/src/lib.rs a-mate/crates/core/src/judge.rs
git commit -m "feat(agent): add R6 judgment prompt and strict JSON parsing"
```

---

## Task 6: `judge` 모듈 — `judge_one` + 결과 변환

엔진 1회 호출과 4가지 결과(worthy/unworthy/malformed/전송실패) 처리, 그리고 결과→(status, judgment_json) 변환. (스펙 §4.2)

**Files:**
- Modify: `a-mate/crates/core/src/judge.rs`
- Test: `a-mate/crates/core/src/judge.rs`

**Interfaces:**
- Consumes: `diary::engine::Engine`, `Judgment`(Task 5), `DraftContext`.
- Produces:
  - `pub enum JudgeOutcome { Judged(Judgment), Malformed(String) }`
  - `pub struct JudgeResult { pub outcome: JudgeOutcome, pub tokens: u64 }`
  - `pub fn judge_one(engine: &dyn Engine, ctx: &DraftContext) -> anyhow::Result<JudgeResult>` (Err = 전송 실패)
  - `pub fn judgment_record(prev_attempts: u32, result: &JudgeResult) -> (Option<&'static str>, serde_json::Value)` (Task 7이 소비)

- [ ] **Step 1: 실패 테스트 작성** — `judge.rs` 테스트 모듈에 추가

```rust
    use crate::diary::engine::{Engine, EngineOutput, MockEngine};

    /// 전송 실패를 흉내내는 엔진.
    struct FailEngine;
    impl Engine for FailEngine {
        fn name(&self) -> String { "fail".into() }
        fn generate(&self, _s: &str, _u: &str) -> anyhow::Result<EngineOutput> {
            Err(anyhow!("connection refused"))
        }
        fn chat(&self, _s: &str, _m: &[crate::diary::engine::ChatMessage]) -> anyhow::Result<EngineOutput> {
            Err(anyhow!("n/a"))
        }
    }

    #[test]
    fn judge_one_worthy_then_record_promotes_to_new() {
        let eng = MockEngine {
            canned: r#"{"worthy":true,"reason":"매번 같은 릴리스 절차","suggested_name":"release-flow"}"#.into(),
        };
        let res = judge_one(&eng, &ctx()).unwrap();
        assert!(matches!(res.outcome, JudgeOutcome::Judged(ref j) if j.worthy));
        assert!(res.tokens > 0, "토큰 계량");
        let (status, rec) = judgment_record(0, &res);
        assert_eq!(status, Some("new"));
        assert_eq!(rec["attempts"], serde_json::json!(1));
        assert_eq!(rec["suggested_name"], serde_json::json!("release-flow"));
    }

    #[test]
    fn judge_one_unworthy_records_rejected() {
        let eng = MockEngine { canned: r#"{"worthy":false,"reason":"대화 접착제","suggested_name":"none"}"#.into() };
        let (status, rec) = judgment_record(0, &judge_one(&eng, &ctx()).unwrap());
        assert_eq!(status, Some("rejected"));
        assert_eq!(rec["worthy"], serde_json::json!(false));
    }

    #[test]
    fn judge_one_malformed_bumps_attempts_keeps_pending() {
        let eng = MockEngine { canned: "판정 불가합니다".into() };
        let res = judge_one(&eng, &ctx()).unwrap();
        assert!(matches!(res.outcome, JudgeOutcome::Malformed(_)));
        let (status, rec) = judgment_record(1, &res);
        assert_eq!(status, None, "형식 불량은 status 유지(pending)");
        assert_eq!(rec["attempts"], serde_json::json!(2), "attempts+1");
        assert!(rec.get("error").is_some());
    }

    #[test]
    fn judge_one_transport_failure_is_err() {
        // 전송 실패는 Err → 상위에서 skip(attempts 미증가)
        assert!(judge_one(&FailEngine, &ctx()).is_err());
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test -p agent_mentor --lib judge::`
Expected: FAIL — `cannot find function 'judge_one'`

- [ ] **Step 3: `judge_one` + `judgment_record` 구현** — `judge.rs`의 `parse_judgment` 아래

```rust
use crate::diary::engine::Engine;

pub enum JudgeOutcome {
    /// 파싱 성공 — worthy/unworthy 판정 확정.
    Judged(Judgment),
    /// 응답이 왔으나 JSON 파싱 실패 — 인프라 실패가 아님(재시도 대상, attempts 증가).
    Malformed(String),
}

pub struct JudgeResult {
    pub outcome: JudgeOutcome,
    pub tokens: u64,
}

/// 후보 1건 판정 — 엔진 1회 호출. Err = **전송 실패**(엔진 다운·타임아웃)로,
/// 상위는 attempts를 올리지 않고 pending에 남겨 다음 스캔에 재시도한다.
pub fn judge_one(engine: &dyn Engine, ctx: &DraftContext) -> Result<JudgeResult> {
    let (system, user) = judgment_prompt(ctx);
    let out = engine.generate(&system, &user)?; // Err → 전송 실패
    let outcome = match parse_judgment(&out.text) {
        Ok(j) => JudgeOutcome::Judged(j),
        Err(e) => JudgeOutcome::Malformed(e.to_string()),
    };
    Ok(JudgeResult { outcome, tokens: out.tokens_used })
}

/// 판정 결과를 (새 status, judgment_json)로 변환.
/// - Judged(worthy)  → Some("new"),      {worthy,reason,suggested_name,attempts,tokens}
/// - Judged(!worthy) → Some("rejected"), 〃 (영구 캐시)
/// - Malformed       → None(status 유지),{attempts,error,tokens} — rejected로 오캐시 금지
pub fn judgment_record(
    prev_attempts: u32,
    result: &JudgeResult,
) -> (Option<&'static str>, serde_json::Value) {
    let attempts = prev_attempts + 1;
    match &result.outcome {
        JudgeOutcome::Judged(j) => {
            let status = if j.worthy { "new" } else { "rejected" };
            (
                Some(status),
                serde_json::json!({
                    "worthy": j.worthy,
                    "reason": j.reason,
                    "suggested_name": j.suggested_name,
                    "attempts": attempts,
                    "tokens": result.tokens,
                }),
            )
        }
        JudgeOutcome::Malformed(e) => (
            None,
            serde_json::json!({ "attempts": attempts, "error": e, "tokens": result.tokens }),
        ),
    }
}
```

> 참고: 파일 상단에 이미 `use ...` 블록이 있으므로 `use crate::diary::engine::Engine;`은 상단으로 병합해도 된다(중복 import 경고 방지).

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p agent_mentor --lib judge::`
Expected: PASS (7개 judge 테스트)

- [ ] **Step 5: 커밋**

```bash
git add a-mate/crates/core/src/judge.rs
git commit -m "feat(agent): add judge_one engine call and judgment record mapping"
```

---

## Task 7: src-tauri 판정 패스 런타임 훅

스캔 완료 후 락 밖에서 배치 판정을 실행한다. 다이어리 생성과 동일한 락 규율(엔진 해석·재료 수집=짧은 락, LLM 호출=락 밖, 결과 저장=짧은 락). (스펙 §4.2)

**Files:**
- Modify: `a-mate/src-tauri/src/pipeline.rs` (`mod runtime`)

**Interfaces:**
- Consumes: `store.pending_r6_for_judgment`·`store.set_judgment`(Task 4), `skill_draft::gather_context`(기존), `judge::judge_one`·`judge::judgment_record`(Task 6), `crate::resolve_engine`(기존 `lib.rs:139`).

> **테스트 메모:** `pipeline.rs`의 `mod runtime`은 `#[cfg(not(test))]`라 유닛 테스트가 없다(기존 `maybe_generate_diaries` 등과 동일). 판정 핵심 로직은 Task 4·6에서 core 테스트로 전부 커버된다. 이 태스크의 검증은 **컴파일 성공 + 스캔 흐름에 호출 배선**이다.

- [ ] **Step 1: `maybe_judge_repeats` 함수 추가** — `pipeline.rs`의 `maybe_generate_diaries` 정의(`pipeline.rs:170`) 바로 앞

```rust
    /// R6 반복 지시 판정 패스(PR2 스펙 §4.2) — 스캔 편승. pending R6를 Engine으로 걸러
    /// worthy→노출(new)/unworthy→영구 캐시(rejected)로 전환한다. 엔진 미설정이면
    /// 그대로 반환(pending 잔류 = fail-safe 침묵). 락 규율은 diary와 동일.
    fn maybe_judge_repeats(store_mutex: &std::sync::Mutex<SqliteStore>) {
        use agent_mentor::diary::engine::Engine;
        // ① 엔진 해석 (짧은 락) — 미설정이면 침묵
        let engine = match store_mutex.lock() {
            Ok(store) => crate::resolve_engine(&store),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        let Some(engine) = engine else { return; };

        // ② 배치 + 판정 재료 수집 (짧은 락, SQL만) → 즉시 해제
        let targets = match store_mutex.lock() {
            Ok(store) => store
                .pending_r6_for_judgment(10)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| {
                    agent_mentor::skill_draft::gather_context(&store, &t.host, &t.representative)
                        .ok()
                        .map(|ctx| (t, ctx))
                })
                .collect::<Vec<_>>(),
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
        if targets.is_empty() { return; }

        // ③ 락 밖: 판정 (LLM 네트워크 I/O)
        let mut results = Vec::new();
        for (t, ctx) in targets {
            match agent_mentor::judge::judge_one(&engine, &ctx) {
                Ok(res) => results.push((t.dedup_key, t.prev_attempts, res)),
                // 전송 실패 — attempts 미증가, pending 잔류(다음 스캔 재시도)
                Err(e) => log::warn!("R6 판정 전송 실패({}): {e}", t.dedup_key),
            }
        }

        // ④ 결과 저장 (짧은 락)
        if let Ok(store) = store_mutex.lock() {
            for (key, prev, res) in results {
                let (status, judgment) = agent_mentor::judge::judgment_record(prev, &res);
                if let Err(e) = store.set_judgment(&key, status, &judgment) {
                    log::warn!("set_judgment({key}) 실패: {e}");
                }
            }
        }
    }
```

- [ ] **Step 2: `run_pipeline_once`에서 호출 배선** — 성공 경로, `maybe_generate_diaries(app, &state.store);` 호출(`pipeline.rs:110`) **앞**

```rust
                // R6 반복 지시 판정 — 엔진 없으면 no-op(pending 침묵), 실패는 조용히(다음 스캔 재시도)
                maybe_judge_repeats(&state.store);
                // 다이어리 실패는 조용히 — 다음 사이클에서 재시도
                maybe_generate_diaries(app, &state.store);
```

- [ ] **Step 3: 컴파일 확인**

Run: `cargo build -p agent-mentor-app`
Expected: 성공 (경고 없음). 실패 시 `use` 경로·타입 불일치 수정.

- [ ] **Step 4: 워크스페이스 전체 테스트 확인**

Run: `cargo test`
Expected: PASS (core + app)

- [ ] **Step 5: 커밋**

```bash
git add a-mate/src-tauri/src/pipeline.rs
git commit -m "feat(agent): run R6 judgment pass after scan"
```

---

## Task 8: R23 룰·코드 폐기 (Rust)

R23은 finding 생산을 중단하고, 유일 소비자였던 시퀀스 초안 경로까지 함께 제거한다. (스펙 §4.4)

**Files:**
- Modify: `a-mate/crates/core/src/ops.rs` (`run_rules` 등록·prune·테스트)
- Modify: `a-mate/crates/core/src/rules/mod.rs`
- Delete: `a-mate/crates/core/src/rules/r23_tool_sequences.rs`
- Modify: `a-mate/crates/core/src/skill_draft.rs` (`gather_context_for_sequence` + 테스트)
- Modify: `a-mate/src-tauri/src/commands.rs` (`generate_skill_draft`)

**Interfaces:**
- Consumes: 없음(제거 작업).
- Produces: `generate_skill_draft(host, representative, suggested_name: Option<String>)` — Task 9(프론트)가 소비. `sequence` 파라미터 제거.

> **사전 확인(스펙 §4.4 요구):** R23 코드의 소비자는 `skill_draft::gather_context_for_sequence` 하나뿐이고, 그것의 소비자는 `commands::generate_skill_draft`의 `sequence` 경로 하나뿐임을 이미 확인했다(`gather_context_for_sequence`·`sessions_containing`·`collect_streams`·`tokenize` grep 결과). R6 초안의 도구 통계는 `tool_usage_for_sessions`(raw_name 기반)라 R23과 무관하다.

- [ ] **Step 1: `run_rules`에서 R23 등록·prune 제거** — `ops.rs:167-192`

`RuleEngine::new` 벡터에서 R23 라인(`ops.rs:170-171`) 삭제:

```rust
    let engine = RuleEngine::new(vec![
        // R6(반복 지시 → 스킬/커맨드화)은 v3 은퇴 대상 아님 — 킥오프 차별점 신규 등록
        Box::new(crate::rules::r6_repeated_prompts::R6RepeatedPrompts::default()),
        Box::new(R7OpusTrivial::default()),
        // R8(MCP 대형 결과) — result_len 수집 승격, 2026-07-19
        Box::new(crate::rules::r8_mcp_large_result::R8McpLargeResult::default()),
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
```

즉 R23 전용 prune 블록(`ops.rs:184-190`, `r23_keys` 수집 + `prune_new_findings_to_current("R23", ...)`)도 함께 삭제한다. (`prune_new_findings_to_current`·`delete_findings_by_rule_and_scope` 함수 자체는 범용 API라 보존.)

- [ ] **Step 2: R23 관련 `ops.rs` 테스트 삭제** — `run_rules_prunes_displaced_r23_cards_but_keeps_dismissed`(`ops.rs:360-409`) 전체 제거. `run_rules_purges_retired_rule_findings`(`ops.rs:334`)는 R23을 다루지 않으므로 유지.

- [ ] **Step 3: 모듈 등록 해제 + 파일 삭제**

`rules/mod.rs:11`의 `pub mod r23_tool_sequences;` 줄 삭제, 그리고:

```bash
git rm a-mate/crates/core/src/rules/r23_tool_sequences.rs
```

- [ ] **Step 4: `skill_draft`의 시퀀스 경로 삭제** — `skill_draft.rs`

`gather_context_for_sequence`(`skill_draft.rs:58-89`) 함수 전체 삭제, 그 테스트 `gather_for_sequence_matches_tool_streams`(`skill_draft.rs:310-346`) 전체 삭제. `gather_context`(R6용)·`skeleton_draft`·`build_draft`·`slugify`·`write_draft`는 유지.

- [ ] **Step 5: `generate_skill_draft` 시그니처 변경** — `commands.rs:1354-1379`

```rust
/// R6 반복 지시로 반복 워크플로를 되짚어 SKILL.md 초안을 생성. host+대표 프롬프트로 매칭.
/// 판정이 제안한 이름(suggested_name)이 있으면 초안 기본 슬러그로 쓴다.
#[tauri::command(async)]
pub fn generate_skill_draft(
    state: State<AppState>,
    host: String,
    representative: String,
    suggested_name: Option<String>,
) -> Result<SkillDraftResult, String> {
    // 1) 재료 수집 + 엔진 구성 (락 안, SQL만)
    let (ctx, engine) = {
        let guard = lock(&state)?;
        let ctx = agent_mentor::skill_draft::gather_context(&guard, &host, &representative)
            .map_err(|e| e.to_string())?;
        (ctx, resolve_engine(&guard))
    };
    // 2) 초안 생성 (락 밖, LLM 네트워크 가능)
    let mut draft = agent_mentor::skill_draft::build_draft(&ctx, engine.as_ref().map(|e| e as &dyn Engine));
    // 판정이 제안한 이름이 있으면 기본 슬러그로 사용(사용자가 저장 전 편집 가능)
    if let Some(name) = suggested_name.as_deref().filter(|s| !s.trim().is_empty()) {
        draft.slug = agent_mentor::skill_draft::slugify(name);
    }
    Ok(SkillDraftResult {
        markdown: draft.markdown,
        slug: draft.slug,
        llm_generated: draft.llm_generated,
        session_count: ctx.session_count,
    })
}
```

- [ ] **Step 6: 컴파일·테스트 확인** — 삭제로 인한 orphan(미사용 import 등) 정리

Run: `cargo test`
Expected: PASS. 컴파일 경고(미사용 `use` 등)가 있으면 해당 파일에서 제거.

- [ ] **Step 7: 커밋**

```bash
git add a-mate/crates/core/src/ops.rs a-mate/crates/core/src/rules/mod.rs a-mate/crates/core/src/skill_draft.rs a-mate/src-tauri/src/commands.rs
git rm a-mate/crates/core/src/rules/r23_tool_sequences.rs
git commit -m "refactor(agent): retire R23 tool-sequence rule and its skill-draft path"
```

---

## Task 9: UI — R23 제거 + R6 판정 사유 + suggested_name

코치 탭에서 R23 흔적을 지우고, R6 카드에 판정 사유를 보여주며, 초안 생성에 제안 이름을 전달한다. (스펙 §4.4, §4.5)

**Files:**
- Modify: `a-mate/src/lib/api.ts`
- Modify: `a-mate/src/lib/ui/coach-helpers.ts`
- Modify: `a-mate/src/lib/ui/coach-helpers.test.ts`
- Modify: `a-mate/src/lib/ui/CoachTab.svelte`

**Interfaces:**
- Consumes: `generate_skill_draft(host, representative, suggested_name)` (Task 8), `FindingRow.judgment`(Task 1, `CoachFinding`에 flatten됨).

- [ ] **Step 1: `coach-helpers.test.ts` 기대 갱신 (실패 유도)** — `coach-helpers.test.ts:48-51`

R23 항목을 제거하고 R6만 남긴다:

```ts
  it('R6은 반복 지시 제목', () => {
    expect(coachTitle('R6', {})).toContain('같은 지시');
  });
```

- [ ] **Step 2: 테스트 실패 확인**

Run (in `a-mate/`): `npm test -- coach-helpers`
Expected: 이 시점엔 아직 PASS일 수 있음(R23 매핑이 남아 있어도 R6 단독 assert는 통과). Step 3에서 매핑 제거 후 다시 확인한다.

- [ ] **Step 3: `coach-helpers.ts`에서 R23 제거** — `COACH_TITLE`(`coach-helpers.ts:39`)에서 R23 줄 삭제

```ts
  R11: '거부한 뒤 결국 허용한 도구가 있어요',
  R12: '설치해둔 스킬이 놀고 있어요',
};
```

(R23 줄 삭제. 다른 항목은 유지.)

- [ ] **Step 4: `api.ts` — `Finding.judgment` 필드 + `generateSkillDraft` 시그니처** 

`Finding` 인터페이스(`api.ts:18-32`)의 `status` 아래에 추가:

```ts
  status: string;
  /** R6 판정 결과(worthy만 노출됨). rejected/pending은 목록에 안 옴. */
  judgment?: { worthy?: boolean; reason?: string; suggested_name?: string } | null;
```

`generateSkillDraft`(`api.ts:299-306`)에서 `sequence`를 `suggestedName`으로 교체:

```ts
/** R6 반복 지시 → SKILL.md 초안 생성. 판정이 제안한 이름(suggestedName)을 기본 슬러그로 쓴다. */
export async function generateSkillDraft(
  host: string,
  representative: string,
  suggestedName: string | null = null,
): Promise<SkillDraft> {
  return invoke<SkillDraft>('generate_skill_draft', { host, representative, suggestedName });
}
```

> Tauri 커맨드 인자는 camelCase로 직렬화된다(`invoke`가 snake_case Rust 인자에 매핑). 기존 `includeHidden`↔`include_hidden`(`api.ts:99`) 선례와 동일하게 `suggestedName`↔`suggested_name`.

- [ ] **Step 5: `CoachTab.svelte` — R23 분기 제거 + 판정 사유 + suggested_name**

`sequenceOf`(`CoachTab.svelte:84-90`) 삭제. `skillifiable`(`CoachTab.svelte:91-92`)을 R6 전용으로:

```ts
  const repeatedPromptOf = (f: CoachFinding): string | null => {
    const ev = f.evidence as { repeated_prompt?: string } | null;
    return ev?.repeated_prompt ?? null;
  };
  const skillifiable = (f: CoachFinding): boolean =>
    f.rule_id === 'R6' && !!repeatedPromptOf(f);
```

`makeDraft`(`CoachTab.svelte:102-113`)에서 seq 제거, suggested_name 전달:

```ts
  async function makeDraft(f: CoachFinding) {
    const rep = repeatedPromptOf(f);
    if (!rep) return;
    const suggested = f.judgment?.suggested_name ?? null;
    draft = { key: f.dedup_key, loading: true, result: null, error: null, savedPath: null, copied: false };
    try {
      const r = await generateSkillDraft(f.scope_host ?? '', rep, suggested);
      draft = { key: f.dedup_key, loading: false, result: r, error: null, savedPath: null, copied: false };
    } catch (e) {
      draft = { key: f.dedup_key, loading: false, result: null, error: String(e), savedPath: null, copied: false };
    }
  }
```

판정 사유 1줄 표시 — active 카드의 `.why`(`CoachTab.svelte:148`) 아래에 추가:

```svelte
        <p class="why">{f.detail}{#if f.scope_kind === 'session'} · {f.occurrences}회 관측{/if}</p>
        {#if f.judgment?.reason}
          <p class="judgment">🧭 코치 판정: {f.judgment.reason}</p>
        {/if}
```

스타일(`<style>` 블록, `.why` 규칙 근처)에 추가:

```css
  .judgment { margin: 2px 0 6px; font-size: 12px; color: var(--accent); }
```

- [ ] **Step 6: 프론트 테스트·타입체크 확인**

Run (in `a-mate/`): `npm test`
Expected: PASS (coach-helpers 포함 전체 Vitest)

Run: `npm run check` (svelte-check, 있는 경우) 또는 `npx tsc --noEmit`
Expected: 타입 에러 없음

- [ ] **Step 7: 커밋**

```bash
git add a-mate/src/lib/api.ts a-mate/src/lib/ui/coach-helpers.ts a-mate/src/lib/ui/coach-helpers.test.ts a-mate/src/lib/ui/CoachTab.svelte
git commit -m "feat(frontend): show R6 judgment reason and drop R23 coach card"
```

---

## Final Verification

- [ ] **전체 Rust 테스트**

Run (in `a-mate/`): `cargo test`
Expected: PASS (워크스페이스 전체)

- [ ] **전체 프론트 테스트**

Run (in `a-mate/`): `npm test`
Expected: PASS

- [ ] **릴리스 빌드 온전성** (선택, 시간 여유 시)

Run (in `a-mate/`): `cargo build --release -p agent-mentor-app`
Expected: 성공

- [ ] **DoD — 문서 아카이브**

`docs-archive` 스킬로 본 계획 문서(`docs/design/a-mate/plans/2026-07-21-repeat-coaching-pr2-judgment-layer.md`)와 스펙(`docs/design/a-mate/specs/2026-07-21-repeat-coaching-judgment-redesign.md`)을 `docs/archive/` 미러로 이관한다. 스펙은 PR1·PR2를 함께 다루므로 **PR2 완료 시점**에 아카이브한다.

---

## Notes / 설계 근거

- **판정 결과를 evidence가 아닌 `judgment_json`에 저장** — 룰 재평가가 evidence를 갱신해도 판정을 덮지 않고, 병합 로직이 불필요하다(스펙 §4.1).
- **`upsert_finding`의 ON CONFLICT가 status를 안 건드림** — 한 번 `rejected`/`new`가 된 R6는 재스캔에도 재판정되지 않는다(판정 캐시). `pending_r6_for_judgment`가 `status='pending'`만 뽑으므로 캐시된 카드는 배치에 다시 안 들어온다.
- **fail-safe 침묵** — 엔진 미설정 시 `maybe_judge_repeats`는 즉시 반환, pending R6는 영원히 비노출. 코치 탭·집계는 이미 `status='new'`만 노출하므로 UI 변경이 최소다.
- **`findings_for_date`/`findings_for_date_all`은 `scope_kind IN (session,project,host)`만 조인** — R6는 `scope_kind='pattern'`이라 다이어리 브리프에 애초에 포함되지 않는다. pending R6가 다이어리로 새지 않음(추가 필터 불필요).
- **전송 실패 vs 형식 불량** — 전송 실패는 `judge_one`이 `Err`를 반환해 상위가 skip(attempts 미증가), 형식 불량은 `attempts+1` 기록 후 3회 도달 시 pending 잔류·재시도 중단. 인프라 실패를 `rejected`로 오캐시하지 않는다(스펙 §4.2).
- **비범위(스펙 §6):** 판정으로 `new`가 된 카드의 `coach:finding` 실시간 알림은 이번 PR 대상이 아니다(`finding_severities` 스냅샷은 판정 전에 찍히므로 severity 무변화 → diff에 안 잡힘). 카드가 코치 탭에 노출되면 충분하며, 실시간 토스트가 필요해지면 별도 검토한다.
