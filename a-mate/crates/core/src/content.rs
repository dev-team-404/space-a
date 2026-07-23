//! 콘텐츠 큐레이션 — 외부/내장 지식을 정규화하고, 역량 프로필로 관련도를 매긴다.
//! (킥오프 `docs/brainstorming/2026-07-14-content-curation-kickoff.md`)
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
            // AX 튜터 원칙(2026-07-19): 소식은 배움의 조미료지 주식이 아니다 — 최대 2건.
            max_items: 2,
        }
    }
}

/// 사용자가 "써볼 수 있는" 기능 소식인가 — 버그픽스·리버트·내부 정리는 배움이 아니다.
/// 기본 폐쇄: 기능 신설 신호가 없으면 버린다 (AX 튜터는 패치노트 리더가 아니다).
fn is_feature_note(line: &str) -> bool {
    let l = line.to_lowercase();
    const STARTS: [&str; 7] = ["add", "new ", "support", "introduc", "enable", "launch", "allow"];
    if STARTS.iter().any(|p| l.starts_with(p)) {
        return true;
    }
    const CONTAINS: [&str; 3] = ["can now", "now supports", "now available"];
    CONTAINS.iter().any(|p| l.contains(p))
}

impl ClaudeChangelogSource {
    /// 몇 개 버전 헤더까지 훑을지 — 최근 버전들이 전부 픽스뿐이어도 기능 릴리스를 찾도록.
    const SCAN_VERSIONS: usize = 12;

    /// 마크다운 CHANGELOG를 관대하게 파싱: `## <version>` 헤더 아래 **기능 신설 bullet만** 모은다.
    /// 2026-07-19 개편: 버전마다 "X.Y.Z 새 기능" 아이템을 반복 생성하지 않고,
    /// 최근 버전들의 기능을 **단 하나의 소식**으로 합친다(중복 노출 제거). 최신 버전이 대표.
    pub fn parse_markdown(&self, md: &str) -> Vec<ContentItem> {
        let mut features: Vec<String> = Vec::new(); // 최신순, 중복 제거
        let mut latest_ver: Option<String> = None;
        let mut in_version = false;
        let mut seen_versions = 0usize;

        for line in md.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("## ") {
                seen_versions += 1;
                if seen_versions > Self::SCAN_VERSIONS {
                    break;
                }
                in_version = true;
                if latest_ver.is_none() {
                    latest_ver = Some(rest.trim().to_string());
                }
            } else if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                let note = rest.trim();
                if in_version && features.len() < 3 && is_feature_note(note)
                    && !features.iter().any(|f| f == note)
                {
                    features.push(note.to_string());
                }
            }
        }

        if features.is_empty() {
            return Vec::new(); // 기능 소식 없음 → 침묵
        }
        let ver = latest_ver.unwrap_or_default();
        vec![ContentItem {
            id: "cc-changelog-latest".into(),
            kind: ItemKind::News,
            // 대표 버전 하나만 표기 — "새 기능 N건"으로 반복이 아니라 한 건임을 명확히.
            title: format!("Claude Code 새 기능 {}건 (최신 {ver})", features.len()),
            body: features.join(" · "),
            source_url: Some(
                "https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md".into(),
            ),
            dimension: None,
            trigger_tags: vec!["changelog".into(), "claude-code".into()],
            base_priority: 10,
        }]
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
// Boris 팁(창시자) — howborisusesclaudecode.com. 공신력↑·사내망 200·HTML 파싱.
// 실제 팁은 .step-title + .step-body 쌍(중첩 태그 → scraper). 관대: 실패해도 빈 벡터.
pub struct BorisTipsSource {
    pub url: String,
    pub max_items: usize,
}

impl Default for BorisTipsSource {
    fn default() -> Self {
        BorisTipsSource {
            url: "https://howborisusesclaudecode.com/".into(),
            max_items: 8,
        }
    }
}

/// 보리스 팁 영어 제목 → 한국어 (알려진 제목 정적 매핑, 미지 제목은 원문 유지).
/// 한국어 UI에 영어 제목이 떠 있으면 배움 카드가 붕 뜬다는 검토(2026-07-19) 반영.
fn boris_ko_title(en: &str) -> String {
    match en.trim() {
        "Run 5 Claudes in Parallel" => "클로드 5개를 병렬로 돌리기".into(),
        "Start in Plan Mode" => "플랜 모드로 먼저 계획하기".into(),
        "Subagents for Common Workflows" => "자주 하는 작업은 서브에이전트로".into(),
        "Parallel Web and Mobile Sessions" => "웹·모바일 세션 병렬 활용".into(),
        "@.claude in Code Reviews" => "코드 리뷰에서 @.claude 태그 쓰기".into(),
        "Slash Commands for Inner Loops" => "반복 루프는 슬래시 커맨드로".into(),
        "Shared CLAUDE.md Documentation" => "CLAUDE.md를 팀과 공유하기".into(),
        "Common Slash Commands" => "자주 쓰는 슬래시 커맨드 정리".into(),
        other => other.to_string(),
    }
}

/// 본문을 간결하게 — 파싱 아티팩트("View original post") 제거 + 공백 정리 + ~140자 축약.
/// (엔진 꺼져 코칭이 없을 때의 폴백용. 코칭이 있으면 프론트가 본문을 숨긴다.)
fn tidy_body(body: &str) -> String {
    let cleaned = body.replace("View original post", " ");
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 140 {
        let short: String = collapsed.chars().take(140).collect();
        format!("{}…", short.trim_end())
    } else {
        collapsed
    }
}

