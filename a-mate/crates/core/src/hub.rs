//! a-hub("Space A") 코칭 지식 공유 — 유의미한 Finding을 이슈→해결 흐름으로 발행.
//!
//! 스펙: docs/design/overview-mentor/specs/2026-07-18-hub-knowledge-sharing-design.md
//! 원칙: 결정론 본문만(정밀도의 선) · 개인정보 스크럽(§4) · 평생 1회 발행(나깅 방지) ·
//! 실패 무해(파이프라인 편승) · 환경변수 미설정 시 조용히 no-op(프라이버시 기본 = 로컬).

use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::store::FindingRow;

/// 공유 화이트리스트 — 팀 일반화 가능한 룰만(스펙 §1). 그 외는 기본 폐쇄.
pub const SHARE_RULES: [&str; 5] = ["R1", "R2", "R10", "R11", "R12"];
/// 유의미 문턱 기본값 (est_tokens_saved).
pub const DEFAULT_MIN_TOKENS: u64 = 1000;
/// est=0 룰(R11·R12)의 대체 문턱 — 반복 확인된 패턴만.
pub const MIN_OCCURRENCES_WHEN_NO_EST: u64 = 3;
/// 스캔당 발행 상한 (도입 시 백로그 폭주 방지).
pub const MAX_PER_SCAN: usize = 3;

// ─────────────────────────────────────────────────────────────────────────
// 설정 — 환경변수를 설정하는 행위 = egress 동의 (Engine 선례)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HubConfig {
    pub base_url: String,
    /// 서버 게이트 키. 빈 문자열 = 게이트 없는 배포.
    pub api_key: String,
    /// 없으면 최초 1회 자동 register (settings에 보존은 호출자 책임).
    pub token: Option<String>,
    pub space_id: String,
    pub user_id: String,
    pub min_tokens: u64,
}

impl HubConfig {
    /// URL 미설정 또는 SPACE_A_SHARE=off → None (공유 기능 전체 no-op).
    pub fn from_env() -> Option<HubConfig> {
        let base_url = std::env::var("SPACE_A_HUB_URL").ok()?;
        if base_url.trim().is_empty() {
            return None;
        }
        if std::env::var("SPACE_A_SHARE").map(|v| v == "off").unwrap_or(false) {
            return None;
        }
        let user_id = std::env::var("SPACE_A_USER")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("USERNAME").ok())
            .unwrap_or_else(|| "unknown".into());
        Some(HubConfig {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: std::env::var("SPACE_A_API_KEY").unwrap_or_default(),
            token: std::env::var("SPACE_A_TOKEN").ok().filter(|s| !s.is_empty()),
            space_id: std::env::var("SPACE_A_SPACE_ID")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "sw-innov".into()),
            user_id,
            min_tokens: std::env::var("SPACE_A_SHARE_MIN_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MIN_TOKENS),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 선별 — 순수 함수 (스펙 §2)
// ─────────────────────────────────────────────────────────────────────────

/// 공유 가치가 있는 Finding 선별: 화이트리스트 ∩ status=new ∩ 문턱 통과 ∩ 미공유, 상한 MAX_PER_SCAN.
/// findings는 est_tokens_saved DESC 정렬로 들어온다(list_findings_current) — 큰 절약부터.
pub fn select_shareable<'a>(
    findings: &'a [FindingRow],
    already_shared: &HashSet<String>,
    min_tokens: u64,
) -> Vec<&'a FindingRow> {
    findings
        .iter()
        .filter(|f| f.status == "new")
        .filter(|f| SHARE_RULES.contains(&f.rule_id.as_str()))
        .filter(|f| !already_shared.contains(&f.dedup_key))
        .filter(|f| {
            if f.est_tokens_saved > 0 {
                f.est_tokens_saved >= min_tokens
            } else {
                // 정량 근거가 없는 룰(R11·R12)은 반복 확인된 패턴만 (스펙 §2)
                f.occurrences >= MIN_OCCURRENCES_WHEN_NO_EST
            }
        })
        .take(MAX_PER_SCAN)
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// 렌더 — 룰별 결정론 템플릿 + 개인정보 스크럽 (스펙 §4)
// ─────────────────────────────────────────────────────────────────────────

/// 발행 콘텐츠. 제목/요약/steps 전부 결정론(LLM 무개입). 수치는 "약(~)" 라벨.
#[derive(Debug, Clone, PartialEq)]
pub struct ShareContent {
    pub title: String,
    pub summary: String,
    pub steps: Vec<String>,
}

fn ev_str<'a>(ev: &'a Value, key: &str) -> Option<&'a str> {
    ev.get(key).and_then(|v| v.as_str())
}

