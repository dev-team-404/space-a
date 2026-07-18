use crate::rules::normalize_project_key;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub source: String, // "project" | "mcpjson" | "plugin"
}

/// 개인(비플러그인) 스킬 1건 — ~/.claude/skills 또는 프로젝트 .claude/skills (코칭 v3 §4.2)
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalSkill {
    pub name: String,
    pub path: String,     // SKILL.md 절대 경로 (PK 성분)
    pub body_chars: u64,  // SKILL.md 전문 글자수 — R13 PersonalSkillHygiene 판정 재료
    pub scope: String,    // "user" | "project"
}

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

/// .mcp.json(프로젝트 로컬 또는 플러그인)에서 서버 이름을 뽑는다.
/// 두 형태 지원: {"mcpServers":{...}} 래퍼 / 최상위에 서버명 직접.
pub fn parse_mcp_json(json: &Value) -> Vec<String> {
    // 래퍼 형태: "mcpServers" 키가 있으면 그 내부만 본다. 값이 객체가 아니면(null/문자열 등)
    // 빈 목록 — "mcpServers" 자체를 서버명으로 오인하지 않는다.
    if json.get("mcpServers").is_some() {
        return json
            .get("mcpServers")
            .and_then(|m| m.as_object())
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
    }
    // 최상위 형태: 값이 서버 정의처럼 보이는(객체 + command|url|type 중 하나) 키만 인정.
    // $schema·inputs 같은 메타 키가 유령 서버로 잡혀 R1 오탐을 내는 것을 방지.
    match json.as_object() {
        Some(map) => map
            .iter()
            .filter(|(_, v)| is_server_def(v))
            .map(|(k, _)| k.clone())
            .collect(),
        None => Vec::new(),
    }
}

/// MCP 서버 정의처럼 보이는가 — 객체이고 전송 방식 키(command|url|type) 중 하나를 가짐.
fn is_server_def(v: &Value) -> bool {
    v.as_object().map_or(false, |o| {
        o.contains_key("command") || o.contains_key("url") || o.contains_key("type")
    })
}

/// 버전 후보 (.mcp.json 경로, 버전 dir mtime) 중 mtime 최신 하나를 고른다.
fn pick_active_version(
    candidates: Vec<(PathBuf, std::time::SystemTime)>,
) -> Option<PathBuf> {
    candidates.into_iter().max_by_key(|(_, t)| *t).map(|(p, _)| p)
}

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
        // 버전 dir mtime: DirEntry::metadata()는 열거 시 확보된 메타데이터를 재사용해
        // 추가 stat/PathBuf 할당을 피한다(Windows에서 특히 저렴).
        if let Ok(mtime) = v.metadata().and_then(|m| m.modified()) {
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

        let vercel = parsed[0].servers.iter().find(|s| s.name == "vercel").unwrap();
        assert_eq!(vercel.source, "mcpjson");
        let ctx = parsed[0].servers.iter().find(|s| s.name == "context7").unwrap();
        assert_eq!(ctx.source, "project");

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
    fn parse_mcp_json_toplevel_excludes_meta_keys() {
        // 최상위 형태에 $schema·inputs 같은 비-서버 메타 키가 섞여도 서버로 오인하지 않는다.
        let json = serde_json::json!({
            "context7": { "command": "npx" },
            "$schema": "https://example/schema.json",
            "inputs": []
        });
        assert_eq!(parse_mcp_json(&json), vec!["context7"]);
    }

    #[test]
    fn parse_mcp_json_wrapper_non_object_yields_empty() {
        // "mcpServers" 키가 있으나 값이 객체가 아니면 "mcpServers"를 서버명으로 오인하지 않는다.
        assert!(parse_mcp_json(&serde_json::json!({ "mcpServers": "nope" })).is_empty());
        assert!(parse_mcp_json(&serde_json::json!({ "mcpServers": null })).is_empty());
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
        let (servers, complete) = resolve_project_servers(&cfg);
        let mut names: Vec<_> = servers.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["explicit", "local_a"]);
        assert!(complete);
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
        let (servers, complete) = resolve_project_servers(&cfg);
        let names: Vec<_> = servers.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["only"], "enableAll=false 면 .mcp.json 안 읽음");
        assert!(complete);
    }

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

        let (servers, complete) = plugin_servers(&settings, cache);
        assert!(complete);
        let mut names: Vec<_> = servers.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["context7", "vercel"]);
        assert!(servers.iter().all(|s| s.source == "plugin"));
    }

    #[test]
    fn plugin_servers_no_enabled_plugins_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (servers, complete) = plugin_servers(&serde_json::json!({}), dir.path());
        assert!(servers.is_empty());
        assert!(complete);
    }

    #[test]
    fn collect_host_inventory_merges_project_and_global() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("plugins").join("cache");
        let c7 = cache.join("mp").join("context7").join("unknown");
        std::fs::create_dir_all(&c7).unwrap();
        std::fs::write(c7.join(".mcp.json"), r#"{"context7":{"command":"npx"}}"#).unwrap();

        let claude_json = serde_json::json!({
            "projects": { "C:\\proj": { "mcpServers": { "local1": {} } } }
        });
        let settings = serde_json::json!({ "enabledPlugins": { "context7@mp": true } });

        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        assert!(inv.complete);
        let proj = inv.entries.iter().find(|(k, _)| k == "c--proj").unwrap();
        assert_eq!(proj.1.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["local1"]);
        let glob = inv.entries.iter().find(|(k, _)| k == "*").unwrap();
        assert_eq!(glob.1.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["context7"]);
    }

    #[test]
    fn collect_host_inventory_omits_global_when_no_plugins() {
        let inv = collect_host_inventory(
            &serde_json::json!({ "projects": { "C:\\p": {} } }),
            &serde_json::json!({}),
            std::path::Path::new(r"Z:\none"),
        );
        assert!(inv.entries.iter().all(|(k, _)| k != "*"), "플러그인 서버 없으면 '*' 항목 없음");
    }

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
}