/// 팁 텍스트 → (역량 축, 태그). 키워드로 프론티어/태그 매칭이 되게 한다(특이도 높은 것 우선).
/// boris·팀 지식 등 소스 무관 공용 — 소스 태그는 호출자가 앞에 붙인다.
fn classify_keywords(text: &str) -> (Option<Dimension>, Vec<String>) {
    use Dimension::*;
    let t = text.to_lowercase();
    let has = |k: &str| t.contains(k);
    let mut dim: Option<Dimension> = None;
    let mut tags: Vec<String> = Vec::new();
    if has("subagent") || has("worktree") || has("parallel") || has("orchestr") || has("background agent") {
        dim = dim.or(Some(Orchestration));
        tags.push("subagent".into());
    }
    if has("skill") {
        dim = dim.or(Some(SkillReuse));
        tags.push("skill".into());
    }
    if has("hook") || has("slash command") || has("permission") || has("auto-accept") || has("github") {
        dim = dim.or(Some(Automation));
        tags.push("hooks".into());
    }
    if has("claude.md") || has("claudemd") || has("compact") || has("context") || has("memory") {
        dim = dim.or(Some(ContextHygiene));
        tags.push("claudemd".into());
    }
    if has("plan mode") || has("model") || has("opus") || has("thinking") {
        dim = dim.or(Some(ModelLiteracy));
        tags.push("model".into());
    }
    if has("mcp") {
        tags.push("mcp".into());
    }
    (dim, tags)
}

impl BorisTipsSource {
    /// HTML을 관대하게 파싱: `.step-title` + `.step-body` 쌍을 팁으로. 파싱 실패는 빈 벡터.
    /// 문서 구조가 step-header→step-title→step-body 1:1 반복이라 순서 zip이 정확히 짝을 맞춘다.
    pub fn parse_html(&self, html: &str) -> Vec<ContentItem> {
        use sha2::{Digest, Sha256};
        let doc = scraper::Html::parse_document(html);
        // gemini 리뷰(#30) 반영: 전역 title/body 리스트 zip은 한 요소만 빠져도 이후 전체가
        // 어긋난다. 문서 순서로 순회하며 "제목 → 다음에 오는 본문"을 짝짓는다 —
        // 고아 제목(본문 없이 다음 제목 등장)은 자연 폐기되어 어긋남이 전파되지 않는다.
        let Ok(pair_sel) = scraper::Selector::parse(".step-title, .step-body") else {
            return vec![];
        };
        let is_title = |e: &scraper::ElementRef| {
            e.value().classes().any(|c| c == "step-title")
        };
        let text_of = |e: &scraper::ElementRef| -> String {
            e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
        };
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut pending: Option<String> = None;
        for el in doc.select(&pair_sel) {
            if is_title(&el) {
                pending = Some(text_of(&el)); // 이전 고아 제목은 덮어써 폐기
            } else if let Some(t) = pending.take() {
                pairs.push((t, text_of(&el)));
            }
        }
        pairs
            .into_iter()
            .filter(|(t, b)| !t.trim().is_empty() && !b.trim().is_empty())
            .take(self.max_items)
            .map(|(t, b)| {
                let (dimension, mut trigger_tags) = classify_keywords(&format!("{t} {b}"));
                trigger_tags.insert(0, "boris".into());
                let hx = Sha256::digest(t.trim().as_bytes());
                ContentItem {
                    id: format!("boris-{:02x}{:02x}{:02x}", hx[0], hx[1], hx[2]),
                    kind: ItemKind::Tip,
                    title: boris_ko_title(&t),
                    body: format!("{} — Boris Cherny(Claude Code 창시자)", tidy_body(&b)),
                    source_url: Some(self.url.clone()),
                    dimension,
                    trigger_tags,
                    base_priority: 6,
                }
            })
            .collect()
    }
}

impl ContentSource for BorisTipsSource {
    fn id(&self) -> &str {
        "boris-tips"
    }
    fn fetch(&self) -> Result<Vec<ContentItem>> {
        // 백그라운드 스캔을 막지 않도록 타임아웃 필수(리뷰 지적).
        let body = ureq::get(&self.url)
            .timeout(std::time::Duration::from_secs(15))
            .call()?
            .into_string()?;
        Ok(self.parse_html(&body))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// E — 공식 마켓플레이스 카탈로그 (②미설치 plugin 추천 재료. changelog 소스의 형제).
// ContentItem이 아니라 CatalogEntry를 반환 — 카탈로그는 노출물이 아니라 매칭 재료.
// 공개 read-only GET(사용자 데이터 미전송). 관대: 파싱 실패·필드 누락 skip, 실패는
// 호출부(ops::fetch_marketplace_catalog)가 빈 벡터로 계속(② 추천만 침묵).
// ─────────────────────────────────────────────────────────────────────────

/// 공식 마켓플레이스 plugin 한 항목 — E ②(미설치 추천)의 존재 검증·링크 재료.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub homepage: Option<String>,
}

pub struct MarketplaceCatalogSource {
    pub url: String,
}

impl Default for MarketplaceCatalogSource {
    fn default() -> Self {
        MarketplaceCatalogSource {
            // 스펙 §8 핀(2026-07-23 실측): 200 OK, ~158KB, 425 plugins.
            url: "https://raw.githubusercontent.com/anthropics/claude-plugins-official/main/.claude-plugin/marketplace.json".into(),
        }
    }
}

impl MarketplaceCatalogSource {
    /// marketplace.json을 관대하게 파싱 — plugins[]에서 name 있는 항목만.
    pub fn parse_json(&self, raw: &str) -> Vec<CatalogEntry> {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) else { return Vec::new() };
        let Some(plugins) = json.get("plugins").and_then(|p| p.as_array()) else { return Vec::new() };
        plugins
            .iter()
            .filter_map(|p| {
                let name = p.get("name")?.as_str()?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                Some(CatalogEntry {
                    name,
                    description: p.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                    category: p.get("category").and_then(|c| c.as_str()).map(String::from),
                    homepage: p.get("homepage").and_then(|h| h.as_str()).map(String::from),
                })
            })
            .collect()
    }

