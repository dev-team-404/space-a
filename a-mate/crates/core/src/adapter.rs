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

/// 시크릿을 담은 Bash 명령 target 대체 문자열 — 원문 대신 저장(본문 미저장 계약).
pub const SECRET_REDACTED: &str = "<redacted: secret>";

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

/// tool_result 상태 분류. 프로즈는 저장하지 않고 여기서 enum만 도출한다(스펙 §4.2).
/// 거부 마커는 Claude Code 메시지 텍스트 의존(안정 API 아님) — 실 트랜스크립트로 핀하고,
/// 미인식 시 error로 폴백해 R11이 오탐 대신 침묵하게 한다. **거부 마커는 §E2E 검증 포인트.**
const DENY_MARKERS: &[&str] = &[
    "doesn't want to proceed",
    "tool use was rejected",
    "user doesn't want to take this action",
    "hasn't granted",
    "requested permissions",
];

pub(crate) fn classify_tool_result(is_error: bool, content: &str) -> crate::model::ResultStatus {
    use crate::model::ResultStatus;
    let low = content.to_ascii_lowercase();
    if DENY_MARKERS.iter().any(|m| low.contains(m)) {
        ResultStatus::Denied
    } else if is_error {
        ResultStatus::Error
    } else {
        ResultStatus::Ok
    }
}

