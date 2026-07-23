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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
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
    // 데이터 디렉터리 오버라이드(멀티 인스턴스 테스트)는 별도 정체성 = 별도 마스코트.
    // 같은 PC에서 두 인스턴스를 띄워도 로봇이 갈리도록 시드에 섞는다.
    match std::env::var("AGENT_MENTOR_DATA_DIR") {
        Ok(d) if !d.trim().is_empty() => format!("{host}|{user}|{d}"),
        _ => format!("{host}|{user}"),
    }
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

/// MBTI 4글자 정규화 — 유효하면 대문자 4글자, 아니면 None. 빈값도 None(미설정).
/// 각 자리는 해당 이분법의 두 글자 중 하나여야 한다(E/I, S/N, T/F, J/P).
pub fn normalize_mbti(raw: &str) -> Option<String> {
    let up = raw.trim().to_ascii_uppercase();
    let b = up.as_bytes();
    if b.len() != 4 {
        return None;
    }
    let ok = matches!(b[0], b'E' | b'I')
        && matches!(b[1], b'S' | b'N')
        && matches!(b[2], b'T' | b'F')
        && matches!(b[3], b'J' | b'P');
    ok.then_some(up)
}

/// 부분집합에서 uuid 해시 바이트로 하나 고른다(결정론).
fn pick(group: &[u8], byte: u8) -> u8 {
    group[(byte as usize) % group.len()]
}

/// 프로필(uuid + 선택 MBTI) → RobotSpec.
/// MBTI가 있으면 4축을 슬롯 부분집합으로 제약하고, uuid 해시가 그 안에서 세부를 고른다
/// (같은 MBTI는 비슷, 사람마다 다름). MBTI가 없으면 `robot_spec_for(uuid)`와 동일.
pub fn robot_spec_from_profile(uuid: &str, mbti: Option<&str>) -> RobotSpec {
    let base = robot_spec_for(uuid);
    let Some(m) = mbti.and_then(normalize_mbti) else {
        return base;
    };
    let d = Sha256::digest(uuid.as_bytes());
    let m = m.as_bytes();
    // S/N → head (실용=각진 / 추상=둥근)
    let head = if m[1] == b'S' { pick(&[1, 5, 4], d[1]) } else { pick(&[0, 2, 3], d[1]) };
    // E/I → eyes(생기/차분) + palette(밝은/무광)
    let (eyes, palette) = if m[0] == b'E' {
        (pick(&[1, 3, 5], d[2]), pick(&[0, 2, 3, 5], d[5]))
    } else {
        (pick(&[0, 2, 4], d[2]), pick(&[1, 4, 6, 7], d[5]))
    };
    // T/F → body (각진 아머 / 부드러운 라운드)
    let body = if m[2] == b'T' { pick(&[1, 5, 4], d[3]) } else { pick(&[0, 3, 2], d[3]) };
    // J/P → arms (정돈 / 여유)
    let arms = if m[3] == b'J' { pick(&[0, 3, 1], d[4]) } else { pick(&[2, 4, 5], d[4]) };
    RobotSpec { antenna: base.antenna, head, eyes, body, arms, palette }
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

/// 오늘 요약 + 활성 findings 사실 블록 — 오늘의 한마디·잡담 프롬프트가 공용하는 사실 재료.
/// (#1 최종 리뷰의 중복 지적 해소 — mascot.rs 안에서만 공용, chat.rs와는 독립 진화 유지)
fn facts_block(ctx: &crate::chat::ChatContext) -> String {
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
        "[오늘({date}) 요약]\n\
         - 세션 {sessions}건 · 입력 {tin} · 출력 {tout} 토큰\n\
         - 절약 가능 총량(누적): {saved} 토큰\n\n\
         [활성 코칭 지적 (무엇이 → 어떻게)]\n{findings_block}",
        date = ctx.date,
        sessions = ctx.session_count,
        tin = ctx.tok_input,
        tout = ctx.tok_output,
        saved = ctx.est_tokens_saved_total,
    )
}

