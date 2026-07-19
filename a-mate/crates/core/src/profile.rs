//! 역량 프로필 감지 — 사용자의 현재 AX 사다리 위치를 로그에서 결정론적으로 추정한다.
//! (콘텐츠 큐레이션 킥오프 `docs/brainstorming/2026-07-14-content-curation-kickoff.md` §What)
//!
//! 순수 읽기 함수: store를 조회해 CompetencyProfile을 만든다. 부수효과·네트워크 없음.
//! 신호는 전부 기존 수집 데이터(events·findings·inventory)에서만 온다 — 새 수집 없음.

use crate::store::SqliteStore;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeSet;

/// AX 역량 사다리의 축. ladder_index 순서대로 밟고 올라간다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Dimension {
    ModelLiteracy,  // Lv0 — 모델 티어 선택
    ContextHygiene, // Lv1 — 미사용 MCP/플러그인 정리, CLAUDE.md
    SkillReuse,     // Lv2 — 반복을 스킬로
    Automation,     // Lv3 — 커스텀 커맨드·hooks·권한 사전허용
    Orchestration,  // Lv4 — 서브에이전트 위임
}

impl Dimension {
    pub fn ladder_index(&self) -> u8 {
        match self {
            Dimension::ModelLiteracy => 0,
            Dimension::ContextHygiene => 1,
            Dimension::SkillReuse => 2,
            Dimension::Automation => 3,
            Dimension::Orchestration => 4,
        }
    }
    pub fn all() -> [Dimension; 5] {
        [
            Dimension::ModelLiteracy,
            Dimension::ContextHygiene,
            Dimension::SkillReuse,
            Dimension::Automation,
            Dimension::Orchestration,
        ]
    }
    pub fn key(&self) -> &'static str {
        match self {
            Dimension::ModelLiteracy => "model_literacy",
            Dimension::ContextHygiene => "context_hygiene",
            Dimension::SkillReuse => "skill_reuse",
            Dimension::Automation => "automation",
            Dimension::Orchestration => "orchestration",
        }
    }
    /// UI용 한글 라벨 (역량 사다리 표시).
    pub fn label_ko(&self) -> &'static str {
        match self {
            Dimension::ModelLiteracy => "모델 리터러시",
            Dimension::ContextHygiene => "컨텍스트 위생",
            Dimension::SkillReuse => "스킬 재사용",
            Dimension::Automation => "자동화",
            Dimension::Orchestration => "오케스트레이션",
        }
    }
    /// "지금 배울 것" — 이 축을 밟을 때 무엇을 하면 되는지 한 줄.
    pub fn learn_hint_ko(&self) -> &'static str {
        match self {
            Dimension::ModelLiteracy => "작업 난이도에 맞춰 모델 티어를 고르기 — 잔심부름은 sonnet/haiku로",
            Dimension::ContextHygiene => "안 쓰는 MCP·플러그인 정리, 반복 지시는 CLAUDE.md에 기재",
            Dimension::SkillReuse => "반복하는 작업 흐름을 스킬(SKILL.md)로 묶어 재사용",
            Dimension::Automation => "커스텀 커맨드·hooks·권한 사전허용으로 마찰 제거",
            Dimension::Orchestration => "큰 작업을 서브에이전트에 위임해 병렬로 처리",
        }
    }
}

/// 한 축에서의 숙련 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Mastery {
    NotStarted, // 아직 이 습관이 없음 (코칭 프론티어 후보)
    InProgress, // 배우는 중
    Mastered,   // 이미 잘 함 → 침묵
}

