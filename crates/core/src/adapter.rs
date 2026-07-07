use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage, ToolKind};
use anyhow::Result;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub trait SourceAdapter {
    fn discover(&self) -> Result<Vec<PathBuf>>;
    fn read_incremental(&self, file: &Path, from_offset: u64) -> Result<(Vec<(u64, String)>, u64)>;
    fn map(&self, line: &str, source_file: &str, source_offset: u64) -> Vec<NormalizedEvent>;
}

pub struct ClaudeCodeAdapter {
    pub root: PathBuf,
    pub host: String,
}

impl ClaudeCodeAdapter {
    /// Windows %USERPROFILE%\.claude 를 기본 루트로.
    pub fn windows() -> ClaudeCodeAdapter {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        ClaudeCodeAdapter {
            root: PathBuf::from(home).join(".claude"),
            host: "Windows".to_string(),
        }
    }
}

/// 디렉터리명/경로를 프로젝트 join 키로 정규화. (Global Constraints의 join 결정)
pub fn decode_project_id(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' => '-',
            other => other,
        })
        .collect()
}

fn as_u64(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

impl SourceAdapter for ClaudeCodeAdapter {
    fn discover(&self) -> Result<Vec<PathBuf>> {
        let projects = self.root.join("projects");
        let mut out = Vec::new();
        if !projects.is_dir() {
            return Ok(out);
        }
        for proj in std::fs::read_dir(&projects)? {
            let proj = proj?.path();
            if !proj.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(&proj)? {
                let f = f?.path();
                if f.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    out.push(f);
                }
            }
        }
        Ok(out)
    }

    fn read_incremental(&self, file: &Path, from_offset: u64) -> Result<(Vec<(u64, String)>, u64)> {
        let mut f = std::fs::File::open(file)?;
        f.seek(SeekFrom::Start(from_offset))?;
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;

        let mut lines = Vec::new();
        let mut consumed = 0u64;
        for segment in buf.split_inclusive('\n') {
            if segment.ends_with('\n') {
                let start = from_offset + consumed;
                consumed += segment.len() as u64;
                let trimmed = segment.trim_end_matches(['\n', '\r']);
                if !trimmed.is_empty() {
                    lines.push((start, trimmed.to_string()));
                }
            }
            // 개행으로 끝나지 않는 마지막 tail은 버림(미완결) → consumed에 미포함
        }
        Ok((lines, from_offset + consumed))
    }

    fn map(&self, line: &str, source_file: &str, source_offset: u64) -> Vec<NormalizedEvent> {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Vec::new(), // 관대한 파싱: 하드 실패 금지
        };
        let ltype = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let session_id = match v.get("sessionId").and_then(|x| x.as_str()) {
            Some(s) => s.to_string(),
            None => return Vec::new(),
        };

        // project_id: 소스 파일의 부모 디렉터리명에서.
        let project_id = Path::new(source_file)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(decode_project_id)
            .unwrap_or_default();

        let mk = |kind: EventKind, off_bump: u64| NormalizedEvent {
            source_agent: "claude-code".to_string(),
            schema_version: v
                .get("version")
                .and_then(|x| x.as_str())
                .unwrap_or("unknown")
                .to_string(),
            host: self.host.clone(),
            project_id: project_id.clone(),
            session_id: session_id.clone(),
            uuid: v.get("uuid").and_then(|x| x.as_str()).map(String::from),
            parent_uuid: v.get("parentUuid").and_then(|x| x.as_str()).map(String::from),
            is_sidechain: v.get("isSidechain").and_then(|x| x.as_bool()).unwrap_or(false),
            ts: v.get("timestamp").and_then(|x| x.as_str()).map(String::from),
            source_file: source_file.to_string(),
            source_offset: source_offset + off_bump,
            kind,
        };

        let mut out = Vec::new();
        if ltype == "assistant" {
            let msg = v.get("message").cloned().unwrap_or(Value::Null);
            let usage = msg.get("usage").cloned().unwrap_or(Value::Null);
            let cc = usage.get("cache_creation").cloned().unwrap_or(Value::Null);
            let stu = usage.get("server_tool_use").cloned().unwrap_or(Value::Null);
            let model = NormModel::from_raw_id(
                msg.get("model").and_then(|x| x.as_str()).unwrap_or("unknown"),
            );
            let tu = TokenUsage {
                input: as_u64(&usage, "input_tokens"),
                output: as_u64(&usage, "output_tokens"),
                cache_read: as_u64(&usage, "cache_read_input_tokens"),
                cache_creation: as_u64(&usage, "cache_creation_input_tokens"),
                eph_1h: as_u64(&cc, "ephemeral_1h_input_tokens"),
                eph_5m: as_u64(&cc, "ephemeral_5m_input_tokens"),
            };
            out.push(mk(
                EventKind::AssistantTurn {
                    model,
                    usage: tu,
                    web_search: as_u64(&stu, "web_search_requests") as u32,
                    web_fetch: as_u64(&stu, "web_fetch_requests") as u32,
                },
                0,
            ));

            // tool_use 블록 → ToolCall
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for (i, block) in content.iter().enumerate() {
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
                            EventKind::ToolCall { kind, raw_name, target, tool_use_id: None },
                            (i + 1) as u64,
                        ));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, ToolKind};

    fn adapter() -> ClaudeCodeAdapter {
        ClaudeCodeAdapter { root: std::path::PathBuf::from("."), host: "Windows".into() }
    }

    #[test]
    fn map_assistant_line_yields_turn_and_tool_calls() {
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","parentUuid":null,
            "isSidechain":false,"timestamp":"2026-07-01T10:00:00Z","cwd":"C:\\Users\\jibin",
            "gitBranch":"main","message":{"model":"claude-opus-4-8",
            "usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5,
            "cache_creation_input_tokens":55000,
            "cache_creation":{"ephemeral_1h_input_tokens":100,"ephemeral_5m_input_tokens":200},
            "server_tool_use":{"web_search_requests":1,"web_fetch_requests":0}},
            "content":[{"type":"text","text":"hi"},
            {"type":"tool_use","name":"Read","input":{"file_path":"C:\\a\\report.xlsx"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert_eq!(evs.len(), 2, "one AssistantTurn + one ToolCall");

        match &evs[0].kind {
            EventKind::AssistantTurn { usage, web_search, .. } => {
                assert_eq!(usage.cache_creation, 55000);
                assert_eq!(usage.eph_1h, 100);
                assert_eq!(*web_search, 1);
            }
            k => panic!("expected AssistantTurn, got {k:?}"),
        }
        match &evs[1].kind {
            EventKind::ToolCall { kind, target, .. } => {
                assert_eq!(*kind, ToolKind::FileRead);
                assert_eq!(target.as_deref(), Some("C:\\a\\report.xlsx"));
            }
            k => panic!("expected ToolCall, got {k:?}"),
        }
        assert_eq!(evs[0].session_id, "s1");
        assert_eq!(evs[0].host, "Windows");
    }

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

    #[test]
    fn map_never_panics_on_garbage() {
        assert!(adapter().map("not json at all", "x.jsonl", 0).is_empty());
        assert!(adapter().map("{}", "x.jsonl", 0).is_empty());
        assert!(adapter().map(r#"{"type":"summary"}"#, "x.jsonl", 0).is_empty());
    }

    #[test]
    fn read_incremental_returns_only_complete_lines() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        // 완결 라인 2개 + 미완결 tail
        write!(f, "{{\"a\":1}}\n{{\"b\":2}}\n{{\"partial\"").unwrap();
        f.flush().unwrap();

        let (lines, new_off) = adapter().read_incremental(&path, 0).unwrap();
        assert_eq!(lines, vec![(0u64, "{\"a\":1}".to_string()), (8u64, "{\"b\":2}".to_string())]);
        // 새 오프셋은 두 완결 라인의 바이트 길이(개행 포함)
        assert_eq!(new_off, ("{\"a\":1}\n{\"b\":2}\n").len() as u64);

        // 같은 오프셋에서 다시 읽으면 새 완결 라인 없음
        let (lines2, _) = adapter().read_incremental(&path, new_off).unwrap();
        assert!(lines2.is_empty());
    }
}