/// 오늘의 한마디 생성 시스템 프롬프트 — 채팅과 동일 인물(1인칭·주인·능청) +
/// 오늘 요약(chat과 같은 사실 블록) + voice_guidance + "짧은 한 문장" 지시 (스펙 §5).
pub fn build_daily_line_prompt(ctx: &crate::chat::ChatContext) -> String {
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         {facts}\n\n\
         오늘 하루의 기분이나 재치를 담아 짧은 한 문장(40자 이내)으로 표현하세요. \
         대화가 아니라 오늘을 한마디로 요약하는 혼잣말입니다. 딱 한 문장만 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        facts = facts_block(ctx),
    )
}

/// 잡담 풀 크기 — 스캔당 LLM 1회 호출로 배치 생성하는 잡담 개수 (스펙 §2).
pub const CHATTER_POOL_SIZE: usize = 5;

/// 오늘 세션이 이 이상이면 "그만 좀 하고 쉬어라" 코믹 지시를 넣는다 (스펙 묶음 B, 조정 가능).
pub const CHATTER_REST_SESSIONS: u64 = 5;

/// 근무 맥락 코믹 지시 — 신호(주말·연속세션·장시간)가 있을 때만 소재 블록 생성, 없으면 빈 문자열.
/// 위로가 아니라 능청·놀림 톤(다이어리 A5의 anti-monotony 결과 일관).
fn comic_directives(ctx: &crate::chat::ChatContext, work: &crate::diary::WorkContext) -> String {
    let mut items: Vec<String> = Vec::new();
    if work.is_weekend {
        items.push(
            "- 오늘은 주말인데 주인이 또 나와서 일하고 있다 — \"주말에 또 나왔어? 일중독이야ㅋㅋ\" 같은 능청."
                .to_string(),
        );
    }
    if ctx.session_count >= CHATTER_REST_SESSIONS {
        items.push(format!(
            "- 오늘 세션이 벌써 {}건 — \"그만 좀 하고 쉬었다 와라\" 같은 잔소리.",
            ctx.session_count
        ));
    }
    if work.long_work {
        items.push(format!(
            "- 오늘 몰입 시간이 {}시간 — \"오래 붙어 있었네, 배터리 방전되겠다\" 같은 챙김.",
            work.active_hours
        ));
    }
    if items.is_empty() {
        return String::new();
    }
    format!(
        "\n\n[오늘 근무 맥락 — 코믹 소재]\n{}\n\
         위 근무 맥락은 사실이니 잡담 일부에 능청스럽게 녹이세요. \
         걱정 어투 말고 웃기게 — 다마고치가 주인을 놀리는 톤.",
        items.join("\n")
    )
}

/// 잡담 풀 생성 시스템 프롬프트 — 오늘의 한마디와 동일 인물·동일 사실 재료,
/// 지시만 "가벼운 잡담 N개"로 다름 (스펙 §5). 코칭 조언 채널(realtime_advice)과의
/// 역할 분리를 프롬프트에 명시한다.
pub fn build_chatter_prompt(
    ctx: &crate::chat::ChatContext,
    work: &crate::diary::WorkContext,
    n: usize,
) -> String {
    format!(
        "당신은 {user}의 AI 코딩 여정을 함께하는 마스코트 에이전트입니다. \
         매일 일기를 쓰는 그 다마고치와 동일 인물로, 1인칭으로 가볍고 능청스럽게 \
         사용자를 '주인'이라고 부릅니다. \
         \
         {voice} \
         \
         정밀도의 선(반드시 지킬 것): 아래 오늘 요약의 사실과 수치에만 근거하고, \
         요약에 없는 구체적 수치를 지어내지 마세요.\n\n\
         {facts}{comic}\n\n\
         위 요약을 재료로, 상주 마스코트가 가끔 툭 던질 가벼운 잡담·혼잣말을 {n}개 만드세요. \
         코칭 조언이나 보고처럼 굴지 마세요(조언은 다른 채널이 합니다). \
         한 줄에 하나씩, 각 40자 이내로, 번호·불릿·따옴표 없이 출력하세요.",
        user = ctx.user_name,
        voice = crate::diary::voice_guidance(),
        facts = facts_block(ctx),
        comic = comic_directives(ctx, work),
    )
}