/// tool_result content(문자열 또는 블록 배열)를 검색 가능한 문자열로 평탄화.
fn tool_result_content_string(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// user content(문자열 또는 블록 배열)의 텍스트 전문을 평탄화. 시크릿 스캔·미리보기 공용.
fn content_text(content: Option<&Value>) -> Option<String> {
    match content {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(arr)) => Some(
            arr.iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// Claude Code가 user 라인에 주입하는 합성 마커 — 사용자 지시가 아니다.
/// (슬래시 커맨드 에코, 로컬 커맨드 출력, IDE 연동 이벤트, 훅/백그라운드 알림)
fn is_synthetic_marker(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<command")
        || t.starts_with("<local-command")
        || t.starts_with("<ide_")
        || t.starts_with("<system-reminder")
        || t.starts_with("<task-notification")
        || t.starts_with("[Request interrupted")
}

/// user 프롬프트 미리보기(첫 줄 ≤120자). content가 문자열이면 그대로, 블록 배열이면
/// 합성 마커 블록(<ide_opened_file> 등)을 제외하고 text 연결.
/// tool_result 라인이나 빈 내용은 None. 합성 마커로 시작하면 None.
fn extract_prompt_preview(content: Option<&Value>) -> Option<String> {
    let raw = match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .filter(|t| !is_synthetic_marker(t))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let first_line = raw.lines().next().unwrap_or("").trim();
    if first_line.is_empty() || is_synthetic_marker(first_line) {
        return None;
    }
    // 시크릿 포함 첫 줄은 미리보기로 저장하지 않는다 (본문 미저장 원칙 — 코칭 v3 §4.2; SecretFlag는 별도 방출됨)
    if !crate::curation::find_secret_patterns(first_line).is_empty() {
        return None;
    }
    Some(first_line.chars().take(120).collect())
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
            msg_id: v
                .get("message")
                .and_then(|m| m.get("id"))
                .and_then(|x| x.as_str())
                .map(String::from),
            kind,
        };

        let mut out = Vec::new();

        // cwd 있는 라인 → SessionMeta (sessions로 라우팅됨, events 미저장)
        if let Some(cwd) = v.get("cwd").and_then(|x| x.as_str()) {
            let git_branch = v.get("gitBranch").and_then(|x| x.as_str()).map(String::from);
            out.push(mk(EventKind::SessionMeta { cwd: cwd.to_string(), git_branch }, 900));
        }
        // compaction 경계 — 구형(isCompactSummary) + 신형(system/compact_boundary) 모두 인지.
        // trigger(auto/manual) 구분은 Windows 실데이터 핀 후 후속 (코칭 v3 §4.1-5, fail-safe: 미인식=침묵)
        let compact_boundary = ltype == "system"
            && v.get("subtype").and_then(|x| x.as_str()) == Some("compact_boundary");
        if v.get("isCompactSummary").and_then(|x| x.as_bool()).unwrap_or(false) || compact_boundary {
            out.push(mk(EventKind::Compaction, 0));
            return out;
        }
        // permission-mode 라인 → 이벤트 (plan=R16, bypassPermissions=R19 재료 — 코칭 v3 §4.1-4)
        if ltype == "permission-mode" {
            if let Some(mode) = v.get("permissionMode").and_then(|x| x.as_str()) {
                out.push(mk(EventKind::PermissionMode { mode: mode.to_string() }, 0));
            }
            return out;
        }

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
                        let tool_use_id =
                            block.get("id").and_then(|x| x.as_str()).map(String::from);
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
                        // Bash command 인자 시크릿 스캔 (코칭 v3 §4.2). 시크릿이 있으면 명령 원문이
                        // target(→events.tool_target)에 저장되지 않도록 마스킹한다 — pattern_id만 저장 계약.
                        let cmd_secrets = input
                            .get("command")
                            .and_then(|x| x.as_str())
                            .map(crate::curation::find_secret_patterns)
                            .unwrap_or_default();
                        let target =
                            if cmd_secrets.is_empty() { target } else { Some(SECRET_REDACTED.into()) };
                        out.push(mk(
                            EventKind::ToolCall { kind, raw_name, target, tool_use_id },
                            (i + 1) as u64,
                        ));
                        // off_bump 800대 — 블록별 8칸.
                        for (j, pid) in cmd_secrets.iter().enumerate() {
                            out.push(mk(
                                EventKind::SecretFlag { pattern_id: pid.to_string() },
                                800 + (i as u64) * 8 + j as u64,
                            ));
                        }
                    }
                }
            }
        }

        if ltype == "user" && !v.get("isMeta").and_then(|x| x.as_bool()).unwrap_or(false) {
            let msg = v.get("message").cloned().unwrap_or(Value::Null);
            let content = msg.get("content");
            let mut had_tool_result = false;
            if let Some(arr) = content.and_then(|c| c.as_array()) {
                for (i, block) in arr.iter().enumerate() {
                    if block.get("type").and_then(|x| x.as_str()) == Some("tool_result") {
                        had_tool_result = true;
                        let tuid = block.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let is_error = block.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false);
                        let cstr = tool_result_content_string(block.get("content"));
                        let status = classify_tool_result(is_error, &cstr);
                        let result_len = cstr.chars().count() as u64;
                        out.push(mk(
                            EventKind::ToolResult { tool_use_id: tuid, status, result_len },
                            (i + 1) as u64,
                        ));
                    }
                }
            }
            if !had_tool_result {
                if let Some(preview) = extract_prompt_preview(content) {
                    out.push(mk(EventKind::UserPrompt { preview }, 0)); // off_bump 0 = 라인 시작(deref 포인터)
                }
                // 시크릿 스캔은 프롬프트 전문 대상 (미리보기 스킵과 독립 — 코칭 v3 §4.2)
                // SecretFlag의 source_offset(off_bump 800대)은 dedup 전용이며 deref 포인터가 아니다
                // (전문 확인은 세션 상세 경유 — PR② R20 참고).
                if let Some(text) = content_text(content) {
                    for (i, pid) in crate::curation::find_secret_patterns(&text).iter().enumerate() {
                        out.push(mk(EventKind::SecretFlag { pattern_id: pid.to_string() }, 800 + i as u64));
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
        assert_eq!(evs.len(), 3, "one SessionMeta(cwd) + one AssistantTurn + one ToolCall");

        match &evs[1].kind {
            EventKind::AssistantTurn { usage, web_search, .. } => {
                assert_eq!(usage.cache_creation, 55000);
                assert_eq!(usage.eph_1h, 100);
                assert_eq!(*web_search, 1);
            }
            k => panic!("expected AssistantTurn, got {k:?}"),
        }
        match &evs[2].kind {
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
    fn map_assistant_line_carries_message_id() {
        // resume 포크 복제본에서도 보존되는 message.id — 논리 dedup 키 재료 (스펙 §3.1)
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u1",
            "message":{"id":"msg_011Ccp9b","model":"claude-opus-4-8",
            "usage":{"input_tokens":1,"output_tokens":2},
            "content":[{"type":"text","text":"hi"}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        let turn = evs.iter().find(|e| matches!(e.kind, EventKind::AssistantTurn { .. })).unwrap();
        assert_eq!(turn.msg_id.as_deref(), Some("msg_011Ccp9b"));
    }

    #[test]
    fn map_user_line_has_no_message_id() {
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u2",
            "message":{"role":"user","content":"이 함수 리팩토링 진행해줘"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(evs.iter().all(|e| e.msg_id.is_none()));
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
    fn classify_tool_result_denied_error_ok() {
        use crate::model::ResultStatus;
        assert_eq!(super::classify_tool_result(true, "The user doesn't want to proceed with this tool use"), ResultStatus::Denied);
        assert_eq!(super::classify_tool_result(false, "the tool use was rejected"), ResultStatus::Denied);
        assert_eq!(super::classify_tool_result(true, "File not found: x.rs"), ResultStatus::Error);
        assert_eq!(super::classify_tool_result(false, "ok result body"), ResultStatus::Ok);
    }

    #[test]
    fn map_user_tool_result_line_yields_toolresult() {
        use crate::model::{EventKind, ResultStatus};
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u2","timestamp":"2026-07-07T10:01:00Z",
            "cwd":"D:\\Project\\cowork","gitBranch":"main",
            "message":{"role":"user","content":[
              {"type":"tool_result","tool_use_id":"toolu_1","is_error":true,
               "content":"The user doesn't want to proceed with this tool use"}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 100);
        // SessionMeta(cwd) + ToolResult
        let tr = evs.iter().find(|e| matches!(e.kind, EventKind::ToolResult { .. })).unwrap();
        match &tr.kind {
            EventKind::ToolResult { tool_use_id, status, .. } => {
                assert_eq!(tool_use_id, "toolu_1");
                assert_eq!(*status, ResultStatus::Denied);
            }
            k => panic!("expected ToolResult, got {k:?}"),
        }
        assert!(evs.iter().any(|e| matches!(&e.kind, EventKind::SessionMeta { cwd, .. } if cwd == "D:\\Project\\cowork")));
    }

    #[test]
    fn map_user_prompt_line_yields_userprompt_preview_at_line_offset() {
        use crate::model::EventKind;
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u3","timestamp":"2026-07-07T10:00:00Z",
            "cwd":"D:\\Project\\cowork","message":{"role":"user","content":"Run this exact Bash command\nand then stop"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 500);
        let up = evs.iter().find(|e| matches!(e.kind, EventKind::UserPrompt { .. })).unwrap();
        match &up.kind {
            EventKind::UserPrompt { preview } => assert_eq!(preview, "Run this exact Bash command"), // 첫 줄만
            k => panic!("expected UserPrompt, got {k:?}"),
        }
        assert_eq!(up.source_offset, 500, "deref 포인터는 라인 시작 offset(off_bump 0)");
    }

    #[test]
    fn map_meta_user_line_is_not_prompt() {
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u4","isMeta":true,
            "message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })));
    }

    #[test]
    fn map_user_prompt_skips_ide_synthetic_block() {
        use crate::model::EventKind;
        // VS Code 연동이 user content 배열 앞에 주입하는 <ide_opened_file> 블록은 지시가 아니다
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u6","timestamp":"2026-07-07T10:00:00Z",
            "message":{"role":"user","content":[
              {"type":"text","text":"<ide_opened_file>The user opened the file /home/j/Work/a.md in the IDE. This may or may not be related to the current task.</ide_opened_file>"},
              {"type":"text","text":"리뷰 중에 잠깐 다음 단계 질문\n둘째 줄"}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        let up = evs.iter().find(|e| matches!(e.kind, EventKind::UserPrompt { .. })).unwrap();
        match &up.kind {
            EventKind::UserPrompt { preview } => assert_eq!(preview, "리뷰 중에 잠깐 다음 단계 질문"),
            k => panic!("expected UserPrompt, got {k:?}"),
        }
    }

    #[test]
    fn map_user_line_with_only_synthetic_blocks_yields_no_prompt() {
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u7",
            "message":{"role":"user","content":[
              {"type":"text","text":"<ide_opened_file>The user opened the file /home/j/Work/a.md in the IDE.</ide_opened_file>"}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })));
    }

    #[test]
    fn map_user_string_content_with_synthetic_marker_yields_no_prompt() {
        // 훅/알림 주입은 문자열 content로도 온다 (<system-reminder> 등)
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u8",
            "message":{"role":"user","content":"<system-reminder>background task done</system-reminder>"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })));
    }

    #[test]
    fn map_user_interrupt_markers_yield_no_prompt() {
        // Claude Code가 인터럽트 시 합성하는 user 라인 — 지시가 아니다.
        // 실사용: 43개 파일에 존재, R6 "27개 세션" junk 카드의 원인 (스펙 §1.1-3)
        for text in ["[Request interrupted by user]", "[Request interrupted by user for tool use]"] {
            let line = format!(
                r#"{{"type":"user","sessionId":"s1","uuid":"u9","message":{{"role":"user","content":"{text}"}}}}"#
            );
            let evs = adapter().map(&line, "s1.jsonl", 0);
            assert!(
                !evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })),
                "인터럽트 마커가 프롬프트로 수집되면 안 됨: {text}"
            );
        }
    }

    #[test]
    fn map_compact_summary_line_yields_compaction() {
        use crate::model::EventKind;
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u5","isCompactSummary":true,
            "message":{"role":"user","content":"[compacted]"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::Compaction)));
    }

    #[test]
    fn map_assistant_tool_use_captures_tool_use_id() {
        use crate::model::EventKind;
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","timestamp":"2026-07-07T10:00:00Z",
            "cwd":"C:\\Users\\jibin","message":{"model":"claude-opus-4-8","usage":{"input_tokens":1,"output_tokens":1},
            "content":[{"type":"tool_use","id":"toolu_9","name":"Read","input":{"file_path":"a.rs"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        let tc = evs.iter().find(|e| matches!(e.kind, EventKind::ToolCall { .. })).unwrap();
        match &tc.kind {
            EventKind::ToolCall { tool_use_id, .. } => assert_eq!(tool_use_id.as_deref(), Some("toolu_9")),
            k => panic!("expected ToolCall, got {k:?}"),
        }
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

    #[test]
    fn map_permission_mode_line_yields_event() {
        use crate::model::EventKind;
        // 실측 형태 (mac jsonl): {"type":"permission-mode","permissionMode":"plan","sessionId":"..."}
        let line = r#"{"type":"permission-mode","permissionMode":"plan","sessionId":"s1"}"#;
        let evs = adapter().map(line, "s1.jsonl", 42);
        assert_eq!(evs.len(), 1);
        match &evs[0].kind {
            EventKind::PermissionMode { mode } => assert_eq!(mode, "plan"),
            k => panic!("expected PermissionMode, got {k:?}"),
        }
        // permissionMode 키 부재 → 침묵 (fail-safe)
        let none = adapter().map(r#"{"type":"permission-mode","sessionId":"s1"}"#, "s1.jsonl", 0);
        assert!(none.is_empty());
    }

    #[test]
    fn map_system_compact_boundary_yields_compaction() {
        use crate::model::EventKind;
        // 신형 auto-compact 경계 (공식 문서 형태). trigger 구분은 Windows 실데이터 핀 후 후속 (스펙 §4.1-5)
        let line = r#"{"type":"system","subtype":"compact_boundary","sessionId":"s1",
            "compactMetadata":{"trigger":"auto","preCompactTokens":155000}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::Compaction)));
        // 다른 system subtype은 침묵 (fail-safe)
        let none = adapter().map(r#"{"type":"system","subtype":"turn_duration","sessionId":"s1"}"#, "s1.jsonl", 0);
        assert!(none.is_empty());
    }

    #[test]
    fn map_local_command_stdout_is_not_prompt() {
        // 실측: 세션 첫 user 라인이 "<local-command-stdout>Set model to ..." 로 오염됨 (스펙 §4.1-2)
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u9",
            "message":{"role":"user","content":"<local-command-stdout>Set model to Fable 5</local-command-stdout>"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, crate::model::EventKind::UserPrompt { .. })));
    }

    #[test]
    fn map_user_prompt_with_secret_emits_flag_without_body() {
        use crate::model::EventKind;
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u1",
            "message":{"role":"user","content":"이 키로 배포해줘\nghp_AbCdEf0123456789"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 100);
        let sf = evs.iter().find(|e| matches!(e.kind, EventKind::SecretFlag { .. })).unwrap();
        match &sf.kind {
            EventKind::SecretFlag { pattern_id } => assert_eq!(pattern_id, "github_token"),
            k => panic!("expected SecretFlag, got {k:?}"),
        }
        // 첫 줄이 평문이므로 UserPrompt도 함께 생성됨 (기능 독립)
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::UserPrompt { .. })));
    }

    #[test]
    fn map_bash_command_with_secret_emits_flag() {
        use crate::model::EventKind;
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u2",
            "message":{"model":"claude-opus-4-8","usage":{"input_tokens":1,"output_tokens":1},
            "content":[{"type":"tool_use","id":"t1","name":"Bash",
                        "input":{"command":"export ANTHROPIC_API_KEY=sk-ant-api03-AbCdEfGh123456"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        let flags: Vec<_> = evs.iter().filter(|e| matches!(e.kind, EventKind::SecretFlag { .. })).collect();
        assert_eq!(flags.len(), 1);
        match &flags[0].kind {
            EventKind::SecretFlag { pattern_id } => assert_eq!(pattern_id, "anthropic_api_key"),
            k => panic!("expected SecretFlag, got {k:?}"),
        }
        // ToolCall(Bash)은 생성되되 target에 명령 원문(시크릿)이 남지 않고 마스킹된다.
        let tc = evs.iter().find(|e| matches!(e.kind, EventKind::ToolCall { .. })).unwrap();
        match &tc.kind {
            EventKind::ToolCall { target, .. } => {
                assert_eq!(target.as_deref(), Some(SECRET_REDACTED));
                assert!(!target.as_deref().unwrap().contains("sk-ant-"));
            }
            k => panic!("expected ToolCall, got {k:?}"),
        }
    }

    #[test]
    fn map_bash_command_without_secret_keeps_target() {
        use crate::model::EventKind;
        // 시크릿이 없으면 명령 원문 target은 그대로 보존.
        let line = r#"{"type":"assistant","sessionId":"s1","uuid":"u2",
            "message":{"model":"claude-opus-4-8","usage":{"input_tokens":1,"output_tokens":1},
            "content":[{"type":"tool_use","id":"t1","name":"Bash",
                        "input":{"command":"cargo test --all"}}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        let tc = evs.iter().find(|e| matches!(e.kind, EventKind::ToolCall { .. })).unwrap();
        match &tc.kind {
            EventKind::ToolCall { target, .. } => assert_eq!(target.as_deref(), Some("cargo test --all")),
            k => panic!("expected ToolCall, got {k:?}"),
        }
    }

    #[test]
    fn map_clean_lines_emit_no_secret_flag() {
        let clean_user = r#"{"type":"user","sessionId":"s1","uuid":"u3",
            "message":{"role":"user","content":"토큰 없이 평범한 요청"}}"#;
        assert!(!adapter().map(clean_user, "s.jsonl", 0).iter()
            .any(|e| matches!(e.kind, crate::model::EventKind::SecretFlag { .. })));
    }

    #[test]
    fn map_user_prompt_first_line_secret_suppresses_preview_but_still_flags() {
        use crate::model::EventKind;
        // 첫 줄 자체에 시크릿이 있으면 미리보기(첫 줄 저장)는 침묵하되, SecretFlag는 그대로 방출.
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u12",
            "message":{"role":"user","content":"배포 토큰은 ghp_AbCdEf0123456789 입니다"}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, EventKind::UserPrompt { .. })));
        assert!(evs.iter().any(|e| matches!(&e.kind,
            EventKind::SecretFlag { pattern_id } if pattern_id == "github_token")));
    }

    #[test]
    fn map_tool_result_content_with_secret_does_not_flag() {
        use crate::model::EventKind;
        // tool_result 본문은 에이전트(도구) 측 산출물 — !had_tool_result 가드로 시크릿 스캔 대상에서 제외.
        let line = r#"{"type":"user","sessionId":"s1","uuid":"u13",
            "message":{"role":"user","content":[
              {"type":"tool_result","tool_use_id":"t1","is_error":false,
               "content":"... ghp_AbCdEf0123456789 ..."}]}}"#;
        let evs = adapter().map(line, "s1.jsonl", 0);
        assert!(!evs.iter().any(|e| matches!(e.kind, EventKind::SecretFlag { .. })));
        assert!(evs.iter().any(|e| matches!(e.kind, EventKind::ToolResult { .. })));
    }
}
