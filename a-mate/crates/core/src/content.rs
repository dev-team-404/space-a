//! 콘텐츠 큐레이션 — 외부/내장 지식을 정규화하고, 역량 프로필로 관련도를 매긴다.
//! (킥오프 `docs/brainstroming/2026-07-14-content-curation-kickoff.md`)
//!
//! 설계 원칙 준수:
//! - 정밀도의 선: 관련도 스코어링은 결정론(태그·프론티어 매칭). LLM 없음.
//! - 번들/피드 분리: 내장 팁(T3)엔 불변 뼈대·URL만. 변하는 사실은 피드(T1/T2)가 채운다.
//! - curation.rs::SkillRecommendationSource의 일반화.

use crate::profile::{CompetencyProfile, Dimension, Mastery};
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ItemKind {
    Tip,  // T3 내장 커리큘럼 팁 (사다리 축에 배치)
    News, // T1/T2 피드 (changelog·행사 등, 태그 게이트)
}

impl ItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Tip => "tip",
            ItemKind::News => "news",
        }
    }
}

/// 스코어링 대상 정규화 단위. 내장 팁도 피드도 이걸로 환원된다.
#[derive(Debug, Clone, Serialize)]
pub struct ContentItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub body: String,
    pub source_url: Option<String>,
    /// 사다리 축(팁). None이면 축 무관(뉴스) — active_tags로만 매칭.
    pub dimension: Option<Dimension>,
    /// 태그 게이트/증거 매칭용. 예: ["mcp"], ["changelog"].
    pub trigger_tags: Vec<String>,
    /// 소스 고유 우선순위(동점 시 정렬 안정화).
    pub base_priority: i64,
}

/// 외부/내장 소스의 공통 인터페이스. 어댑터 하나 추가 = 소스 하나 추가.
pub trait ContentSource {
    fn id(&self) -> &str;
    fn fetch(&self) -> Result<Vec<ContentItem>>;
}

// ─────────────────────────────────────────────────────────────────────────
// T3 — 내장 팁 카탈로그 (사다리 × 공식 커리큘럼). 네트워크 0에서 동작.
// 본문에 모델명·가격·강좌 개수 같은 "변하는 사실" 하드코딩 금지 (원리만).
// URL은 팁 주제별 공식 문서 딥링크. 2026-07-15 code.claude.com/docs 인덱스(llms.txt)로
// 슬러그 직접 검증. 강좌 홈(skilljar) 대신 mcp/memory/hooks/sub-agents 등 구체 페이지로 연결.
// ─────────────────────────────────────────────────────────────────────────

pub struct BuiltinTipsSource;

fn tip(id: &str, dim: Dimension, tags: &[&str], title: &str, body: &str, url: &str) -> ContentItem {
    ContentItem {
        id: id.into(),
        kind: ItemKind::Tip,
        title: title.into(),
        body: body.into(),
        source_url: Some(url.into()),
        dimension: Some(dim),
        trigger_tags: tags.iter().map(|s| s.to_string()).collect(),
        base_priority: 0,
    }
}

