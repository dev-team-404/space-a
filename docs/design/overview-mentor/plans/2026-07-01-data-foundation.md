# Data Foundation (호스트 열거 + 인벤토리 완결성 + R1 정합성) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Windows + WSL 여러 호스트의 Claude 사용 기록을 하나의 스토어로 통합 수집하고, "세션에서 활성인 MCP 셋"을 3소스(claude.json·프로젝트 `.mcp.json`·글로벌 플러그인)로 정확히 산출해 R1(안 쓰는 always-on MCP)이 멀티호스트에서 오염 없이 동작하게 만든다.

**Architecture:** 기존 `SourceAdapter → NormalizedEvent → SQLite → Rule` 파이프라인은 유지한다. 새 `hosts` 모듈이 Windows + WSL 소스를 열거해 `HostSource` 목록을 만들고, `main`이 각 호스트별로 ingest·inventory를 돌린다. 인벤토리는 host-global 플러그인 MCP를 `project_id="*"` 로, 프로젝트별 MCP를 정규화 키로 적재한다. R1은 인벤토리 행의 host(그리고 글로벌이면 host 전체, 아니면 host+project)로 스코핑해 호출/상주 비용을 조회한다.

**Tech Stack:** Rust 2021, `rusqlite`(bundled), `serde_json`, `std::process::Command`(wsl.exe 열거), `chrono`. **새 crate 의존성 없음**(notify는 이번 범위에서 유예).

## Global Constraints

이 절의 제약은 **모든 태스크의 요구사항에 암묵적으로 포함**된다.

