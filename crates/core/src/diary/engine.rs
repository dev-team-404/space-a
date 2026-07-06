use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub struct EngineOutput {
    pub text: String,
    pub tokens_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub trait Engine {
    fn name(&self) -> String;
    fn generate(&self, system: &str, user: &str) -> Result<EngineOutput>;
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<EngineOutput>;
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
        let msgs = vec![ChatMessage { role: "user".into(), content: "안녕?".into() }];
        let out = eng.chat("system prompt", &msgs).unwrap();
        assert_eq!(out.text, "안녕 주인");
        assert!(out.tokens_used > 0);
    }
}
