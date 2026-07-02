use crate::rules::normalize_project_key;
use serde_json::Value;

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
}
