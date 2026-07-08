//! 마스코트 로봇 절차 생성 — 안정적 ID 해시로 파츠 조합을 결정한다.
//! 파츠 도트 데이터 자체는 프론트(TS)에 있고, 여기선 "어떤 파츠 조합인지"만 결정.
//! 그리고 마스코트 보이스(오늘의 한마디, #1) — chat 컨텍스트·voice_guidance로 오늘 하루를 한 문장 생성.
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const ANTENNA_VARIANTS: u8 = 6;
pub const HEAD_VARIANTS: u8 = 6;
pub const EYES_VARIANTS: u8 = 6;
pub const BODY_VARIANTS: u8 = 6;
pub const ARMS_VARIANTS: u8 = 6;
pub const PALETTE_VARIANTS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RobotSpec {
    pub antenna: u8,
    pub head: u8,
    pub eyes: u8,
    pub body: u8,
    pub arms: u8,
    pub palette: u8,
}

/// 호스트명+사용자명 — 계정 없이 사용자마다 안정적인 시드.
pub fn stable_identity() -> String {
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".into());
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    format!("{host}|{user}")
}

pub fn robot_spec_for(identity: &str) -> RobotSpec {
    let d = Sha256::digest(identity.as_bytes());
    RobotSpec {
        antenna: d[0] % ANTENNA_VARIANTS,
        head: d[1] % HEAD_VARIANTS,
        eyes: d[2] % EYES_VARIANTS,
        body: d[3] % BODY_VARIANTS,
        arms: d[4] % ARMS_VARIANTS,
        palette: d[5] % PALETTE_VARIANTS,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// 오늘의 한마디 (마스코트 보이스 #1) — 채팅과 동일 인물이 오늘 하루를 한 문장으로.
// 사실 기반은 chat::ChatContext(오늘 요약), 자연스러움은 diary::voice_guidance() 재사용.
// #3 상주봇 주기 말풍선이 나중에 이 프롬프트/정적 문구를 재사용한다.

/// 오늘 활동 0건일 때 LLM 없이 캐시하는 고정 폴백 문구 (스펙 §2).
pub const STATIC_DAILY_LINE: &str = "오늘은 널널하네. 근데 좀 심심;;;";

pub fn static_daily_line() -> &'static str {
    STATIC_DAILY_LINE
}

/// 사실 지문 — 이 값이 바뀌었거나 캐시가 없을 때만 한마디를 재생성한다 (스펙 §2·§3).
pub fn facts_fingerprint(ctx: &crate::chat::ChatContext) -> String {
    format!(
        "{}|{}|{}|{}",
        ctx.session_count,
        ctx.tok_input,
        ctx.tok_output,
        ctx.findings.len()
    )
}

/// 오늘의 한마디 생성 시스템 프롬프트 — 채팅과 동일 인물(1인칭·주인·능청) +
/// 오늘 요약(chat과 같은 사실 블록) + voice_guidance + "짧은 한 문장" 지시 (스펙 §5).
pub fn build_daily_line_prompt(ctx: &crate::chat::ChatContext) -> String {
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
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         [오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}\n\n\
         오늘 하루의 기분이나 재치를 담아 짧은 한 문장(40자 이내)으로 표현하세요. \
         대화가 아니라 오늘을 한마디로 요약하는 혼잣말입니다. 딱 한 문장만 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
    )
}