- **관대한 파싱, 하드 실패 금지**: JSON/파일/명령 실패는 로깅만 하고 빈 결과로 진행. `unwrap()`/`expect()`로 실행 흐름을 죽이지 않는다(테스트 코드 제외).
- **에이전트 중립**: 에이전트별 로직은 `SourceAdapter` 뒤로. 규칙·저장·인벤토리 조립은 `NormalizedEvent`/정규화 타입만 본다.
- **Windows 전용**: macOS/Linux 분기 없음. WSL은 Windows에서 UNC 경로(`\\wsl.localhost\`, 폴백 `\\wsl$\`)로 접근.
- **로컬 처리 기본**: 외부 전송은 Engine 선택으로만(이 범위에는 egress 없음).
- **Tauri v2 배선 대비**: 현재는 순수 백엔드 크레이트. 공개 함수는 나중에 `#[tauri::command]`로 배선 가능하게 순수/결정론적으로 유지. v1 API 금지.
- **username 하드코딩 금지**: WSL username(`jayb`) ≠ Windows(`jibin`). 홈 경로는 `home\*` 스캔으로 발견.
- **빌드 환경(비표준)**: 모든 `cargo` 명령은 Git Bash에서 아래를 **먼저 export**한 셸에서 실행한다(시스템 PATH에 Rust 없음; GNU 툴체인 + mingw 필수).
  ```bash
  export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
  export CARGO_HTTP_CHECK_REVOKE=false
  export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
  ```

## Out of Scope (명시적 유예 — 이 플랜에서 안 함)

브레인스토밍/스코프 확정에서 유예된 항목. 구현하지 말 것.

- `notify` 실시간 파일 감시 → Tauri 배선 단계로 유예(현재 배치 `discover` 유지).
- R1 옵트인 정확 프로브(MCP 서버 1회 실행 → 스키마 토큰 계수) → 유예. flat 휴리스틱("약~") 유지.
- R1 차등(세션 간 cache_creation 델타) 귀속 → 유예.
- `EventKind::ToolResult` / `UserPrompt` emit, `SessionMeta` emit → 유예(이를 쓰는 R3/R4/R8은 이번 스프린트 밖).
- `tool_assets` 채우기(신규 도구 감지) → 유예(코칭 스펙 §5.1, 별개).
- 증분 rollup / `ingest_state.last_mtime` mtime 복구 / `tok_eph_5m` 컬럼 / `sessions.parent_session` → 유예.

---

## File Structure

- **Create** `src/hosts.rs` — 호스트/소스 열거(Windows + WSL). `HostSource`, WSL distro 파싱, username-agnostic 홈 스캔. 책임: **소스가 어디 있는가**.
- **Modify** `src/lib.rs` — `pub mod hosts;` 추가.
- **Modify** `src/inventory.rs` — 3소스 활성 MCP 셋 산출. `ProjectMcpConfig`, `parse_claude_json`(완결성), `parse_mcp_json`(두 형태), `resolve_project_servers`, `plugin_servers`, `collect_host_inventory`. 책임: **무엇이 활성인가**.
- **Modify** `src/rules/r1_unused_mcp.rs` — host 인지 + host-global(`"*"`) 스코프. 책임: **무엇이 안 쓰이는가**.
- **Modify** `src/main.rs` — `enumerate_hosts()`로 멀티호스트 ingest/inventory 배선.

각 태스크는 인라인 `#[cfg(test)] mod tests`로 독립 검증한다(Rust 관례). "Test:" 는 해당 소스 파일 내 테스트 모듈을 뜻한다.

---

### Task 1: 호스트/소스 열거 모듈 (`src/hosts.rs`)

**Files:**
- Create: `src/hosts.rs`
- Modify: `src/lib.rs` (모듈 등록)
- Test: `src/hosts.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `crate::adapter::ClaudeCodeAdapter { root: PathBuf, host: String }` (기존 공개 필드).
- Produces:
  - `pub struct HostSource { pub host: String, pub claude_root: PathBuf }`
  - `impl HostSource { pub fn claude_json(&self) -> PathBuf; pub fn settings_json(&self) -> PathBuf; pub fn adapter(&self) -> ClaudeCodeAdapter }`
  - `pub fn parse_wsl_distros(raw: &[u8]) -> Vec<String>`
  - `pub fn find_claude_roots_under(home_base: &Path) -> Vec<PathBuf>`
  - `pub fn enumerate_hosts() -> Vec<HostSource>`
  - host 라벨 규약: Windows = `"Windows"`, WSL = `"wsl:<distro>"`.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/hosts.rs`를 새로 만들고 아래를 넣는다(본문 함수는 아직 없음).

```rust
use crate::adapter::ClaudeCodeAdapter;
use std::path::{Path, PathBuf};

// ── 구현은 Step 3에서 채운다 ──

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le_with_bom(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE]; // UTF-16LE BOM
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn parse_wsl_distros_decodes_utf16le_with_bom_and_crlf() {
        // wsl.exe -l -q 는 보통 UTF-16LE + CRLF 를 낸다.
        let raw = utf16le_with_bom("Ubuntu-22.04\r\nDebian\r\n");
        assert_eq!(parse_wsl_distros(&raw), vec!["Ubuntu-22.04", "Debian"]);
    }

    #[test]
    fn parse_wsl_distros_handles_utf8_fallback_and_blanks() {
        let raw = b"Ubuntu-22.04\n\n  \nDebian\n";
        assert_eq!(parse_wsl_distros(raw), vec!["Ubuntu-22.04", "Debian"]);
    }

    #[test]
    fn parse_wsl_distros_empty_input_is_empty() {
        assert!(parse_wsl_distros(b"").is_empty());
    }

    #[test]
    fn find_claude_roots_scans_home_without_username_hardcoding() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        // 사용자 홈 'jayb' 에만 .claude/projects 존재
        std::fs::create_dir_all(home.join("jayb").join(".claude").join("projects")).unwrap();
        // 'root' 는 .claude 없음
        std::fs::create_dir_all(home.join("root")).unwrap();

        let roots = find_claude_roots_under(home);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], home.join("jayb").join(".claude"));
    }

    #[test]
    fn find_claude_roots_missing_base_is_empty() {
        assert!(find_claude_roots_under(Path::new(r"\\wsl.localhost\nope\home")).is_empty());
    }

    #[test]
    fn host_source_derives_sibling_paths() {
        let hs = HostSource {
            host: "Windows".into(),
            claude_root: PathBuf::from(r"C:\Users\jibin\.claude"),
        };
        assert_eq!(hs.claude_json(), PathBuf::from(r"C:\Users\jibin\.claude.json"));
        assert_eq!(hs.settings_json(), PathBuf::from(r"C:\Users\jibin\.claude\settings.json"));
        let a = hs.adapter();
        assert_eq!(a.host, "Windows");
        assert_eq!(a.root, PathBuf::from(r"C:\Users\jibin\.claude"));
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test hosts:: 2>&1 | head -40`
Expected: 컴파일 에러(`cannot find function parse_wsl_distros` 등). 아직 구현 없음.

- [ ] **Step 3: 최소 구현 작성** — `src/hosts.rs`의 `// ── 구현은 Step 3에서 채운다 ──` 자리에 삽입.

```rust
/// 하나의 Claude 소스 위치(호스트 라벨 + .claude 루트).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSource {
    pub host: String,         // "Windows" | "wsl:Ubuntu-22.04"
    pub claude_root: PathBuf, // .../.claude
}

impl HostSource {
    /// ~/.claude.json (인벤토리 원천) — .claude 의 형제 파일.
    pub fn claude_json(&self) -> PathBuf {
        match self.claude_root.parent() {
            Some(p) => p.join(".claude.json"),
            None => PathBuf::from(".claude.json"),
        }
    }

    /// settings.json (enabledPlugins 원천).
    pub fn settings_json(&self) -> PathBuf {
        self.claude_root.join("settings.json")
    }

    pub fn adapter(&self) -> ClaudeCodeAdapter {
        ClaudeCodeAdapter { root: self.claude_root.clone(), host: self.host.clone() }
    }
}

/// `wsl.exe -l -q` 출력(보통 UTF-16LE)을 문자열로 디코드. BOM 제거.
fn decode_wsl_output(raw: &[u8]) -> String {
    let has_le_bom = raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE;
    let zeros = raw.iter().filter(|&&b| b == 0).count();
    // ASCII UTF-16LE 는 바이트 절반이 0. UTF-8 ASCII 는 0이 거의 없다.
    let likely_utf16 = has_le_bom || (raw.len() >= 4 && zeros * 3 >= raw.len());
    if likely_utf16 {
        let start = if has_le_bom { 2 } else { 0 };
        let u16s: Vec<u16> = raw[start..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    }
}

/// `wsl.exe -l -q` 출력에서 distro 이름을 뽑는다. 순수·테스트 가능.
pub fn parse_wsl_distros(raw: &[u8]) -> Vec<String> {
    decode_wsl_output(raw)
        .lines()
        .map(|l| l.trim().trim_matches(|c| c == '\0' || c == '\u{feff}').trim())
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// `wsl.exe -l -q` 를 실행해 distro 목록을 얻는다. 실패(미설치 등)는 빈 목록.
pub fn wsl_list_distros() -> Vec<String> {
    match std::process::Command::new("wsl.exe").args(["-l", "-q"]).output() {
        Ok(o) => parse_wsl_distros(&o.stdout),
        Err(_) => Vec::new(),
    }
}

/// home 베이스 아래 사용자 홈들을 훑어 `.claude/projects` 를 가진 .claude 루트를 반환.
/// username 하드코딩 금지: `home\*` 를 스캔한다. 접근 불가/부재는 빈 목록.
pub fn find_claude_roots_under(home_base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(home_base) else {
        return out;
    };
    for e in entries.flatten() {
        let root = e.path().join(".claude");
        if root.join("projects").is_dir() {
            out.push(root);
        }
    }
    out
}

/// WSL distro 의 home 베이스 UNC 경로. localhost=true 면 `\\wsl.localhost\`, 아니면 `\\wsl$\`.
fn wsl_home_base(distro: &str, localhost: bool) -> PathBuf {
    let prefix = if localhost { r"\\wsl.localhost" } else { r"\\wsl$" };
    PathBuf::from(format!(r"{prefix}\{distro}\home"))
}

fn windows_claude_root() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok()?;
    Some(PathBuf::from(home).join(".claude"))
}

/// Windows + 모든 WSL distro 의 Claude 소스를 열거.
pub fn enumerate_hosts() -> Vec<HostSource> {
    let mut out = Vec::new();

    if let Some(root) = windows_claude_root() {
        if root.is_dir() {
            out.push(HostSource { host: "Windows".into(), claude_root: root });
        }
    }

    for distro in wsl_list_distros() {
        // \\wsl.localhost\ 우선, 비면 \\wsl$\ 폴백.
        let mut roots = find_claude_roots_under(&wsl_home_base(&distro, true));
        if roots.is_empty() {
            roots = find_claude_roots_under(&wsl_home_base(&distro, false));
        }
        for root in roots {
            out.push(HostSource { host: format!("wsl:{distro}"), claude_root: root });
        }
    }

    out
}
```

- [ ] **Step 4: `src/lib.rs`에 모듈 등록** — `pub mod finding;` 다음 줄에 추가.

```rust
pub mod hosts;
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test hosts::`
Expected: PASS (6 tests). 경고 없이 통과.

- [ ] **Step 6: 커밋**

```bash
git add src/hosts.rs src/lib.rs
git commit -m "feat: WSL/Windows 호스트 소스 열거(hosts 모듈)"
```

---

### Task 2: claude.json 인벤토리 완결성 (`src/inventory.rs`)

`disabledMcpjsonServers` 차감 + `enableAllProjectMcpServers` 플래그 + 실제 경로 보존을 위해 반환 타입을 `ProjectMcpConfig`로 바꾼다.

**Files:**
- Modify: `src/inventory.rs` (`parse_claude_json` 재정의 + 기존 테스트 갱신)
- Modify: `src/main.rs` (`cmd_inventory`를 새 타입에 맞춰 최소 수정 — 트리 컴파일 유지용)
- Test: `src/inventory.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `serde_json::Value`(claude.json), `crate::rules::normalize_project_key`.
- Produces:
  - `pub struct ProjectMcpConfig { pub key: String, pub real_path: String, pub servers: Vec<McpServer>, pub enable_all_project: bool, pub disabled: Vec<String> }`
  - `pub fn parse_claude_json(json: &Value) -> Vec<ProjectMcpConfig>` (반환 타입 변경 — 이전은 `Vec<(String, Vec<McpServer>)>`)
  - `McpServer.source` 값 확장: `"project"` | `"mcpjson"` | `"plugin"`.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/inventory.rs`의 기존 `mod tests`를 아래로 **교체**한다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_json_extracts_servers_and_paths() {
        let json = serde_json::json!({
            "projects": {
                "C:\\Users\\jibin": {
                    "mcpServers": { "context7": {}, "playwright": {} },
                    "enabledMcpjsonServers": ["vercel"]
                },
                "D:\\Project\\agent-mentor": { "mcpServers": {} }
            }
        });
        let mut parsed = parse_claude_json(&json);
        parsed.sort_by(|a, b| a.key.cmp(&b.key));

        assert_eq!(parsed[0].key, "c--users-jibin");
        assert_eq!(parsed[0].real_path, "C:\\Users\\jibin");
        let mut names: Vec<_> = parsed[0].servers.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["context7", "playwright", "vercel"]);

        assert_eq!(parsed[1].key, "d--project-agent-mentor");
        assert!(parsed[1].servers.is_empty());
    }

    #[test]
    fn parse_claude_json_subtracts_disabled_servers() {
        let json = serde_json::json!({
            "projects": {
                "C:\\p": {
                    "mcpServers": { "keep": {}, "gone": {} },
                    "enabledMcpjsonServers": ["also_gone"],
                    "disabledMcpjsonServers": ["gone", "also_gone"]
                }
            }
        });
        let parsed = parse_claude_json(&json);
        let names: Vec<_> = parsed[0].servers.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["keep"], "disabled 서버는 활성 셋에서 제외");
        assert_eq!(parsed[0].disabled, vec!["gone", "also_gone"]);
    }

    #[test]
    fn parse_claude_json_reads_enable_all_flag() {
        let json = serde_json::json!({
            "projects": { "C:\\p": { "enableAllProjectMcpServers": true } }
        });
        assert!(parse_claude_json(&json)[0].enable_all_project);
    }

    #[test]
    fn parse_claude_json_no_projects_is_empty() {
        assert!(parse_claude_json(&serde_json::json!({})).is_empty());
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test inventory::tests 2>&1 | head -40`
Expected: 컴파일 에러(`no field 'key' on ...`, `parse_claude_json` 반환 타입 불일치).

- [ ] **Step 3: 최소 구현 작성** — `src/inventory.rs`에서 `McpServer` 아래 주석과 기존 `parse_claude_json` 전체를 아래로 **교체**한다.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub source: String, // "project" | "mcpjson" | "plugin"
}

/// claude.json 한 프로젝트 항목의 활성 MCP 설정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMcpConfig {
    pub key: String,             // 정규화된 project_id (events 와 join)
    pub real_path: String,       // claude.json 원본 키(실제 cwd) — .mcp.json 읽기용
    pub servers: Vec<McpServer>, // mcpServers + enabledMcpjsonServers − disabled
    pub enable_all_project: bool,
    pub disabled: Vec<String>,
}

fn push_unique(v: &mut Vec<McpServer>, s: McpServer) {
    if !v.iter().any(|x| x.name == s.name) {
        v.push(s);
    }
}

/// ~/.claude.json 을 받아 프로젝트별 활성 MCP 설정을 반환.
/// 관대한 파싱: 없는 키/타입 불일치는 무시.
pub fn parse_claude_json(json: &Value) -> Vec<ProjectMcpConfig> {
    let mut out = Vec::new();
    let Some(projects) = json.get("projects").and_then(|p| p.as_object()) else {
        return out;
    };
    for (path, entry) in projects {
        let disabled: Vec<String> = entry
            .get("disabledMcpjsonServers")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let enable_all = entry
            .get("enableAllProjectMcpServers")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);

        let mut servers: Vec<McpServer> = Vec::new();
        if let Some(map) = entry.get("mcpServers").and_then(|m| m.as_object()) {
            for name in map.keys() {
                push_unique(&mut servers, McpServer { name: name.clone(), source: "project".into() });
            }
        }
        if let Some(arr) = entry.get("enabledMcpjsonServers").and_then(|a| a.as_array()) {
            for name in arr.iter().filter_map(|v| v.as_str()) {
                push_unique(&mut servers, McpServer { name: name.to_string(), source: "mcpjson".into() });
            }
        }
        servers.retain(|s| !disabled.contains(&s.name));

        out.push(ProjectMcpConfig {
            key: normalize_project_key(path),
            real_path: path.clone(),
            servers,
            enable_all_project: enable_all,
            disabled,
        });
    }
    out
}
```

- [ ] **Step 4: `src/main.rs` `cmd_inventory` 최소 수정** — 트리 컴파일 유지용. 기존 루프를 아래로 교체(Task 6에서 다시 전면 교체됨).

```rust
    let cfgs = parse_claude_json(&json);
    let mut servers = 0;
    for cfg in &cfgs {
        servers += cfg.servers.len();
        store.upsert_inventory("Windows", &cfg.key, &cfg.servers)?;
    }
    println!("inventory: {} projects, {servers} servers", cfgs.len());
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `cargo test inventory:: && cargo build`
Expected: PASS (4 tests) + 빌드 성공.

- [ ] **Step 6: 커밋**

```bash
git add src/inventory.rs src/main.rs
git commit -m "feat: claude.json 인벤토리 완결성(disabled 차감, enableAll, 실경로 보존)"
```

---

### Task 3: `.mcp.json` 두 형태 파서 + 프로젝트 서버 해석 (`src/inventory.rs`)

`.mcp.json`은 두 형태(최상위 서버명 / `{"mcpServers":{...}}` 래퍼)로 존재한다(실측). 둘 다 처리하고, `enableAllProjectMcpServers=true`일 때 프로젝트 `.mcp.json`을 읽어 활성 셋에 합친다.

**Files:**
- Modify: `src/inventory.rs`
- Test: `src/inventory.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `ProjectMcpConfig`(Task 2), `serde_json::Value`.
- Produces:
  - `pub fn parse_mcp_json(json: &Value) -> Vec<String>`
  - `pub fn resolve_project_servers(cfg: &ProjectMcpConfig) -> Vec<McpServer>`

- [ ] **Step 1: 실패하는 테스트 작성** — `src/inventory.rs`의 `mod tests` 안에 추가.

```rust
    #[test]
    fn parse_mcp_json_handles_wrapper_shape() {
        // vercel 형태: {"mcpServers": {...}}
        let json = serde_json::json!({ "mcpServers": { "vercel": { "type": "http" } } });
        assert_eq!(parse_mcp_json(&json), vec!["vercel"]);
    }

    #[test]
    fn parse_mcp_json_handles_toplevel_shape() {
        // context7 형태: 최상위에 서버명 직접
        let json = serde_json::json!({ "context7": { "command": "npx", "args": [] } });
        assert_eq!(parse_mcp_json(&json), vec!["context7"]);
    }

    #[test]
    fn resolve_project_servers_reads_mcp_json_when_enable_all() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path();
        std::fs::write(
            proj.join(".mcp.json"),
            r#"{"mcpServers":{"local_a":{},"local_b":{}}}"#,
        )
        .unwrap();

        let cfg = ProjectMcpConfig {
            key: "k".into(),
            real_path: proj.to_string_lossy().to_string(),
            servers: vec![McpServer { name: "explicit".into(), source: "project".into() }],
            enable_all_project: true,
            disabled: vec!["local_b".into()], // disabled 는 .mcp.json 서버도 제외
        };
        let mut names: Vec<_> = resolve_project_servers(&cfg).iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["explicit", "local_a"]);
    }

    #[test]
    fn resolve_project_servers_noop_when_not_enable_all() {
        let cfg = ProjectMcpConfig {
            key: "k".into(),
            real_path: r"Z:\nonexistent".into(),
            servers: vec![McpServer { name: "only".into(), source: "project".into() }],
            enable_all_project: false,
            disabled: vec![],
        };
        let names: Vec<_> = resolve_project_servers(&cfg).iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["only"], "enableAll=false 면 .mcp.json 안 읽음");
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test inventory::tests::parse_mcp_json 2>&1 | head -30`
Expected: 컴파일 에러(`cannot find function parse_mcp_json`).

- [ ] **Step 3: 최소 구현 작성** — `src/inventory.rs`의 `parse_claude_json` 함수 아래에 추가.

```rust
/// .mcp.json(프로젝트 로컬 또는 플러그인)에서 서버 이름을 뽑는다.
/// 두 형태 지원: {"mcpServers":{...}} 래퍼 / 최상위에 서버명 직접.
pub fn parse_mcp_json(json: &Value) -> Vec<String> {
    let obj = json
        .get("mcpServers")
        .and_then(|m| m.as_object())
        .or_else(|| json.as_object());
    match obj {
        Some(map) => map.keys().cloned().collect(),
        None => Vec::new(),
    }
}

/// enableAllProjectMcpServers=true 면 프로젝트 .mcp.json 의 모든 서버를 활성으로 합친다.
/// 파일 부재/접근 불가/파싱 실패는 조용히 무시(관대한 파싱).
pub fn resolve_project_servers(cfg: &ProjectMcpConfig) -> Vec<McpServer> {
    let mut servers = cfg.servers.clone();
    if !cfg.enable_all_project {
        return servers;
    }
    let mcp_path = std::path::Path::new(&cfg.real_path).join(".mcp.json");
    let Ok(raw) = std::fs::read_to_string(&mcp_path) else {
        return servers;
    };
    let Ok(json) = serde_json::from_str::<Value>(&raw) else {
        return servers;
    };
    for name in parse_mcp_json(&json) {
        if !cfg.disabled.contains(&name) && !servers.iter().any(|s| s.name == name) {
            servers.push(McpServer { name, source: "mcpjson".into() });
        }
    }
    servers
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test inventory::`
Expected: PASS (8 tests — Task 2의 4개 + 신규 4개).

- [ ] **Step 5: 커밋**

```bash
git add src/inventory.rs
git commit -m "feat: .mcp.json 두 형태 파서 + enableAll 프로젝트 서버 해석"
```

---

### Task 4: 글로벌 플러그인 MCP 해석 (`src/inventory.rs`)

`settings.json`의 `enabledPlugins`(값 `true`)를 순회해 각 플러그인 캐시의 `.mcp.json`에서 서버를 뽑는다. 플러그인 MCP는 host-global이라 호출자(Task 6)가 `project_id="*"`로 귀속한다.

**Files:**
- Modify: `src/inventory.rs`
- Test: `src/inventory.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `serde_json::Value`(settings.json), `parse_mcp_json`(Task 3), 플러그인 캐시 디렉터리 `Path`.
- Produces: `pub fn plugin_servers(settings: &Value, plugins_cache_dir: &Path) -> Vec<McpServer>`
- 캐시 경로 규약(실측): `<plugins_cache_dir>/<marketplace>/<name>/<version>/.mcp.json`. `enabledPlugins` 키 = `"<name>@<marketplace>"`.

- [ ] **Step 1: 실패하는 테스트 작성** — `mod tests`에 추가. 파일 상단 `use` 에 `use std::path::Path;`가 없으면 테스트에서 `super::*`로 이미 들어온다(본문에서 `std::path::Path`를 쓰므로 별도 import 불필요).

```rust
    #[test]
    fn plugin_servers_reads_enabled_plugins_only() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path();
        // context7@mp: 최상위 형태
        let c7 = cache.join("mp").join("context7").join("unknown");
        std::fs::create_dir_all(&c7).unwrap();
        std::fs::write(c7.join(".mcp.json"), r#"{"context7":{"command":"npx"}}"#).unwrap();
        // vercel@mp: 래퍼 형태
        let vc = cache.join("mp").join("vercel").join("0.44.0");
        std::fs::create_dir_all(&vc).unwrap();
        std::fs::write(vc.join(".mcp.json"), r#"{"mcpServers":{"vercel":{"type":"http"}}}"#).unwrap();
        // off@mp: 비활성 → 무시
        let off = cache.join("mp").join("off").join("1.0.0");
        std::fs::create_dir_all(&off).unwrap();
        std::fs::write(off.join(".mcp.json"), r#"{"off_server":{}}"#).unwrap();

        let settings = serde_json::json!({
            "enabledPlugins": {
                "context7@mp": true,
                "vercel@mp": true,
                "off@mp": false
            }
        });

        let mut names: Vec<_> = plugin_servers(&settings, cache).iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["context7", "vercel"]);
        assert!(plugin_servers(&settings, cache).iter().all(|s| s.source == "plugin"));
    }

    #[test]
    fn plugin_servers_no_enabled_plugins_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(plugin_servers(&serde_json::json!({}), dir.path()).is_empty());
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test inventory::tests::plugin_servers 2>&1 | head -30`
Expected: 컴파일 에러(`cannot find function plugin_servers`).

- [ ] **Step 3: 최소 구현 작성** — `src/inventory.rs`의 `resolve_project_servers` 아래에 추가. 파일 상단 import에 `use std::path::{Path, PathBuf};`가 없으면 추가(현재 `use serde_json::Value;`만 있음).

파일 상단 import 수정:
```rust
use crate::rules::normalize_project_key;
use serde_json::Value;
use std::path::{Path, PathBuf};
```

함수 추가:
```rust
/// <marketplace>/<name>/*/.mcp.json 을 찾는다(버전 디렉터리가 여러 개일 수 있음).
fn find_plugin_mcp_files(plugin_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(versions) = std::fs::read_dir(plugin_dir) else {
        return out;
    };
    for v in versions.flatten() {
        let candidate = v.path().join(".mcp.json");
        if candidate.is_file() {
            out.push(candidate);
        }
    }
    out
}

/// settings.json 의 enabledPlugins(값 true) → 각 플러그인 캐시의 .mcp.json 서버.
/// plugins_cache_dir = <.claude>/plugins/cache. host-global 서버(호출자가 "*"로 귀속).
pub fn plugin_servers(settings: &Value, plugins_cache_dir: &Path) -> Vec<McpServer> {
    let mut out: Vec<McpServer> = Vec::new();
    let Some(plugins) = settings.get("enabledPlugins").and_then(|p| p.as_object()) else {
        return out;
    };
    for (id, enabled) in plugins {
        if enabled.as_bool() != Some(true) {
            continue;
        }
        // id = "<name>@<marketplace>"
        let mut parts = id.splitn(2, '@');
        let name = parts.next().unwrap_or("");
        let marketplace = parts.next().unwrap_or("");
        if name.is_empty() || marketplace.is_empty() {
            continue;
        }
        let plugin_dir = plugins_cache_dir.join(marketplace).join(name);
        for mcp in find_plugin_mcp_files(&plugin_dir) {
            let Ok(raw) = std::fs::read_to_string(&mcp) else { continue };
            let Ok(json) = serde_json::from_str::<Value>(&raw) else { continue };
            for sname in parse_mcp_json(&json) {
                if !out.iter().any(|s| s.name == sname) {
                    out.push(McpServer { name: sname, source: "plugin".into() });
                }
            }
        }
    }
    out
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test inventory::`
Expected: PASS (10 tests).

- [ ] **Step 5: 커밋**

```bash
git add src/inventory.rs
git commit -m "feat: 글로벌 플러그인 MCP 해석(enabledPlugins → 캐시 .mcp.json)"
```

---

### Task 5: R1 host 인지 + host-global(`"*"`) 스코프 (`src/rules/r1_unused_mcp.rs`)

멀티호스트 오염 수정: 호출/상주 쿼리에 `host` 필터 추가. 인벤토리 `project_id="*"`(글로벌 플러그인)는 host 전체에서 호출 0 + host 상주 비용으로 평가.

**Files:**
- Modify: `src/rules/r1_unused_mcp.rs` (`evaluate` 본문 + 테스트)
- Test: `src/rules/r1_unused_mcp.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `mcp_inventory(host, project_id, server)`, `events(host, project_id, kind, tool_kind, tool_server, tok_cache_create)`.
- Produces: R1 `Finding`. project 스코프는 `scope_kind="project"`, 글로벌은 `scope_kind="host"`(`scope_project=None`, `scope_ref=host`). `dedup_key = "R1|{host}|{scope_ref}|{server}"`.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/rules/r1_unused_mcp.rs`의 `mod tests`에 추가(기존 두 테스트는 유지).

```rust
    // host 필터가 없으면 wsl 세션 호출이 Windows 인벤토리를 잘못 "사용됨"으로 만든다.
    #[test]
    fn r1_does_not_cross_contaminate_across_hosts() {
        let store = SqliteStore::open_in_memory().unwrap();
        let proj = "shared-proj";

        // Windows: playwright 호출됨(사용). 상주 55k.
        store.upsert_events(&[
            first_turn(proj, 55000),
            mcp_call(proj, "w1", "playwright"),
        ]).unwrap();
        // wsl: 같은 project_id, 하지만 playwright 호출 없음. 상주 55k.
        let mut wsl_turn = first_turn(proj, 55000);
        wsl_turn.host = "wsl:Ubuntu-22.04".into();
        wsl_turn.uuid = Some("wsl_turn".into());
        store.upsert_events(&[wsl_turn]).unwrap();

        // 두 호스트 모두 playwright 인벤토리 보유.
        store.upsert_inventory("Windows", proj, &[
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();
        store.upsert_inventory("wsl:Ubuntu-22.04", proj, &[
            McpServer { name: "playwright".into(), source: "project".into() },
        ]).unwrap();

        let findings = R1UnusedMcp::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1, "wsl 호스트에서만 미사용으로 잡혀야 함");
        assert_eq!(findings[0].scope_host.as_deref(), Some("wsl:Ubuntu-22.04"));
        assert_eq!(findings[0].evidence["server"], "playwright");
    }

    #[test]
    fn r1_flags_host_global_plugin_server_unused_across_host() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 상주 55k 세션이 프로젝트 p1 에 존재, context7(글로벌)은 어디서도 호출 안 됨.
        store.upsert_events(&[first_turn("p1", 55000)]).unwrap();
        store.upsert_inventory("Windows", "*", &[
            McpServer { name: "context7".into(), source: "plugin".into() },
        ]).unwrap();

        let findings = R1UnusedMcp::default().evaluate(&store).unwrap();
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.scope_kind, "host");
        assert_eq!(f.scope_project, None);
        assert_eq!(f.scope_ref, "Windows");
        assert_eq!(f.evidence["server"], "context7");
    }

    #[test]
    fn r1_host_global_silent_when_called_anywhere_on_host() {
        let store = SqliteStore::open_in_memory().unwrap();
        // context7 이 다른 프로젝트 p2 에서 호출됨 → 글로벌 미사용 아님.
        store.upsert_events(&[
            first_turn("p1", 55000),
            mcp_call("p2", "c1", "context7"),
        ]).unwrap();
        store.upsert_inventory("Windows", "*", &[
            McpServer { name: "context7".into(), source: "plugin".into() },
        ]).unwrap();
        assert!(R1UnusedMcp::default().evaluate(&store).unwrap().is_empty());
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test r1_unused_mcp:: 2>&1 | head -40`
Expected: FAIL — `r1_does_not_cross_contaminate_across_hosts`가 2건을 반환(host 필터 없어 wsl+Windows 둘 다), host-global 테스트는 `"*"`를 project로 취급해 상주 조회가 빗나가 FAIL.

- [ ] **Step 3: 최소 구현 작성** — `evaluate` 함수 본문 전체를 아래로 교체.

```rust
    fn evaluate(&self, store: &SqliteStore) -> Result<Vec<Finding>> {
        // 1) 인벤토리에 등록된 (host, project, server) 전체
        let mut inv_stmt = store
            .conn
            .prepare("SELECT host, project_id, server FROM mcp_inventory")?;
        let inv: Vec<(String, String, String)> = inv_stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;

        let mut out = Vec::new();
        for (host, project, server) in inv {
            let is_global = project == "*";

            // 2) 호출 횟수 — host 필터 필수. 글로벌은 host 전체, 프로젝트는 host+project.
            let calls: i64 = if is_global {
                store.conn.query_row(
                    "SELECT COUNT(*) FROM events
                     WHERE host=?1 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?2",
                    params![host, server],
                    |r| r.get(0),
                )?
            } else {
                store.conn.query_row(
                    "SELECT COUNT(*) FROM events
                     WHERE host=?1 AND project_id=?2 AND kind='tool_call' AND tool_kind='mcp_call' AND tool_server=?3",
                    params![host, project, server],
                    |r| r.get(0),
                )?
            };
            if calls > 0 {
                continue;
            }

            // 3) 대표 상주 비용(첫 턴 cache_create 근사 = MAX). 글로벌은 host 전체, 프로젝트는 host+project.
            let resident: i64 = if is_global {
                store.conn.query_row(
                    "SELECT COALESCE(MAX(tok_cache_create), 0) FROM events
                     WHERE host=?1 AND kind='assistant_turn'",
                    params![host],
                    |r| r.get(0),
                )?
            } else {
                store.conn.query_row(
                    "SELECT COALESCE(MAX(tok_cache_create), 0) FROM events
                     WHERE host=?1 AND project_id=?2 AND kind='assistant_turn'",
                    params![host, project],
                    |r| r.get(0),
                )?
            };
            if (resident as u64) <= self.min_resident_tokens {
                continue;
            }

            let scope_kind = if is_global { "host" } else { "project" };
            let scope_ref = if is_global { host.clone() } else { project.clone() };
            out.push(Finding {
                rule_id: "R1".into(),
                severity: Severity::Warn,
                scope_host: Some(host.clone()),
                scope_project: if is_global { None } else { Some(project.clone()) },
                scope_kind: scope_kind.into(),
                scope_ref: scope_ref.clone(),
                evidence: serde_json::json!({
                    "server": server,
                    "resident_tokens_total": resident,
                    "calls": 0,
                    "scope": scope_kind,
                    "note": "약(~) 추정 — 서버별 정확 귀속은 옵트인 프로브(유예)"
                }),
                est_tokens_saved: self.heuristic_tokens_per_server,
                prescription: Some(Prescription {
                    kind: "remove_mcp".into(),
                    payload: serde_json::json!({ "server": server }),
                }),
                dedup_key: format!("R1|{host}|{scope_ref}|{server}"),
            });
        }
        Ok(out)
    }
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test r1_unused_mcp::`
Expected: PASS (5 tests — 기존 2 + 신규 3). 기존 `r1_flags_configured_but_unused_server_with_resident_cost`는 events/inventory 모두 host=`"Windows"`라 host 필터 후에도 그대로 통과.

- [ ] **Step 5: 커밋**

```bash
git add src/rules/r1_unused_mcp.rs
git commit -m "fix: R1 host 인지(멀티호스트 오염 제거) + 글로벌 플러그인 '*' 스코프"
```

---

### Task 6: 멀티호스트 배선 (`collect_host_inventory` + `src/main.rs`)

한 호스트의 3소스 인벤토리를 조립하는 테스트 가능한 함수를 만들고, `main`이 `enumerate_hosts()`로 모든 호스트를 ingest·inventory 한다.

**Files:**
- Modify: `src/inventory.rs` (`collect_host_inventory` + 테스트)
- Modify: `src/main.rs` (`cmd_ingest`, `cmd_inventory` 재작성)
- Test: `src/inventory.rs` 내 `mod tests`

**Interfaces:**
- Consumes: `parse_claude_json`/`resolve_project_servers`/`plugin_servers`(Task 2~4), `crate::hosts::enumerate_hosts`(Task 1), `SqliteStore::upsert_inventory(host, project_id, &[McpServer])`(기존).
- Produces: `pub fn collect_host_inventory(claude_json: &Value, settings: &Value, plugins_cache_dir: &Path) -> Vec<(String, Vec<McpServer>)>` — `"*"` 키는 host-global 플러그인 MCP.

- [ ] **Step 1: 실패하는 테스트 작성** — `src/inventory.rs`의 `mod tests`에 추가.

```rust
    #[test]
    fn collect_host_inventory_merges_project_and_global() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("plugins").join("cache");
        let c7 = cache.join("mp").join("context7").join("unknown");
        std::fs::create_dir_all(&c7).unwrap();
        std::fs::write(c7.join(".mcp.json"), r#"{"context7":{}}"#).unwrap();

        let claude_json = serde_json::json!({
            "projects": { "C:\\proj": { "mcpServers": { "local1": {} } } }
        });
        let settings = serde_json::json!({ "enabledPlugins": { "context7@mp": true } });

        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        // 프로젝트 항목
        let proj = inv.iter().find(|(k, _)| k == "c--proj").unwrap();
        assert_eq!(proj.1.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["local1"]);
        // 글로벌 항목
        let glob = inv.iter().find(|(k, _)| k == "*").unwrap();
        assert_eq!(glob.1.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["context7"]);
    }

    #[test]
    fn collect_host_inventory_omits_global_when_no_plugins() {
        let inv = collect_host_inventory(
            &serde_json::json!({ "projects": { "C:\\p": {} } }),
            &serde_json::json!({}),
            std::path::Path::new(r"Z:\none"),
        );
        assert!(inv.iter().all(|(k, _)| k != "*"), "플러그인 서버 없으면 '*' 항목 없음");
    }
```

- [ ] **Step 2: 테스트 실패 확인**

Run: `cargo test inventory::tests::collect_host_inventory 2>&1 | head -30`
Expected: 컴파일 에러(`cannot find function collect_host_inventory`).

- [ ] **Step 3: 최소 구현 작성** — `src/inventory.rs`의 `plugin_servers` 아래에 추가.

```rust
/// 한 호스트의 전체 인벤토리를 (project_key, servers) 목록으로 조립.
/// "*" 키 = host-global 플러그인 MCP.
pub fn collect_host_inventory(
    claude_json: &Value,
    settings: &Value,
    plugins_cache_dir: &Path,
) -> Vec<(String, Vec<McpServer>)> {
    let mut out = Vec::new();
    for cfg in parse_claude_json(claude_json) {
        out.push((cfg.key.clone(), resolve_project_servers(&cfg)));
    }
    let plugins = plugin_servers(settings, plugins_cache_dir);
    if !plugins.is_empty() {
        out.push(("*".to_string(), plugins));
    }
    out
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test inventory::`
Expected: PASS (13 tests).

- [ ] **Step 5: `src/main.rs` 멀티호스트 재배선** — import와 `cmd_ingest`/`cmd_inventory`를 교체.

파일 상단 import 교체(기존 `use agent_mentor::inventory::parse_claude_json;` 및 adapter import 부분):
```rust
use agent_mentor::adapter::SourceAdapter;
use agent_mentor::diary::engine::{Engine, MockEngine, OpenAiCompatEngine};
use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
use agent_mentor::hosts::enumerate_hosts;
use agent_mentor::inventory::collect_host_inventory;
use agent_mentor::rules::r1_unused_mcp::R1UnusedMcp;
use agent_mentor::rules::r5_repeated_read::R5RepeatedRead;
use agent_mentor::rules::RuleEngine;
use agent_mentor::store::{ingest_file, SqliteStore};
use anyhow::Result;
```
(`ClaudeCodeAdapter`, `parse_claude_json`, `std::path::PathBuf` 직접 import는 제거 — 아래 재작성으로 미사용.)

`cmd_ingest` 교체:
```rust
fn cmd_ingest(store: &SqliteStore) -> Result<()> {
    let mut total = 0usize;
    let mut file_count = 0usize;
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        let files = adapter.discover().unwrap_or_default();
        file_count += files.len();
        for f in &files {
            total += ingest_file(store, &adapter, f)?;
        }
        println!("  [{}] {} files", hs.host, files.len());
    }
    store.rebuild_rollup()?;
    println!("ingested {total} new events from {file_count} files across all hosts");
    Ok(())
}
```

`cmd_inventory` 교체:
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

- [ ] **Step 6: 전체 테스트 + 빌드 + 수동 스모크**

Run: `cargo test && cargo build`
Expected: 전 테스트 PASS + 빌드 성공.

Run(수동 스모크, 실제 데이터): `cargo run -- inventory && cargo run -- rules`
Expected: `inventory: N servers across all hosts` 출력. `rules`에서 글로벌 플러그인 중 이 머신에서 호출 0인 서버가 `scope=host`로, 프로젝트별 미사용 서버가 `scope=project`로 뜨는지 눈으로 확인(있다면). 하드 실패/패닉 없어야 함.

- [ ] **Step 7: 커밋**

```bash
git add src/inventory.rs src/main.rs
git commit -m "feat: 멀티호스트 ingest/inventory 배선(enumerate_hosts + 3소스 조립)"
```

---

## Self-Review

**1. Spec coverage (데이터 파운데이션 스펙 §3~§8, 스코프 확정분):**
- §7 호스트 열거(WSL `wsl -l -q`, `\\wsl.localhost\` + `\\wsl$\` 폴백, username-agnostic, host 라벨) → Task 1 ✅
- §2.2 인벤토리 3소스(claude.json + disabled 차감, 프로젝트 `.mcp.json`, 글로벌 플러그인) → Task 2·3·4 ✅
- §8 R1 멀티호스트 정합성(host 필터) + 글로벌 상주 스코프 → Task 5 ✅
- §10 스프린트 ②(호스트 열거 통합) ③(인벤토리+R1) 통합 배선 → Task 6 ✅
- 유예 항목(notify, 옵트인 프로브, ToolResult/UserPrompt, tool_assets, 증분 rollup 등)은 "Out of Scope"에 명시, 태스크 없음 — 의도된 갭 ✅

**2. Placeholder scan:** 모든 코드/테스트 스텝은 실제 코드 블록 포함, TBD/TODO 없음. 명령·기대 출력 명시 ✅

**3. Type consistency:**
- `HostSource { host, claude_root }` — Task 1 정의, Task 6에서 `hs.host`/`hs.claude_root`/`hs.adapter()`/`hs.claude_json()`/`hs.settings_json()` 사용 일치 ✅
- `ProjectMcpConfig { key, real_path, servers, enable_all_project, disabled }` — Task 2 정의, Task 3(`resolve_project_servers`)·Task 6(`collect_host_inventory`) 사용 일치 ✅
- `McpServer.source` ∈ {"project","mcpjson","plugin"} — Task 2·3·4 일관 ✅
- R1 `dedup_key = "R1|{host}|{scope_ref}|{server}"`, `scope_kind` ∈ {"project","host"} — Task 5 내부 일관 ✅
- `collect_host_inventory(&Value, &Value, &Path) -> Vec<(String, Vec<McpServer>)>` — Task 6 정의/호출 시그니처 일치 ✅
- `upsert_inventory(host, project_id, &[McpServer])` — 기존 store API 변경 없음, 호출 일치 ✅

**주의(실행자용):** `parse_claude_json` 반환 타입이 Task 2에서 바뀐다. Task 2가 `main.rs`의 `cmd_inventory`를 최소 수정해 트리를 green으로 유지한 뒤, Task 6이 전면 재작성한다. 태스크 순서를 지킬 것.

---

## Execution Handoff

플랜 저장 위치: `docs/plans/2026-07-01-data-foundation.md`. 실행은 `superpowers:subagent-driven-development`(권장)로 태스크별 fresh 서브에이전트 + 2단계 리뷰.
