//! 채팅 탭 시스템 프롬프트 — 다이어리 페르소나와 동일 인물, 결정론 조립 (스펙 §5).
//! 전송 경계: 트랜스크립트 원문은 절대 포함하지 않는다 — 집계 수치·advice 요약만.
//!
//! 티어 라우팅(코칭 v3 설계): 사용자 질문의 의도를 **결정론적으로** 분류해
//! ① 정량(수치) ② 서사(오늘 근황) ③ 질적 코칭(Tier 2, "깊게 봐줘")로 나눈다.
//! Tier 2는 주간 추세·역량 프로필·findings·고생 세션을 한데 모은 **코칭 브리프**로 답한다.

/// 채팅 의도 — 어떤 컨텍스트로 답할지 결정하는 결정론 분류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatIntent {
    /// 수치 질의 ("이번 주 토큰 얼마?") — 정량 블록 위주
    Quantitative,
    /// 근황/서사 질의 (기본) — 오늘 요약 페르소나 대화
    Narrative,
    /// 질적 코칭 ("깊게 봐줘 / 내 약점 / 어떻게 개선") — Tier 2 브리프
    Coaching,
}

impl ChatIntent {
    pub fn key(&self) -> &'static str {
        match self {
            ChatIntent::Quantitative => "quant",
            ChatIntent::Narrative => "narrative",
            ChatIntent::Coaching => "coaching",
        }
    }
}

/// 질적 코칭 신호 — 있으면 Tier 2. "무엇을 어떻게 개선/성장" 계열.
const COACHING_MARKERS: &[&str] = &[
    "깊게", "깊이", "리뷰", "review", "약점", "개선", "고칠", "고치", "코칭", "coach",
    "피드백", "feedback", "평가", "진단", "분석해", "돌아봐", "돌아보", "회고",
    "배워야", "뭘 배", "무엇을 배", "성장", "습관", "잘하고", "잘 하고", "못하고", "못 하고",
    "어떻게 하면", "어떻게 개선", "어떡하면", "나아지", "더 잘",
];
/// 정량 신호 — 수치를 직접 묻는 계열.
const QUANT_MARKERS: &[&str] =
    &["얼마", "몇 ", "몇개", "몇 개", "몇번", "몇 번", "총 ", "합계", "통계", "how many", "how much", "count"];

