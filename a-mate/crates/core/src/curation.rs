//! 큐레이션 데이터 — 알려진 플러그인/MCP 용도 사전(스펙 §4.6)과
//! 작업 패턴→추천 스킬 테이블(스펙 §4.5). 결정론 유지: 코드 상수만.
//! 6② 확장: 사내 스킬허브 검색 API는 SkillRecommendationSource 어댑터로 후속(사내망).

/// R12가 탐지하는 작업 패턴. v1은 1개로 시작 (스펙 §4.5).
pub enum WorkPattern {
    /// 대형 구현 세션(file_edits ≥ 임계)인데 Skill 툴콜 0회
    LargeImplNoSkill,
}

pub struct SkillRecommendation {
    /// plugin_inventory의 스킬 이름과 부분 문자열 매칭해 "설치됨" 판정
    pub match_substrings: &'static [&'static str],
    /// 카드에 표기할 스킬 이름
    pub display: &'static str,
}

pub trait SkillRecommendationSource {
    fn recommend(&self, pattern: &WorkPattern) -> Vec<SkillRecommendation>;
}

/// 내장 큐레이션 테이블 소스 (v1 유일 구현)
pub struct BuiltinCurationSource;

impl SkillRecommendationSource for BuiltinCurationSource {
    fn recommend(&self, pattern: &WorkPattern) -> Vec<SkillRecommendation> {
        match pattern {
            WorkPattern::LargeImplNoSkill => vec![
                SkillRecommendation {
                    match_substrings: &["writing-plans"],
                    display: "superpowers:writing-plans",
                },
                SkillRecommendation {
                    match_substrings: &["subagent-driven-development"],
                    display: "superpowers:subagent-driven-development",
                },
                SkillRecommendation {
                    match_substrings: &["brainstorming"],
                    display: "superpowers:brainstorming",
                },
            ],
        }
    }
}

/// 알려진 MCP 서버 → 한 줄 용도 (R1 서사 강화용)
pub fn mcp_server_purpose(server: &str) -> Option<&'static str> {
    match server {
        "playwright" => Some("브라우저 자동화·E2E 테스트"),
        "chrome-devtools" => Some("웹페이지 디버깅·성능 분석"),
        "context7" => Some("라이브러리 최신 문서 조회"),
        "serena" => Some("코드베이스 시맨틱 분석·심볼 편집"),
        _ => None,
    }
}

/// 알려진 플러그인 → 한 줄 용도 (R2 서사 강화용). plugin_key의 `@마켓` 접미는 무시.
pub fn plugin_purpose(plugin_key: &str) -> Option<&'static str> {
    let name = plugin_key.split('@').next().unwrap_or(plugin_key);
    match name {
        "superpowers" => Some("브레인스토밍→플랜→TDD 개발 워크플로"),
        "frontend-design" => Some("UI 디자인·시각 완성도 작업"),
        "vercel" => Some("Vercel 배포·Next.js 개발"),
        "chrome-devtools-mcp" => Some("웹페이지 디버깅·성능 분석"),
        "playwright" => Some("브라우저 자동화·E2E 테스트"),
        "context7" => Some("라이브러리 최신 문서 조회"),
        "codex" => Some("Codex 보조 에이전트 위임"),
        "claude-md-management" => Some("CLAUDE.md 유지보수"),
        "claude-hud" => Some("상태줄(statusline) 표시"),
        _ => None,
    }
}

/// R20 시크릿 패턴 큐레이션 (코칭 v3 §11.2). (pattern_id, 접두). 버전업 가능한 상수.
/// 주의: 매칭된 본문은 절대 저장·로그하지 않는다 — pattern_id만 반환.
const SECRET_PREFIXES: &[(&str, &str)] = &[
    ("anthropic_api_key", "sk-ant-"),
    ("github_token", "ghp_"),
    ("github_token", "gho_"),
    ("github_token", "github_pat_"),
    ("slack_token", "xoxb-"),
    ("slack_token", "xoxp-"),
    ("slack_token", "xoxa-"),
    ("google_api_key", "AIza"),
];

/// 접두 뒤 토큰 문자([A-Za-z0-9_-]) 연속 길이 — 짧은 언급(문서 인용) 오탐 억제용.
fn token_len_after(text: &str, start: usize) -> usize {
    text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .count()
}

