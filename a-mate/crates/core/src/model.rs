use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelFamily { Opus, Sonnet, Haiku, Other }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelTier { High, Mid, Low }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ResultStatus { Ok, Denied, Error }

impl ResultStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResultStatus::Ok => "ok",
            ResultStatus::Denied => "denied",
            ResultStatus::Error => "error",
        }
    }
}

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
    Skill { name: String },
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
            "Task" | "Agent" => ToolKind::SubAgent,
            other => ToolKind::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EventKind {
    AssistantTurn { model: NormModel, usage: TokenUsage, web_search: u32, web_fetch: u32 },
    ToolCall { kind: ToolKind, raw_name: String, target: Option<String>, tool_use_id: Option<String> },
    /// result_len = 결과 content의 문자 수 (R8 대형 결과 탐지 재료 — 본문은 미저장).
    ToolResult { tool_use_id: String, status: ResultStatus, result_len: u64 },
    Compaction,
    UserPrompt { preview: String },
    /// permission-mode 라인 (plan·bypassPermissions 등) — R16/R19 재료 (코칭 v3 §4.1-4)
    PermissionMode { mode: String },
    /// 프롬프트/명령에서 시크릿 패턴 감지 — 본문 미저장, pattern_id만 (코칭 v3 §4.2)
    SecretFlag { pattern_id: String },
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
    /// assistant 라인의 API message id (`msg_…`) — resume 포크 복제본·다중 라인에서
    /// 보존되는 논리 식별자. AssistantTurn dedup 키 재료 (데이터 위생 스펙 §3.1).
    pub msg_id: Option<String>,
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

    #[test]
    fn agent_raw_name_maps_to_sub_agent() {
        // 신형 하네스는 서브에이전트 툴명이 Task → Agent로 바뀜 (스펙 §4.1-1)
        assert_eq!(ToolKind::from_raw_name("Agent"), ToolKind::SubAgent);
        assert_eq!(ToolKind::from_raw_name("Task"), ToolKind::SubAgent); // 구형 유지
    }

    #[test]
    fn result_status_as_str() {
        assert_eq!(ResultStatus::Ok.as_str(), "ok");
        assert_eq!(ResultStatus::Denied.as_str(), "denied");
        assert_eq!(ResultStatus::Error.as_str(), "error");
    }
}