impl ContentSource for BuiltinTipsSource {
    fn id(&self) -> &str {
        "builtin-tips"
    }
    fn fetch(&self) -> Result<Vec<ContentItem>> {
        use Dimension::*;
        Ok(vec![
            // Lv0 모델 리터러시
            tip("T-L0-1", ModelLiteracy, &["model"],
                "잔심부름엔 굳이 상위 모델 아니어도 돼요",
                "파일 이름 바꾸기 같은 단순 작업까지 최상위 모델로 돌리면 토큰만 태워요. 작업 난이도에 맞춰 모델 티어를 고르는 게 첫 걸음입니다.",
                "https://code.claude.com/docs/en/model-config"),
            tip("T-L0-2", ModelLiteracy, &["model"],
                "에이전트는 챗봇이 아니라 '루프'예요",
                "Claude Code는 계획→도구 실행→관찰을 반복하는 에이전틱 루프로 돕습니다. 이 구조를 알면 왜 컨텍스트·권한이 중요한지 감이 와요.",
                "https://code.claude.com/docs/en/overview"),
            tip("T-L0-3", ModelLiteracy, &["model", "plan"],
                "복잡한 작업은 Plan Mode로 먼저 계획하게 하세요",
                "바로 코드부터 짜게 하면 헤맬 수 있어요. 먼저 계획을 세우게 하고(Plan Mode) 승인한 뒤 실행하면 토큰도 아끼고 결과도 좋아집니다.",
                "https://code.claude.com/docs/en/interactive-mode"),
            tip("T-L0-4", ModelLiteracy, &["model"],
                "자동 수락(auto-accept)은 신뢰가 쌓인 다음에",
                "처음엔 승인 모드로 무엇을 하는지 보고, 흐름이 익으면 자동 수락으로 속도를 올리세요. 순서를 지키면 사고를 줄이면서 빨라져요.",
                "https://code.claude.com/docs/en/permissions"),
            // Lv1 컨텍스트 위생
            tip("T-L1-1", ContextHygiene, &["mcp"],
                "안 쓰는 MCP는 대화 시작 전부터 토큰을 깔아요",
                "연결만 해두고 안 쓰는 MCP는 매 세션 도구 정의로 수천 토큰을 상주 소모합니다. 안 쓰면 정리하는 게 이득이에요.",
                "https://code.claude.com/docs/en/mcp"),
            tip("T-L1-2", ContextHygiene, &["claudemd"],
                "CLAUDE.md로 매번 같은 설명을 아끼세요",
                "프로젝트 규칙·경로·관례를 CLAUDE.md에 적어두면 세션마다 다시 설명할 필요가 없어요. Explore→Plan→Code→Commit 흐름의 기반입니다.",
                "https://code.claude.com/docs/en/memory"),
            tip("T-L1-3", ContextHygiene, &["context"],
                "긴 세션은 /compact로 컨텍스트를 정리하세요",
                "한 세션이 길어지면 예전 대화가 토큰을 계속 먹어요. 맥락을 요약해 눌러담는 정리 명령으로 창을 가볍게 유지하면 비용이 줄어요.",
                "https://code.claude.com/docs/en/commands"),
            tip("T-L1-4", ContextHygiene, &["context"],
                "독립적인 새 작업은 새 세션에서",
                "관련 없는 작업을 한 세션에 몰아넣으면 앞 맥락이 계속 따라다녀요. 주제가 바뀌면 세션을 나누는 게 컨텍스트 위생의 기본입니다.",
                "https://code.claude.com/docs/en/common-workflows"),
            // Lv2 스킬 재사용
            tip("T-L2-1", SkillReuse, &["skill"],
                "같은 걸 세 번 설명했다면 스킬로 만드세요",
                "반복하는 작업 방식은 SKILL.md로 한 번 가르쳐두면 다음부터 자동 적용돼요. '반복을 멈추고 한 번만 가르치기'가 핵심입니다.",
                "https://code.claude.com/docs/en/skills"),
            tip("T-L2-2", SkillReuse, &["skill"],
                "스킬 vs CLAUDE.md vs hooks, 언제 뭘?",
                "항상 적용할 규칙은 CLAUDE.md, 조건부로 불러올 절차는 스킬, 자동 실행 훅은 hooks. 역할을 구분하면 컨텍스트가 깔끔해져요.",
                "https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview"),
            tip("T-L2-3", SkillReuse, &["skill"],
                "스킬은 progressive disclosure로 가볍게",
                "스킬 본문을 다 싣지 말고 필요할 때만 불러오도록 쪼개면 상주 컨텍스트가 줄어요. '필요한 만큼만 펼치기'가 스킬 설계의 핵심입니다.",
                "https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices"),
            tip("T-L2-4", SkillReuse, &["skill"],
                "잘 만든 스킬은 팀에 공유하세요",
                "저장소·플러그인·설정으로 스킬을 배포하면 팀 전체가 같은 방식으로 일하게 돼요. 개인의 노하우가 조직 자산이 되는 지점입니다.",
                "https://code.claude.com/docs/en/plugins"),
            // Lv3 자동화
            tip("T-L3-1", Automation, &["permission"],
                "매번 거부→승인 반복이면 권한을 미리 허용하세요",
                "같은 도구를 계속 승인하고 있다면 설정에 사전 허용을 넣어 마찰을 없앨 수 있어요. 반복 승인은 시간·토큰 낭비입니다.",
                "https://code.claude.com/docs/en/settings"),
            tip("T-L3-2", Automation, &["hooks"],
                "포맷·검사는 hooks로 자동화하세요",
                "저장 후 포맷터, 커밋 전 린트 같은 반복 명령은 hooks로 자동 실행할 수 있어요. 손으로 시키던 걸 파이프라인에 맡기는 단계입니다.",
                "https://code.claude.com/docs/en/hooks"),
            tip("T-L3-3", Automation, &["command"],
                "자주 치는 지시는 커스텀 커맨드로",
                "매번 길게 설명하는 반복 작업은 커스텀 슬래시 커맨드로 묶어두면 한 번에 불러와요. 반복 입력이 곧 자동화 후보입니다.",
                "https://code.claude.com/docs/en/skills"),
            tip("T-L3-4", Automation, &["github"],
                "코드 리뷰는 GitHub 연동으로 자동화",
                "PR마다 사람이 도는 대신 GitHub 통합으로 자동 리뷰를 붙일 수 있어요. 반복되는 검토 흐름을 파이프라인에 태우는 단계입니다.",
                "https://code.claude.com/docs/en/code-review"),
            // Lv4 오케스트레이션
            tip("T-L4-1", Orchestration, &["subagent"],
                "긴 세션은 서브에이전트로 컨텍스트를 나누세요",
                "독립적인 조사·구현은 서브에이전트에 위임하면 메인 컨텍스트가 깨끗하게 유지돼요. 긴 작업일수록 효과가 큽니다.",
                "https://code.claude.com/docs/en/sub-agents"),
            tip("T-L4-2", Orchestration, &["subagent"],
                "언제 위임하고 언제 직접 할지 판단하기",
                "모든 걸 서브에이전트로 쪼갤 필요는 없어요. 컨텍스트 격리가 이득인 작업만 위임하는 설계 감각이 다음 레벨입니다.",
                "https://code.claude.com/docs/en/workflows"),
            tip("T-L4-3", Orchestration, &["subagent"],
                "서브에이전트엔 구조화된 출력·에러 처리를",
                "위임한 결과가 들쭉날쭉하면 오히려 손해예요. 출력 형식과 실패 처리를 정해두면 위임이 안정적으로 굴러갑니다.",
                "https://code.claude.com/docs/en/sub-agents"),
        ])
    }
}

