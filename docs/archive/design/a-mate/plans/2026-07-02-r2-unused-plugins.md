---
status: done
archived: 2026-07-19
---

# R2 미사용 플러그인(스킬 제공) 규칙 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** enabled이지만 그 스킬·MCP를 한 번도 쓰지 않은 스킬-제공 플러그인을 플러그인별로 지목(R2)하고 `disable_plugin` 처방을 제시한다.

**Architecture:** ① 어댑터가 `Skill` 툴콜의 스킬명을 이벤트에 캡처(신규 `ToolKind::Skill`). ② enabled 플러그인의 활성버전 dir을 스캔해 skills/*/SKILL.md(상주토큰 추정)와 .mcp.json(서버명)을 per-plugin `plugin_inventory` 테이블에 원자 교체(completeness 전파). ③ R2 규칙이 그 테이블을 읽어 "스킬 사용 OR MCP 사용"이 전무한 플러그인을 host 단위로 지목. R1(MCP 서버 단위)과 비충돌.

**Tech Stack:** Rust 2021, rusqlite(bundled), serde_json, anyhow. 테스트: `#[cfg(test)]` + tempfile + filetime(이미 dev-dep).

## Global Constraints

- **제품/스택:** Tauri v2 지향이나 현재는 순수 백엔드 크레이트 단계. 공개 함수는 `#[tauri::command]` 배선 가능하게 유지. Tauri v1 API 금지.
- **플랫폼:** Windows 전용. macOS/Linux 분기 불필요.
- **에이전트 추상화:** 에이전트별 로직 하드코딩 금지 — R2는 정규화 events + plugin_inventory만 소비.
- **빌드/테스트 환경:** `cargo`는 Git Bash에서 GNU 툴체인 + mingw PATH export 후 실행:
  ```
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```
- **품질 기준:** 현재 `cargo test` 75개 통과·무경고. 신규 코드도 경고 0 유지(미사용 변수/import 금지).
- **에러 철학(데이터 파운데이션):** 파싱/읽기 실패는 관대 처리(하드 실패 금지) — completeness 신호(`complete=false`)로 전파. NotFound(부재)은 정당(complete=true). DB/트랜잭션 오류만 `?`로 전파.
- **스코프:** R2는 host-global(`scope_kind="host"`, `scope_ref=host`). 사용자/프로젝트 로컬 스킬은 범위 밖.
- **네임스페이스(v0):** 스킬 호출 `tool_target`="`<ns>:<skill>`", ns = plugin_key(`name@marketplace`)의 `@` 앞 = plugin name. name↔ns 불일치 가능성은 최종 실엔진 e2e로 검증(스펙 §8).

**참조 스펙:** `docs/specs/2026-07-02-r2-unused-plugin-skills-design.md`

---

## Task 1: 어댑터 — Skill 툴콜 캡처

**Files:**
- Modify: `src/model.rs` (`ToolKind`에 `Skill` 배리언트)
- Modify: `src/adapter.rs` (tool_use 파싱에서 `Skill` 특수 처리 + 테스트)
- Modify: `src/store.rs` (`tool_kind_str`에 `Skill` arm)

**Interfaces:**
- Produces: `ToolKind::Skill { name: String }`. 어댑터는 `raw_name=="Skill"`인 tool_use를 `ToolKind::Skill{name: input.skill}`로, `target=Some(name)`로 매핑. `tool_kind_str(Skill) => "skill"` → events 테이블 `tool_kind='skill'`, `tool_target='<ns>:<skill>'`.

- [ ] **Step 1: 모델에 Skill 배리언트 추가**

`src/model.rs`의 `ToolKind` enum에 `Other` 직전에 한 줄 추가:

```rust
pub enum ToolKind {
    FileRead,
    FileEdit,
    FileWrite,
    Search,
    Execute,
    McpCall { server: String, tool: String },
    WebSearch,
    WebFetch,
    SubAgent,
    Skill { name: String },
    Other(String),
}
```

- [ ] **Step 2: store tool_kind_str에 arm 추가**

`src/store.rs`의 `tool_kind_str` match에서 `ToolKind::Other(_)` 직전에 추가:

```rust
        ToolKind::SubAgent => "sub_agent",
        ToolKind::Skill { .. } => "skill",
        ToolKind::Other(_) => "other",
```

- [ ] **Step 3: 실패하는 어댑터 테스트 추가**

`src/adapter.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn map_skill_tool_use_captures_skill_name() {
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","parentUuid":null,
            "isSidechain":false,"timestamp":"2026-07-01T10:00:00Z","cwd":"C:\\Users\\jibin",
            "gitBranch":"main","message":{"model":"claude-opus-4-8",
            "usage":{"input_tokens":1,"output_tokens":1},
            "content":[{"type":"tool_use","name":"Skill","input":{"skill":"superpowers:brainstorming"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        // AssistantTurn + ToolCall(Skill)
        let tool = evs.iter().find(|e| matches!(e.kind, EventKind::ToolCall { .. })).unwrap();
        match &tool.kind {
            EventKind::ToolCall { kind, target, .. } => {
                assert_eq!(*kind, ToolKind::Skill { name: "superpowers:brainstorming".into() });
                assert_eq!(target.as_deref(), Some("superpowers:brainstorming"));
            }
            k => panic!("expected ToolCall, got {k:?}"),
        }
    }
```

- [ ] **Step 4: 테스트 실행(실패 확인)**

Run: `cargo test map_skill_tool_use_captures_skill_name`
Expected: FAIL — 현재 `Skill`은 `ToolKind::Other("Skill")`로 매핑되고 `target`은 None(input.skill 미추출).

- [ ] **Step 5: 어댑터 파싱에 Skill 특수 처리**

`src/adapter.rs`의 tool_use 블록 처리(현재 `let target = input.get("file_path")...` ~ `out.push(mk(...))` 부분)를 아래로 교체:

```rust
                    if block.get("type").and_then(|x| x.as_str()) == Some("tool_use") {
                        let raw_name =
                            block.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").cloned().unwrap_or(Value::Null);
                        let (kind, target) = if raw_name == "Skill" {
                            let sname = input
                                .get("skill")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
                            (ToolKind::Skill { name: sname.clone() }, Some(sname))
                        } else {
                            let t = input
                                .get("file_path")
                                .or_else(|| input.get("command"))
                                .and_then(|x| x.as_str())
                                .map(String::from);
                            (ToolKind::from_raw_name(&raw_name), t)
                        };
                        out.push(mk(
                            EventKind::ToolCall { kind, raw_name, target },
                            (i + 1) as u64,
                        ));
                    }
```

- [ ] **Step 6: 테스트 실행(통과 확인) + 전체**

Run: `cargo test map_skill_tool_use_captures_skill_name` → PASS.
Run: `cargo test` → 전체 통과(기존 75 + 신규 1 = 76), 무경고.

- [ ] **Step 7: 커밋**

```bash
git add src/model.rs src/adapter.rs src/store.rs
git commit -m "feat: 어댑터가 Skill 툴콜의 스킬명 캡처(ToolKind::Skill, tool_kind=skill)"
```

---

## Task 2: plugin_inventory 테이블 + PluginRecord + replace_plugin_inventory

**Files:**
- Modify: `src/inventory.rs` (`PluginRecord` struct)
- Modify: `src/store.rs` (SCHEMA + `replace_plugin_inventory` + 테스트)

**Interfaces:**
- Produces: `pub struct PluginRecord { pub plugin_key: String, pub namespace: String, pub skill_count: u64, pub resident_tokens: u64, pub skills: Vec<String>, pub mcp_servers: Vec<String> }`. `SqliteStore::replace_plugin_inventory(&mut self, host: &str, records: &[PluginRecord]) -> Result<()>` — host 단위 트랜잭션 원자 교체. 테이블 `plugin_inventory(host, plugin_key, namespace, skill_count, resident_tokens, skills_json, mcp_servers_json)`, PK `(host, plugin_key)`.

- [ ] **Step 1: PluginRecord struct 추가**

`src/inventory.rs`에서 `McpServer` 정의 근처에 추가:

```rust
/// enabled 플러그인 하나의 스캔 결과(스킬·MCP 서버). R2 대상은 skill_count>=1.
#[derive(Debug, Clone)]
pub struct PluginRecord {
    pub plugin_key: String,   // "name@marketplace"
    pub namespace: String,    // 스킬 호출 네임스페이스(v0: plugin name)
    pub skill_count: u64,
    pub resident_tokens: u64,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
}
```

- [ ] **Step 2: SCHEMA에 plugin_inventory 추가**

`src/store.rs`의 `SCHEMA` 상수에서 `mcp_inventory` 블록 뒤에 추가:

```sql
CREATE TABLE IF NOT EXISTS plugin_inventory (
  host TEXT NOT NULL, plugin_key TEXT NOT NULL, namespace TEXT NOT NULL,
  skill_count INTEGER DEFAULT 0, resident_tokens INTEGER DEFAULT 0,
  skills_json TEXT NOT NULL, mcp_servers_json TEXT NOT NULL,
  PRIMARY KEY (host, plugin_key)
);
```

- [ ] **Step 3: 실패하는 테스트 추가**

`src/store.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn replace_plugin_inventory_atomic_swap() {
        use crate::inventory::PluginRecord;
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            PluginRecord {
                plugin_key: "superpowers@mp".into(), namespace: "superpowers".into(),
                skill_count: 3, resident_tokens: 900,
                skills: vec!["brainstorming".into(), "writing-plans".into(), "tdd".into()],
                mcp_servers: vec![],
            },
        ]).unwrap();
        // 재교체: 다른 셋 → stale 제거 확인
        store.replace_plugin_inventory("Windows", &[
            PluginRecord {
                plugin_key: "vercel@mp".into(), namespace: "vercel".into(),
                skill_count: 1, resident_tokens: 400,
                skills: vec!["deploy".into()],
                mcp_servers: vec!["vercel".into()],
            },
        ]).unwrap();

        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM plugin_inventory WHERE host='Windows'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "원자 교체 — superpowers 행은 제거됨");
        let (ns, skills_json, mcp_json): (String, String, String) = store.conn.query_row(
            "SELECT namespace, skills_json, mcp_servers_json FROM plugin_inventory WHERE plugin_key='vercel@mp'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(ns, "vercel");
        assert_eq!(serde_json::from_str::<Vec<String>>(&skills_json).unwrap(), vec!["deploy"]);
        assert_eq!(serde_json::from_str::<Vec<String>>(&mcp_json).unwrap(), vec!["vercel"]);
    }
```

- [ ] **Step 4: 테스트 실행(실패 확인)**

Run: `cargo test replace_plugin_inventory_atomic_swap`
Expected: FAIL(컴파일) — `replace_plugin_inventory` 미정의.

- [ ] **Step 5: replace_plugin_inventory 구현**

`src/store.rs`의 `impl SqliteStore` 안, `replace_host_inventory` 뒤에 추가:

```rust
    /// 한 호스트의 플러그인 인벤토리를 현재 셋으로 원자 교체(트랜잭션 DELETE 후 재INSERT).
    /// events(사용 이력)는 건드리지 않는다.
    pub fn replace_plugin_inventory(
        &mut self,
        host: &str,
        records: &[crate::inventory::PluginRecord],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM plugin_inventory WHERE host = ?1", params![host])?;
        for r in records {
            let skills_json = serde_json::to_string(&r.skills)?;
            let mcp_json = serde_json::to_string(&r.mcp_servers)?;
            tx.execute(
                "INSERT INTO plugin_inventory
                 (host, plugin_key, namespace, skill_count, resident_tokens, skills_json, mcp_servers_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(host, plugin_key) DO UPDATE SET
                   namespace=?3, skill_count=?4, resident_tokens=?5, skills_json=?6, mcp_servers_json=?7",
                params![host, r.plugin_key, r.namespace, r.skill_count as i64,
                        r.resident_tokens as i64, skills_json, mcp_json],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
```

- [ ] **Step 6: 테스트 실행(통과 확인)**

Run: `cargo test replace_plugin_inventory_atomic_swap` → PASS.
Run: `cargo test` → 전체 통과, 무경고.

- [ ] **Step 7: 커밋**

```bash
git add src/inventory.rs src/store.rs
git commit -m "feat: plugin_inventory 테이블 + PluginRecord + replace_plugin_inventory(원자 교체)"
```

---

## Task 3: 플러그인 인벤토리 스캔 (inventory.rs)

**Files:**
- Modify: `src/inventory.rs` (`parse_skill_frontmatter`, `scan_skills_dir`, `active_version_dir`, `scan_plugin_inventory` + 테스트)

**Interfaces:**
- Consumes: 기존 `pick_active_version(Vec<(PathBuf, SystemTime)>) -> Option<PathBuf>`, `parse_mcp_json(&Value) -> Vec<String>`, Task 2의 `PluginRecord`.
- Produces: `pub fn scan_plugin_inventory(settings: &Value, plugins_cache_dir: &Path) -> (Vec<PluginRecord>, bool)`. 스킬 0개 플러그인은 제외. complete=false는 스캔 IO/파싱 실패 시(부재는 complete).

- [ ] **Step 1: frontmatter 파서 + 스킬 dir 스캔 + 활성버전 dir 헬퍼 추가**

`src/inventory.rs`의 `pick_active_version` 근처에 추가:

```rust
/// SKILL.md frontmatter에서 (name, description). v0: 단일 라인, 따옴표 제거.
/// 프론트매터(--- ... ---)가 없거나 name이 없으면 None.
fn parse_skill_frontmatter(text: &str) -> Option<(String, String)> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut name: Option<String> = None;
    let mut description = String::new();
    for line in lines {
        let t = line.trim();
        if t == "---" {
            break;
        }
        if let Some(v) = t.strip_prefix("name:") {
            name = Some(unquote(v));
        } else if let Some(v) = t.strip_prefix("description:") {
            description = unquote(v);
        }
    }
    name.map(|n| (n, description))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let stripped = s
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')));
    stripped.unwrap_or(s).to_string()
}

/// <version>/skills/*/SKILL.md → (스킬 로컬명 정렬 목록, 상주토큰 합, complete).
/// skills/ 부재(NotFound)는 (빈, 0, true). read_dir IO 에러/파일 read·파싱 실패는 complete=false.
/// 상주토큰 ≈ (name+description) chars / 4 (heuristic).
fn scan_skills_dir(skills_dir: &Path) -> (Vec<String>, u64, bool) {
    let entries = match std::fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), 0, true),
        Err(_) => return (Vec::new(), 0, false),
    };
    let mut names = Vec::new();
    let mut resident = 0u64;
    let mut complete = true;
    for v in entries.flatten() {
        let md = v.path().join("SKILL.md");
        if !md.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&md) else {
            complete = false;
            continue;
        };
        match parse_skill_frontmatter(&raw) {
            Some((name, desc)) => {
                resident += ((name.chars().count() + desc.chars().count()) / 4) as u64;
                names.push(name);
            }
            None => complete = false,
        }
    }
    names.sort();
    (names, resident, complete)
}

