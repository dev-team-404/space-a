use crate::rules::normalize_project_key;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub source: String, // "project" | "global"
}

/// ~/.claude.json 을 받아 프로젝트별 활성 MCP 서버 목록을 반환.
/// 관대한 파싱: 없는 키/타입 불일치는 무시.
pub fn parse_claude_json(json: &Value) -> Vec<(String, Vec<McpServer>)> {
    let mut out = Vec::new();
    let Some(projects) = json.get("projects").and_then(|p| p.as_object()) else {
        return out;
    };
    for (path, entry) in projects {
        let key = normalize_project_key(path);
        let mut servers = Vec::new();

        if let Some(map) = entry.get("mcpServers").and_then(|m| m.as_object()) {
            for name in map.keys() {
                servers.push(McpServer { name: name.clone(), source: "project".into() });
            }
        }
        if let Some(arr) = entry.get("enabledMcpjsonServers").and_then(|a| a.as_array()) {
            for name in arr.iter().filter_map(|v| v.as_str()) {
                if !servers.iter().any(|s| s.name == name) {
                    servers.push(McpServer { name: name.to_string(), source: "project".into() });
                }
            }
        }
        out.push((key, servers));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_json_extracts_servers_per_project() {
        let json = serde_json::json!({
            "projects": {
                "C:\\Users\\jibin": {
                    "mcpServers": { "context7": {}, "playwright": {} },
                    "enabledMcpjsonServers": ["vercel"]
                },
                "D:\\Project\\agent-mentor": {
                    "mcpServers": {}
                }
            }
        });
        let mut parsed = parse_claude_json(&json);
        parsed.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(parsed[0].0, "c--users-jibin");
        let mut names: Vec<_> = parsed[0].1.iter().map(|s| s.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["context7", "playwright", "vercel"]);

        assert_eq!(parsed[1].0, "d--project-agent-mentor");
        assert!(parsed[1].1.is_empty());
    }
}
