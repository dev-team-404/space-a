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
}