/// 플러그인 dir의 활성 버전 dir(mtime 최신). (dir, complete).
/// 부재(NotFound)/버전 없음은 (None, true). read_dir IO 에러는 (None, false).
fn active_version_dir(plugin_dir: &Path) -> (Option<PathBuf>, bool) {
    let entries = match std::fs::read_dir(plugin_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (None, true),
        Err(_) => return (None, false),
    };
    let mut with_mtime: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    let mut all: Vec<PathBuf> = Vec::new();
    for v in entries.flatten() {
        let p = v.path();
        if !p.is_dir() {
            continue;
        }
        all.push(p.clone());
        if let Ok(mtime) = v.metadata().and_then(|m| m.modified()) {
            with_mtime.push((p, mtime));
        }
    }
    if all.is_empty() {
        return (None, true);
    }
    match pick_active_version(with_mtime) {
        Some(active) => (Some(active), true),
        None => (all.into_iter().next(), true), // mtime 전부 실패 폴백
    }
}
```

- [ ] **Step 2: 실패하는 테스트 추가**

`src/inventory.rs`의 `mod tests`에 추가:

```rust
    #[test]
    fn scan_plugin_inventory_reads_skills_and_mcp() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        // superpowers: 스킬 2개, MCP 없음
        let sp = cache.join("mp").join("superpowers").join("1.0.0").join("skills");
        std::fs::create_dir_all(sp.join("brainstorming")).unwrap();
        std::fs::write(sp.join("brainstorming").join("SKILL.md"),
            "---\nname: brainstorming\ndescription: \"explore ideas\"\n---\nbody").unwrap();
        std::fs::create_dir_all(sp.join("tdd")).unwrap();
        std::fs::write(sp.join("tdd").join("SKILL.md"),
            "---\nname: tdd\ndescription: test first\n---\nbody").unwrap();
        // vercel: 스킬 1개 + MCP 서버
        let vc = cache.join("mp").join("vercel").join("2.0.0");
        std::fs::create_dir_all(vc.join("skills").join("deploy")).unwrap();
        std::fs::write(vc.join("skills").join("deploy").join("SKILL.md"),
            "---\nname: deploy\ndescription: ship it\n---\nbody").unwrap();
        std::fs::write(vc.join(".mcp.json"), r#"{"vercel":{"command":"x"}}"#).unwrap();
        // mcponly: MCP만, 스킬 없음 → 제외
        let mo = cache.join("mp").join("mcponly").join("1.0.0");
        std::fs::create_dir_all(&mo).unwrap();
        std::fs::write(mo.join(".mcp.json"), r#"{"srv":{"command":"x"}}"#).unwrap();

        let settings = serde_json::json!({ "enabledPlugins": {
            "superpowers@mp": true, "vercel@mp": true, "mcponly@mp": true, "off@mp": false
        }});
        let (records, complete) = scan_plugin_inventory(&settings, cache);
        assert!(complete);
        let mut keys: Vec<_> = records.iter().map(|r| r.plugin_key.as_str()).collect();
        keys.sort();
        assert_eq!(keys, vec!["superpowers@mp", "vercel@mp"], "MCP전용·off 플러그인 제외");

        let sp_rec = records.iter().find(|r| r.plugin_key == "superpowers@mp").unwrap();
        assert_eq!(sp_rec.namespace, "superpowers");
        assert_eq!(sp_rec.skill_count, 2);
        assert_eq!(sp_rec.skills, vec!["brainstorming", "tdd"]);
        assert!(sp_rec.mcp_servers.is_empty());
        assert!(sp_rec.resident_tokens > 0);

        let vc_rec = records.iter().find(|r| r.plugin_key == "vercel@mp").unwrap();
        assert_eq!(vc_rec.mcp_servers, vec!["vercel"]);
    }

    #[test]
    fn scan_plugin_inventory_incomplete_on_corrupt_skill() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let sp = cache.join("mp").join("p").join("1.0.0").join("skills").join("s");
        std::fs::create_dir_all(&sp).unwrap();
        std::fs::write(sp.join("SKILL.md"), "no frontmatter here").unwrap();
        let settings = serde_json::json!({ "enabledPlugins": { "p@mp": true }});
        let (_records, complete) = scan_plugin_inventory(&settings, cache);
        assert!(!complete, "frontmatter 없는 SKILL.md → incomplete");
    }

    #[test]
    fn scan_plugin_inventory_picks_active_version() {
        use filetime::{set_file_mtime, FileTime};
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        let base = cache.join("mp").join("p");
        // old: 스킬 old_skill
        let old = base.join("0.1.0");
        std::fs::create_dir_all(old.join("skills").join("old_skill")).unwrap();
        std::fs::write(old.join("skills").join("old_skill").join("SKILL.md"),
            "---\nname: old_skill\ndescription: x\n---\n").unwrap();
        // new: 스킬 new_skill
        let new = base.join("0.2.0");
        std::fs::create_dir_all(new.join("skills").join("new_skill")).unwrap();
        std::fs::write(new.join("skills").join("new_skill").join("SKILL.md"),
            "---\nname: new_skill\ndescription: x\n---\n").unwrap();
        set_file_mtime(&old, FileTime::from_unix_time(1000, 0)).unwrap();
        set_file_mtime(&new, FileTime::from_unix_time(2000, 0)).unwrap();

        let settings = serde_json::json!({ "enabledPlugins": { "p@mp": true }});
        let (records, complete) = scan_plugin_inventory(&settings, cache);
        assert!(complete);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].skills, vec!["new_skill"], "활성(최신 mtime) 버전만");
    }