// ─────────────────────────────────────────────────────────────────────────
// T1 — Claude Code changelog 피드 (실제 네트워크 fetch, pull-only).
// 관대한 파싱: 실패해도 하드 에러 아님 — 빈 벡터/부분 결과 반환(호출부가 계속 진행).
// ─────────────────────────────────────────────────────────────────────────

pub struct ClaudeChangelogSource {
    pub url: String,
    pub max_items: usize,
}

impl Default for ClaudeChangelogSource {
    fn default() -> Self {
        ClaudeChangelogSource {
            url: "https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md"
                .into(),
            max_items: 5,
        }
    }
}

impl ClaudeChangelogSource {
    /// 마크다운 CHANGELOG를 관대하게 파싱: `## <version>` 헤더 + 뒤따르는 첫 bullet들.
    pub fn parse_markdown(&self, md: &str) -> Vec<ContentItem> {
        let mut out = Vec::new();
        let mut cur_ver: Option<String> = None;
        let mut bullets: Vec<String> = Vec::new();

        let flush = |ver: &Option<String>, bullets: &[String], out: &mut Vec<ContentItem>| {
            let Some(ver) = ver else { return };
            if bullets.is_empty() {
                return;
            }
            let body = bullets.join(" · ");
            out.push(ContentItem {
                id: format!("cc-changelog-{ver}"),
                kind: ItemKind::News,
                title: format!("Claude Code {ver} 업데이트"),
                body,
                source_url: Some(
                    "https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md".into(),
                ),
                dimension: None,
                trigger_tags: vec!["changelog".into(), "claude-code".into()],
                base_priority: 10, // 최신 소식은 살짝 가산
            });
        };

        for line in md.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("## ") {
                // 새 버전 헤더 → 직전 블록 확정
                flush(&cur_ver, &bullets, &mut out);
                bullets.clear();
                if out.len() >= self.max_items {
                    return out;
                }
                cur_ver = Some(rest.trim().to_string());
            } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                if cur_ver.is_some() && bullets.len() < 4 {
                    bullets.push(rest.trim().to_string());
                }
            }
        }
        flush(&cur_ver, &bullets, &mut out);
        out.truncate(self.max_items);
        out
    }
}

