//! 세션 원본 트랜스크립트(JSONL) 상세보기용 경량 파서 — 코칭 탭 "세션 상세" 팝업 (스펙 §3 확장).
//! adapter와 달리 토큰이 아니라 사람이 읽을 텍스트를 뽑는다. 관대한 파싱: 하드 실패 금지.

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// 한 줄(user/assistant 턴)의 사람이 읽는 요약.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TranscriptEntry {
    pub ts: Option<String>,
    pub role: String, // "user" | "assistant"
    pub text: String,
    pub tools: Vec<String>, // "Read: src/main.rs" 형태의 tool_use 요약
    pub model: Option<String>,
}

const MAX_TEXT_CHARS: usize = 2000;
const MAX_TOOL_TARGET_CHARS: usize = 80;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

/// JSONL 한 줄 → 엔트리. user/assistant 외 타입·사이드체인·내용 없는 줄(도구 결과만)은 None.
pub fn parse_line(line: &str) -> Option<TranscriptEntry> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ltype = v.get("type").and_then(|x| x.as_str())?;
    if ltype != "user" && ltype != "assistant" {
        return None;
    }
    if v.get("isSidechain").and_then(|x| x.as_bool()).unwrap_or(false) {
        return None; // 서브에이전트 사이드체인은 메인 대화 뷰에서 제외
    }
    let msg = v.get("message")?;

    let mut texts: Vec<String> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    match msg.get("content") {
        Some(Value::String(s)) => {
            if !s.trim().is_empty() {
                texts.push(s.trim().to_string());
            }
        }
        Some(Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(|x| x.as_str()) {
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(|x| x.as_str()) {
                            if !t.trim().is_empty() {
                                texts.push(t.trim().to_string());
                            }
                        }
                    }
                    Some("tool_use") => {
                        let name = b.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                        let input = b.get("input").cloned().unwrap_or(Value::Null);
                        let target = input
                            .get("file_path")
                            .or_else(|| input.get("command"))
                            .or_else(|| input.get("skill"))
                            .or_else(|| input.get("pattern"))
                            .and_then(|x| x.as_str())
                            .map(|t| truncate_chars(t, MAX_TOOL_TARGET_CHARS));
                        tools.push(match target {
                            Some(t) => format!("{name}: {t}"),
                            None => name.to_string(),
                        });
                    }
                    _ => {} // tool_result 등은 노이즈 — 제외
                }
            }
        }
        _ => {}
    }

    if texts.is_empty() && tools.is_empty() {
        return None;
    }
    Some(TranscriptEntry {
        ts: v.get("timestamp").and_then(|x| x.as_str()).map(String::from),
        role: ltype.to_string(),
        text: truncate_chars(&texts.join("\n\n"), MAX_TEXT_CHARS),
        tools,
        model: msg.get("model").and_then(|x| x.as_str()).map(String::from),
    })
}

/// 파일 전체를 엔트리 목록으로. max_entries 초과분은 버린다(팝업 뷰 상한).
pub fn read_transcript(path: &Path, max_entries: usize) -> Result<Vec<TranscriptEntry>> {
    let f = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if let Some(e) = parse_line(&line) {
            out.push(e);
            if out.len() >= max_entries {
                break;
            }
        }
    }
    Ok(out)
}

/// claude_root/projects/*/{session_id}.jsonl 탐색. session_id는 uuid 문자만 허용(경로 주입 방지).
pub fn find_session_file(claude_root: &Path, session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty()
        || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    let projects = claude_root.join("projects");
    let entries = std::fs::read_dir(&projects).ok()?;
    for proj in entries.flatten() {
        let candidate = proj.path().join(format!("{session_id}.jsonl"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_line_extracts_text_tools_model() {
        let line = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-07-06T01:00:00Z",
            "message":{"model":"claude-opus-4-8","content":[
              {"type":"text","text":"파일을 볼게요"},
              {"type":"tool_use","name":"Read","input":{"file_path":"src/main.rs"}},
              {"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}]}}"#;
        let e = parse_line(line).unwrap();
        assert_eq!(e.role, "assistant");
        assert_eq!(e.text, "파일을 볼게요");
        assert_eq!(e.tools, vec!["Read: src/main.rs", "Bash: cargo test"]);
        assert_eq!(e.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(e.ts.as_deref(), Some("2026-07-06T01:00:00Z"));
    }

    #[test]
    fn user_string_content_and_noise_filters() {
        let user = r#"{"type":"user","sessionId":"s1","message":{"content":"버그 고쳐줘"}}"#;
        assert_eq!(parse_line(user).unwrap().text, "버그 고쳐줘");
        // 도구 결과만 있는 user 줄은 노이즈
        let tool_result = r#"{"type":"user","sessionId":"s1","message":{"content":[
            {"type":"tool_result","tool_use_id":"t1","content":"..."}]}}"#;
        assert!(parse_line(tool_result).is_none());
        // 사이드체인·비대화 타입·쓰레기
        let side = r#"{"type":"assistant","isSidechain":true,"message":{"content":"x"}}"#;
        assert!(parse_line(side).is_none());
        assert!(parse_line(r#"{"type":"summary"}"#).is_none());
        assert!(parse_line("garbage").is_none());
    }

    #[test]
    fn long_text_is_truncated() {
        let long = "가".repeat(3000);
        let line = format!(r#"{{"type":"user","message":{{"content":"{long}"}}}}"#);
        let e = parse_line(&line).unwrap();
        assert!(e.text.chars().count() <= MAX_TEXT_CHARS + 1); // +… 문자
        assert!(e.text.ends_with('…'));
    }

    #[test]
    fn read_transcript_caps_and_find_session_file() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("projects").join("d--project-x");
        std::fs::create_dir_all(&proj).unwrap();
        let sid = "abc-123";
        let file = proj.join(format!("{sid}.jsonl"));
        let mut body = String::new();
        for i in 0..5 {
            body.push_str(&format!(
                "{{\"type\":\"user\",\"message\":{{\"content\":\"메시지 {i}\"}}}}\n"
            ));
        }
        std::fs::write(&file, body).unwrap();

        assert_eq!(find_session_file(dir.path(), sid).as_deref(), Some(file.as_path()));
        assert!(find_session_file(dir.path(), "no-such").is_none());
        assert!(find_session_file(dir.path(), "../evil").is_none());

        let all = read_transcript(&file, 100).unwrap();
        assert_eq!(all.len(), 5);
        let capped = read_transcript(&file, 3).unwrap();
        assert_eq!(capped.len(), 3);
        assert_eq!(capped[0].text, "메시지 0");
    }
}