```

- [ ] **Step 3: 테스트 실행(실패 확인)**

Run: `cargo test scan_plugin_inventory`
Expected: FAIL(컴파일) — `scan_plugin_inventory` 미정의.

- [ ] **Step 4: scan_plugin_inventory 구현**

`src/inventory.rs`에 추가(위 헬퍼들 뒤):

```rust
/// enabled 플러그인(스킬 제공)의 per-plugin 인벤토리 + completeness.
/// 각 플러그인: 활성버전 dir의 skills/*/SKILL.md(상주토큰) + .mcp.json(서버명) 스캔.
/// 스킬 0개(MCP 전용)는 제외 — R2 대상 아님(R1 담당).
pub fn scan_plugin_inventory(
    settings: &Value,
    plugins_cache_dir: &Path,
) -> (Vec<PluginRecord>, bool) {
    let mut out: Vec<PluginRecord> = Vec::new();
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
        let (active, dir_ok) = active_version_dir(&plugin_dir);
        if !dir_ok {
            complete = false;
        }
        let Some(active) = active else { continue };

        let (skills, resident, skills_ok) = scan_skills_dir(&active.join("skills"));
        if !skills_ok {
            complete = false;
        }
        if skills.is_empty() {
            continue; // MCP 전용 → R2 대상 아님
        }

        let mut mcp_servers = Vec::new();
        let mcp_path = active.join(".mcp.json");
        if mcp_path.is_file() {
            match std::fs::read_to_string(&mcp_path) {
                Ok(raw) => match serde_json::from_str::<Value>(&raw) {
                    Ok(json) => mcp_servers = parse_mcp_json(&json),
                    Err(_) => complete = false,
                },
                Err(_) => complete = false,
            }
        }

        out.push(PluginRecord {
            plugin_key: id.clone(),
            namespace: name.to_string(),
            skill_count: skills.len() as u64,
            resident_tokens: resident,
            skills,
            mcp_servers,
        });
    }
    (out, complete)
}
```

- [ ] **Step 5: 테스트 실행(통과 확인)**

Run: `cargo test scan_plugin_inventory` → 3 tests PASS.
Run: `cargo test` → 전체 통과, 무경고.

- [ ] **Step 6: 커밋**

```bash
git add src/inventory.rs
git commit -m "feat: scan_plugin_inventory — 활성버전 skills/.mcp.json 스캔(상주토큰 추정, MCP전용 제외)"
```

---

## Task 4: R2 규칙

**Files:**
- Create: `src/rules/r2_unused_plugins.rs`
- Modify: `src/rules/mod.rs` (모듈 등록)

**Interfaces:**
- Consumes: `crate::rules::Rule`, `crate::finding::{Finding, Prescription, Severity}`, `plugin_inventory` 테이블, `events`(tool_kind='skill'/'mcp_call').
- Produces: `pub struct R2UnusedPluginSkills { pub min_resident_tokens: u64 }` + `Default`(300) + `Rule`. Finding: `rule_id="R2"`, `scope_kind="host"`, `prescription=Some(disable_plugin, {"plugin":plugin_key})`, `dedup_key="R2|{host}|{plugin_key}"`, evidence `{plugin, skill_count, skills, resident_tokens, note}`.

- [ ] **Step 1: 모듈 등록**

`src/rules/mod.rs`의 `pub mod` 목록에 알파벳 순으로 추가:

```rust
pub mod r1_unused_mcp;
pub mod r2_unused_plugins;
pub mod r5_repeated_read;
pub mod r7_opus_trivial;
pub mod r9_web_overuse;
```

- [ ] **Step 2: 규칙 + 테스트 작성**

`src/rules/r2_unused_plugins.rs`를 생성하고 아래 전체를 쓴다:

```rust
use crate::finding::{Finding, Prescription, Severity};
use crate::rules::Rule;
use crate::store::SqliteStore;
use anyhow::Result;
use rusqlite::params;