/// LLM 출력에서 잡담 줄을 방어적으로 추출 — trim, 선두 불릿/번호 제거,
/// 감싼 따옴표 한 겹 제거, 빈 줄 제거, 최대 max_n개 (스펙 §5 파싱 내성).
pub fn parse_chatter_lines(raw: &str, max_n: usize) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            let mut l = line.trim();
            for p in ["- ", "• ", "* "] {
                if let Some(rest) = l.strip_prefix(p) {
                    l = rest;
                    break;
                }
            }
            l = strip_leading_number(l).trim_start();
            let l = strip_wrapping_quotes(l).trim();
            if l.is_empty() { None } else { Some(l.to_string()) }
        })
        .take(max_n)
        .collect()
}

/// "1. " / "2) " 류 선두 번호 매김 제거 — 번호가 아니면 원문 그대로.
fn strip_leading_number(s: &str) -> &str {
    let rest = s.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() < s.len() {
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return r;
        }
    }
    s
}

/// 생성 텍스트를 감싼 따옴표 한 겹 제거 — LLM이 문장을 따옴표로 감싸 반환할 때
/// UI(초상 밑 `“…”`)에서 이중 따옴표가 되는 것을 방지한다. 매칭되는 쌍일 때만 벗기고,
/// `strip_prefix`/`strip_suffix`라 멀티바이트 UTF-8 경계에서도 안전(패닉 없음).
fn strip_wrapping_quotes(s: &str) -> &str {
    for (open, close) in [('"', '"'), ('\'', '\''), ('“', '”'), ('‘', '’')] {
        if let Some(inner) = s.strip_prefix(open).and_then(|x| x.strip_suffix(close)) {
            return inner.trim();
        }
    }
    s
}

/// 오늘의 한마디 계산 — store 접근 없음, 네트워크만. 호출자가 락 밖에서 부른다
/// (diary::render_diary 선례). 반환: None=재생성 불필요(fp 동일, skip) /
/// Some((text, fp))=이 값으로 캐시하라.
/// - fp가 캐시와 동일: None(skip). idle 상태의 불필요한 재-upsert도 여기서 걸러진다.
/// - 오늘 활동 0건(session_count==0): 엔진 호출 없이 정적 문구.
/// - 그 외: 엔진으로 오늘 한 문장 생성(앞뒤 공백·감싼 따옴표 제거).
pub fn compute_daily_line(
    engine: &dyn crate::diary::engine::Engine,
    ctx: &crate::chat::ChatContext,
    cached_fp: Option<&str>,
) -> anyhow::Result<Option<(String, String)>> {
    let fp = facts_fingerprint(ctx);
    if cached_fp == Some(fp.as_str()) {
        return Ok(None);
    }
    if ctx.session_count == 0 {
        return Ok(Some((static_daily_line().to_string(), fp)));
    }
    let system = build_daily_line_prompt(ctx);
    let raw = engine.generate(&system, "")?.text;
    let text = strip_wrapping_quotes(raw.trim()).to_string();
    Ok(Some((text, fp)))
}

