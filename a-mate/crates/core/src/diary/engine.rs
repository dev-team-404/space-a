use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub struct EngineOutput {
    pub text: String,
    pub tokens_used: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// 어시스턴트가 낸 툴콜 에코(툴 루프 재요청용). 비면 직렬화 생략.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// role=="tool" 결과 메시지의 대상 툴콜 id. 비면 직렬화 생략.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 엔진에 넘기는 툴 정의(OpenAI function tool로 직렬화).
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 모델이 낸 툴콜 1건.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String, // JSON 문자열
}

/// tool-aware chat 한 턴 결과: 최종 텍스트이거나 툴콜 목록.
#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tokens_used: u64,
}

pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput>;
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput>;

    /// tool-aware chat. 기본 구현은 툴을 무시하고 `chat`을 래핑한다(툴 미지원 엔진 graceful degrade).
    fn chat_with_tools(
        &self,
        system: &str,
        messages: &[ChatMessage],
        _tools: &[ToolDef],
    ) -> Result<ChatTurn> {
        let out = self.chat(system, messages)?;
        Ok(ChatTurn { text: Some(out.text), tool_calls: vec![], tokens_used: out.tokens_used })
    }
}

/// OpenAI chat/completions 응답에서 텍스트 또는 툴콜을 추출한다.
pub fn parse_tool_turn(v: &serde_json::Value) -> ChatTurn {
    let msg = v.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("message"));
    let tool_calls = msg
        .and_then(|m| m.get("tool_calls"))
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    Some(ToolCall {
                        id: tc.get("id")?.as_str()?.to_string(),
                        name: tc.get("function")?.get("name")?.as_str()?.to_string(),
                        arguments: tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}")
                            .to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let text = msg
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let tokens_used = v
        .get("usage")
        .and_then(|u| u.get("total_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    ChatTurn { text, tool_calls, tokens_used }
}

/// ChatMessage를 OpenAI messages 요소로 변환한다(툴콜 에코·tool 결과 포함).
fn chat_message_to_json(m: &ChatMessage) -> serde_json::Value {
    if let Some(tcid) = &m.tool_call_id {
        return serde_json::json!({"role": m.role, "tool_call_id": tcid, "content": m.content});
    }
    if !m.tool_calls.is_empty() {
        let tcs: Vec<serde_json::Value> = m
            .tool_calls
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id, "type": "function",
                    "function": {"name": t.name, "arguments": t.arguments}
                })
            })
            .collect();
        return serde_json::json!({"role": m.role, "content": m.content, "tool_calls": tcs});
    }
    serde_json::json!({"role": m.role, "content": m.content})
}

/// 결정적 테스트용. 실제 호출 없이 정해진 텍스트 반환.
pub struct MockEngine {
    pub canned: String,
}

impl Engine for MockEngine {
    fn name(&self) -> String {
        "mock".to_string()
    }
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput> {
        let approx = ((system.len() + user.len() + self.canned.len()) / 4).max(1) as u64;
        Ok(EngineOutput { text: self.canned.clone(), tokens_used: approx })
    }
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput> {
        let user_len: usize = messages.iter().map(|m| m.content.len()).sum();
        let approx = ((system.len() + user_len + self.canned.len()) / 4).max(1) as u64;
        Ok(EngineOutput { text: self.canned.clone(), tokens_used: approx })
    }
}

/// 사내 on-prem(OpenAI 호환) 또는 OpenAI API. base_url 예: "https://.../v1"
pub struct OpenAiCompatEngine {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiCompatEngine {
    pub fn from_env() -> Option<OpenAiCompatEngine> {
        let base_url = std::env::var("AGENT_MENTOR_ENGINE_URL").ok()?;
        let api_key = std::env::var("AGENT_MENTOR_ENGINE_KEY").unwrap_or_default();
        let model = std::env::var("AGENT_MENTOR_ENGINE_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());
        Some(OpenAiCompatEngine { base_url, api_key, model })
    }

    fn request(&self, messages: serde_json::Value) -> Result<EngineOutput> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.7
        });
        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(60))
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| anyhow!("engine request failed: {e}"))?;

        let v: serde_json::Value = resp.into_json()?;
        let text = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow!("engine response missing choices[0].message.content"))?
            .to_string();
        let tokens_used = v
            .get("usage")
            .and_then(|u| u.get("total_tokens"))
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        Ok(EngineOutput { text, tokens_used })
    }
}