impl Mastery {
    /// 프론트 직렬화용 안정 키.
    pub fn key(&self) -> &'static str {
        match self {
            Mastery::NotStarted => "not_started",
            Mastery::InProgress => "in_progress",
            Mastery::Mastered => "mastered",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DimState {
    pub dimension: Dimension,
    pub mastery: Mastery,
    /// 사람이 읽을 근거 한 줄 (다이어리·카드 evidence용)
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompetencyProfile {
    pub dims: Vec<DimState>,
    /// 태그 게이트 콘텐츠(뉴스·MCP 강좌 등) 매칭용 활성 관심 태그
    pub active_tags: BTreeSet<String>,
    pub total_events: u64,
}

impl CompetencyProfile {
    pub fn mastery(&self, d: Dimension) -> Mastery {
        self.dims
            .iter()
            .find(|s| s.dimension == d)
            .map(|s| s.mastery)
            .unwrap_or(Mastery::NotStarted)
    }

    /// 프론티어 = 아직 Mastered가 아닌 사다리 최하단 축. 전부 마스터면 None.
    pub fn frontier(&self) -> Option<Dimension> {
        Dimension::all()
            .into_iter()
            .find(|d| self.mastery(*d) != Mastery::Mastered)
    }
}

/// 활성(무시/해결 안 된) finding의 rule_id 집합.
fn active_rule_ids(store: &SqliteStore) -> Result<BTreeSet<String>> {
    let mut stmt = store.conn.prepare(
        "SELECT DISTINCT rule_id FROM findings WHERE status NOT IN ('dismissed','resolved')",
    )?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    Ok(ids)
}

fn scalar_u64(store: &SqliteStore, sql: &str) -> Result<u64> {
    let v: i64 = store.conn.query_row(sql, [], |r| r.get(0))?;
    Ok(v as u64)
}

/// 상위 모델 출력이 이 토큰 미만이면 "단순 작업(잔심부름)"으로 본다.
const TRIVIAL_OUTPUT_TOKENS: u64 = 200;
/// 상위 모델 턴 중 단순 작업 비율이 이 % 이상이면 티어링을 프론티어(NotStarted)로.
const MODEL_FRONTIER_TRIVIAL_PCT: u64 = 40;
/// 이 % 이상이면 개선 여지(InProgress). 미만이면 Mastered.
const MODEL_INPROGRESS_TRIVIAL_PCT: u64 = 20;

/// store 조회로 현재 역량 프로필을 추정한다(결정론).
pub fn detect_profile(store: &SqliteStore) -> Result<CompetencyProfile> {
    let rules = active_rule_ids(store)?;
    let total_events = scalar_u64(store, "SELECT COUNT(*) FROM events")?;

    // Lv0 모델 리터러시: "작업 난이도에 맞춰 모델 티어를 고르는가".
    // 마스터 신호는 둘 중 하나 — ① 하위 모델을 실질적으로 혼용, 또는
    // ② 상위 모델을 (짧은 응답=잔심부름이 아니라) 주로 실질 작업에 사용.
    // 상위 모델을 단순 작업에 남발할 때만 프론티어로 잡는다.
    let opus = scalar_u64(
        store,
        "SELECT COUNT(*) FROM events WHERE kind='assistant_turn' AND model_family='opus'",
    )?;
    let cheaper = scalar_u64(
        store,
        "SELECT COUNT(*) FROM events WHERE kind='assistant_turn' AND model_family IN ('sonnet','haiku')",
    )?;
    // 상위 모델을 짧은 응답(단순 작업)에 쓴 턴 수 — "잔심부름에 최상위 모델" 신호.
    let opus_trivial = scalar_u64(
        store,
        &format!(
            "SELECT COUNT(*) FROM events WHERE kind='assistant_turn' \
             AND model_family='opus' AND tok_output < {TRIVIAL_OUTPUT_TOKENS}"
        ),
    )?;
    let model = if opus + cheaper == 0 {
        DimState { dimension: Dimension::ModelLiteracy, mastery: Mastery::NotStarted,
            evidence: "아직 어시스턴트 턴 기록이 없음".into() }
    } else if cheaper.saturating_mul(5) >= opus {
        // 하위 모델을 실질적으로 혼용 중 → 티어링 습관 있음
        DimState { dimension: Dimension::ModelLiteracy, mastery: Mastery::Mastered,
            evidence: format!("모델 티어 혼용 중 (하위 {cheaper} / 상위 {opus})") }
    } else {
        // 상위 모델 위주 — 단순 작업 남발 비율로 판정.
        let trivial_pct = if opus == 0 { 0 } else { opus_trivial.saturating_mul(100) / opus };
        if trivial_pct >= MODEL_FRONTIER_TRIVIAL_PCT {
            DimState { dimension: Dimension::ModelLiteracy, mastery: Mastery::NotStarted,
                evidence: format!("상위 모델을 단순 작업에 {opus_trivial}/{opus}턴({trivial_pct}%) — 티어 선택 필요") }
        } else if trivial_pct >= MODEL_INPROGRESS_TRIVIAL_PCT {
            DimState { dimension: Dimension::ModelLiteracy, mastery: Mastery::InProgress,
                evidence: format!("상위 모델 단순작업 비율 {trivial_pct}% — 티어링 개선 여지") }
        } else {
            DimState { dimension: Dimension::ModelLiteracy, mastery: Mastery::Mastered,
                evidence: format!("상위 모델을 주로 실질 작업에 사용 (단순작업 {trivial_pct}%)") }
        }
    };

    // Lv1 컨텍스트 위생: R1(미사용 MCP)·R2(미사용 플러그인)가 살아있으면 상주 토큰 새는 중.
    let ctx = if rules.contains("R1") || rules.contains("R2") {
        DimState { dimension: Dimension::ContextHygiene, mastery: Mastery::NotStarted,
            evidence: "미사용 MCP/플러그인 finding 활성 (상주 토큰 낭비)".into() }
    } else if total_events == 0 {
        DimState { dimension: Dimension::ContextHygiene, mastery: Mastery::NotStarted,
            evidence: "데이터 없음".into() }
    } else {
        DimState { dimension: Dimension::ContextHygiene, mastery: Mastery::Mastered,
            evidence: "미사용 MCP/플러그인 지적 없음".into() }
    };

    // Lv2 스킬 재사용: 스킬 툴콜 경험 유무.
    let skill_calls = scalar_u64(
        store,
        "SELECT COUNT(*) FROM events WHERE kind='tool_call' AND tool_kind='skill'",
    )?;
    let skill = if skill_calls > 0 {
        DimState { dimension: Dimension::SkillReuse, mastery: Mastery::Mastered,
            evidence: format!("스킬 {skill_calls}회 호출 — 재사용 습관 있음") }
    } else {
        DimState { dimension: Dimension::SkillReuse, mastery: Mastery::NotStarted,
            evidence: "스킬 호출 0회".into() }
    };

    // Lv3 자동화: 권한 마찰(R11)·자동화 버스트(R10)가 있으면 워크플로 정돈 필요.
    let autom = if rules.contains("R11") || rules.contains("R10") {
        DimState { dimension: Dimension::Automation, mastery: Mastery::NotStarted,
            evidence: "권한 마찰/자동화 버스트 finding 활성".into() }
    } else if total_events == 0 {
        DimState { dimension: Dimension::Automation, mastery: Mastery::NotStarted,
            evidence: "데이터 없음".into() }
    } else {
        DimState { dimension: Dimension::Automation, mastery: Mastery::InProgress,
            evidence: "마찰 신호 없음 (hooks/커맨드 사용은 미측정)".into() }
    };

    // Lv4 오케스트레이션: 서브에이전트(사이드체인) 사용 유무.
    let sub = scalar_u64(
        store,
        "SELECT COUNT(*) FROM events WHERE is_sidechain=1 \
         OR (kind='tool_call' AND tool_kind='sub_agent')",
    )?;
    let orch = if sub > 0 {
        DimState { dimension: Dimension::Orchestration, mastery: Mastery::Mastered,
            evidence: format!("서브에이전트 {sub}건 — 위임 활용 중") }
    } else {
        DimState { dimension: Dimension::Orchestration, mastery: Mastery::NotStarted,
            evidence: "서브에이전트 사용 없음".into() }
    };

    // 활성 관심 태그
    let mut active_tags = BTreeSet::new();
    let mcp_calls = scalar_u64(
        store,
        "SELECT COUNT(*) FROM events WHERE kind='tool_call' AND tool_kind='mcp_call'",
    )?;
    if mcp_calls > 0 || rules.contains("R1") {
        active_tags.insert("mcp".to_string());
    }
    if skill_calls > 0 || rules.contains("R12") {
        active_tags.insert("skill".to_string());
    }

    Ok(CompetencyProfile {
        dims: vec![model, ctx, skill, autom, orch],
        active_tags,
        total_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Finding, Severity};
    use crate::model::*;
    use crate::store::SqliteStore;

    fn turn(session: &str, uuid: &str, model: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-14T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }
    }
    fn turn_out(session: &str, uuid: &str, model: &str, out: u64) -> NormalizedEvent {
        let mut e = turn(session, uuid, model);
        if let EventKind::AssistantTurn { usage, .. } = &mut e.kind { usage.output = out; }
        e
    }
    fn skill_call(session: &str, uuid: &str) -> NormalizedEvent {
        NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: session.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-14T10:01:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            kind: EventKind::ToolCall {
                kind: ToolKind::Skill { name: "superpowers:brainstorming".into() },
                raw_name: "Skill".into(), target: Some("superpowers:brainstorming".into()),
                tool_use_id: None,
            },
        }
    }
    fn finding(rule: &str) -> Finding {
        Finding {
            rule_id: rule.into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 100,
            prescription: None, dedup_key: format!("{rule}|x"),
        }
    }

    #[test]
    fn junior_all_opus_no_skills_frontier_is_model_literacy() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8"),
            turn("s1", "u2", "claude-opus-4-8"),
        ]).unwrap();
        let p = detect_profile(&store).unwrap();
        assert_eq!(p.mastery(Dimension::ModelLiteracy), Mastery::NotStarted);
        assert_eq!(p.frontier(), Some(Dimension::ModelLiteracy));
    }

    #[test]
    fn opus_on_substantial_work_masters_model_literacy() {
        // 하위 모델을 안 써도, 상위 모델을 큰 작업(긴 출력)에만 쓰면 티어 감각 있음 → Mastered.
        let store = SqliteStore::open_in_memory().unwrap();
        let evs: Vec<_> = (0..8)
            .map(|i| turn_out("s1", &format!("u{i}"), "claude-opus-4-8", 1500))
            .collect();
        store.upsert_events(&evs).unwrap();
        let p = detect_profile(&store).unwrap();
        assert_eq!(p.mastery(Dimension::ModelLiteracy), Mastery::Mastered);
        assert_ne!(p.frontier(), Some(Dimension::ModelLiteracy));
    }

    #[test]
    fn opus_on_trivial_work_is_the_frontier() {
        // 상위 모델을 잔심부름(짧은 출력)에 남발하면 프론티어로 남는다.
        let store = SqliteStore::open_in_memory().unwrap();
        let evs: Vec<_> = (0..8)
            .map(|i| turn_out("s1", &format!("u{i}"), "claude-opus-4-8", 40))
            .collect();
        store.upsert_events(&evs).unwrap();
        let p = detect_profile(&store).unwrap();
        assert_ne!(p.mastery(Dimension::ModelLiteracy), Mastery::Mastered);
        assert_eq!(p.frontier(), Some(Dimension::ModelLiteracy));
    }

    #[test]
    fn mixed_models_advances_frontier_past_model_literacy() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8"),
            turn("s1", "u2", "claude-haiku-4-5"),
            turn("s1", "u3", "claude-sonnet-4-6"),
        ]).unwrap();
        let p = detect_profile(&store).unwrap();
        assert_eq!(p.mastery(Dimension::ModelLiteracy), Mastery::Mastered);
        // 모델은 통과 → 스킬 미사용이 다음 프론티어
        assert_eq!(p.frontier(), Some(Dimension::SkillReuse));
    }

    #[test]
    fn unused_mcp_finding_makes_context_hygiene_the_frontier() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 모델 혼용(Lv0 통과)했지만 R1 활성 → Lv1이 프론티어
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8"),
            turn("s1", "u2", "claude-haiku-4-5"),
        ]).unwrap();
        store.upsert_finding(&finding("R1"), "2026-07-14T10:00:00Z").unwrap();
        let p = detect_profile(&store).unwrap();
        assert_eq!(p.frontier(), Some(Dimension::ContextHygiene));
        assert!(p.active_tags.contains("mcp"));
    }

    #[test]
    fn skill_usage_marks_skill_reuse_mastered() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[
            turn("s1", "u1", "claude-opus-4-8"),
            turn("s1", "u2", "claude-haiku-4-5"),
            skill_call("s1", "u3"),
        ]).unwrap();
        let p = detect_profile(&store).unwrap();
        assert_eq!(p.mastery(Dimension::SkillReuse), Mastery::Mastered);
        assert!(p.active_tags.contains("skill"));
    }

    #[test]
    fn dismissed_finding_does_not_count_as_active() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.upsert_events(&[turn("s1", "u1", "claude-opus-4-8"), turn("s1","u2","claude-haiku-4-5")]).unwrap();
        store.upsert_finding(&finding("R1"), "2026-07-14T10:00:00Z").unwrap();
        store.set_finding_status("R1|x", "dismissed").unwrap();
        let p = detect_profile(&store).unwrap();
        // R1 무시됨 → 컨텍스트 위생은 프론티어 아님
        assert_eq!(p.mastery(Dimension::ContextHygiene), Mastery::Mastered);
    }
}