/// 오늘의 한마디 계산 — store 접근 없음, 네트워크만. 호출자가 락 밖에서 부른다
/// (diary::render_diary 선례). 반환: None=재생성 불필요(fp 동일, skip) /
/// Some((text, fp))=이 값으로 캐시하라.
/// - 오늘 활동 0건(session_count==0): 엔진 호출 없이 정적 문구.
/// - fp가 캐시와 동일: None(skip).
/// - 그 외: 엔진으로 오늘 한 문장 생성.
pub fn compute_daily_line(
    engine: &dyn crate::diary::engine::Engine,
    ctx: &crate::chat::ChatContext,
    cached_fp: Option<&str>,
) -> anyhow::Result<Option<(String, String)>> {
    let fp = facts_fingerprint(ctx);
    if ctx.session_count == 0 {
        return Ok(Some((static_daily_line().to_string(), fp)));
    }
    if cached_fp == Some(fp.as_str()) {
        return Ok(None);
    }
    let system = build_daily_line_prompt(ctx);
    let text = engine.generate(&system, "")?.text;
    Ok(Some((text, fp)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_deterministic_and_in_range() {
        let a = robot_spec_for("HOSTA|alice");
        let b = robot_spec_for("HOSTA|alice");
        assert_eq!(a, b, "같은 identity → 같은 스펙");
        assert!(a.antenna < ANTENNA_VARIANTS && a.head < HEAD_VARIANTS
            && a.eyes < EYES_VARIANTS && a.body < BODY_VARIANTS
            && a.arms < ARMS_VARIANTS && a.palette < PALETTE_VARIANTS);
    }

    #[test]
    fn different_identity_differs() {
        // SHA-256 기반이므로 이 두 입력은 최소 한 슬롯이 다르다 (사전 확인된 페어).
        assert_ne!(robot_spec_for("HOSTA|alice"), robot_spec_for("HOSTB|bob"));
    }

    #[test]
    fn identity_uses_env_or_fallback() {
        let id = stable_identity();
        assert!(id.contains('|'));
    }
}

#[cfg(test)]
mod daily_line_tests {
    use super::*;
    use crate::chat::ChatContext;

    fn ctx(session_count: u64, tin: u64, tout: u64, n_findings: usize) -> ChatContext {
        ChatContext {
            user_name: "jibin".into(),
            date: "2026-07-08".into(),
            session_count,
            tok_input: tin,
            tok_output: tout,
            est_tokens_saved_total: 4200,
            findings: (0..n_findings)
                .map(|i| (format!("detail {i}"), format!("action {i}")))
                .collect(),
        }
    }

    #[test]
    fn static_daily_line_is_fixed_idle_copy() {
        assert_eq!(static_daily_line(), "오늘은 널널하네. 근데 좀 심심;;;");
    }

    #[test]
    fn fingerprint_reflects_facts_and_is_stable() {
        assert_eq!(facts_fingerprint(&ctx(3, 100, 200, 1)), "3|100|200|1");
        // 같은 사실 → 같은 fp
        assert_eq!(facts_fingerprint(&ctx(3, 100, 200, 1)), facts_fingerprint(&ctx(3, 100, 200, 1)));
        // 세션 수 / findings 수가 바뀌면 fp 달라짐
        assert_ne!(facts_fingerprint(&ctx(3, 100, 200, 1)), facts_fingerprint(&ctx(4, 100, 200, 1)));
        assert_ne!(facts_fingerprint(&ctx(3, 100, 200, 1)), facts_fingerprint(&ctx(3, 100, 200, 2)));
    }

    #[test]
    fn prompt_carries_persona_facts_voice_and_one_line_directive() {
        let p = build_daily_line_prompt(&ctx(3, 100, 200, 1));
        assert!(p.contains("주인"));                         // 페르소나 호칭
        assert!(p.contains("jibin"));                        // 유저명
        assert!(p.contains("3건"));                          // 오늘 세션 수(사실)
        assert!(p.contains(crate::diary::voice_guidance())); // voice_guidance 그대로 주입
        assert!(p.contains("한 문장"));                      // 한 문장 지시
        assert!(p.contains("40자"));                         // 길이 상한
        assert!(p.contains("지어내지 마세요"));              // 정밀도의 선
    }

    use crate::diary::engine::MockEngine;

    #[test]
    fn compute_generates_when_active_and_uncached() {
        let eng = MockEngine { canned: "오늘 주인이 나를 꽤 굴렸다".into() };
        let out = compute_daily_line(&eng, &ctx(3, 100, 200, 1), None).unwrap();
        assert_eq!(
            out,
            Some(("오늘 주인이 나를 꽤 굴렸다".to_string(), "3|100|200|1".to_string()))
        );
    }

    #[test]
    fn compute_skips_when_fingerprint_matches_cache() {
        let eng = MockEngine { canned: "안 나와야 함".into() };
        let c = ctx(3, 100, 200, 1);
        let fp = facts_fingerprint(&c);
        assert_eq!(compute_daily_line(&eng, &c, Some(&fp)).unwrap(), None);
    }

    #[test]
    fn compute_uses_static_line_without_engine_when_idle() {
        // 활동 0건 → 엔진을 부르지 않고 정적 문구. canned(≠정적)가 나오면 엔진이 호출됐다는 뜻이라 실패.
        let eng = MockEngine { canned: "엔진이 불렸다면 이게 나온다".into() };
        let out = compute_daily_line(&eng, &ctx(0, 0, 0, 2), None).unwrap();
        assert_eq!(
            out,
            Some(("오늘은 널널하네. 근데 좀 심심;;;".to_string(), "0|0|0|2".to_string()))
        );
    }
}