/// R2 — 미사용 플러그인(스킬 제공).
///
/// enabled + 스킬≥1 제공 플러그인 중, 그 플러그인의 스킬·MCP를 하나도 안 쓴 것을 host 단위로 지목.
/// - "사용됨" = 그 플러그인 네임스페이스의 스킬 호출 ≥1 OR 그 플러그인 MCP 서버 호출 ≥1.
/// - R1 비충돌: MCP 서버가 쓰이면 침묵("쓰는 플러그인 끄기" 방지). MCP 전용 미사용은 R1(서버 단위)이 담당.
/// - 스코프 host-global(enabledPlugins·스킬은 호스트 전 세션 상주). 상주토큰은 chars/4 heuristic.
pub struct R2UnusedPluginSkills {
    pub min_resident_tokens: u64,
}

impl Default for R2UnusedPluginSkills {
    fn default() -> Self {
        R2UnusedPluginSkills { min_resident_tokens: 300 }
    }
}

impl Rule for R2UnusedPluginSkills {
    fn id(&self) -> &'static str {
        "R2"
    }

    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        let mut stmt = store.conn.prepare(
            "SELECT host, plugin_key, namespace, skill_count, resident_tokens, skills_json, mcp_servers_json
             FROM plugin_inventory
             ORDER BY resident_tokens DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,      // host
                    r.get::<_, String>(1)?,      // plugin_key
                    r.get::<_, String>(2)?,      // namespace
                    r.get::<_, i64>(3)? as u64,  // skill_count
                    r.get::<_, i64>(4)? as u64,  // resident_tokens
                    r.get::<_, String>(5)?,      // skills_json
                    r.get::<_, String>(6)?,      // mcp_servers_json
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut out = Vec::new();
        for (host, plugin_key, namespace, skill_count, resident, skills_json, mcp_json) in rows {
            // 1) 스킬 사용?
            let skill_used: i64 = store.conn.query_row(
                "SELECT COUNT(*) FROM events
                 WHERE host=?1 AND kind='tool_call' AND tool_kind='skill' AND tool_target LIKE ?2",
                params![host, format!("{namespace}:%")],
                |r| r.get(0),
            )?;
            if skill_used > 0 {
                continue;
            }
            // 2) MCP 사용? (R1 비충돌)
            let servers: Vec<String> = serde_json::from_str(&mcp_json).unwrap_or_default();
            let mut mcp_used = false;
            for srv in &servers {
                let n: i64 = store.conn.query_row(
                    "SELECT COUNT(*) FROM events
                     WHERE host=?1 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?2",
                    params![host, srv],
                    |r| r.get(0),
                )?;
                if n > 0 {
                    mcp_used = true;
                    break;
                }
            }
            if mcp_used {
                continue;
            }
            // 3) 임계값
            if resident < self.min_resident_tokens {
                continue;
            }

            let skills: Vec<String> = serde_json::from_str(&skills_json).unwrap_or_default();
            out.push(Finding {
                rule_id: "R2".into(),
                severity: Severity::Suggest,
                scope_host: Some(host.clone()),
                scope_project: None,
                scope_kind: "host".into(),
                scope_ref: host.clone(),
                evidence: serde_json::json!({
                    "plugin": plugin_key,
                    "skill_count": skill_count,
                    "skills": skills,
                    "resident_tokens": resident,
                    "note": "약(~) 추정 — chars/4 heuristic"
                }),
                est_tokens_saved: resident,
                prescription: Some(Prescription {
                    kind: "disable_plugin".into(),
                    payload: serde_json::json!({ "plugin": plugin_key }),
                }),
                dedup_key: format!("R2|{host}|{plugin_key}"),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::PluginRecord;
    use crate::model::*;
    use crate::store::SqliteStore;

    fn skill_call(host: &str, uuid: &str, skill: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::Skill { name: skill.into() },
                raw_name: "Skill".into(), target: Some(skill.into()),
            },
        }
    }

    fn mcp_call(host: &str, uuid: &str, server: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: host.into(), project_id: "p".into(), session_id: "s1".into(),
            uuid: Some(uuid.into()), parent_uuid: None, is_sidechain: false,
            ts: Some("2026-07-01T10:00:00Z".into()), source_file: "s.jsonl".into(),
            source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::McpCall { server: server.into(), tool: "x".into() },
                raw_name: format!("mcp__{server}__x"), target: None,
            },
        }
    }

    fn rec(plugin_key: &str, namespace: &str, resident: u64, skills: &[&str], mcp: &[&str]) -> PluginRecord {
        PluginRecord {
            plugin_key: plugin_key.into(), namespace: namespace.into(),
            skill_count: skills.len() as u64, resident_tokens: resident,
            skills: skills.iter().map(|s| s.to_string()).collect(),
            mcp_servers: mcp.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn r2_flags_unused_skill_plugin() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming", "tdd"], &[]),
        ]).unwrap();
        // 다른 플러그인 스킬만 호출됨 → superpowers 미사용
        store.upsert_events(&[skill_call("Windows", "u1", "vercel:deploy")]).unwrap();

        let findings = R2UnusedPluginSkills::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.rule_id, "R2");
        assert_eq!(f.scope_kind, "host");
        assert_eq!(f.scope_ref, "Windows");
        assert_eq!(f.evidence["plugin"], "superpowers@mp");
        assert_eq!(f.evidence["skill_count"], 2);
        assert_eq!(f.est_tokens_saved, 900);
        let p = f.prescription.as_ref().unwrap();
        assert_eq!(p.kind, "disable_plugin");
        assert_eq!(p.payload["plugin"], "superpowers@mp");
    }

    #[test]
    fn r2_silent_when_skill_used() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming"], &[]),
        ]).unwrap();
        store.upsert_events(&[skill_call("Windows", "u1", "superpowers:brainstorming")]).unwrap();
        assert!(R2UnusedPluginSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r2_silent_when_plugin_mcp_used() {
        // R1 비충돌: 플러그인 MCP 서버가 쓰이면 R2 침묵.
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("vercel@mp", "vercel", 900, &["deploy"], &["vercel"]),
        ]).unwrap();
        store.upsert_events(&[mcp_call("Windows", "u1", "vercel")]).unwrap();
        assert!(R2UnusedPluginSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r2_silent_under_threshold() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        // 상주 100 < 300
        store.replace_plugin_inventory("Windows", &[
            rec("tiny@mp", "tiny", 100, &["one"], &[]),
        ]).unwrap();
        assert!(R2UnusedPluginSkills::default().evaluate(&store).unwrap().is_empty());
    }

    #[test]
    fn r2_scoped_per_host() {
        // 같은 플러그인이 두 호스트에 상주, wsl에서만 스킬 사용 → Windows만 지목.
        let mut store = SqliteStore::open_in_memory().unwrap();
        store.replace_plugin_inventory("Windows", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming"], &[]),
        ]).unwrap();
        store.replace_plugin_inventory("wsl:Ubuntu", &[
            rec("superpowers@mp", "superpowers", 900, &["brainstorming"], &[]),
        ]).unwrap();
        store.upsert_events(&[skill_call("wsl:Ubuntu", "u1", "superpowers:brainstorming")]).unwrap();

        let findings = R2UnusedPluginSkills::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].scope_ref, "Windows");
    }
}
```

- [ ] **Step 3: 테스트 실행(통과 확인)**

Run: `cargo test r2_ -- --nocapture`
Expected: 5 tests pass (r2_flags_unused_skill_plugin, r2_silent_when_skill_used, r2_silent_when_plugin_mcp_used, r2_silent_under_threshold, r2_scoped_per_host).

- [ ] **Step 4: 전체 테스트 + 커밋**

Run: `cargo test` → 전체 통과, 무경고.

```bash
git add src/rules/mod.rs src/rules/r2_unused_plugins.rs
git commit -m "feat: R2 미사용 플러그인 규칙(host 단위, 스킬·MCP 미사용, disable_plugin 처방)"
```

---

## Task 5: finding_advice R2 arm (다이어리 서사)

**Files:**
- Modify: `src/diary/mod.rs` (`finding_advice` match + tests)

**Interfaces:**
- Consumes: `finding_advice(rule_id, evidence, est_tokens_saved) -> (String, String)`. R2 evidence `{plugin, skill_count, resident_tokens}`.

- [ ] **Step 1: 실패하는 테스트 추가**

`src/diary/mod.rs`의 `mod tests`에 추가(기존 finding_advice 테스트 근처):

```rust
    #[test]
    fn finding_advice_r2() {
        let (detail, action) = super::finding_advice(
            "R2",
            &serde_json::json!({"plugin":"superpowers@mp","skill_count":12,"resident_tokens":900}),
            900,
        );
        assert!(detail.contains("superpowers@mp"));
        assert!(detail.contains("12"));
        assert!(detail.contains("900"));
        assert!(action.contains("비활성"));
    }