impl Engine for OpenAiCompatEngine {
    fn name(&self) -> String {
        format!("openai-compat:{}", self.model)
    }

    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput> {
        self.request(serde_json::json!([
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]))
    }

    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput> {
        let mut arr = vec![serde_json::json!({"role": "system", "content": system})];
        arr.extend(messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})));
        self.request(serde_json::Value::Array(arr))
    }

    fn chat_with_tools(
        &self,
        system: &str,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> Result<ChatTurn> {
        let mut arr = vec![serde_json::json!({"role": "system", "content": system})];
        for m in messages {
            arr.push(chat_message_to_json(m));
        }
        let tools_json: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": arr,
            "temperature": 0.7,
            "tools": tools_json,
            "tool_choice": "auto",
        });
        let resp = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(60))
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| anyhow!("engine tool request failed: {e}"))?;
        let v: serde_json::Value = resp.into_json()?;
        Ok(parse_tool_turn(&v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_engine_returns_canned_text_and_meters_tokens() {
        let eng = MockEngine { canned: "오늘 주인은 나를 꽤 굴렸다.".into() };
        let out = eng.generate("system prompt", "user brief json").unwrap();
        assert_eq!(out.text, "오늘 주인은 나를 꽤 굴렸다.");
        assert!(out.tokens_used > 0, "mock meters an approximate token count");
        assert_eq!(eng.name(), "mock");
    }

    #[test]
    fn mock_engine_chat_returns_canned_and_meters_tokens() {
        let eng = MockEngine { canned: "안녕 주인".into() };
        let msgs = vec![ChatMessage { role: "user".into(), content: "안녕?".into(), ..Default::default() }];
        let out = eng.chat("system prompt", &msgs).unwrap();
        assert_eq!(out.text, "안녕 주인");
        assert!(out.tokens_used > 0);
    }

    #[test]
    fn parse_tool_turn_reads_tool_calls() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
              "choices":[{"message":{"content":null,"tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"save_memory","arguments":"{\"text\":\"주인은 비건임\"}"}}
              ]}}],
              "usage":{"total_tokens":42}
            }"#,
        )
        .unwrap();
        let turn = parse_tool_turn(&v);
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].name, "save_memory");
        assert_eq!(turn.tool_calls[0].id, "call_1");
        assert!(turn.tool_calls[0].arguments.contains("비건"));
        assert_eq!(turn.tokens_used, 42);
    }

    #[test]
    fn parse_tool_turn_reads_plain_text() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"choices":[{"message":{"content":"기억했어요!"}}],"usage":{"total_tokens":5}}"#,
        )
        .unwrap();
        let turn = parse_tool_turn(&v);
        assert!(turn.tool_calls.is_empty());
        assert_eq!(turn.text.as_deref(), Some("기억했어요!"));
    }

    #[test]
    fn default_chat_with_tools_degrades_to_text() {
        // 기본 구현(MockEngine)은 툴을 무시하고 텍스트만 반환한다.
        let eng = MockEngine { canned: "안녕 주인".into() };
        let msgs = vec![ChatMessage { role: "user".into(), content: "안녕?".into(), ..Default::default() }];
        let turn = eng.chat_with_tools("sys", &msgs, &[]).unwrap();
        assert!(turn.tool_calls.is_empty());
        assert_eq!(turn.text.as_deref(), Some("안녕 주인"));
    }
}