/// 키워드 포함 검사. 순수 ASCII 알파벳 키워드는 **단어 경계**를 요구해 오탐을 막는다
/// (예: "review"가 "preview"에, "count"가 "account"에 걸리는 것 방지). 한글 등은 부분일치 그대로.
/// 바이트 인덱스 기반 — 한/영 혼합 문자열에서도 안전(char-index 혼동 없음).
fn contains_keyword(msg: &str, keyword: &str) -> bool {
    if !keyword.is_ascii() || keyword.bytes().any(|b| !b.is_ascii_alphabetic()) {
        return msg.contains(keyword); // 비-ASCII·공백 포함 키워드는 그대로
    }
    let bytes = msg.as_bytes();
    let klen = keyword.len();
    let mut start = 0;
    while let Some(idx) = msg[start..].find(keyword) {
        let abs = start + idx;
        let before_ok = abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric();
        let end = abs + klen;
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// 사용자 메시지 하나의 의도를 결정론적으로 분류. 코칭 신호 우선(질적 도움이 최우선 가치).
pub fn classify_intent(msg: &str) -> ChatIntent {
    let m = msg.to_lowercase();
    if COACHING_MARKERS.iter().any(|k| contains_keyword(&m, k)) {
        return ChatIntent::Coaching;
    }
    if QUANT_MARKERS.iter().any(|k| contains_keyword(&m, k)) {
        return ChatIntent::Quantitative;
    }
    ChatIntent::Narrative
}

/// Tier 2 질적 코칭 브리프 — 여러 신호를 한데 모은 결정론 스냅샷.
/// 전부 파생 수치·요약이며 트랜스크립트 원문은 없다(전송 경계 유지).
pub struct CoachingBrief {
    pub user_name: String,
    /// 이번 주(7일) 세션·입출력 토큰과 지난주 대비 세션 증감(%)
    pub week_sessions: u64,
    pub week_tok_input: u64,
    pub week_tok_output: u64,
    pub week_session_delta_pct: Option<i64>,
    /// 역량 사다리: (라벨, 숙련도, 근거)
    pub profile: Vec<(String, String, String)>,
    /// 지금 배울 것(프론티어): (라벨, 학습 힌트)
    pub frontier: Option<(String, String)>,
    /// 활성 코칭 지적: (무엇이, 어떻게)
    pub findings: Vec<(String, String)>,
    /// 최근 "고생 끝 해결" 세션 수 (질적 회고 후보)
    pub struggle_count: u64,
    /// 모델 믹스: (티어, 토큰)
    pub model_mix: Vec<(String, u64)>,
}

fn mastery_ko(key: &str) -> &'static str {
    match key {
        "mastered" => "숙달",
        "in_progress" => "배우는 중",
        _ => "아직",
    }
}

/// Tier 2 시스템 프롬프트 — 질적 코칭. 브리프의 사실에만 근거하고 구조화된 코칭을 요구한다.
pub fn build_coaching_system_prompt(brief: &CoachingBrief) -> String {
    let delta = match brief.week_session_delta_pct {
        Some(p) if p > 0 => format!(" (지난주 대비 +{p}%)"),
        Some(p) if p < 0 => format!(" (지난주 대비 {p}%)"),
        Some(_) => " (지난주와 비슷)".to_string(),
        None => String::new(),
    };
    let profile_block = if brief.profile.is_empty() {
        "- (아직 역량 데이터가 부족해요)".to_string()
    } else {
        brief
            .profile
            .iter()
            .map(|(label, m, evi)| format!("- {label}: {}{}", mastery_ko(m), if evi.is_empty() { String::new() } else { format!(" — {evi}") }))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let frontier_line = match &brief.frontier {
        Some((label, hint)) => format!("{label} — {hint}"),
        None => "5개 축을 모두 숙달했어요".to_string(),
    };
    let findings_block = if brief.findings.is_empty() {
        "- (지금은 활성 코칭 지적이 없어요)".to_string()
    } else {
        brief
            .findings
            .iter()
            .map(|(detail, action)| format!("- {detail}\n  → {action}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mix_block = if brief.model_mix.is_empty() {
        "- (모델 사용 기록 없음)".to_string()
    } else {
        brief
            .model_mix
            .iter()
            .map(|(tier, tok)| format!("- {tier}: {tok} 토큰"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "당신은 {user}의 AX(에이전트 활용) 튜터입니다. 매일 일기를 쓰는 그 마스코트와 동일 인물로, \
         1인칭·대화체로 사용자를 '주인'이라 부르되, 지금은 **질적 코칭 모드**입니다. \
         \
         답변 방식(반드시): (1) 아래 브리프의 사실·수치에만 근거하고 없는 값은 지어내지 마세요. \
         (2) 짧은 진단 → 근거(무엇이) → **다음 한 걸음**(가장 임팩트 큰 것 하나) 순서로 구조화하세요. \
         (3) 프론티어(지금 배울 것)를 최우선으로 연결하세요. (4) 비난 대신 성장 관점으로, 3~6문장 이내로 간결하게. \
         (5) 모르면 모른다고 하세요.\n\n\
         [이번 주 브리프 — {user}]\n\
         - 세션 {ws}건{delta} · 입력 {wi} · 출력 {wo} 토큰\n\
         - 최근 '고생 끝 해결' 세션 {struggle}건\n\n\
         [역량 사다리]\n{profile_block}\n\
         → 지금 배울 것(프론티어): {frontier_line}\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}\n\n\
         [모델 사용 믹스]\n{mix_block}",
        user = brief.user_name,
        ws = brief.week_sessions,
        wi = brief.week_tok_input,
        wo = brief.week_tok_output,
        struggle = brief.struggle_count,
        delta = delta,
    )
}

/// Tier 2 코칭 브리프를 store에서 결정론적으로 조립. CLI·Tauri 공용 (중복 제거).
/// 전부 파생 수치·요약이며 트랜스크립트 원문은 없다(전송 경계).
pub fn assemble_coaching_brief(store: &crate::store::SqliteStore) -> anyhow::Result<CoachingBrief> {
    use crate::profile::{detect_profile, Dimension};
    let user_name = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER")) // Unix 계열은 USER
        .unwrap_or_else(|_| "주인".into());
    let today = chrono::Local::now().date_naive();
    let d = |n: i64| (today - chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
    // 이번 주(0..6일) vs 지난주(7..13일)
    let (ws, wi, wo, _) = store.range_totals(&d(6), &d(0))?;
    let (pw_sessions, _, _, _) = store.range_totals(&d(13), &d(7))?;
    let week_session_delta_pct = if pw_sessions > 0 {
        Some((ws as i64 - pw_sessions as i64) * 100 / pw_sessions as i64)
    } else {
        None
    };
    // 역량 사다리
    let prof = detect_profile(store)?;
    let profile: Vec<(String, String, String)> = Dimension::all()
        .into_iter()
        .map(|dim| {
            let st = prof.dims.iter().find(|s| s.dimension == dim);
            (
                dim.label_ko().to_string(),
                st.map(|s| s.mastery.key()).unwrap_or("not_started").to_string(),
                st.map(|s| s.evidence.clone()).unwrap_or_default(),
            )
        })
        .collect();
    let frontier = prof
        .frontier()
        .map(|dim| (dim.label_ko().to_string(), dim.learn_hint_ko().to_string()));
    // 활성 코칭 지적 상위 8 → (무엇이, 어떻게)
    let findings: Vec<(String, String)> = store
        .list_findings_current(false)?
        .into_iter()
        .take(8)
        .map(|row| crate::diary::finding_advice(&row.rule_id, &row.evidence, row.est_tokens_saved))
        .collect();
    // "고생 끝 해결" 세션 수 (오류≥3 ∧ 규모≥20 ∧ 회복으로 끝)
    let now = chrono::Utc::now().to_rfc3339();
    let struggle_count =
        store.struggle_sessions(3, 20, &now)?.iter().filter(|s| s.last_result_ok).count() as u64;
    let model_mix = store.model_mix_for_range(Some(&d(6)), &d(0))?;
    Ok(CoachingBrief {
        user_name,
        week_sessions: ws,
        week_tok_input: wi,
        week_tok_output: wo,
        week_session_delta_pct,
        profile,
        frontier,
        findings,
        struggle_count,
        model_mix,
    })
}

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

    #[test]
    fn classify_intent_routes_three_tiers() {
        // 질적 코칭
        assert_eq!(classify_intent("이번 주 내 세션들 깊게 봐줘"), ChatIntent::Coaching);
        assert_eq!(classify_intent("내 약점이 뭐야? 어떻게 개선하지?"), ChatIntent::Coaching);
        assert_eq!(classify_intent("피드백 좀 줘"), ChatIntent::Coaching);
        assert_eq!(classify_intent("나 뭘 배워야 해?"), ChatIntent::Coaching);
        // 정량
        assert_eq!(classify_intent("이번 주 토큰 얼마 썼어?"), ChatIntent::Quantitative);
        assert_eq!(classify_intent("세션 몇 개나 돌렸어?"), ChatIntent::Quantitative);
        // 서사(기본)
        assert_eq!(classify_intent("오늘 어땠어?"), ChatIntent::Narrative);
        assert_eq!(classify_intent("안녕!"), ChatIntent::Narrative);
    }

    #[test]
    fn classify_intent_prioritizes_coaching_over_quant() {
        // 코칭 신호가 있으면 수치 단어가 섞여도 코칭 우선
        assert_eq!(classify_intent("토큰을 얼마나 아꼈는지 말고, 어떻게 개선할지 코칭해줘"), ChatIntent::Coaching);
    }

    #[test]
    fn classify_intent_no_false_positive_on_substring_keywords() {
        // 영어 키워드가 다른 단어의 일부일 때 오탐 금지 (gemini 리뷰):
        assert_eq!(classify_intent("markdown preview 좀 보여줘"), ChatIntent::Narrative); // review ⊄
        assert_eq!(classify_intent("내 account 잔액 알려줘"), ChatIntent::Narrative);     // count ⊄
        assert_eq!(classify_intent("cockroach 사진 찾아줘"), ChatIntent::Narrative);       // coach ⊄
        // 단어로 쓰이면 여전히 코칭
        assert_eq!(classify_intent("please review my week"), ChatIntent::Coaching);
        assert_eq!(classify_intent("how many sessions this week?"), ChatIntent::Quantitative);
    }

    fn sample_brief() -> CoachingBrief {
        CoachingBrief {
            user_name: "jibin".into(),
            week_sessions: 12, week_tok_input: 30000, week_tok_output: 8000,
            week_session_delta_pct: Some(20),
            profile: vec![
                ("모델 리터러시".into(), "mastered".into(), "상위 모델 실질 사용".into()),
                ("자동화".into(), "in_progress".into(), "마찰 신호 없음".into()),
            ],
            frontier: Some(("자동화".into(), "커스텀 커맨드·hooks로 마찰 제거".into())),
            findings: vec![("큰 MCP 결과 반복".into(), "필드 좁히기".into())],
            struggle_count: 2,
            model_mix: vec![("opus".into(), 5000), ("sonnet".into(), 33000)],
        }
    }

    #[test]
    fn coaching_prompt_includes_brief_signals() {
        let p = build_coaching_system_prompt(&sample_brief());
        assert!(p.contains("질적 코칭 모드"));
        assert!(p.contains("다음 한 걸음"));        // 구조화 지시
        assert!(p.contains("12건"));               // 주간 세션
        assert!(p.contains("+20%"));               // 지난주 대비 증감
        assert!(p.contains("고생 끝 해결' 세션 2건")); // 고생 세션
        assert!(p.contains("자동화"));             // 프론티어 라벨
        assert!(p.contains("커스텀 커맨드"));       // 프론티어 힌트
        assert!(p.contains("큰 MCP 결과 반복"));    // finding
        assert!(p.contains("sonnet: 33000"));      // 모델 믹스
        assert!(p.contains("지어내지 마세요"));      // 정밀도의 선
    }

    #[test]
    fn coaching_prompt_handles_empty_and_mastered() {
        let brief = CoachingBrief {
            user_name: "u".into(), week_sessions: 0, week_tok_input: 0, week_tok_output: 0,
            week_session_delta_pct: None, profile: vec![], frontier: None,
            findings: vec![], struggle_count: 0, model_mix: vec![],
        };
        let p = build_coaching_system_prompt(&brief);
        assert!(p.contains("역량 데이터가 부족"));
        assert!(p.contains("모두 숙달"));
        assert!(p.contains("활성 코칭 지적이 없어요"));
    }
}