/// text에서 감지된 시크릿 pattern_id 목록 (정렬·dedup). 본문은 반환하지 않는다.
pub fn find_secret_patterns(text: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (id, prefix) in SECRET_PREFIXES {
        // 첫 발생만 보면 앞선 짧은 언급(오탐 억제 대상)에 가려 뒤의 실제 키를 놓친다 —
        // 유효 토큰이 붙은 발생을 찾을 때까지 모든 위치를 훑는다.
        let mut search_start = 0;
        while let Some(rel) = text[search_start..].find(prefix) {
            let pos = search_start + rel;
            if token_len_after(text, pos + prefix.len()) >= 8 {
                out.push(id);
                break;
            }
            search_start = pos + prefix.len();
        }
    }
    // 개인키 블록: BEGIN 헤더에 PRIVATE KEY 명시된 경우만
    if let Some(pos) = text.find("-----BEGIN ") {
        if text[pos..].contains("PRIVATE KEY-----") {
            out.push("private_key_block");
        }
    }
    // AWS Access Key ID: "AKIA" + 대문자/숫자 16자 (마찬가지로 모든 발생을 훑는다)
    let mut search_start = 0;
    while let Some(rel) = text[search_start..].find("AKIA") {
        let pos = search_start + rel;
        let rest = &text.as_bytes()[pos + 4..];
        if rest.len() >= 16
            && rest[..16].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            out.push("aws_access_key");
            break;
        }
        search_start = pos + 4;
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_dictionaries_hit_and_miss() {
        assert!(mcp_server_purpose("playwright").is_some());
        assert!(mcp_server_purpose("unknown-server-xyz").is_none());
        assert!(plugin_purpose("frontend-design@claude-plugins-official").is_some()); // @뒤 무시
        assert!(plugin_purpose("frontend-design").is_some());
        assert!(plugin_purpose("no-such-plugin@mp").is_none());
    }

    #[test]
    fn builtin_source_recommends_planning_skills_for_large_impl() {
        let recs = BuiltinCurationSource.recommend(&WorkPattern::LargeImplNoSkill);
        assert!(!recs.is_empty());
        assert!(recs.iter().any(|r| r.match_substrings.iter().any(|s| s.contains("writing-plans"))));
    }

    #[test]
    fn secret_patterns_hit_known_key_shapes() {
        assert_eq!(find_secret_patterns("here sk-ant-api03-AbCdEfGh123456 end"), vec!["anthropic_api_key"]);
        assert_eq!(find_secret_patterns("token=ghp_AbCdEf0123456789"), vec!["github_token"]);
        assert_eq!(find_secret_patterns("pat github_pat_11ABCDEFG_xyz123"), vec!["github_token"]);
        assert_eq!(find_secret_patterns("AKIAIOSFODNN7EXAMPLE"), vec!["aws_access_key"]);
        assert_eq!(find_secret_patterns("xoxb-123456789012-abcdef"), vec!["slack_token"]);
        assert_eq!(find_secret_patterns("key=AIzaSyA1234567890abcdefghij"), vec!["google_api_key"]);
        assert_eq!(find_secret_patterns("xoxa-2-123456789012-abcdef"), vec!["slack_token"]);
        assert_eq!(find_secret_patterns("-----BEGIN RSA PRIVATE KEY-----\nMII..."), vec!["private_key_block"]);
        // 복수 종류 → 정렬된 dedup 목록
        assert_eq!(
            find_secret_patterns("ghp_AbCdEf0123456789 and sk-ant-api03-AbCdEfGh123456"),
            vec!["anthropic_api_key", "github_token"]
        );
    }

    #[test]
    fn secret_patterns_detect_real_key_after_short_prefix_mention() {
        // 앞선 짧은 접두 언급(오탐 억제 대상) 뒤에 실제 키가 오면 놓치면 안 된다 —
        // 첫 발생만 검사하던 회귀 방지.
        assert_eq!(
            find_secret_patterns("ghp_ 접두를 쓰세요; 실제키 ghp_AbCdEf0123456789"),
            vec!["github_token"]
        );
        assert_eq!(
            find_secret_patterns("AKIA는 접두일 뿐; 실제 AKIAIOSFODNN7EXAMPLE"),
            vec!["aws_access_key"]
        );
    }

    #[test]
    fn secret_patterns_suppress_short_or_prose_mentions() {
        // 접두 뒤 토큰이 짧으면(문서 언급 수준) 침묵 — 오탐 억제
        assert!(find_secret_patterns("환경변수 이름은 sk-ant- 로 시작해요").is_empty());
        assert!(find_secret_patterns("ghp_ 접두 토큰을 쓰세요").is_empty());
        assert!(find_secret_patterns("AKIA만 적으면 안 돼요").is_empty());
        assert!(find_secret_patterns("AIza 로 시작하는 키").is_empty());
        assert!(find_secret_patterns("-----BEGIN CERTIFICATE-----").is_empty()); // PRIVATE KEY 아님
        assert!(find_secret_patterns("평범한 문장").is_empty());
    }
}
