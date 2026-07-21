//! R6 반복 지시 → "스킬화할 가치가 있는가" 의미 판정 (PR2 스펙 §4).
//! 채굴(SQL)이 잡은 pending R6를 Engine으로 걸러 precision을 확보한다.
//! 순수 로직(프롬프트·파싱·결과 변환)만 여기 있고, 락·네트워크는 src-tauri가 조립한다.

use crate::diary::engine::Engine;
use crate::skill_draft::DraftContext;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Judgment {
    pub worthy: bool,
    pub reason: String,
    pub suggested_name: String,
}

/// 판정 프롬프트 (정밀도 우선, 한국어). system=기준, user=재료. (스펙 §4.3)
pub fn judgment_prompt(ctx: &DraftContext) -> (String, String) {
    let system = "당신은 Claude Code 사용 습관을 코칭하는 심사관입니다. \
사용자가 여러 세션에서 반복한 지시가 '재사용 가능한 스킬/커맨드로 묶을 가치'가 있는지 판정하세요.\n\
worthy=true: 재사용 가능한 절차·규칙·체크리스트를 담은 지시 — 매번 같은 다단계 작업, 정해진 형식이나 규칙을 요구하는 지시.\n\
worthy=false: 대화 접착제('진행해줘','계속','ㅇㅋ' 등), 일회성·문맥 의존 지시, 단순 질문·피드백, 인사.\n\
정밀도가 최우선입니다 — 확신이 없으면 worthy=false로 판정하세요.\n\
반드시 아래 형태의 JSON 객체 하나만 출력하고, 그 밖의 설명·코드펜스는 붙이지 마세요:\n\
{\"worthy\": true 또는 false, \"reason\": \"판정 이유를 담은 한국어 한 문장\", \"suggested_name\": \"kebab-case 슬러그\"}"
        .to_string();

    let samples = if ctx.sample_prompts.is_empty() {
        "- (변형 표본 없음)".to_string()
    } else {
        ctx.sample_prompts
            .iter()
            .map(|p| format!("- \"{}\"", p.replace('"', "'")))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let tools = if ctx.top_tools.is_empty() {
        "(도구 기록 없음)".to_string()
    } else {
        ctx.top_tools
            .iter()
            .take(6)
            .map(|(t, n)| format!("{t}({n})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let user = format!(
        "대표 지시: \"{rep}\"\n반복된 세션 수: {count}\n변형 표본:\n{samples}\n주요 도구: {tools}",
        rep = ctx.representative.replace('"', "'"),
        count = ctx.session_count,
        samples = samples,
        tools = tools,
    );
    (system, user)
}

/// 엔진 응답에서 엄격 JSON 판정을 추출. 코드펜스·사족을 관대히 벗기되(첫 '{'~마지막 '}'),
/// 필수 필드(worthy·reason·suggested_name)가 없으면 실패로 본다.
pub fn parse_judgment(text: &str) -> Result<Judgment> {
    let start = text.find('{').ok_or_else(|| anyhow!("판정 응답에 JSON 객체 없음"))?;
    let end = text.rfind('}').ok_or_else(|| anyhow!("판정 응답에 JSON 객체 없음"))?;
    if end < start {
        return Err(anyhow!("판정 응답 JSON 경계 불량"));
    }
    let j: Judgment = serde_json::from_str(&text[start..=end])?;
    Ok(j)
}

pub enum JudgeOutcome {
    /// 파싱 성공 — worthy/unworthy 판정 확정.
    Judged(Judgment),
    /// 응답이 왔으나 JSON 파싱 실패 — 인프라 실패가 아님(재시도 대상, attempts 증가).
    Malformed(String),
}

pub struct JudgeResult {
    pub outcome: JudgeOutcome,
    pub tokens: u64,
}

/// 후보 1건 판정 — 엔진 1회 호출. Err = **전송 실패**(엔진 다운·타임아웃)로,
/// 상위는 attempts를 올리지 않고 pending에 남겨 다음 스캔에 재시도한다.
pub fn judge_one(engine: &dyn Engine, ctx: &DraftContext) -> Result<JudgeResult> {
    let (system, user) = judgment_prompt(ctx);
    let out = engine.generate(&system, &user)?; // Err → 전송 실패
    let outcome = match parse_judgment(&out.text) {
        Ok(j) => JudgeOutcome::Judged(j),
        Err(e) => JudgeOutcome::Malformed(e.to_string()),
    };
    Ok(JudgeResult { outcome, tokens: out.tokens_used })
}

/// 판정 결과를 (새 status, judgment_json)로 변환.
/// - Judged(worthy)  → Some("new"),      {worthy,reason,suggested_name,attempts,tokens}
/// - Judged(!worthy) → Some("rejected"), 〃 (영구 캐시)
/// - Malformed       → None(status 유지),{attempts,error,tokens} — rejected로 오캐시 금지
pub fn judgment_record(
    prev_attempts: u32,
    result: &JudgeResult,
) -> (Option<&'static str>, serde_json::Value) {
    let attempts = prev_attempts + 1;
    match &result.outcome {
        JudgeOutcome::Judged(j) => {
            let status = if j.worthy { "new" } else { "rejected" };
            (
                Some(status),
                serde_json::json!({
                    "worthy": j.worthy,
                    "reason": j.reason,
                    "suggested_name": j.suggested_name,
                    "attempts": attempts,
                    "tokens": result.tokens,
                }),
            )
        }
        JudgeOutcome::Malformed(e) => (
            None,
            serde_json::json!({ "attempts": attempts, "error": e, "tokens": result.tokens }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> DraftContext {
        DraftContext {
            representative: "PR 리뷰 코멘트 종합 검토해서 조치해줘".into(),
            session_count: 4,
            sample_prompts: vec!["PR 리뷰 코멘트 정리".into(), "리뷰 코멘트 반영해줘".into()],
            top_tools: vec![("Bash".into(), 12), ("Read".into(), 8)],
        }
    }

    #[test]
    fn prompt_carries_criteria_and_evidence() {
        let (system, user) = judgment_prompt(&ctx());
        assert!(system.contains("worthy"));
        assert!(system.contains("확신"), "정밀도 우선 기준 명시");
        assert!(system.contains("JSON"));
        assert!(user.contains("PR 리뷰 코멘트"));
        assert!(user.contains("4"), "세션 수 포함");
        assert!(user.contains("Bash"), "주요 도구 포함");
    }

    #[test]
    fn parse_accepts_strict_json() {
        let j = parse_judgment(r#"{"worthy":true,"reason":"매번 같은 리뷰 절차","suggested_name":"pr-review-triage"}"#).unwrap();
        assert!(j.worthy);
        assert_eq!(j.suggested_name, "pr-review-triage");
    }

    #[test]
    fn parse_tolerates_codefence_and_prose() {
        let j = parse_judgment("판정 결과:\n```json\n{\"worthy\":false,\"reason\":\"대화 접착제\",\"suggested_name\":\"none\"}\n```").unwrap();
        assert!(!j.worthy);
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(parse_judgment("죄송하지만 판정할 수 없습니다").is_err());
        assert!(parse_judgment(r#"{"worthy":true}"#).is_err(), "필수 필드 누락은 실패");
    }

    use crate::diary::engine::{EngineOutput, MockEngine};

    /// 전송 실패를 흉내내는 엔진.
    struct FailEngine;
    impl Engine for FailEngine {
        fn name(&self) -> String { "fail".into() }
        fn generate(&self, _s: &str, _u: &str) -> anyhow::Result<EngineOutput> {
            Err(anyhow!("connection refused"))
        }
        fn chat(&self, _s: &str, _m: &[crate::diary::engine::ChatMessage]) -> anyhow::Result<EngineOutput> {
            Err(anyhow!("n/a"))
        }
    }

    #[test]
    fn judge_one_worthy_then_record_promotes_to_new() {
        let eng = MockEngine {
            canned: r#"{"worthy":true,"reason":"매번 같은 릴리스 절차","suggested_name":"release-flow"}"#.into(),
        };
        let res = judge_one(&eng, &ctx()).unwrap();
        assert!(matches!(res.outcome, JudgeOutcome::Judged(ref j) if j.worthy));
        assert!(res.tokens > 0, "토큰 계량");
        let (status, rec) = judgment_record(0, &res);
        assert_eq!(status, Some("new"));
        assert_eq!(rec["attempts"], serde_json::json!(1));
        assert_eq!(rec["suggested_name"], serde_json::json!("release-flow"));
    }

    #[test]
    fn judge_one_unworthy_records_rejected() {
        let eng = MockEngine { canned: r#"{"worthy":false,"reason":"대화 접착제","suggested_name":"none"}"#.into() };
        let (status, rec) = judgment_record(0, &judge_one(&eng, &ctx()).unwrap());
        assert_eq!(status, Some("rejected"));
        assert_eq!(rec["worthy"], serde_json::json!(false));
    }

    #[test]
    fn judge_one_malformed_bumps_attempts_keeps_pending() {
        let eng = MockEngine { canned: "판정 불가합니다".into() };
        let res = judge_one(&eng, &ctx()).unwrap();
        assert!(matches!(res.outcome, JudgeOutcome::Malformed(_)));
        let (status, rec) = judgment_record(1, &res);
        assert_eq!(status, None, "형식 불량은 status 유지(pending)");
        assert_eq!(rec["attempts"], serde_json::json!(2), "attempts+1");
        assert!(rec.get("error").is_some());
    }

    #[test]
    fn judge_one_transport_failure_is_err() {
        // 전송 실패는 Err → 상위에서 skip(attempts 미증가)
        assert!(judge_one(&FailEngine, &ctx()).is_err());
    }
}