fn ev_u64(ev: &Value, key: &str) -> u64 {
    ev.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn presc_payload(f: &FindingRow) -> Value {
    f.prescription
        .as_ref()
        .and_then(|p| p.get("payload").cloned())
        .unwrap_or(Value::Null)
}

/// 화이트리스트 필드만 사용해 렌더. None = 공유 대상 룰이 아님.
/// 스크럽 보장: cwd·프롬프트 미리보기·session_id·로컬 경로 슬러그(scope_project)는 절대 넣지 않는다.
pub fn render_share(f: &FindingRow) -> Option<ShareContent> {
    let ev = &f.evidence;
    let payload = presc_payload(f);
    match f.rule_id.as_str() {
        "R1" => {
            let server = payload
                .get("server")
                .and_then(|v| v.as_str())
                .or_else(|| ev_str(ev, "server"))
                .unwrap_or("(unknown)")
                .to_string();
            let est = f.est_tokens_saved;
            Some(ShareContent {
                title: format!("[a-mate] 미사용 always-on MCP '{server}' — 상주 토큰 낭비"),
                summary: format!(
                    "MCP '{server}'가 활성 설정에 있으나 호출 0회. 세션마다 도구 정의가 \
                     컨텍스트에 상주하며 약 ~{est} 토큰/세션을 소모(추정). 같은 MCP를 안 쓰는 \
                     팀원이라면 동일하게 새는 비용이다. 제거하면 대화 시작 전 비용이 사라진다."
                ),
                steps: vec![
                    "감지: 활성 인벤토리에 있으나 사용 이벤트 0회 (a-mate R1)".into(),
                    "확인: 본인 워크플로에서 정말 안 쓰는지 점검".into(),
                    format!("적용: claude mcp remove {server}"),
                    format!("효과: 세션당 약 ~{est} 토큰 절약 (추정, 약(~) 라벨)"),
                ],
            })
        }
        "R2" => {
            let plugin = payload
                .get("plugin")
                .and_then(|v| v.as_str())
                .or_else(|| ev_str(ev, "plugin"))
                .unwrap_or("(unknown)")
                .to_string();
            let est = f.est_tokens_saved;
            let skill_count = ev_u64(ev, "skill_count");
            Some(ShareContent {
                title: format!("[a-mate] 미사용 플러그인 '{plugin}' — 상주 비용 정리"),
                summary: format!(
                    "플러그인 '{plugin}'(스킬 {skill_count}개)이 설치돼 있으나 사용 흔적 없음. \
                     스킬 설명이 매 세션 상주하며 약 ~{est} 토큰을 차지(추정). 안 쓰면 비활성이 이득."
                ),
                steps: vec![
                    "감지: 설치 인벤토리에 있으나 스킬 호출 0회 (a-mate R2)".into(),
                    format!("적용: claude plugin disable {plugin}"),
                    format!("효과: 세션당 약 ~{est} 토큰 절약 (추정)"),
                ],
            })
        }
        "R10" => {
            // 스크럽: session_ids·rep_cwd·rep_first_prompt는 사용하지 않는다.
            let total = ev_u64(ev, "total_sessions");
            let opus = ev_u64(ev, "opus_session_count");
            let to_model = payload.get("to").and_then(|v| v.as_str()).unwrap_or("haiku");
            let est = f.est_tokens_saved;
            Some(ShareContent {
                title: "[a-mate] 자동화 도구가 만든 세션 버스트 — 모델 설정 점검".into(),
                summary: format!(
                    "자동화 도구가 생성한 동형 초단기 세션 버스트 감지(관측 {total}세션 중 \
                     상위모델 {opus}세션). 도구가 만든 세션이 기본 상위 모델로 돌면 비용이 \
                     조용히 샌다. 같은 자동화 도구를 쓰는 팀이라면 동일 상황일 가능성이 높다."
                ),
                steps: vec![
                    "감지: 짧은 간격·적은 턴의 동형 세션 다발 (a-mate R10)".into(),
                    format!("적용: 자동화 도구의 모델 설정을 {to_model} 등 하위 모델로"),
                    format!("효과: 비용-등가 기준 약 ~{est} 토큰 절약 (추정)"),
                ],
            })
        }
        "R11" => {
            // 스크럽: session_ids·friction_events 상세(명령줄 포함 가능)는 사용하지 않는다.
            let tools: Vec<String> = payload
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let tool_list = if tools.is_empty() { "(도구 목록 없음)".to_string() } else { tools.join(", ") };
            let n = ev_u64(ev, "friction_events_count");
            Some(ShareContent {
                title: format!("[a-mate] 권한 마찰 반복 — allowlist 후보: {tool_list}"),
                summary: format!(
                    "\"권한 거부 → 결국 승인\" 패턴이 반복 관측됨({n}회). 매번 승인하는 도구는 \
                     사전 허용이 낫다. 같은 레포에서 일하는 팀원에게도 같은 allowlist가 통한다."
                ),
                steps: vec![
                    "감지: 거부 후 동일 도구 재시도·승인 반복 (a-mate R11)".into(),
                    format!("적용: 프로젝트 .claude/settings.json permissions.allow에 추가: {tool_list}"),
                    "효과: 반복 승인 마찰 제거 (팀 공용 레포면 커밋해 공유)".into(),
                ],
            })
        }
        "R12" => {
            // 스크럽: session_ids는 사용하지 않는다.
            let skills: Vec<String> = payload
                .get("skills")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let skill_list = if skills.is_empty() { "(스킬 없음)".to_string() } else { skills.join(", ") };
            Some(ShareContent {
                title: format!("[a-mate] 설치된 스킬 활용 제안 — {skill_list}"),
                summary: format!(
                    "큰 구현 작업을 스킬 없이 진행하는 패턴이 관측됨. 이미 설치된 스킬 \
                     [{skill_list}]이 이런 작업에 맞는다 — 아는 사람만 쓰는 스킬은 팀 자산이 아니다."
                ),
                steps: vec![
                    "감지: 대형 구현 세션에서 스킬 호출 없음 (a-mate R12)".into(),
                    format!("적용: 해당 작업 시 스킬 사용: {skill_list}"),
                    "효과: 반복 작업 표준화 + 토큰 절약".into(),
                ],
            })
        }
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// HTTP 클라이언트 — 얇게 (ureq, x-api-key + Bearer)
// ─────────────────────────────────────────────────────────────────────────

const TIMEOUT_SECS: u64 = 10;

pub struct HubClient {
    pub base_url: String,
    pub api_key: String,
    pub token: String,
}

impl HubClient {
    fn req(&self, method: &str, path: &str) -> ureq::Request {
        let mut r = ureq::request(method, &format!("{}{}", self.base_url, path))
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .set("Authorization", &format!("Bearer {}", self.token));
        if !self.api_key.is_empty() {
            r = r.set("x-api-key", &self.api_key);
        }
        r
    }

    /// 토큰 발급(등록). 같은 user_id 재등록 = 같은 계정 재사용 (허브 계약).
    /// 등록 자체는 Bearer가 필요 없으므로 token 없이 호출 가능.
    pub fn register(cfg: &HubConfig) -> Result<(String, String)> {
        let mut r = ureq::post(&format!("{}/agents/register", cfg.base_url))
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .set("Content-Type", "application/json");
        if !cfg.api_key.is_empty() {
            r = r.set("x-api-key", &cfg.api_key);
        }
        let resp: Value = r
            .send_json(serde_json::json!({
                "user_id": cfg.user_id,
                "name": format!("a-mate/{}", cfg.user_id),
                "space_id": cfg.space_id,
            }))
            .map_err(|e| anyhow!("hub register 실패: {e}"))?
            .into_json()?;
        let agent_id = resp
            .get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("register 응답에 agent_id 없음"))?
            .to_string();
        let token = resp
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("register 응답에 token 없음"))?
            .to_string();
        Ok((agent_id, token))
    }

    pub fn open_issue(&self, space_id: &str, title: &str) -> Result<String> {
        let resp: Value = self
            .req("POST", "/issues")
            .set("Content-Type", "application/json")
            .send_json(serde_json::json!({ "title": title, "space_id": space_id }))
            .map_err(|e| anyhow!("hub open_issue 실패: {e}"))?
            .into_json()?;
        resp.get("issue_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("open_issue 응답에 issue_id 없음"))
    }

    /// resolve + 지식 발행. 반환 = 발행된 page_id (publish_knowledge=true).
    pub fn resolve_issue(&self, issue_id: &str, summary: &str, steps: &[String]) -> Result<Option<String>> {
        let resp: Value = self
            .req("POST", &format!("/issues/{issue_id}/resolve"))
            .set("Content-Type", "application/json")
            .send_json(serde_json::json!({
                "summary": summary,
                "steps": steps,
                "publish_knowledge": true,
                "visibility": "org",
            }))
            .map_err(|e| anyhow!("hub resolve_issue 실패: {e}"))?
            .into_json()?;
        Ok(resp.get("page_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
    }
}

/// pull 방향(팀 지식 → 큐레이션 피드) 소스. 토큰이 없으면 None —
/// push 경로가 최초 register로 settings(hub_token)를 채우면 그때부터 활성.
pub fn pull_source(
    cfg: &HubConfig,
    stored_token: Option<String>,
) -> Option<crate::content::HubKnowledgeSource> {
    let token = cfg.token.clone().or(stored_token)?;
    Some(crate::content::HubKnowledgeSource {
        base_url: cfg.base_url.clone(),
        api_key: cfg.api_key.clone(),
        token,
        space_id: cfg.space_id.clone(),
        own_agent_id: cfg.user_id.clone(),
        max_items: 5,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// 오케스트레이션 — CLI·단일 스레드용 (Tauri는 락 규율에 맞춰 단계 호출)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ShareReport {
    pub published: Vec<(String, String)>, // (dedup_key, page_id)
    pub resumed: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

/// 전 과정을 순차 실행: 토큰 확보 → 미완(resume) resolve → 신규 선별·발행 → 상태 persist.
/// 실패는 warnings로 모으고 절대 하드 실패하지 않는다(다음 스캔 재시도).
pub fn run_share(store: &crate::store::SqliteStore, cfg: &HubConfig) -> Result<ShareReport> {
    let mut report = ShareReport::default();
    let now = chrono::Utc::now().to_rfc3339();

    // ① 토큰 확보: env > settings 보존분 > 자동 register(결과는 settings에 보존)
    let token = match cfg.token.clone().or(store.get_setting("knowledge_hub_token")?) {
        Some(t) => t,
        None => match HubClient::register(cfg) {
            Ok((agent_id, t)) => {
                store.set_setting("knowledge_hub_agent_id", &agent_id)?;
                store.set_setting("knowledge_hub_token", &t)?;
                t
            }
            Err(e) => {
                report.warnings.push(format!("register 실패(다음 스캔 재시도): {e}"));
                return Ok(report);
            }
        },
    };
    let client = HubClient {
        base_url: cfg.base_url.clone(),
        api_key: cfg.api_key.clone(),
        token,
    };

    // ② 미완 재개: open은 됐는데 resolve가 안 된 것 (중복 이슈 방지 — 스펙 §6)
    for (dedup_key, issue_id) in store.hub_share_pending()? {
        let Some(f) = store.find_finding(&dedup_key)? else { continue };
        let Some(content) = render_share(&f) else { continue };
        match client.resolve_issue(&issue_id, &content.summary, &content.steps) {
            Ok(Some(page_id)) => {
                store.hub_mark_published(&dedup_key, &page_id, &now)?;
                report.resumed += 1;
                report.published.push((dedup_key, page_id));
            }
            Ok(None) => report.warnings.push(format!("{dedup_key}: resolve 응답에 page_id 없음")),
            Err(e) => report.warnings.push(format!("{dedup_key}: resolve 재시도 실패: {e}")),
        }
    }

    // ③ 신규 선별 → open_issue → resolve(발행)
    let findings = store.list_findings_current(false)?;
    let already = store.hub_shared_or_pending_keys()?;
    let picked: Vec<FindingRow> =
        select_shareable(&findings, &already, cfg.min_tokens).into_iter().cloned().collect();
    report.skipped = findings.len().saturating_sub(picked.len());

    for f in picked {
        let Some(content) = render_share(&f) else { continue };
        let issue_id = match client.open_issue(&cfg.space_id, &content.title) {
            Ok(id) => id,
            Err(e) => {
                report.warnings.push(format!("{}: open_issue 실패: {e}", f.dedup_key));
                continue;
            }
        };
        // open 성공 즉시 기록 — resolve가 실패해도 다음 스캔이 재개(중복 이슈 방지)
        store.hub_mark_issue(&f.dedup_key, &issue_id, &now)?;
        match client.resolve_issue(&issue_id, &content.summary, &content.steps) {
            Ok(Some(page_id)) => {
                store.hub_mark_published(&f.dedup_key, &page_id, &now)?;
                report.published.push((f.dedup_key.clone(), page_id));
            }
            Ok(None) => report.warnings.push(format!("{}: resolve 응답에 page_id 없음", f.dedup_key)),
            Err(e) => report.warnings.push(format!("{}: resolve 실패(다음 스캔 재개): {e}", f.dedup_key)),
        }
    }

    Ok(report)
}

// ─────────────────────────────────────────────────────────────────────────
// 테스트 — 선별·렌더·스크럽 (부수효과 없는 순수 함수만)
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(rule_id: &str, est: u64, occ: u64, status: &str, dedup: &str) -> FindingRow {
        FindingRow {
            rule_id: rule_id.into(),
            severity: "warn".into(),
            scope_host: Some("Windows".into()),
            scope_project: Some("c--users-someone-secret-path".into()),
            scope_kind: "host".into(),
            scope_ref: "Windows".into(),
            evidence: json!({}),
            est_tokens_saved: est,
            prescription: None,
            dedup_key: dedup.into(),
            last_seen: None,
            occurrences: occ,
            status: status.into(),
        }
    }

    #[test]
    fn select_filters_whitelist_threshold_dedup_and_cap() {
        let findings = vec![
            row("R1", 12000, 1, "new", "R1|a"),
            row("R7", 99999, 9, "new", "R7|x"),    // 화이트리스트 밖
            row("R1", 500, 1, "new", "R1|small"),  // 문턱 미달
            row("R2", 3000, 1, "dismissed", "R2|hidden"), // 사용자 숨김
            row("R11", 0, 3, "new", "R11|p"),      // est=0 → occurrences 게이트 통과
            row("R11", 0, 1, "new", "R11|q"),      // est=0 → occurrences 미달
            row("R2", 2000, 1, "new", "R2|b"),
            row("R10", 8000, 1, "new", "R10|c"),
            row("R1", 5000, 1, "new", "R1|d"),     // 상한(3) 초과분
        ];
        let mut already = HashSet::new();
        already.insert("R2|b".to_string()); // 이미 공유됨
        let picked = select_shareable(&findings, &already, DEFAULT_MIN_TOKENS);
        let keys: Vec<&str> = picked.iter().map(|f| f.dedup_key.as_str()).collect();
        // DESC 정렬 입력 가정이 아니라 벡터 순서 그대로 상한 적용 — 여기선 명시 순서로 검증
        assert_eq!(keys, vec!["R1|a", "R11|p", "R10|c"]);
    }

    #[test]
    fn render_r1_has_command_and_no_project_slug() {
        let mut f = row("R1", 12000, 1, "new", "R1|Windows|Windows|jira");
        f.evidence = json!({
            "server": "jira", "resident_tokens_total": 45000, "calls": 0,
            "scope": "host", "note": "약(~) 추정"
        });
        f.prescription = Some(json!({ "kind": "remove_mcp", "payload": { "server": "jira" } }));
        let c = render_share(&f).expect("R1은 공유 대상");
        assert!(c.title.contains("jira"));
        assert!(c.steps.iter().any(|s| s.contains("claude mcp remove jira")));
        // 스크럽: 로컬 경로 슬러그(scope_project)는 어디에도 없다
        let all = format!("{} {} {}", c.title, c.summary, c.steps.join(" "));
        assert!(!all.contains("c--users-someone-secret-path"));
    }

    #[test]
    fn render_r10_scrubs_cwd_prompt_and_session_ids() {
        let mut f = row("R10", 8000, 1, "new", "R10|h|p");
        f.evidence = json!({
            "session_ids": ["sess-abc-123"],
            "total_sessions": 40, "opus_session_count": 33,
            "rep_cwd": "C:/Users/secret/company-repo",
            "rep_first_prompt": "우리 회사 기밀 프로젝트 X를 배포해줘",
            "dominant_model_raw": "claude-opus-4-8"
        });
        f.prescription = Some(json!({ "kind": "automation_model_config", "payload": { "to": "haiku" } }));
        let c = render_share(&f).expect("R10은 공유 대상");
        let all = format!("{} {} {}", c.title, c.summary, c.steps.join(" "));
        assert!(!all.contains("secret"), "cwd 누출 금지");
        assert!(!all.contains("기밀 프로젝트"), "프롬프트 미리보기 누출 금지");
        assert!(!all.contains("sess-abc-123"), "session id 누출 금지");
        assert!(all.contains("haiku"), "처방은 포함");
    }

    #[test]
    fn render_r11_uses_tool_names_only() {
        let mut f = row("R11", 0, 4, "new", "R11|h|p");
        f.evidence = json!({
            "session_ids": ["s-1"],
            "friction_events": [{"cmd": "rm -rf /secret/dir"}],
            "friction_events_count": 4,
            "by_tool": {"Bash": 4}
        });
        f.prescription = Some(json!({ "kind": "permission_allowlist", "payload": { "tools": ["Bash(git *)"] } }));
        let c = render_share(&f).expect("R11은 공유 대상");
        let all = format!("{} {} {}", c.title, c.summary, c.steps.join(" "));
        assert!(all.contains("Bash(git *)"));
        assert!(!all.contains("rm -rf"), "friction 이벤트 상세(명령줄) 누출 금지");
        assert!(!all.contains("s-1"));
    }

    #[test]
    fn render_non_whitelisted_rule_is_none() {
        assert!(render_share(&row("R7", 99999, 9, "new", "R7|x")).is_none());
        assert!(render_share(&row("R5", 7000, 2, "new", "R5|y")).is_none());
    }

    #[test]
    fn config_absent_env_is_none() {
        // 주의: 환경변수 전역 상태라 미설정 케이스만 검증 (설정 케이스는 E2E에서)
        std::env::remove_var("SPACE_A_HUB_URL");
        assert!(HubConfig::from_env().is_none());
    }
}
