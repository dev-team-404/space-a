use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelFamily { Opus, Sonnet, Haiku, Other }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelTier { High, Mid, Low }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NormModel {
    pub family: ModelFamily,
    pub tier: ModelTier,
    pub raw_id: String,
}

impl NormModel {
    pub fn from_raw_id(raw: &str) -> NormModel {
        let l = raw.to_ascii_lowercase();
        let (family, tier) = if l.contains("opus") {
            (ModelFamily::Opus, ModelTier::High)
        } else if l.contains("sonnet") {
            (ModelFamily::Sonnet, ModelTier::Mid)
        } else if l.contains("haiku") {
            (ModelFamily::Haiku, ModelTier::Low)
        } else {
            (ModelFamily::Other, ModelTier::Mid)
        };
        NormModel { family, tier, raw_id: raw.to_string() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub eph_1h: u64,
    pub eph_5m: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    Other(String),
}

impl ToolKind {
    pub fn from_raw_name(raw: &str) -> ToolKind {
        if let Some(rest) = raw.strip_prefix("mcp__") {
            let mut parts = rest.splitn(2, "__");
            let server = parts.next().unwrap_or("").to_string();
            let tool = parts.next().unwrap_or("").to_string();
            return ToolKind::McpCall { server, tool };
        }
        match raw {
            "Read" => ToolKind::FileRead,
            "Edit" | "MultiEdit" => ToolKind::FileEdit,
            "Write" => ToolKind::FileWrite,
            "Grep" | "Glob" => ToolKind::Search,
            "Bash" => ToolKind::Execute,
            "WebSearch" => ToolKind::WebSearch,
            "WebFetch" => ToolKind::WebFetch,
            "Task" => ToolKind::SubAgent,
            other => ToolKind::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EventKind {
    AssistantTurn { model: NormModel, usage: TokenUsage, web_search: u32, web_fetch: u32 },
    ToolCall { kind: ToolKind, raw_name: String, target: Option<String> },
    SessionMeta { cwd: String, git_branch: Option<String> },
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedEvent {
    pub source_agent: String,
    pub schema_version: String,
    pub host: String,
    pub project_id: String,
    pub session_id: String,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub is_sidechain: bool,
    pub ts: Option<String>,
    pub source_file: String,
    pub source_offset: u64,
    pub kind: EventKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tiering_maps_family_and_tier() {
        let m = NormModel::from_raw_id("claude-opus-4-8");
        assert_eq!(m.family, ModelFamily::Opus);
        assert_eq!(m.tier, ModelTier::High);
        assert_eq!(m.raw_id, "claude-opus-4-8");

        assert_eq!(NormModel::from_raw_id("claude-haiku-4-5").tier, ModelTier::Low);
        assert_eq!(NormModel::from_raw_id("weird-model").family, ModelFamily::Other);
    }

    #[test]
    fn mcp_name_parses_server_and_tool() {
        match ToolKind::from_raw_name("mcp__context7__query-docs") {
            ToolKind::McpCall { server, tool } => {
                assert_eq!(server, "context7");
                assert_eq!(tool, "query-docs");
            }
            other => panic!("expected McpCall, got {other:?}"),
        }
        assert_eq!(ToolKind::from_raw_name("Read"), ToolKind::FileRead);
        assert_eq!(ToolKind::from_raw_name("Bash"), ToolKind::Execute);
        assert_eq!(
            ToolKind::from_raw_name("SomethingNew"),
            ToolKind::Other("SomethingNew".to_string())
        );
    }
}
