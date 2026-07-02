use crate::rules::normalize_project_key;
use serde_json::Value;
use std::path::{Path, PathBuf};

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
}