    /// 네트워크 fetch — 백그라운드 스캔을 막지 않도록 타임아웃 필수(boris 선례).
    pub fn fetch(&self) -> Result<Vec<CatalogEntry>> {
        let body = ureq::get(&self.url)
            .timeout(std::time::Duration::from_secs(15))
            .call()?
            .into_string()?;
        Ok(self.parse_json(&body))
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 팀 지식(Space A) — a-hub에 발행된 다른 팀원의 지식 페이지를 "오늘의 배움" 피드로.
// pull 방향 (push는 hub.rs). 내 페이지는 제외(에코 방지 — agent_id == user_id 계약).
// 관대: 트리/페이지 실패는 스킵, 소스 전체 실패도 상위에서 warn 후 계속.
// ─────────────────────────────────────────────────────────────────────────

pub struct HubKnowledgeSource {
    pub base_url: String,
    pub api_key: String,
    pub token: String,
    pub space_id: String,
    /// 내 발행분 제외용 — 허브 계약상 agent_id == user_id.
    pub own_agent_id: String,
    pub max_items: usize,
}

impl HubKnowledgeSource {
    fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let mut r = ureq::get(&format!("{}{}", self.base_url.trim_end_matches('/'), path))
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {}", self.token));
        if !self.api_key.is_empty() {
            r = r.set("x-api-key", &self.api_key);
        }
        Ok(r.call()?.into_json()?)
    }

    /// 트리 평탄화 → 남의 페이지만 → 최신순(page id 숫자 접미사 DESC) 상위 max — 순수.
    pub fn pick_page_ids(tree: &serde_json::Value, own_agent_id: &str, max: usize) -> Vec<String> {
        fn walk(nodes: &[serde_json::Value], own: &str, out: &mut Vec<(u64, String)>) {
            for n in nodes {
                let created_by = n.get("created_by").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(id) = n.get("page_id").and_then(|v| v.as_str()) {
                    if created_by != own {
                        let seq = id
                            .rsplit(['_', '-'])
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        out.push((seq, id.to_string()));
                    }
                }
                if let Some(ch) = n.get("children").and_then(|v| v.as_array()) {
                    walk(ch, own, out);
                }
            }
        }
        let mut acc: Vec<(u64, String)> = Vec::new();
        if let Some(nodes) = tree.as_array() {
            walk(nodes, own_agent_id, &mut acc);
        }
        acc.sort_by(|a, b| b.0.cmp(&a.0));
        acc.into_iter().take(max).map(|(_, id)| id).collect()
    }

    /// 페이지 JSON → ContentItem — 순수. 제목/본문 빈 것은 None.
    pub fn item_from_page(page: &serde_json::Value) -> Option<ContentItem> {
        let id = page.get("page_id").and_then(|v| v.as_str())?;
        let title = page.get("title").and_then(|v| v.as_str())?.trim().to_string();
        if title.is_empty() {
            return None;
        }
        let body = page.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let author = page.get("created_by").and_then(|v| v.as_str()).unwrap_or("팀");
        let (dimension, mut tags) = classify_keywords(&format!("{title} {body}"));
        tags.insert(0, "team".into());
        Some(ContentItem {
            id: format!("hub-{id}"),
            kind: ItemKind::Tip,
            title,
            body: format!("{} — {author}님의 팀 지식 (Space A)", tidy_body(body)),
            source_url: None, // 허브에 사람용 웹 UI가 없어 링크 생략 (a-lens가 사람용 뷰)
            dimension,
            trigger_tags: tags,
            base_priority: 5,
        })
    }
}

impl ContentSource for HubKnowledgeSource {
    fn id(&self) -> &str {
        "hub-knowledge"
    }
    fn fetch(&self) -> Result<Vec<ContentItem>> {
        let tree = self.get_json(&format!("/spaces/{}/tree", self.space_id))?;
        let nodes = tree.get("tree").cloned().unwrap_or_else(|| serde_json::json!([]));
        let ids = Self::pick_page_ids(&nodes, &self.own_agent_id, self.max_items);
        let mut out = Vec::new();
        for id in ids {
            match self.get_json(&format!("/pages/{id}")) {
                Ok(p) => {
                    if let Some(item) = Self::item_from_page(&p) {
                        out.push(item);
                    }
                }
                Err(e) => eprintln!("[curation] hub page {id} fetch 실패(계속): {e}"),
            }
        }
        Ok(out)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 개인 실전 레슨 (2026-07-19) — "정말 내 얘기"가 최상단에 오게. A-Mate의 본분:
// 일반 커리큘럼이 아니라 **내 로그의 사건**에서 나온 교훈 + 구체적 다음 행동.
// 전부 결정론(수치는 store 실측만), id 고정(레슨 종류별) → 닫으면 계속 조용.
// ─────────────────────────────────────────────────────────────────────────

fn lesson(id: &str, tags: &[&str], title: String, body: String, url: &str) -> ContentItem {
    let mut trigger_tags = vec!["personal".to_string()];
    trigger_tags.extend(tags.iter().map(|s| s.to_string()));
    ContentItem {
        id: id.into(),
        kind: ItemKind::Tip,
        title,
        body,
        source_url: Some(url.into()),
        dimension: None, // 마스터 억제 대상 아님 — 실측 사건은 항상 배울 가치
        trigger_tags,
        base_priority: 0,
    }
}

/// 최근(오늘·어제) 로그에서 실전 레슨 생성. 조건 미충족이면 침묵 — 억지 레슨 금지.
pub fn personal_lessons(
    store: &crate::store::SqliteStore,
    today: &str,
    yesterday: &str,
) -> Vec<ContentItem> {
    let mut out = Vec::new();
    let parse = |d: &str| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    let day_before = parse(yesterday)
        .map(|d| (d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string());

    // ① 시행착오 세션 — 오류 반복 후 회복한 최근 세션 (hub 회고와 같은 신호, 코칭 프레임)
    if let Ok(sessions) = store.struggle_sessions(3, 20, "9999-12-31T00:00:00Z") {
        if let Some(s) = sessions
            .iter()
            .find(|s| {
                s.last_result_ok
                    && s.last_ts
                        .as_deref()
                        .map(|t| t.starts_with(today) || t.starts_with(yesterday))
                        .unwrap_or(false)
            })
        {
            let tools = s
                .error_tools
                .iter()
                .take(2)
                .map(|(t, n)| format!("{t} {n}회"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(lesson(
                "lesson-struggle",
                &["plan"],
                format!("최근 세션에서 도구 오류 {}회 — 시행착오 줄이는 법", s.error_count),
                format!(
                    "당신 로그: 최근 세션에서 {tools} 오류가 났다가 회복했어요.\n\
                     • 원리: 계획 없이 바로 손대면 Claude가 이 방법 저 방법 시도하며 헤매고, \
                     그 실패 과정이 전부 토큰이에요.\n\
                     • 이렇게: 복잡한 작업은 처음에 `Shift+Tab`(플랜 모드)으로 '계획부터 세워줘'라고 \
                     한 뒤 계획을 확인·승인하고 실행시키세요. 같은 명령이 2번 실패하면 \
                     '접근을 바꿔보자'라고 방향을 틀어주면 시행착오가 확 줄어요."
                ),
                "https://code.claude.com/docs/en/common-workflows",
            ));
        }
    }

    // ②-칭찬: 캐시 레슨을 보여준 적 있고, 어제 재읽기가 그제보다 30%+ 줄었다면 — 코칭의 완성은 피드백
    let mut cache_praised = false;
    if let (Some(db), Ok(true)) = (day_before.as_deref(), store.content_item_exists("lesson-cache")) {
        if let (Ok(y), Ok(b)) = (store.summary_for_date(yesterday), store.summary_for_date(db)) {
            if b.tok_cache_read > 50_000_000 && y.tok_cache_read * 10 <= b.tok_cache_read * 7 {
                let pct = 100 - (y.tok_cache_read * 100 / b.tok_cache_read.max(1));
                cache_praised = true;
                out.push(lesson(
                    "lesson-praise-cache",
                    &["context"],
                    format!("재읽기가 {pct}% 줄었어요 — 조언을 실천하셨네요 👏"),
                    format!(
                        "당신 로그: 캐시 재읽기가 그제 대비 {pct}% 감소했어요. 세션을 나누고                          위임하는 습관이 자리잡는 중입니다. 이 리듬 그대로 가면 돼요."
                    ),
                    "https://code.claude.com/docs/en/sub-agents",
                ));
            }
        }
    }

    // ②-칭찬: 시행착오 레슨 실천 감지 — 어제 오류가 그제의 절반 이하
    if let (Some(db), Ok(true)) = (day_before.as_deref(), store.content_item_exists("lesson-struggle")) {
        if let (Ok(ye), Ok(be)) = (store.errors_on_local_date(yesterday), store.errors_on_local_date(db)) {
            if be >= 5 && ye * 2 <= be {
                out.push(lesson(
                    "lesson-praise-struggle",
                    &["plan"],
                    format!("도구 오류가 {be}회 → {ye}회로 줄었어요 👏"),
                    format!(
                        "당신 로그: 어제 시행착오가 그제({be}회)의 절반 이하({ye}회)였어요.                          계획 먼저·접근 전환 습관이 통하고 있습니다."
                    ),
                    "https://code.claude.com/docs/en/common-workflows",
                ));
            }
        }
    }

    // ②-보강: 캐시를 만들고 떠남 — 1h 캐시 생성 후 재사용 없이 세션 이탈 (R4 계열)
    if let Ok(day) = store.summary_for_date(yesterday) {
        if day.tok_cache_create > 3_000_000 && day.tok_cache_read < day.tok_cache_create {
            out.push(lesson(
                "lesson-cache-create",
                &["context"],
                "캐시를 만들고 바로 떠나고 있어요".into(),
                format!(
                    "당신 로그: 어제 캐시 생성 {}만 토큰인데 재사용은 그보다 적어요.\n\
                     • 원리: 세션을 열면 Claude가 파일·컨텍스트를 캐시로 저장해요(생성 비용). \
                     그 세션에서 이어서 작업하면 캐시를 재사용해 싸지는데, 금방 닫으면 생성 비용만 \
                     내고 버리는 셈이에요.\n\
                     • 이렇게: 관련된 작업은 창을 닫지 말고 한 세션에서 몰아서 하고, '이거 하나만' \
                     같은 짧은 질문은 새 세션 대신 가벼운 모델로 빠르게 끝내세요.",
                    day.tok_cache_create / 10_000
                ),
                "https://code.claude.com/docs/en/model-config",
            ));
        }
    }

    // ③-주간: 월·화요일엔 지난주 리포트 (종단 서사의 시작)
    if let Some(t) = parse(today) {
        use chrono::Datelike;
        let wd = t.weekday().num_days_from_monday(); // 0=월
        if wd <= 1 {
            let this_mon = t - chrono::Duration::days(wd as i64);
            let last_mon = this_mon - chrono::Duration::days(7);
            let last_sun = this_mon - chrono::Duration::days(1);
            let prev_mon = last_mon - chrono::Duration::days(7);
            let prev_sun = last_mon - chrono::Duration::days(1);
            let f = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();
            if let (Ok(lw), Ok(pw)) = (
                store.range_totals(&f(last_mon), &f(last_sun)),
                store.range_totals(&f(prev_mon), &f(prev_sun)),
            ) {
                if lw.0 > 0 {
                    let delta = |now: u64, before: u64| -> String {
                        if before == 0 { "—".into() }
                        else {
                            let p = now as i64 * 100 / before as i64 - 100;
                            if p >= 0 { format!("+{p}%") } else { format!("{p}%") }
                        }
                    };
                    let iso = last_mon.iso_week();
                    out.push(lesson(
                        &format!("lesson-weekly-{}-W{:02}", iso.year(), iso.week()),
                        &["weekly"],
                        "지난주 AI 사용 리포트".into(),
                        format!(
                            "당신 로그: 지난주 세션 {}개({}), 출력 {}만 토큰({}), 캐시 재읽기                              {}만 토큰({}). 괄호는 전주 대비예요. 재읽기가 늘고 있다면 세션                              분리·위임을, 세션이 늘었다면 반복 지시의 커맨드화를 점검해 보세요.",
                            lw.0, delta(lw.0, pw.0),
                            lw.2 / 10_000, delta(lw.2, pw.2),
                            lw.3 / 10_000, delta(lw.3, pw.3),
                        ),
                        "https://code.claude.com/docs/en/common-workflows",
                    ));
                }
            }
        }
    }

    // ② 캐시 재읽기 과다 — 컨텍스트가 눈덩이처럼 굴러가는 중 (칭찬이 나갔으면 잔소리 생략)
    if let Ok(day) = store.summary_for_date(yesterday) {
        if !cache_praised && day.tok_cache_read > 50_000_000 && day.tok_cache_read > day.tok_input.saturating_mul(5_000) {
            let eok = day.tok_cache_read / 100_000_000;
            let label = if eok > 0 { format!("약 {eok}억") } else { format!("{}", day.tok_cache_read) };
            out.push(lesson(
                "lesson-cache",
                &["context", "subagent"],
                "긴 세션 하나에 몰아치는 중 — 컨텍스트를 나눠보세요".into(),
                format!(
                    "당신 로그: 어제 캐시 재읽기가 {label} 토큰이었어요.\n\
                     • 원리: 한 대화창에 파일·검색 결과가 쌓일수록 Claude는 매 턴 그걸 통째로 \
                     다시 읽어요 — 그게 '재읽기' 비용이에요.\n\
                     • 이렇게: ① 성격이 다른 일은 새 창(세션)에서 시작하세요. ② 큰 조사·구현은 \
                     '이 폴더 전체를 조사해서 핵심만 요약해줘'처럼 통째로 맡기면, Claude가 별도 \
                     창(서브에이전트)에서 처리하고 요약만 가져와 내 대화창은 안 불어나요."
                ),
                "https://code.claude.com/docs/en/sub-agents",
            ));
        }
    }

    // ③ 상위 모델 단일화 — 잔심부름까지 비싼 모델로
    if let Ok(mix) = store.model_mix_for_range(Some(yesterday), today) {
        let total: u64 = mix.iter().map(|(_, t)| t).sum();
        let high: u64 = mix
            .iter()
            .filter(|(m, _)| crate::model::NormModel::from_raw_id(m).tier == crate::model::ModelTier::High)
            .map(|(_, t)| t)
            .sum();
        if total > 100_000 && high * 10 >= total * 9 {
            let pct = high * 100 / total;
            out.push(lesson(
                "lesson-model",
                &["model"],
                "잔심부름까지 최상위 모델을 쓰고 있어요".into(),
                format!(
                    "당신 로그: 어제·오늘 토큰의 {pct}%가 최상위 모델(Opus)이에요.\n\
                     • 원리: Opus는 어려운 추론에 강하지만 그만큼 비싸요. 파일 정리·오타 수정 \
                     같은 단순 작업엔 성능이 남아돌아 돈만 더 나가요.\n\
                     • 이렇게: 그런 잔심부름은 대화 중 `/model` 로 haiku나 sonnet으로 바꿔서 \
                     시키세요. 결과 품질은 그대로인데 비용이 가장 크게 줄어드는 구간이에요."
                ),
                "https://code.claude.com/docs/en/model-config",
            ));
        }
    }

    // ④ 역량 사다리 → "다음 단계" 한 줄 (별도 패널 대신 오늘의 배움에 통합, 2026-07-19 사용자 요청).
    //    실측 사건 레슨보다 아래에 오도록 base_priority를 낮춘다. 데이터가 충분하고 프론티어가
    //    있을 때만 — 전부 숙달이면 침묵(칭찬은 다른 레슨이 담당).
    if let Ok(prof) = crate::profile::detect_profile(store) {
        if prof.total_events >= 200 {
            if let Some(front) = prof.frontier() {
                let mastered = prof
                    .dims
                    .iter()
                    .filter(|d| d.mastery == crate::profile::Mastery::Mastered)
                    .count();
                let mut l = lesson(
                    "lesson-frontier",
                    &[],
                    format!("다음에 익히면 좋은 것: {}", front.label_ko()),
                    format!(
                        "당신 로그: AX 역량 5개 축 중 {mastered}개를 이미 익히셨고, 남은 다음 단계는 \
                         '{}'예요.\n👉 첫걸음: {}",
                        front.label_ko(),
                        front.first_step_ko(),
                    ),
                    "https://code.claude.com/docs/en/common-workflows",
                );
                l.base_priority = -5; // 실측 사건(struggle·cache·model)보다 뒤로
                out.push(l);
            }
        }
    }

    out
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
/// changelog 소식 기본 점수 — 어떤 팁(비프론티어 ~40 포함)보다도 낮게 (2026-07-19 품질 개편).
pub const SCORE_NEWS: i64 = 25;
/// 개인 실전 레슨 — 내 로그의 사건이 일반 커리큘럼(프론티어 500)보다 먼저다.
pub const SCORE_PERSONAL: i64 = 550;

/// 이 아이템을 지금 이 사용자에게 보여줄 가치. 클수록 상단. 0 미만은 숨김 후보.
pub fn score(item: &ContentItem, profile: &CompetencyProfile) -> i64 {
    // 개인 실전 레슨 — 내 로그의 사건은 항상 최우선 (마스터 억제·태그 게이트 미적용)
    if item.trigger_tags.iter().any(|t| t == "personal") {
        return SCORE_PERSONAL + item.base_priority;
    }
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
            if hit {
                SCORE_TAG_MATCH + item.base_priority
            } else if item.trigger_tags.iter().any(|t| t == "changelog") {
                // 소식은 배움(팁)보다 항상 아래 — 팁이 전부 쿨다운일 때만 상단에 오른다.
                // (AX 튜터 원칙: 패치노트가 커리큘럼을 밀어내면 안 된다)
                SCORE_NEWS + item.base_priority
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
        인사말·따옴표·과장 없이 핵심만. \
        새 기능 소식이라면: 원문(패치노트)을 번역·반복하지 말고, 이 기능으로 '무엇이 가능해졌고 \
        언제 어떤 명령·방법으로 써보면 되는지'를 구체적으로 안내하세요. 쓸 만한 활용법이 \
        떠오르지 않는 소식이면 억지로 포장하지 말고 어떤 상황에 해당되는지만 짧게 알려주세요.         절대 금지: 카드 제목·본문에 이미 있는 문장을 반복·번역·재서술하는 것. 본문에 없는         '다음 행동 딱 한 걸음'을 더할 수 없으면 빈 문자열만 반환하세요."
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
    fn changelog_keeps_only_feature_notes_and_skips_fix_only_versions() {
        let src = ClaudeChangelogSource::default();
        let md = "# Changelog\n\n\
            ## 2.1.2\n\n- Fixed /model dialog blocked in background sessions\n- Reverted an overly broad guard\n\n\
            ## 2.1.0\n\n- Added ToolSearch for deferred tools\n- Fixed a crash\n\n\
            ## 2.0.9\n\n- Improved permissions\n\n\
            ## 2.0.8\n\n- You can now resume agents across restarts\n";
        let items = src.parse_markdown(md);
        // 2026-07-19 개편: 버전별 반복 대신 최근 기능들을 단 하나의 소식으로 합친다.
        assert_eq!(items.len(), 1, "여러 버전이라도 소식은 한 건으로 합침");
        assert!(items[0].title.contains("새 기능"));
        assert!(items[0].title.contains("2.1.2"), "대표(최신) 버전 표기: {}", items[0].title);
        // 픽스만 있는 버전(2.1.2 fix·2.0.9)은 제외, 기능 라인만 body에
        assert!(items[0].body.contains("ToolSearch"));
        assert!(items[0].body.contains("resume agents"));
        assert!(!items[0].body.contains("Fixed"), "픽스 라인 제외: {}", items[0].body);
        assert!(items[0].trigger_tags.contains(&"changelog".to_string()));
    }

    // ── E: 마켓플레이스 카탈로그 ──

    #[test]
    fn marketplace_catalog_parses_entries_generously() {
        let src = MarketplaceCatalogSource::default();
        let raw = r#"{
            "name": "claude-plugins-official",
            "plugins": [
                {"name": "frontend-design", "description": "Design guidance",
                 "category": "design", "homepage": "https://example.com/fd"},
                {"description": "이름 없음 — skip"},
                {"name": "context7"}
            ]
        }"#;
        let entries = src.parse_json(raw);
        assert_eq!(entries.len(), 2, "name 없는 항목은 관대하게 skip");
        assert_eq!(entries[0].name, "frontend-design");
        assert_eq!(entries[0].category.as_deref(), Some("design"));
        assert_eq!(entries[0].homepage.as_deref(), Some("https://example.com/fd"));
        assert_eq!(entries[1].name, "context7");
        assert_eq!(entries[1].description, "", "description 없어도 항목 유지");
        // 공식 카탈로그 원본 URL 고정 (스펙 §8 핀)
        assert!(src.url.contains("anthropics/claude-plugins-official"));
    }

    #[test]
    fn marketplace_catalog_malformed_yields_empty() {
        let src = MarketplaceCatalogSource::default();
        assert!(src.parse_json("{not json").is_empty());
        assert!(src.parse_json(r#"{"plugins": "nope"}"#).is_empty());
        assert!(src.parse_json("{}").is_empty());
        assert!(src.parse_json("").is_empty());
    }

    // ── 개인 실전 레슨 ──

    #[test]
    fn personal_lessons_fire_on_real_signals_and_stay_silent_without() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        // 데이터 없음 → 억지 레슨 금지
        assert!(personal_lessons(&store, "2026-07-19", "2026-07-18").is_empty());

        // 캐시 재읽기 과다 시드 (어제): cache_read 2.1억, input 1천
        store.conn.execute(
            "INSERT INTO daily_rollup (host, project_id, date, tok_input, tok_output, tok_cache_read, tok_cache_create, session_count)
             VALUES ('Windows','p','2026-07-18', 1000, 50000, 210000000, 100, 3)",
            [],
        ).unwrap();
        let lessons = personal_lessons(&store, "2026-07-19", "2026-07-18");
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].id, "lesson-cache");
        assert!(lessons[0].body.contains("2억"), "실측 수치 인용: {}", lessons[0].body);
        assert!(lessons[0].trigger_tags.contains(&"personal".to_string()));
    }

    #[test]
    fn praise_fires_when_cache_improves_after_lesson_shown() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        // 레슨을 보여준 적 있음
        let mk = |id: &str| ContentItem {
            id: id.into(), kind: ItemKind::Tip, title: "t".into(), body: "b".into(),
            source_url: None, dimension: None, trigger_tags: vec!["personal".into()], base_priority: 0,
        };
        store.replace_content_items(&[(mk("lesson-cache"), 550)], "2026-07-17T00:00:00Z").unwrap();
        // 그제 2.1억 → 어제 0.6억 (71% 감소)
        store.conn.execute(
            "INSERT INTO daily_rollup (host,project_id,date,tok_input,tok_output,tok_cache_read,tok_cache_create,session_count)
             VALUES ('W','p','2026-07-17',1000,1,210000000,100,2), ('W','p','2026-07-18',1000,1,60000000,100,2)",
            [],
        ).unwrap();
        let lessons = personal_lessons(&store, "2026-07-19", "2026-07-18");
        let praise = lessons.iter().find(|l| l.id == "lesson-praise-cache").expect("칭찬 발화");
        assert!(praise.title.contains("줄었어요"));
        assert!(praise.body.contains("71%") || praise.body.contains("72%"), "{}", praise.body);
        // 칭찬이 나가면 같은 축 잔소리(lesson-cache)는 침묵
        assert!(!lessons.iter().any(|l| l.id == "lesson-cache"));
    }

    #[test]
    fn weekly_report_fires_on_monday_with_last_week_data() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        // 지난주(7/6~7/12)와 전주(6/29~7/5) 데이터
        store.conn.execute(
            "INSERT INTO daily_rollup (host,project_id,date,tok_input,tok_output,tok_cache_read,tok_cache_create,session_count)
             VALUES ('W','p','2026-07-08',10,2000000,50000000,1,4), ('W','p','2026-07-01',10,1000000,80000000,1,2)",
            [],
        ).unwrap();
        // 2026-07-13 = 월요일
        let lessons = personal_lessons(&store, "2026-07-13", "2026-07-12");
        let weekly = lessons.iter().find(|l| l.id.starts_with("lesson-weekly-")).expect("주간 리포트");
        assert!(weekly.body.contains("+100%"), "세션 2→4: {}", weekly.body); // 세션 전주 대비
        // 수요일엔 침묵
        assert!(!personal_lessons(&store, "2026-07-15", "2026-07-14")
            .iter().any(|l| l.id.starts_with("lesson-weekly-")));
    }

    #[test]
    fn boris_titles_localized_for_known_entries() {
        let html = r#"
          <div class="step-header"><div class="step-title">Start in Plan Mode</div></div>
          <div class="step-body">Plan first.</div>
        "#;
        let items = BorisTipsSource::default().parse_html(html);
        assert_eq!(items[0].title, "플랜 모드로 먼저 계획하기");
    }

    #[test]
    fn personal_lesson_outranks_frontier_tip() {
        let l = lesson("lesson-x", &["model"], "t".into(), "b".into(), "https://x");
        // 전 축 마스터 프로필이어도 (마스터 억제 미적용) 최상위
        let p = profile_with(&[], &[]);
        assert_eq!(score(&l, &p), SCORE_PERSONAL);
        assert!(score(&l, &p) > SCORE_FRONTIER_BOOST);
    }

    #[test]
    fn stale_feed_items_are_pruned_but_dismissed_kept() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        let mk = |id: &str| ContentItem {
            id: id.into(), kind: ItemKind::News, title: "t".into(), body: "b".into(),
            source_url: None, dimension: None, trigger_tags: vec!["changelog".into()],
            base_priority: 0,
        };
        // 1차 스캔: old-news 2건 (하나는 사용자가 닫음)
        store.replace_content_items(&[(mk("old-1"), 100), (mk("old-2"), 100)], "2026-07-18T00:00:00Z").unwrap();
        store.set_content_status("old-2", "dismissed", "2026-07-18T01:00:00Z").unwrap();
        // 2차 스캔: 피드에 new-1만 남음 → old-1(new)은 프룬, old-2(dismissed)는 쿨다운 기록으로 보존
        store.replace_content_items(&[(mk("new-1"), 50)], "2026-07-19T00:00:00Z").unwrap();
        let ids: Vec<String> = store
            .conn
            .prepare("SELECT id FROM content_items ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(ids, vec!["new-1".to_string(), "old-2".to_string()]);
    }

    #[test]
    fn changelog_collapses_many_versions_into_one_item_with_up_to_three_features() {
        let src = ClaudeChangelogSource::default();
        let mut md = String::from("# Changelog\n\n");
        for i in 0..6 {
            md.push_str(&format!("## 3.0.{i}\n\n- Added feature {i}\n\n"));
        }
        let items = src.parse_markdown(&md);
        assert_eq!(items.len(), 1, "여러 버전 → 단 하나의 소식으로 합침");
        // 최신 3개 기능만 (feature 0,1,2), 반복 없음
        assert_eq!(items[0].body.matches("Added feature").count(), 3);
        assert!(items[0].title.contains("3건"));
    }

    #[test]
    fn news_scores_below_any_tip() {
        // 소식(태그 미적중)은 비프론티어 팁(100-60+0=40)보다도 낮아야 한다
        let news = ContentItem {
            id: "cc-changelog-9.9".into(), kind: ItemKind::News, title: "t".into(), body: "b".into(),
            source_url: None, dimension: None,
            trigger_tags: vec!["changelog".into(), "claude-code".into()],
            base_priority: 10,
        };
        let p = profile_with(&[Dimension::ContextHygiene, Dimension::SkillReuse], &[]);
        let s = score(&news, &p);
        assert!(s > 0, "그래도 노출은 가능해야 (팁 전멸 시)");
        assert!(s < 40, "팁보다 항상 아래: {s}");
    }

    #[test]
    fn changelog_parse_is_lenient_on_empty_or_garbage() {
        let src = ClaudeChangelogSource::default();
        assert!(src.parse_markdown("").is_empty());
        assert!(src.parse_markdown("no headers here\njust text").is_empty());
    }

    #[test]
    fn boris_parses_step_tips_with_dimension_and_attribution() {
        // 실제 사이트 구조: .step-header → .step-title → .step-body (1:1 반복)
        let html = r#"
          <div class="tab"><div class="tab-label">parallel</div>
            <div class="step-header"><div class="step-number">1</div>
              <div class="step-title">Run 5 Claudes in Parallel</div></div>
            <div class="step-body">Boris runs 5 instances using 5 <b>git worktrees</b> of the same repo.</div>
          </div>
          <div class="tab"><div class="tab-label">context</div>
            <div class="step-header"><div class="step-title">Update your CLAUDE.md</div></div>
            <div class="step-body">After every correction, ask Claude to update CLAUDE.md so it won't repeat.</div>
          </div>
          <div class="tab">
            <div class="step-title">   </div><div class="step-body">   </div>
          </div>
        "#;
        let items = BorisTipsSource::default().parse_html(html);
        assert_eq!(items.len(), 2, "빈 쌍은 걸러야 함 (got {})", items.len());
        // 첫 팁: worktree/parallel → Orchestration + subagent 태그, 창시자 출처
        assert_eq!(items[0].title, "클로드 5개를 병렬로 돌리기"); // 한국어화(2026-07-19)
        assert!(items[0].body.contains("git worktrees"), "본문 텍스트: {}", items[0].body);
        assert!(items[0].body.contains("Boris"), "출처 표기 필요: {}", items[0].body);
        assert_eq!(items[0].dimension, Some(Dimension::Orchestration));
        assert!(items[0].trigger_tags.iter().any(|t| t == "subagent"));
        assert!(items[0].trigger_tags.iter().any(|t| t == "boris"));
        assert_eq!(items[0].source_url.as_deref(), Some("https://howborisusesclaudecode.com/"));
        // 둘째 팁: CLAUDE.md → ContextHygiene
        assert_eq!(items[1].title, "Update your CLAUDE.md");
        assert_eq!(items[1].dimension, Some(Dimension::ContextHygiene));
        assert!(items[1].trigger_tags.iter().any(|t| t == "claudemd"));
    }

    #[test]
    fn boris_orphan_title_does_not_misalign_following_pairs() {
        // gemini 리뷰(#30) 회귀 테스트: 본문 없는 고아 제목이 있어도 이후 짝이 어긋나지 않는다
        let html = r#"
          <div class="step-title">Orphan Title Without Body</div>
          <div class="step-title">Real Tip</div>
          <div class="step-body">Real body.</div>
        "#;
        let items = BorisTipsSource::default().parse_html(html);
        assert_eq!(items.len(), 1);
        assert!(items[0].title.contains("Real Tip") || items[0].title == "Real Tip",
            "고아 제목 폐기, 진짜 짝만: {}", items[0].title);
        assert!(items[0].body.contains("Real body"));
    }

    #[test]
    fn boris_is_lenient_on_empty_or_garbage() {
        assert!(BorisTipsSource::default().parse_html("").is_empty());
        assert!(BorisTipsSource::default().parse_html("<p>no steps here</p>").is_empty());
    }

    #[test]
    fn boris_body_is_tidied_no_artifacts_and_short() {
        let html = r#"
          <div class="step-header"><div class="step-title">Long Tip</div></div>
          <div class="step-body">This is a long body that repeats itself many times to exceed the limit so we can verify truncation works for the home card and stays concise enough. View original post</div>
        "#;
        let items = BorisTipsSource::default().parse_html(html);
        assert_eq!(items.len(), 1);
        assert!(!items[0].body.contains("View original post"), "아티팩트 제거: {}", items[0].body);
        assert!(items[0].body.contains('…'), "긴 본문 축약: {}", items[0].body);
    }

    // ── 팀 지식(pull) — HubKnowledgeSource 순수 함수 ──

    #[test]
    fn hub_pick_excludes_own_pages_sorts_desc_and_caps() {
        let tree = serde_json::json!([
            { "page_id": "page_3", "created_by": "salt", "children": [
                { "page_id": "page_9", "created_by": "salt", "children": [] }
            ]},
            { "page_id": "page_7", "created_by": "palen", "children": [] }, // 내 것 — 제외
            { "page_id": "page_5", "created_by": "jun", "children": [] },
            { "page_id": "page_1", "created_by": "salt", "children": [] },
        ]);
        let ids = HubKnowledgeSource::pick_page_ids(&tree, "palen", 3);
        assert_eq!(ids, vec!["page_9", "page_5", "page_3"]); // 최신순, 내 것 제외, 상한 3
    }

    #[test]
    fn hub_item_maps_page_with_team_tag_and_author() {
        let page = serde_json::json!({
            "page_id": "page_42",
            "title": "CI 캐시 키 구성 정리",
            "body": "브랜치별 캐시 키에 lockfile 해시를 섞으면 miss가 줄어든다.",
            "created_by": "salt"
        });
        let item = HubKnowledgeSource::item_from_page(&page).expect("유효한 페이지");
        assert_eq!(item.id, "hub-page_42");
        assert_eq!(item.title, "CI 캐시 키 구성 정리");
        assert_eq!(item.trigger_tags.first().map(|s| s.as_str()), Some("team"));
        assert!(item.body.contains("salt님의 팀 지식"), "저자 표기: {}", item.body);
    }

    #[test]
    fn hub_item_rejects_empty_title_and_classifies_keywords() {
        assert!(HubKnowledgeSource::item_from_page(&serde_json::json!({
            "page_id": "page_1", "title": "  ", "body": "x"
        }))
        .is_none());
        // 키워드 분류 공용화 — skill 언급이면 SkillReuse 축
        let item = HubKnowledgeSource::item_from_page(&serde_json::json!({
            "page_id": "page_2", "title": "Use the deploy skill", "body": "repeatable skill flow", "created_by": "salt"
        }))
        .unwrap();
        assert_eq!(item.dimension, Some(Dimension::SkillReuse));
    }
}
