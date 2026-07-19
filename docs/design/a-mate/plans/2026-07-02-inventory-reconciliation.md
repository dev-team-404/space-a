# 인벤토리 Reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `cmd_inventory` 재실행 시 각 호스트의 `mcp_inventory`를 현재 활성셋으로 원자적 교체(snapshot-replace)해, 설정에서 제거된 MCP 서버·플러그인·프로젝트의 stale 행이 R1을 오탐시키지 않게 한다.

**Architecture:** 기존 `enumerate_hosts → collect_host_inventory → upsert_inventory` 파이프라인에서 마지막 단계를 **호스트별 원자 교체**로 바꾼다. 새 `store::replace_host_inventory`가 트랜잭션으로 `DELETE WHERE host` 후 현재셋을 재삽입한다. `main::read_json_guarded`가 설정 읽기 성공 여부를 판정해, 읽기가 실패한 호스트는 교체하지 않고 스킵(기존 행 유지)한다.

**Tech Stack:** Rust 2021, `rusqlite`(bundled, 트랜잭션), `serde_json`, `tempfile`(dev). **새 crate 의존성 없음.**

## Global Constraints

이 절의 제약은 **모든 태스크의 요구사항에 암묵적으로 포함**된다.

- **`events` 불변**: reconciliation은 `mcp_inventory`(현재 설정 스냅샷)만 건드린다. 사용 이력(`events`)은 절대 삭제하지 않는다.
- **관대한 파싱**: 설정 파일 읽기/파싱 실패는 하드 실패 없이 처리(실패 호스트 스킵 + 로깅). 단 **DB(rusqlite) 오류는 `?`로 전파**(인프라 오류는 전파 — 데이터 파운데이션 철학).
- **원자성**: DELETE + 재INSERT는 한 트랜잭션. 중간 크래시 시 인벤토리가 빈 채로 남지 않는다.
- **에이전트 중립**: 정규화 타입(`McpServer`)만 사용. Tauri v2 대비 결정론적 유지.
- **베이스**: 이 브랜치(`feat/inventory-reconciliation`)는 머지된 `main`(PR #3, 데이터 파운데이션) 위에 있다. `enumerate_hosts`, `collect_host_inventory`, `McpServer`, `SqliteStore::{upsert_inventory, active_servers}` 등이 모두 존재한다.
- **BUILD ENVIRONMENT (비표준 — REQUIRED before EVERY cargo command).** Git Bash에서 아래를 **먼저 export**한 셸에서 cargo 실행(시스템 PATH에 Rust 없음):
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```

## Out of Scope (스펙 §7, 유예 유지 — 구현 금지)

- 옵트인 정확 프로브 / 차등 귀속.
- 설정 변경 히스토리·"뭘 뺐다" 코칭 신호 (generation 마커 방식).
- mark-and-sweep 방식의 행별 컬럼 보존.
- `find_plugin_mcp_files` 다중 버전 디렉터리 "활성 버전" 선택 (현재셋 계산 정확도 문제로 별개).

---

## File Structure

- **Modify** `src/store.rs` — `replace_host_inventory(&mut self, host, entries)` 추가(트랜잭션 교체). 기존 `upsert_inventory`는 유지(재삽입 로직에서 in-line INSERT 사용, 테스트 시딩에서도 사용).
- **Modify** `src/main.rs` — `read_json_guarded(path) -> Option<Value>` 추가; `cmd_inventory`를 reconcile 방식으로 재작성(`&mut SqliteStore`); `main`의 store 바인딩·호출부 `&mut` 조정.

Rust 관례대로 테스트는 각 파일의 인라인 `#[cfg(test)] mod tests`.

---

### Task 1: `replace_host_inventory` (원자 교체) — `src/store.rs`

**Files:**
- Modify: `src/store.rs` (메서드 추가 + 인라인 테스트 2건)
- Test: `src/store.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `crate::inventory::McpServer { name: String, source: String }`, 기존 `SqliteStore { pub conn: Connection }`, `SqliteStore::{upsert_inventory, active_servers, open_in_memory}`.
- Produces: `pub fn replace_host_inventory(&mut self, host: &str, entries: &[(String, Vec<crate::inventory::McpServer>)]) -> anyhow::Result<()>` — 한 호스트의 모든 `mcp_inventory` 행을 삭제하고 `entries`를 재삽입(트랜잭션). `entries`의 `"*"` project_id는 host-global 플러그인 스코프.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/store.rs`의 `mod tests` 안에 추가.

```rust
    #[test]
    fn replace_host_inventory_drops_stale_keeps_other_hosts() {
        use crate::inventory::McpServer;
        let mut store = SqliteStore::open_in_memory().unwrap();
        // 시드: host "H" 에 (P,"A"),(P,"stale"); host "H2" 에 (Q,"keep")
        store.upsert_inventory("H", "P", &[
            McpServer { name: "A".into(), source: "project".into() },
            McpServer { name: "stale".into(), source: "project".into() },
        ]).unwrap();
        store.upsert_inventory("H2", "Q", &[
            McpServer { name: "keep".into(), source: "project".into() },
        ]).unwrap();

        // 현재셋 = (P,[A]) 로 교체 → stale 제거
        store.replace_host_inventory("H", &[
            ("P".to_string(), vec![McpServer { name: "A".into(), source: "project".into() }]),
        ]).unwrap();

        let mut h = store.active_servers("H", "P").unwrap();
        h.sort();
        assert_eq!(h, vec!["A"], "stale 서버는 제거, 현재 서버는 유지");
        assert_eq!(store.active_servers("H2", "Q").unwrap(), vec!["keep"], "다른 호스트 불변");
    }

    #[test]
    fn replace_host_inventory_empty_clears_host() {
        use crate::inventory::McpServer;
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.upsert_inventory("H", "P", &[
            McpServer { name: "A".into(), source: "project".into() },
        ]).unwrap();
        store.replace_host_inventory("H", &[]).unwrap();
        assert!(store.active_servers("H", "P").unwrap().is_empty(), "빈 셋이면 호스트 행 전부 제거");
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test store::tests::replace_host_inventory 2>&1 | head -30`
Expected: 컴파일 에러(`no method named 'replace_host_inventory'`).

- [ ] **Step 3: 최소 구현 작성** — `src/store.rs`의 `upsert_inventory` 메서드 바로 아래에 추가.

```rust
    /// 한 호스트의 인벤토리를 현재 셋으로 원자 교체(트랜잭션: DELETE 후 재INSERT).
    /// entries = (project_id, servers) 목록. "*" 는 host-global 플러그인 스코프.
    /// events(사용 이력)는 건드리지 않는다.
    pub fn replace_host_inventory(
        &mut self,
        host: &str,
        entries: &[(String, Vec<crate::inventory::McpServer>)],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM mcp_inventory WHERE host = ?1", params![host])?;
        for (project, servers) in entries {
            for s in servers {
                tx.execute(
                    "INSERT INTO mcp_inventory (host, project_id, server, source)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(host, project_id, server) DO UPDATE SET source = ?4",
                    params![host, project, s.name, s.source],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test store::tests::replace_host_inventory`
Expected: PASS (2 tests).

- [ ] **Step 5: 전체 스위트 확인**

Run: `cargo test`
Expected: 전 테스트 PASS(기존 + 신규 2). 무경고.

- [ ] **Step 6: 커밋**

```bash
git add src/store.rs
git commit -m "feat: replace_host_inventory 원자 교체(reconciliation 지반)"
```

---

### Task 2: `cmd_inventory` reconcile 배선 + 읽기 가드 — `src/main.rs`

**Files:**
- Modify: `src/main.rs` (`read_json_guarded` 추가, `cmd_inventory` 재작성 → `&mut`, `main` 호출부 조정, 인라인 테스트)
- Test: `src/main.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `store::replace_host_inventory(&mut self, host, entries)`(Task 1), `hosts::enumerate_hosts`, `HostSource::{claude_json, settings_json, claude_root, host}`, `inventory::collect_host_inventory(&Value, &Value, &Path) -> Vec<(String, Vec<McpServer>)>`(모두 기존).
- Produces: `fn read_json_guarded(path: &std::path::Path) -> Option<serde_json::Value>` — 읽기+파싱 성공 → `Some(Value)`; 파일 부재(NotFound) → `Some(Value::Null)`(정당한 빈 설정); 존재하나 IO/파싱 실패 → `None`(스킵 신호). `cmd_inventory(store: &mut SqliteStore)`.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/main.rs` **맨 끝**에 새 테스트 모듈 추가(현재 main.rs엔 테스트 모듈 없음).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_json_guarded_valid_absent_broken() {
        let dir = tempfile::tempdir().unwrap();

        // 유효 JSON → Some(Value)
        let valid = dir.path().join("valid.json");
        std::fs::write(&valid, r#"{"a":1}"#).unwrap();
        assert_eq!(read_json_guarded(&valid), Some(serde_json::json!({"a":1})));

        // 파일 부재 → Some(Null) (정당한 빈 설정)
        let absent = dir.path().join("nope.json");
        assert_eq!(read_json_guarded(&absent), Some(serde_json::Value::Null));

        // 존재하나 깨진 JSON → None (스킵)
        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "{not json").unwrap();
        assert_eq!(read_json_guarded(&broken), None);
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test --bin agent-mentor read_json_guarded 2>&1 | head -30`
Expected: 컴파일 에러(`cannot find function read_json_guarded`).

- [ ] **Step 3: `read_json_guarded` 추가** — `src/main.rs`의 `fn cmd_inventory` **바로 위**에 추가.

```rust
/// 설정 파일을 읽어 reconcile 안전성을 판정.
/// 읽기+파싱 성공 → Some(Value); 파일 부재(NotFound) → Some(Null)(정당한 빈 설정);
/// 존재하나 IO/파싱 실패 → None(그 호스트 reconcile 스킵, 기존 행 유지).
fn read_json_guarded(path: &std::path::Path) -> Option<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(serde_json::Value::Null),
        Err(_) => None,
    }
}
```

- [ ] **Step 4: `cmd_inventory` 재작성** — 기존 함수 전체를 아래로 교체.

기존(제거 대상):
```rust
fn cmd_inventory(store: &SqliteStore) -> Result<()> {
    let mut total_servers = 0usize;
    for hs in enumerate_hosts() {
        let claude_json = std::fs::read_to_string(hs.claude_json())
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .unwrap_or(serde_json::Value::Null);
        let settings = std::fs::read_to_string(hs.settings_json())
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .unwrap_or(serde_json::Value::Null);
        let cache = hs.claude_root.join("plugins").join("cache");

        for (project, servers) in collect_host_inventory(&claude_json, &settings, &cache) {
            total_servers += servers.len();
            store.upsert_inventory(&hs.host, &project, &servers)?;
        }
    }
    println!("inventory: {total_servers} servers across all hosts");
    Ok(())
}
```

신규:
```rust
fn cmd_inventory(store: &mut SqliteStore) -> Result<()> {
    let mut total_servers = 0usize;
    for hs in enumerate_hosts() {
        // 읽기 가드: claude.json·settings.json 중 하나라도 읽기 실패(존재하나 IO/파싱)면
        // 그 호스트는 reconcile 스킵(기존 인벤토리 유지) — 일시 실패로 인한 오삭제 방지.
        let Some(claude_json) = read_json_guarded(&hs.claude_json()) else {
            eprintln!("warn: host {} claude.json 읽기 실패 — 인벤토리 유지, reconcile 스킵", hs.host);
            continue;
        };
        let Some(settings) = read_json_guarded(&hs.settings_json()) else {
            eprintln!("warn: host {} settings.json 읽기 실패 — 인벤토리 유지, reconcile 스킵", hs.host);
            continue;
        };
        let cache = hs.claude_root.join("plugins").join("cache");
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        total_servers += inv.iter().map(|(_, servers)| servers.len()).sum::<usize>();
        // 원자 교체: 이 호스트의 기존 행 삭제 후 현재셋 삽입 → stale 제거.
        store.replace_host_inventory(&hs.host, &inv)?;
    }
    println!("inventory: {total_servers} servers across all hosts (reconciled)");
    Ok(())
}
```

- [ ] **Step 5: `main` 호출부 `&mut` 조정** — store 바인딩과 `cmd_inventory` 호출을 수정.

`src/main.rs`의 `fn main` 안에서:
- `let store = db()?;` → `let mut store = db()?;`
- `"inventory" => cmd_inventory(&store)?,` → `"inventory" => cmd_inventory(&mut store)?,`
- `"all"` arm의 `cmd_inventory(&store)?;` → `cmd_inventory(&mut store)?;`

(다른 `cmd_ingest(&store)`, `cmd_rules(&store)`, `cmd_diary(&store, ...)`는 `&SqliteStore` 그대로 — 순차 실행이라 borrow 충돌 없음.)

- [ ] **Step 6: 테스트 통과 + 빌드 확인**

Run: `cargo test --bin agent-mentor read_json_guarded && cargo test && cargo build`
Expected: read_json_guarded 테스트 PASS, 전 스위트 PASS, 빌드 무경고.

- [ ] **Step 7: 수동 스모크(실데이터, 회귀 없음 확인)**

Run:
```bash
rm -f agent-mentor.db
cargo run --quiet -- ingest
cargo run --quiet -- inventory
cargo run --quiet -- inventory   # 두 번째 실행: 서버 수 안정(누적 아님)
cargo run --quiet -- rules 2>&1 | tail -8
```
Expected: 두 `inventory` 실행 모두 `inventory: N servers across all hosts (reconciled)`로 **동일 N**(예: 12; 누적/중복 없음). `rules`는 여전히 글로벌 플러그인(context7·playwright·vercel 등)을 `scope=host`로 지목, 패닉 없음. (stale 제거 시맨틱 자체는 Task 1 단위 테스트가 증명 — 실 config를 편집하지 않는다.)

- [ ] **Step 8: 커밋**

```bash
git add src/main.rs
git commit -m "feat: cmd_inventory 호스트별 원자 reconcile + 읽기 성공 가드"
```

---

## Self-Review

**1. Spec coverage** (`docs/specs/2026-07-02-inventory-reconciliation-design.md`):
- §2.1 snapshot-replace(Option B) → Task 1 `replace_host_inventory`(DELETE WHERE host + 재INSERT) ✅
- §2.2 원자적 교체(트랜잭션) → Task 1 `self.conn.transaction()` + `commit` ✅
- §2.3 읽기 성공 가드 → Task 2 `read_json_guarded`(Ok/Null/None) + `cmd_inventory`의 `else { continue }` ✅
- §2.4 events 불변 → 두 태스크 모두 `mcp_inventory`만 조작(events 미접근) ✅
- §3.1 `replace_host_inventory` / §3.2 `read_json_guarded` + `&mut` 배선 → Task 1/2 ✅
- §6 테스트(drops_stale/empty_clears/read_json_guarded valid·absent·broken) → 전부 태스크에 포함 ✅
- §7 Non-goals → "Out of Scope"에 명시, 태스크 없음 ✅

**2. Placeholder scan:** 모든 스텝에 실제 코드/명령/기대출력. TBD/TODO 없음 ✅

**3. Type consistency:**
- `replace_host_inventory(&mut self, host: &str, entries: &[(String, Vec<McpServer>)]) -> Result<()>` — Task 1 정의, Task 2 `store.replace_host_inventory(&hs.host, &inv)` 호출 시 `inv: Vec<(String, Vec<McpServer>)>`(collect_host_inventory 반환형) 일치 ✅
- `read_json_guarded(&Path) -> Option<Value>` — Task 2 정의/사용(`let Some(x) = ... else`) 일치 ✅
- `cmd_inventory(&mut SqliteStore)` — Task 2 정의, `main` 호출부 `&mut store` 일치(Step 5) ✅
- `active_servers(host, project_id) -> Result<Vec<String>>` — 기존 API, Task 1 테스트에서 사용 ✅

**스펙 대비 의도적 정제(무해):** 읽기 가드를 스펙의 `Result<Value,()>` 대신 `Option<Value>`(None=스킵)로 — 의미 동일, 유닛 에러 타입 회피(더 관용적). 재삽입은 DELETE 후 충돌이 없으므로 `ON CONFLICT` 절은 방어적(entries 내 중복 방지) — collect_host_inventory는 프로젝트 키가 유일하고 서버는 push_unique로 dedup되어 실제 충돌은 없음.

---

## Execution Handoff

플랜 저장: `docs/plans/2026-07-02-inventory-reconciliation.md`. 실행은 `superpowers:subagent-driven-development`(권장)로 태스크별 fresh 서브에이전트 + 2단계 리뷰.