impl ContentSource for ClaudeChangelogSource {
    fn id(&self) -> &str {
        "claude-changelog"
    }
    fn fetch(&self) -> Result<Vec<ContentItem>> {
        // ureq는 이미 core 의존성. 실패는 상위에서 warn 처리(하드 실패 금지).
        let body = ureq::get(&self.url).call()?.into_string()?;
        Ok(self.parse_markdown(&body))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 스코어링 — 프론티어 부스트 + 마스터 억제 + 태그 게이트 (결정론).
// ─────────────────────────────────────────────────────────────────────────

/// 팁을 닫으면 그 축(dimension)이 조용해지는 기간(일). list_content 쿨다운.
pub const CONTENT_COOLDOWN_DAYS: f64 = 14.0;

pub const SCORE_SUPPRESS: i64 = -1000;
pub const SCORE_FRONTIER_BOOST: i64 = 500;
pub const SCORE_TAG_MATCH: i64 = 300;
pub const SCORE_TAG_MISS: i64 = -600;

/// 이 아이템을 지금 이 사용자에게 보여줄 가치. 클수록 상단. 0 미만은 숨김 후보.
pub fn score(item: &ContentItem, profile: &CompetencyProfile) -> i64 {
    match item.dimension {
        // 사다리 팁: 프론티어 정렬
        Some(dim) => {
            let mastery = profile.mastery(dim);
            if mastery == Mastery::Mastered {
                return SCORE_SUPPRESS + item.base_priority; // 이미 잘함 → 침묵
            }
            let frontier = profile.frontier();
            match frontier {
                Some(f) if f == dim => SCORE_FRONTIER_BOOST + item.base_priority, // 딱 다음 단계
                Some(f) => {
                    // 프론티어보다 먼 미래 축은 유예(거리만큼 감점)
                    let dist = dim.ladder_index() as i64 - f.ladder_index() as i64;
                    100 - dist.max(0) * 60 + item.base_priority
                }
                None => SCORE_SUPPRESS + item.base_priority, // 전부 마스터
            }
        }
        // 뉴스/태그 게이트: 관심 태그와 겹칠 때만 노출
        None => {
            let hit = item
                .trigger_tags
                .iter()
                .any(|t| profile.active_tags.contains(t));
            if hit || item.trigger_tags.iter().any(|t| t == "changelog") {
                // changelog는 전원 관심사(신기능) — 소소하게 노출 허용
                SCORE_TAG_MATCH + item.base_priority
            } else {
                SCORE_TAG_MISS + item.base_priority
            }
        }
    }
}

/// 여러 소스의 아이템을 프로필로 스코어링해 내림차순 정렬. (id, item, score)
pub fn rank(items: Vec<ContentItem>, profile: &CompetencyProfile) -> Vec<(ContentItem, i64)> {
    let mut scored: Vec<(ContentItem, i64)> =
        items.into_iter().map(|it| { let s = score(&it, profile); (it, s) }).collect();
    // 점수 내림차순, 동점은 id 오름차순으로 안정화
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
    scored
}

/// LLM 코칭용 프롬프트(system, user). 사용자 실측 근거(personal)를 반드시 녹여
/// "일반론"이 아니라 이 사람 데이터에 기반한 조언이 나오게 한다. (2) LLM 레이어.
pub fn coach_prompt(title: &str, body: &str, personal: Option<&str>) -> (String, String) {
    let system = "당신은 사용자의 AI 코딩(Claude Code) 습관을 코칭하는 멘토입니다. \
        반드시 사용자의 실제 로그 데이터를 근거로, 일반론이 아니라 이 사람에게 맞는 조언을 \
        존댓말로 1~2문장(120자 이내) 한국어로 쓰세요. 데이터 수치를 자연스럽게 인용하고, \
        인사말·따옴표·과장 없이 핵심만."
        .to_string();
    let data = personal.unwrap_or("(개인 데이터 없음 — 일반 원칙만)");
    let user = format!(
        "코칭 주제: {title}\n일반 설명: {body}\n이 사용자의 실제 데이터: {data}\n\n\
         위 데이터를 인용해 이 사용자만을 위한 코칭 1~2문장을 써주세요."
    );
    (system, user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{CompetencyProfile, DimState};
    use std::collections::BTreeSet;

    fn profile_with(frontier_missing: &[Dimension], tags: &[&str]) -> CompetencyProfile {
        // frontier_missing에 든 축만 NotStarted, 나머지 Mastered
        let dims = Dimension::all()
            .into_iter()
            .map(|d| DimState {
                dimension: d,
                mastery: if frontier_missing.contains(&d) {
                    Mastery::NotStarted
                } else {
                    Mastery::Mastered
                },
                evidence: String::new(),
            })
            .collect();
        CompetencyProfile {
            dims,
            active_tags: tags.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
            total_events: 1,
        }
    }

    #[test]
    fn builtin_catalog_covers_all_five_dimensions() {
        let tips = BuiltinTipsSource.fetch().unwrap();
        for d in Dimension::all() {
            assert!(
                tips.iter().any(|t| t.dimension == Some(d)),
                "축 {:?}에 팁이 없음", d
            );
        }
    }

    #[test]
    fn coach_prompt_grounds_in_user_data_and_runs_via_engine() {
        use crate::diary::engine::{Engine, MockEngine};
        let (system, user) = coach_prompt(
            "안 쓰는 MCP는 대화 시작 전부터 토큰을 깔아요",
            "안 쓰면 정리하는 게 이득이에요.",
            Some("당신 로그: 미사용 MCP `chrome-devtools`가 상주 ~432K토큰"),
        );
        // LLM에 사용자 실데이터가 반드시 전달돼야 한다 (일반론 방지).
        assert!(user.contains("chrome-devtools"), "user 프롬프트에 실데이터: {user}");
        assert!(user.contains("안 쓰는 MCP"), "주제 포함: {user}");
        assert!(!system.is_empty());
        // 엔진을 통해 실제로 코칭 문장이 나온다 (mock으로 계약 검증).
        let eng = MockEngine {
            canned: "chrome-devtools MCP가 상주 토큰을 꽤 먹고 있어요, 안 쓰면 정리해보세요.".into(),
        };
        let out = eng.generate(&system, &user).unwrap();
        assert!(!out.text.is_empty());
    }

    #[test]
    fn tips_deep_link_to_topic_specific_docs() {
        let tips = BuiltinTipsSource.fetch().unwrap();
        let url = |id: &str| -> String {
            tips.iter().find(|t| t.id == id).unwrap().source_url.clone().unwrap()
        };
        // 각 팁은 자기 주제의 구체적 공식 문서로 가야 한다 (강좌 홈이 아니라).
        assert!(url("T-L1-1").contains("/mcp"), "MCP 팁 -> {}", url("T-L1-1"));
        assert!(url("T-L1-2").contains("/memory"), "CLAUDE.md 팁 -> {}", url("T-L1-2"));
        assert!(url("T-L3-2").contains("/hooks"), "hooks 팁 -> {}", url("T-L3-2"));
        assert!(url("T-L4-1").contains("/sub-agents"), "서브에이전트 팁 -> {}", url("T-L4-1"));
        assert!(url("T-L2-1").contains("/skills"), "스킬 팁 -> {}", url("T-L2-1"));
        assert!(url("T-L3-4").contains("/code-review"), "코드리뷰 팁 -> {}", url("T-L3-4"));
        // 구식 강좌 홈(skilljar)으로 가는 팁이 남아있으면 안 된다.
        assert!(
            tips.iter().all(|t| !t.source_url.as_deref().unwrap_or("").contains("skilljar")),
            "아직 skilljar 강좌 홈 링크가 남아있음"
        );
        // 과거엔 고유 URL이 4개뿐이었다 — 이제 주제별로 충분히 분산돼야 한다.
        let distinct: BTreeSet<_> = tips.iter().filter_map(|t| t.source_url.clone()).collect();
        assert!(distinct.len() >= 10, "고유 URL이 {}개뿐 — 여전히 뭉쳐있음", distinct.len());
        // 전부 https 형식.
        assert!(tips
            .iter()
            .all(|t| t.source_url.as_deref().unwrap_or("").starts_with("https://")));
    }

    #[test]
    fn frontier_tip_beats_future_tip_beats_mastered_tip() {
        // 프론티어 = ContextHygiene (Lv1). ModelLiteracy는 마스터, Automation은 미래.
        let prof = profile_with(&[Dimension::ContextHygiene, Dimension::Automation], &[]);
        let tips = BuiltinTipsSource.fetch().unwrap();
        let get = |dim: Dimension| {
            let it = tips.iter().find(|t| t.dimension == Some(dim)).unwrap();
            score(it, &prof)
        };
        let mastered = get(Dimension::ModelLiteracy);
        let frontier = get(Dimension::ContextHygiene);
        let future = get(Dimension::Automation);
        assert!(frontier > future, "프론티어({frontier}) > 미래({future})");
        assert!(future > mastered, "미래({future}) > 마스터({mastered})");
        assert!(mastered < 0, "마스터 축은 억제되어 음수여야: {mastered}");
    }

    #[test]
    fn rank_puts_frontier_tip_on_top() {
        let prof = profile_with(&[Dimension::SkillReuse], &[]);
        let ranked = rank(BuiltinTipsSource.fetch().unwrap(), &prof);
        assert_eq!(ranked[0].0.dimension, Some(Dimension::SkillReuse));
    }

    #[test]
    fn tag_gated_news_shows_only_on_matching_interest() {
        let mcp_item = ContentItem {
            id: "n1".into(), kind: ItemKind::News, title: "MCP 심화".into(),
            body: "".into(), source_url: None, dimension: None,
            trigger_tags: vec!["mcp".into()], base_priority: 0,
        };
        let with_mcp = profile_with(&[], &["mcp"]);
        let without = profile_with(&[], &[]);
        assert!(score(&mcp_item, &with_mcp) > 0);
        assert!(score(&mcp_item, &without) < 0);
    }

    #[test]
    fn changelog_parse_extracts_versions_and_bullets() {
        let src = ClaudeChangelogSource::default();
        let md = "# Changelog\n\n## 2.1.0\n\n- Added ToolSearch for deferred tools\n- Fixed a crash\n\n## 2.0.9\n\n- Improved permissions\n";
        let items = src.parse_markdown(md);
        assert_eq!(items.len(), 2);
        assert!(items[0].title.contains("2.1.0"));
        assert!(items[0].body.contains("ToolSearch"));
        assert_eq!(items[0].dimension, None);
        assert!(items[0].trigger_tags.contains(&"changelog".to_string()));
    }

    #[test]
    fn changelog_parse_is_lenient_on_empty_or_garbage() {
        let src = ClaudeChangelogSource::default();
        assert!(src.parse_markdown("").is_empty());
        assert!(src.parse_markdown("no headers here\njust text").is_empty());
    }
}