```

- [ ] **Step 2: 테스트 실행(실패 확인)**

Run: `cargo test finding_advice_r2`
Expected: FAIL — R2는 default arm으로 떨어져 action이 빈 문자열.

- [ ] **Step 3: R2 arm 구현**

`src/diary/mod.rs::finding_advice`의 `match rule_id {` 안, `_ =>` 직전에 추가:

```rust
        "R2" => {
            let plugin = evidence.get("plugin").and_then(|v| v.as_str()).unwrap_or("(unknown)");
            let n = evidence.get("skill_count").and_then(|v| v.as_u64()).unwrap_or(0);
            (
                format!("플러그인 {plugin}의 스킬 {n}개(~{est_tokens_saved}토큰)를 한 번도 쓰지 않았어요"),
                "안 쓰는 플러그인은 설정에서 비활성화하면 매 세션 상주 토큰을 아껴요".to_string(),
            )
        }
```

- [ ] **Step 4: 테스트 실행(통과 확인)**

Run: `cargo test finding_advice` → 전체 finding_advice 테스트 PASS.

- [ ] **Step 5: 커밋**

```bash
git add src/diary/mod.rs
git commit -m "feat: finding_advice R2 arm(미사용 플러그인 근거·개선방향 서사)"
```

---

## Task 6: 배선 — cmd_rules 등록 + cmd_inventory 플러그인 스캔

**Files:**
- Modify: `src/main.rs` (import + cmd_rules RuleEngine 벡터 + cmd_inventory 플러그인 스캔)

**Interfaces:**
- Consumes: `R2UnusedPluginSkills`(Task 4), `scan_plugin_inventory`(Task 3), `replace_plugin_inventory`(Task 2).
- Produces: `agent-mentor rules`가 R5·R1·R2·R7·R9 실행. `agent-mentor inventory`가 각 호스트의 plugin_inventory를 스캔·원자 교체(불완전 호스트 스킵).

- [ ] **Step 1: import 추가**

`src/main.rs` 상단 use 블록에 추가(기존 rule import 옆 + inventory 함수):

```rust
use agent_mentor::rules::r2_unused_plugins::R2UnusedPluginSkills;
```

`scan_plugin_inventory`는 `agent_mentor::inventory::` 경로로 호출한다(기존 `collect_host_inventory` import 스타일에 맞춤 — 이미 inventory 항목을 import 중이면 목록에 `scan_plugin_inventory` 추가, 아니면 완전 경로 사용).

- [ ] **Step 2: cmd_rules 벡터에 R2 등록**

`cmd_rules`의 `RuleEngine::new(vec![...])`를 아래로 교체(R1 뒤, 알파벳/번호 순):

```rust
    let engine = RuleEngine::new(vec![
        Box::new(R5RepeatedRead::default()),
        Box::new(R1UnusedMcp::default()),
        Box::new(R2UnusedPluginSkills::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(R9WebOveruse::default()),
    ]);
```

- [ ] **Step 3: cmd_inventory에 플러그인 스캔 배선**

`src/main.rs::cmd_inventory`에서 각 호스트 처리 중 `collect_host_inventory` 결과로 `replace_host_inventory`를 호출하는 부분(§Task5에서 만든 `let cache = ...` 이후 블록) 바로 뒤에 플러그인 스캔을 추가한다. `store.replace_host_inventory(...)?;` 다음 줄들에 삽입:

```rust
        // 플러그인(스킬 제공) 인벤토리 — R2 재료. 불완전 스캔은 스킵(파괴적 부분 교체 방지).
        let (plugins, plugins_complete) =
            agent_mentor::inventory::scan_plugin_inventory(&settings, &cache);
        if !plugins_complete {
            eprintln!("warn: host {} 플러그인 스킬 스캔 실패 — plugin_inventory 유지, R2 스킵", hs.host);
        } else {
            store.replace_plugin_inventory(&hs.host, &plugins)?;
        }
```

> 주: `settings`·`cache`·`hs.host`·`store`(가변)는 Task 5에서 cmd_inventory에 이미 존재하는 바인딩이다. `collect_host_inventory`가 불완전해 `continue`로 스킵된 호스트는 이 코드에 도달하지 않는다(플러그인 스캔도 자연히 스킵).

- [ ] **Step 4: 빌드 + 전체 테스트**

Run: `cargo build && cargo test`
Expected: 빌드 성공(무경고), 전체 통과(기존 75 + Task1~5 신규 = 약 85).

- [ ] **Step 5: 커밋**

```bash
git add src/main.rs
git commit -m "feat: cmd_rules에 R2 등록 + cmd_inventory 플러그인 스킬 스캔 배선"
```

---

## 최종 검증

- [ ] `cargo test` 전체 통과(기존 75 + 어댑터 1 + store 1 + scan 3 + R2 5 + finding_advice 1 = **약 86개**), 무경고.
- [ ] (실엔진 e2e, 스펙 §8 검증) `.env.ref`로 실 히스토리에서:
  - `agent-mentor inventory` → plugin_inventory에 스킬-제공 플러그인이 채워지는지, MCP 전용은 제외되는지.
  - 실제 `Skill` 툴콜이 events에 `tool_kind='skill'`·`tool_target='<ns>:<skill>'`로 남고, ns가 `enabledPlugins`의 name과 매칭되는지(불일치 시 `scan_plugin_inventory`의 `namespace`를 별칭 매핑으로 보정).
  - `agent-mentor rules` → 실제 미사용 스킬-제공 플러그인이 R2로 뜨고, 쓰는 플러그인(스킬 또는 MCP)은 침묵하는지.
  - `agent-mentor diary <date>` 서사에 R2 근거·개선방향이 반영되는지.

---

## Self-Review 결과 (작성 시점)

- **스펙 커버리지:** §3.1 어댑터 Skill 캡처(Task 1), §3.2 plugin_inventory 테이블·스캔(Task 2·3), §3.3 host 스코프(Task 4), §4 R2 규칙·판정·비충돌·처방(Task 4), §5 finding_advice·cmd_rules·수집 배선(Task 5·6), §6 테스트(각 태스크), §8 실검증(최종 검증). 전부 매핑됨.
- **타입 일관성:** `PluginRecord{plugin_key,namespace,skill_count,resident_tokens,skills,mcp_servers}` — Task 2 정의, Task 3 생산, Task 4 테스트·Task 6 소비 일치. `scan_plugin_inventory(&Value,&Path)->(Vec<PluginRecord>,bool)`·`replace_plugin_inventory(&mut,&str,&[PluginRecord])`·`ToolKind::Skill{name}` 시그니처 태스크 간 일치. plugin_inventory 컬럼(namespace/skill_count/resident_tokens/skills_json/mcp_servers_json)이 SCHEMA·replace·R2 SELECT에서 일치.
- **플레이스홀더:** 없음(모든 스텝에 실제 코드/명령/기대출력).
- **R1 비충돌:** R2는 스킬 제공 플러그인만(MCP 전용 제외), MCP 사용 시 침묵 → R1(서버 단위)과 겹치지 않음.
