//! 채팅 탭 시스템 프롬프트 — 다이어리 페르소나와 동일 인물, 결정론 조립 (스펙 §5).
//! 전송 경계: 트랜스크립트 원문은 절대 포함하지 않는다 — 집계 수치·advice 요약만.

pub struct ChatContext {
    pub user_name: String,
    pub date: String,
    pub session_count: u64,
    pub tok_input: u64,
    pub tok_output: u64,
    pub est_tokens_saved_total: u64,
    /// (detail, suggested_action) — 활성 findings 상위 N개
    pub findings: Vec<(String, String)>,
}

pub fn build_chat_system_prompt(ctx: &ChatContext) -> String {
    let findings_block = if ctx.findings.is_empty() {
        "- (지금은 활성 코칭 지적이 없어요)".to_string()
    } else {
        ctx.findings
            .iter()
            .map(|(detail, action)| format!("- {detail}\n  → {action}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 대화하며 \
         사용자를 '주인'이라고 부릅니다. 답은 짧고 대화체로. \
         \
         정밀도의 선(반드시 지킬 것): 아래 컨텍스트의 사실과 수치에만 근거해 답하고, \
         컨텍스트에 없는 구체적 수치를 지어내지 마세요. 모르면 모른다고 말하세요. \
         코칭 지적에 대해 물으면 그 지적의 근거(무엇이)와 개선 방향(어떻게)을 쉽게 풀어 설명하세요.\n\n\
         [오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}",
        user = ctx.user_name,
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_prompt_includes_persona_summary_and_findings() {
        let ctx = ChatContext {
            user_name: "jibin".into(),
            date: "2026-07-06".into(),
            session_count: 3,
            tok_input: 100,
            tok_output: 200,
            est_tokens_saved_total: 4200,
            findings: vec![("playwright가 상주하는데 호출 0회".into(), "제거하면 아껴요".into())],
        };
        let p = build_chat_system_prompt(&ctx);
        assert!(p.contains("주인"));           // 페르소나 호칭
        assert!(p.contains("jibin"));          // 유저명
        assert!(p.contains("2026-07-06"));     // 오늘 날짜
        assert!(p.contains("3건"));            // 세션 수
        assert!(p.contains("4200"));           // 절약 가능 총량
        assert!(p.contains("playwright가 상주하는데 호출 0회")); // finding detail
        assert!(p.contains("→ 제거하면 아껴요"));                // suggested_action
        assert!(p.contains("지어내지 마세요")); // 정밀도의 선
    }

    #[test]
    fn chat_prompt_empty_findings_says_none() {
        let ctx = ChatContext {
            user_name: "u".into(), date: "2026-07-06".into(),
            session_count: 0, tok_input: 0, tok_output: 0,
            est_tokens_saved_total: 0, findings: vec![],
        };
        assert!(build_chat_system_prompt(&ctx).contains("활성 코칭 지적이 없어요"));
    }
}