/// 잡담 풀 계산 — store 접근 없음, 네트워크만. 호출자가 락 밖에서 부른다
/// (compute_daily_line 선례). 반환: None=재생성 불필요(fp 동일, skip) /
/// Some((lines, fp))=이 값으로 캐시하라.
/// - fp가 캐시와 동일: None(skip).
/// - 오늘 활동 0건(session_count==0): 엔진 호출 없이 빈 풀(정적 폴백은 프론트 담당).
/// - 그 외: 엔진 1회 호출로 잡담 N개 배치 생성·파싱(전부 실패면 빈 풀 캐시).
pub fn compute_chatter_pool(
    engine: &dyn crate::diary::engine::Engine,
    ctx: &crate::chat::ChatContext,
    work: &crate::diary::WorkContext,
    cached_fp: Option<&str>,
) -> anyhow::Result<Option<(Vec<String>, String)>> {
    let fp = facts_fingerprint(ctx);
    if cached_fp == Some(fp.as_str()) {
        return Ok(None);
    }
    if ctx.session_count == 0 {
        return Ok(Some((Vec::new(), fp)));
    }
    let system = build_chatter_prompt(ctx, work, CHATTER_POOL_SIZE);
    let raw = engine.generate(&system, "")?.text;
    Ok(Some((parse_chatter_lines(&raw, CHATTER_POOL_SIZE), fp)))
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

    #[test]
    fn mbti_normalize_accepts_valid_rejects_invalid() {
        assert_eq!(normalize_mbti("intj").as_deref(), Some("INTJ"));
        assert_eq!(normalize_mbti(" ENFP ").as_deref(), Some("ENFP"));
        assert_eq!(normalize_mbti(""), None); // 미설정
        assert_eq!(normalize_mbti("INT"), None); // 길이
        assert_eq!(normalize_mbti("XNTJ"), None); // 1자리 X
        assert_eq!(normalize_mbti("IXTJ"), None); // 2자리 X (S/N 아님)
        assert_eq!(normalize_mbti("INTX"), None); // 4자리 X (J/P 아님)
    }

    #[test]
    fn profile_without_mbti_equals_uuid_spec() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(robot_spec_from_profile(uuid, None), robot_spec_for(uuid));
        assert_eq!(robot_spec_from_profile(uuid, Some("")), robot_spec_for(uuid));
        assert_eq!(robot_spec_from_profile(uuid, Some("bad")), robot_spec_for(uuid));
    }

    #[test]
    fn mbti_constrains_slots_to_expected_groups() {
        // 여러 uuid에 대해 INTJ면 head/body/arms/eyes/palette가 항상 지정 그룹 안에 든다.
        for i in 0..30 {
            let uuid = format!("uuid-{i}");
            let s = robot_spec_from_profile(&uuid, Some("INTJ"));
            assert!([0u8, 2, 3].contains(&s.head), "N → 둥근 head: {}", s.head); // N
            assert!([1u8, 5, 4].contains(&s.body), "T → 아머 body: {}", s.body); // T
            assert!([0u8, 3, 1].contains(&s.arms), "J → 정돈 arms: {}", s.arms); // J
            assert!([0u8, 2, 4].contains(&s.eyes), "I → 차분 eyes: {}", s.eyes); // I
            assert!([1u8, 4, 6, 7].contains(&s.palette), "I → 무광 palette: {}", s.palette); // I
        }
    }

    #[test]
    fn same_mbti_varies_by_uuid() {
        // 같은 MBTI라도 uuid가 다르면 세부가 갈린다 — 여러 표본에서 서로 다른 스펙이 2종 이상 나온다.
        use std::collections::HashSet;
        let set: HashSet<_> = (0..20)
            .map(|i| robot_spec_from_profile(&format!("uuid-{i}"), Some("ENFP")))
            .collect();
        assert!(set.len() > 1, "같은 MBTI라도 uuid로 세부가 달라야 함 (distinct={})", set.len());
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
            memories: vec![],
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
    fn compute_skips_when_idle_and_fingerprint_matches() {
        // idle(session_count==0)이라도 캐시된 fp가 이미 idle-fp와 같으면 재-upsert 없이 skip.
        // (idle-fp로 저장된 text는 항상 정적 문구이므로 skip해도 캐시 상태 동일)
        let eng = MockEngine { canned: "안 나와야 함".into() };
        let c = ctx(0, 0, 0, 2);
        let fp = facts_fingerprint(&c); // "0|0|0|2"
        assert_eq!(compute_daily_line(&eng, &c, Some(&fp)).unwrap(), None);
    }

    #[test]
    fn compute_trims_and_strips_wrapping_quotes() {
        // LLM이 앞뒤 공백·감싼 따옴표를 붙여 반환해도 정제 — UI 이중 따옴표 방지.
        let eng = MockEngine { canned: "  \"오늘 좀 굴렀다\"  ".into() };
        let out = compute_daily_line(&eng, &ctx(3, 100, 200, 1), None).unwrap();
        assert_eq!(out, Some(("오늘 좀 굴렀다".to_string(), "3|100|200|1".to_string())));
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

#[cfg(test)]
mod chatter_tests {
    use super::*;
    use crate::chat::ChatContext;

    fn ctx(session_count: u64, tin: u64, tout: u64, n_findings: usize) -> ChatContext {
        ChatContext {
            user_name: "jibin".into(),
            date: "2026-07-10".into(),
            session_count,
            tok_input: tin,
            tok_output: tout,
            est_tokens_saved_total: 4200,
            findings: (0..n_findings)
                .map(|i| (format!("detail {i}"), format!("action {i}")))
                .collect(),
            memories: vec![],
        }
    }

    fn work(is_weekend: bool, active_hours: f64, long_work: bool) -> crate::diary::WorkContext {
        crate::diary::WorkContext { is_weekend, is_holiday: false, active_hours, long_work }
    }

    #[test]
    fn chatter_prompt_carries_persona_facts_voice_and_directives() {
        let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &Default::default(), 5);
        assert!(p.contains("주인"));                         // 페르소나 호칭
        assert!(p.contains("jibin"));                        // 유저명
        assert!(p.contains("3건"));                          // 오늘 세션 수(사실)
        assert!(p.contains(crate::diary::voice_guidance())); // voice_guidance verbatim
        assert!(p.contains("잡담"));                         // 잡담 지시
        assert!(p.contains("5개"));                          // 개수 N
        assert!(p.contains("40자"));                         // 길이 상한
        assert!(p.contains("지어내지 마세요"));              // 정밀도의 선
        assert!(p.contains("조언"));                         // "코칭 조언처럼 굴지 말 것"
        assert!(p.contains("detail 0"));                     // findings 블록 포함
    }

    #[test]
    fn chatter_prompt_weekend_adds_comic_directive() {
        let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &work(true, 2.0, false), 5);
        assert!(p.contains("[오늘 근무 맥락"));   // 코믹 소재 블록 헤더
        assert!(p.contains("주말"));              // 주말 신호
        assert!(p.contains("일중독"));            // 능청 예시 문구
        assert!(!p.contains("쉬었다 와라"));      // 세션 3건 → 쉬어라 지시 없음
    }

    #[test]
    fn chatter_prompt_rest_directive_at_session_threshold() {
        // 경계: CHATTER_REST_SESSIONS(5) 이상이면 on, 미만이면 off
        let on = build_chatter_prompt(&ctx(5, 100, 200, 1), &work(false, 2.0, false), 5);
        assert!(on.contains("5건"));              // 오늘 세션 수(사실) 인용
        assert!(on.contains("쉬었다 와라"));      // 잔소리 지시
        let off = build_chatter_prompt(&ctx(4, 100, 200, 1), &work(false, 2.0, false), 5);
        assert!(!off.contains("쉬었다 와라"));
    }

    #[test]
    fn chatter_prompt_long_work_adds_care_directive() {
        let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &work(false, 7.5, true), 5);
        assert!(p.contains("7.5시간"));           // 몰입 시간(사실) 인용
        assert!(p.contains("배터리"));            // 다마고치 능청 예시(묶음 A A5 톤과 일관)
    }

    #[test]
    fn chatter_prompt_without_signals_has_no_comic_block() {
        // 무신호(평일·세션 적음·짧은 몰입) → 코믹 블록 자체가 없어 기존 프롬프트와 동일 골격
        let p = build_chatter_prompt(&ctx(3, 100, 200, 1), &work(false, 2.0, false), 5);
        assert!(!p.contains("근무 맥락"));
    }

    #[test]
    fn parse_keeps_clean_lines_up_to_max() {
        let raw = "오늘 세션 셋. 손이 빨랐다\n커밋은 자주, 후회는 짧게\n토큰 아낀 날";
        assert_eq!(
            parse_chatter_lines(raw, 5),
            vec![
                "오늘 세션 셋. 손이 빨랐다".to_string(),
                "커밋은 자주, 후회는 짧게".to_string(),
                "토큰 아낀 날".to_string(),
            ]
        );
        // max_n 초과는 절단
        assert_eq!(parse_chatter_lines("a\nb\nc", 2), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_strips_bullets_numbers_quotes_and_blanks() {
        let raw = "- 불릿 잡담\n2. 번호 잡담\n\n  \n\"따옴표 잡담\"\n* 별표 잡담\n3) 괄호번호 잡담";
        assert_eq!(
            parse_chatter_lines(raw, 10),
            vec![
                "불릿 잡담".to_string(),
                "번호 잡담".to_string(),
                "따옴표 잡담".to_string(),
                "별표 잡담".to_string(),
                "괄호번호 잡담".to_string(),
            ]
        );
    }

    #[test]
    fn parse_returns_empty_for_garbage() {
        assert_eq!(parse_chatter_lines("", 5), Vec::<String>::new());
        assert_eq!(parse_chatter_lines("  \n\n\t\n\"\"", 5), Vec::<String>::new());
    }

    use crate::diary::engine::MockEngine;

    #[test]
    fn compute_pool_generates_and_parses_when_active_and_uncached() {
        let eng = MockEngine { canned: "오늘 세션 셋, 좀 굴렀다\n- 커밋은 자주\n\"토큰 아낀 날\"".into() };
        let out = compute_chatter_pool(&eng, &ctx(3, 100, 200, 1), &Default::default(), None).unwrap();
        assert_eq!(
            out,
            Some((
                vec![
                    "오늘 세션 셋, 좀 굴렀다".to_string(),
                    "커밋은 자주".to_string(),
                    "토큰 아낀 날".to_string(),
                ],
                "3|100|200|1".to_string()
            ))
        );
    }

    #[test]
    fn compute_pool_skips_when_fingerprint_matches_cache() {
        let eng = MockEngine { canned: "안 나와야 함".into() };
        let c = ctx(3, 100, 200, 1);
        let fp = facts_fingerprint(&c);
        assert_eq!(compute_chatter_pool(&eng, &c, &Default::default(), Some(&fp)).unwrap(), None);
    }

    #[test]
    fn compute_pool_returns_empty_without_engine_when_idle() {
        // 활동 0건 → 엔진 미호출·빈 풀 캐시 (canned가 파싱돼 나오면 엔진이 불렸다는 뜻이라 실패)
        let eng = MockEngine { canned: "엔진이 불렸다면 이게 나온다".into() };
        let out = compute_chatter_pool(&eng, &ctx(0, 0, 0, 2), &Default::default(), None).unwrap();
        assert_eq!(out, Some((Vec::new(), "0|0|0|2".to_string())));
    }

    #[test]
    fn compute_pool_caches_empty_when_output_is_garbage() {
        // 전부 파싱 실패 → 빈 풀 + fp 캐시 (다음 스캔까지 재시도 안 함, 프론트는 정적 폴백)
        let eng = MockEngine { canned: "  \n\n".into() };
        let out = compute_chatter_pool(&eng, &ctx(3, 100, 200, 1), &Default::default(), None).unwrap();
        assert_eq!(out, Some((Vec::new(), "3|100|200|1".to_string())));
    }
}
