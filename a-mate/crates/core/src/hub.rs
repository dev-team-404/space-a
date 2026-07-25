//! a-hub("Space A") 코칭 지식 공유 — 유의미한 Finding을 이슈→해결 흐름으로 발행.
//!
//! 스펙: docs/design/overview-mentor/specs/2026-07-18-hub-knowledge-sharing-design.md
//! 원칙: 결정론 본문만(정밀도의 선) · 개인정보 스크럽(§4) · 평생 1회 발행(나깅 방지) ·
//! 실패 무해(파이프라인 편승) · 환경변수 미설정 시 조용히 no-op(프라이버시 기본 = 로컬).

use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::store::FindingRow;

/// 공유 화이트리스트 — 팀 일반화 가능 ∩ 개인 텍스트 없음 인 **살아있는** 룰만. 그 외는 기본 폐쇄.
///
/// ⚠ 이 목록은 반드시 `ops::registered_rule_ids()`의 부분집합이어야 한다. 한때
/// 은퇴한 룰(R1·R2·R10·R11·R12)만 남아 발행이 영구 0건이었다(2026-07-25 발견).
/// 불변식은 `share_rules_are_active_and_renderable` 테스트가 강제한다 — 룰을 은퇴시키면
/// 그 테스트가 깨지므로 여기를 함께 갱신하게 된다.
///
/// 현재 대상 판정(스펙 §3):
/// - **R8**: 증거가 서버명·건수·문자수뿐 → 개인 텍스트 없음, 같은 MCP를 쓰는 팀에 그대로 적용 ✅
/// - R6: `repeated_prompt`·`member_norms`가 프롬프트 원문 → 스크럽하면 의미 소멸 ❌
/// - R7: 개인의 세션 단위 모델 선택 → 팀 지식 아님 ❌
pub const SHARE_RULES: [&str; 1] = ["R8"];
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
    /// 해석 우선순위: **설정(store) → 환경변수**. `resolve_engine`과 같은 규율이다.
    ///
    /// store를 먼저 보는 이유: `.env`는 `#[cfg(debug_assertions)]`에서만 로드되므로
    /// **배포(릴리스) 빌드는 환경변수만으로는 허브에 절대 연결되지 않는다.** 설정 창에서
    /// 값을 넣으면 배포본에서도 지식 공유가 동작해야 한다.
    /// (설정 키는 `knowledge_hub_*` — 기존 `hub_*` 키는 Life 서버 몫이라 이름을 분리한다.)
    pub fn resolve(store: &crate::store::SqliteStore) -> Option<HubConfig> {
        let get = |k: &str| {
            store
                .get_setting(k)
                .ok()
                .flatten()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let Some(base_url) = get("knowledge_hub_url") else {
            return HubConfig::from_env();
        };
        // 설정에서 명시적으로 껐으면 env로 되살아나지 않는다.
        if store.get_setting("knowledge_hub_share").ok().flatten().as_deref() == Some("off") {
            return None;
        }
        Some(HubConfig {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: get("knowledge_hub_api_key").unwrap_or_default(),
            token: get("knowledge_hub_token"),
            space_id: get("knowledge_hub_space_id").unwrap_or_else(|| "sw-innov".into()),
            user_id: get("knowledge_hub_user")
                .or_else(|| get("user_name"))
                .unwrap_or_else(|| "unknown".into()),
            min_tokens: get("knowledge_hub_min_tokens")
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MIN_TOKENS),
        })
    }

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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShareContent {
    pub title: String,
    pub summary: String,
    pub steps: Vec<String>,
    /// 같은 지식 판별용 안정 키(`share_marker`). summary에 심겨 발행 Page 제목에 남는다.
    /// 비어 있으면 검색·인용을 시도하지 않는다(중복 발행 방지보다 오인용 방지가 우선).
    pub marker: String,
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
                marker: String::new(),
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
                marker: String::new(),
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
                marker: String::new(),
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
                marker: String::new(),
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
                marker: String::new(),
            })
        }
        "R8" => {
            // 스크럽 안전: R8 증거에는 서버명·건수·문자수만 있고 프롬프트·경로·session_id가 없다.
            let server = payload
                .get("server")
                .and_then(|v| v.as_str())
                .or_else(|| ev_str(ev, "server"))
                .unwrap_or("(unknown)")
                .to_string();
            let n = ev_u64(ev, "large_result_count");
            let avg_tok = ev_u64(ev, "approx_tokens_avg");
            let total_tok = ev_u64(ev, "approx_tokens_total");
            let days = ev_u64(ev, "window_days");
            Some(ShareContent {
                title: share_title_r8(&server),
                summary: format!(
                    "{marker} MCP '{server}'가 최근 {days}일 동안 대형 결과를 {n}회 반환했다\
                     (평균 약 ~{avg_tok} 토큰, 합계 약 ~{total_tok} 토큰 추정). 결과 전체가 \
                     컨텍스트에 실리면 정작 중요한 내용이 밀려난다. 같은 MCP를 쓰는 팀원에게도 \
                     동일하게 발생하므로, 질의 범위를 좁히는 방법은 팀 공용 지식이 된다.",
                    marker = share_marker("R8", &server)
                ),
                steps: vec![
                    format!("감지: '{server}' 응답이 임계 문자수를 넘긴 호출 {n}회 (a-mate R8)"),
                    "확인: 그 호출에서 실제로 필요한 필드가 무엇인지 좁힌다".into(),
                    format!("적용: '{server}' 호출 시 범위·필드·limit 파라미터로 결과를 줄인다"),
                    format!("효과: 호출당 약 ~{avg_tok} 토큰 규모의 컨텍스트 점유 감소 (추정)"),
                ],
                marker: share_marker("R8", &server),
            })
        }
        _ => None,
    }
}

/// R8 공유 제목(이슈 제목).
pub fn share_title_r8(server: &str) -> String {
    format!("[a-mate] MCP '{server}' 대형 결과 반복 — 질의 범위 좁히기")
}

/// 같은 지식인지 기계가 판별하는 안정 키. 예: `[a-mate:R8:github]`.
///
/// **본문(summary) 안에 심는다.** 허브가 `resolve_issue`에서 발행 Page의 제목을
/// **이슈 제목이 아니라 summary로** 만들기 때문이다(`services.py` resolve_issue: `title=summary`).
/// 그래서 이슈 제목으로는 나중에 그 지식을 다시 찾을 수 없다 — E2E로 확인한 사실이다.
/// 마커는 provenance 표시도 겸한다(이 글이 a-mate R8에서 왔다는 근거).
pub fn share_marker(rule_id: &str, key: &str) -> String {
    format!("[a-mate:{rule_id}:{key}]")
}

/// 검색 결과에서 인용할 페이지를 고른다 (순수 함수).
///
/// 규칙: 제목에 **마커가 포함**되고 **자기 글이 아닌** 첫 페이지.
/// 마커는 결정론이라 같은 룰·같은 대상이면 동일하다 — 느슨한 유사도 매칭은 오인용 위험이 있어 쓰지 않는다.
pub fn pick_citable<'a>(
    results: &'a [HubSearchHit],
    marker: &str,
    own_agent_id: &str,
) -> Option<&'a HubSearchHit> {
    results
        .iter()
        .find(|h| h.title.contains(marker) && h.created_by.as_deref() != Some(own_agent_id))
}

/// `POST /pages/search` 결과 1건.
#[derive(Debug, Clone, PartialEq)]
pub struct HubSearchHit {
    pub page_id: String,
    pub title: String,
    pub created_by: Option<String>,
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
        Self::register_into(cfg, &cfg.space_id)
    }

    /// 특정 공간으로 등록(멤버십 병합) — 텔레메트리 전용 공간 부트스트랩용.
    pub fn register_into(cfg: &HubConfig, space_id: &str) -> Result<(String, String)> {
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
                "space_id": space_id,
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

    /// Page 저작 (텔레메트리 캐리어). 반환 = page_id.
    pub fn create_page(&self, space_id: &str, title: &str, body: &str) -> Result<String> {
        let resp: Value = self
            .req("POST", &format!("/spaces/{space_id}/pages"))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_json(serde_json::json!({ "title": title, "body": body, "visibility": "org" }))
            .map_err(|e| anyhow!("hub create_page 실패: {e}"))?
            .into_json()?;
        resp.get("page_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("create_page 응답에 page_id 없음"))
    }

    /// 공간 생성 (이미 있으면 서버가 4xx — 호출자가 무시).
    pub fn create_space(&self, id: &str, name: &str) -> Result<()> {
        self.req("POST", "/spaces")
            .set("Content-Type", "application/json")
            .send_json(serde_json::json!({ "id": id, "name": name }))
            .map_err(|e| anyhow!("hub create_space 실패: {e}"))?;
        Ok(())
    }

    /// 지식 검색 — 발행 전에 "이미 누가 풀어놨는지" 확인한다 (스펙 §4).
    ///
    /// `space_id=None`이면 **org 전체**(내가 볼 수 있는 모든 공간)를 검색한다. 기본값이 None인
    /// 이유: README의 핵심 시나리오가 "A팀이 등록한 걸 **B팀**이 찾아 쓴다"이기 때문이다.
    /// 자기 공간으로 좁히면 교차 팀 재사용이 구조적으로 불가능해진다(허브 `search_knowledge`는
    /// space_id가 있으면 그 공간만 본다).
    pub fn search_knowledge(
        &self,
        space_id: Option<&str>,
        query: &str,
        limit: u32,
    ) -> Result<Vec<HubSearchHit>> {
        let mut body = serde_json::json!({ "query": query, "limit": limit });
        if let Some(sid) = space_id {
            body["space_id"] = serde_json::json!(sid);
        }
        let resp: Value = self
            .req("POST", "/pages/search")
            .set("Content-Type", "application/json; charset=utf-8")
            .send_json(body)
            .map_err(|e| anyhow!("hub search_knowledge 실패: {e}"))?
            .into_json()?;
        Ok(resp
            .get("results")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        Some(HubSearchHit {
                            page_id: r.get("page_id")?.as_str()?.to_string(),
                            title: r.get("title")?.as_str()?.to_string(),
                            created_by: r.get("created_by").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// 기존 지식 인용 — 이 호출이 ReuseEvent를 만든다(재사용의 실증). 반환 = reuse_id.
    pub fn cite_knowledge(&self, issue_id: &str, page_id: &str, note: &str) -> Result<String> {
        let resp: Value = self
            .req("POST", &format!("/issues/{issue_id}/cite"))
            .set("Content-Type", "application/json; charset=utf-8")
            .send_json(serde_json::json!({ "page_id": page_id, "note": note }))
            .map_err(|e| anyhow!("hub cite_knowledge 실패: {e}"))?
            .into_json()?;
        resp.get("reuse_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("cite 응답에 reuse_id 없음"))
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

// ─────────────────────────────────────────────────────────────────────────
// 텔레메트리 — 목표 아키텍처(#46)의 "A-Mate → A-Hub Work" 행 구현.
// 파생 신호만(카운트·집계·상태): 토큰 사용량 · 모델 믹스 · 코칭 채택/절감 · MCP 사용 카운트.
// 원문·경로·프롬프트는 절대 싣지 않는다 ("원문은 로컬을 떠나지 않는다").
// 캐리어: 허브에 전용 엔드포인트가 생기기 전까지 기존 C2 create_page를 전용 공간에 사용
// (계약 제안: docs/design/overview-mentor/specs/2026-07-18-hub-telemetry-design.md).
// ─────────────────────────────────────────────────────────────────────────

/// 계약 버전 태그 — a-lens 등 소비자가 파싱 분기할 수 있게 본문 JSON에 명시.
pub const TELEMETRY_KIND: &str = "a-mate-telemetry/v0";

pub fn telemetry_space_id() -> String {
    std::env::var("SPACE_A_TELEMETRY_SPACE_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "a-mate-telemetry".into())
}

/// 하루치 텔레메트리 브리프 — 전부 store 집계에서 파생(순수 조회, LLM·원문 무개입).
pub fn build_telemetry_brief(
    store: &crate::store::SqliteStore,
    date: &str,
    agent_id: &str,
) -> Result<serde_json::Value> {
    let day = store.summary_for_date(date)?;
    let mix: serde_json::Map<String, serde_json::Value> = store
        .model_mix_for_date(date)?
        .into_iter()
        .map(|(m, toks)| (m, serde_json::json!(toks)))
        .collect();
    let mcp: serde_json::Map<String, serde_json::Value> = store
        .mcp_call_counts_for_date(date)?
        .into_iter()
        .map(|(s, n)| (s, serde_json::json!(n)))
        .collect();
    let coaching: serde_json::Map<String, serde_json::Value> = store
        .findings_status_counts()?
        .into_iter()
        .map(|(s, n)| (s, serde_json::json!(n)))
        .collect();
    Ok(serde_json::json!({
        "kind": TELEMETRY_KIND,
        "date": date,
        "agent": agent_id,
        "sessions": day.session_count,
        "tokens": {
            "input": day.tok_input,
            "output": day.tok_output,
            "cache_read": day.tok_cache_read,
            "cache_create": day.tok_cache_create,
        },
        "model_mix": mix,
        "coaching": {
            "by_status": coaching,
            "est_tokens_savable": store.sum_est_tokens_saved()?,
        },
        "mcp_calls": mcp,
    }))
}

#[derive(Debug)]
pub enum TelemetryOutcome {
    Published { date: String, page_id: String },
    AlreadySent,
    Skipped(String),
}

/// 텔레메트리 상태 키 (hub_share_state 재사용 — page_id가 있으면 그 날짜는 완료).
pub fn telemetry_state_key(date: &str) -> String {
    format!("telemetry|{date}")
}

/// 활동이 전혀 없는 날은 발행하지 않는다 (허브 노이즈 방지 — 빈 날은 신호가 아니다).
pub fn telemetry_is_empty(brief: &serde_json::Value) -> bool {
    let sessions = brief.get("sessions").and_then(|v| v.as_u64()).unwrap_or(0);
    let toks = brief
        .get("tokens")
        .and_then(|t| t.as_object())
        .map(|o| o.values().filter_map(|v| v.as_u64()).sum::<u64>())
        .unwrap_or(0);
    sessions == 0 && toks == 0
}

/// 하루 1회 텔레메트리 발행. 전용 공간이 없으면 생성 + 멤버십 확보(재등록) 후 1회 재시도.
pub fn run_telemetry_push(
    store: &crate::store::SqliteStore,
    cfg: &HubConfig,
    date: &str,
) -> Result<TelemetryOutcome> {
    let key = telemetry_state_key(date);
    if store.hub_shared_or_pending_keys()?.contains(&key) {
        return Ok(TelemetryOutcome::AlreadySent);
    }
    // 빈 날 가드 — 네트워크 전에 판정 (마크하지 않음: 뒤늦은 backfill이 있으면 다음에 발행)
    {
        let probe = build_telemetry_brief(store, date, &cfg.user_id)?;
        if telemetry_is_empty(&probe) {
            return Ok(TelemetryOutcome::Skipped(format!("{date}: 활동 없음 — 발행 생략")));
        }
    }
    // 토큰 확보는 기존 공간(cfg.space_id) 기준 — 텔레메트리 공간은 아래 부트스트랩이 담당.
    let token = match cfg.token.clone().or(store.get_setting("knowledge_hub_token")?) {
        Some(t) => t,
        None => match HubClient::register(cfg) {
            Ok((agent_id, t)) => {
                store.set_setting("knowledge_hub_agent_id", &agent_id)?;
                store.set_setting("knowledge_hub_token", &t)?;
                t
            }
            Err(e) => return Ok(TelemetryOutcome::Skipped(format!("register 실패: {e}"))),
        },
    };
    let mut client = HubClient {
        base_url: cfg.base_url.clone(),
        api_key: cfg.api_key.clone(),
        token,
    };
    let brief = build_telemetry_brief(store, date, &cfg.user_id)?;
    let title = format!("[telemetry] {} {}", cfg.user_id, date);
    let body = serde_json::to_string_pretty(&brief)?;
    let space = telemetry_space_id();

    let page_id = match client.create_page(&space, &title, &body) {
        Ok(id) => id,
        Err(_first) => {
            // 부트스트랩: 공간 미존재/미멤버십 가능 — 공간 생성(이미 있으면 무시) →
            // 재등록으로 멤버십 병합(+새 토큰 보존) → 1회 재시도.
            let _ = client.create_space(&space, "a-mate telemetry");
            match HubClient::register_into(cfg, &space) {
                Ok((agent_id, t)) => {
                    store.set_setting("knowledge_hub_agent_id", &agent_id)?;
                    store.set_setting("knowledge_hub_token", &t)?;
                    client.token = t;
                }
                Err(e) => return Ok(TelemetryOutcome::Skipped(format!("멤버십 확보 실패: {e}"))),
            }
            match client.create_page(&space, &title, &body) {
                Ok(id) => id,
                Err(e) => return Ok(TelemetryOutcome::Skipped(format!("create_page 실패: {e}"))),
            }
        }
    };
    let now = chrono::Utc::now().to_rfc3339();
    store.hub_mark_published(&key, &page_id, &now)?;
    Ok(TelemetryOutcome::Published { date: date.to_string(), page_id })
}

// ─────────────────────────────────────────────────────────────────────────
// 세션 회고 — "고생 끝 해결" 세션을 로컬 생성 요약으로 발행 (스펙 2026-07-19).
// 트리거는 결정론(오류→회복), 서사는 Engine(LLM), 수치는 브리프만(정밀도의 선).
// 허브 글쓰기 단일 창구 = a-mate: 세션 속 에이전트는 재사용 가치를 모른다(사후 조망 필요).
// ─────────────────────────────────────────────────────────────────────────

/// 하루 상한 — 허브도 나깅 방지 대상.
pub const RETRO_DAILY_CAP: u64 = 2;
pub const RETRO_MIN_ERRORS_DEFAULT: u64 = 3;
pub const RETRO_MIN_EVENTS_DEFAULT: u64 = 20;
/// 세션 "종료" 판정 — 마지막 활동 후 이 시간 조용하면 끝난 세션.
pub const RETRO_SETTLE_MINUTES: i64 = 30;

pub fn retro_state_key(session_id: &str) -> String {
    format!("retro|{session_id}")
}

fn retro_min_errors() -> u64 {
    std::env::var("SPACE_A_RETRO_MIN_ERRORS").ok().and_then(|v| v.parse().ok()).unwrap_or(RETRO_MIN_ERRORS_DEFAULT)
}
fn retro_min_events() -> u64 {
    std::env::var("SPACE_A_RETRO_MIN_EVENTS").ok().and_then(|v| v.parse().ok()).unwrap_or(RETRO_MIN_EVENTS_DEFAULT)
}
pub fn retro_enabled() -> bool {
    !std::env::var("SPACE_A_RETRO").map(|v| v == "off").unwrap_or(false)
}

/// 소요 분 계산 (rfc3339). 실패 시 0.
fn minutes_between(a: Option<&str>, b: Option<&str>) -> i64 {
    match (
        a.and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()),
        b.and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()),
    ) {
        (Some(x), Some(y)) => (y - x).num_minutes().max(0),
        _ => 0,
    }
}

/// 회고 후보 선별 — store 읽기만 (락 규율: 호출자가 짧은 락 안에서 부른다).
/// 반환: (state_key, 세션). 회복(last ok) ∧ 미발행 ∧ 하루 상한 이내.
pub fn select_retros(
    store: &crate::store::SqliteStore,
    now_utc: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<(String, crate::store::StruggleSession)>> {
    let cutoff = (now_utc - chrono::Duration::minutes(RETRO_SETTLE_MINUTES)).to_rfc3339();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let already = store.hub_shared_or_pending_keys()?;
    let used_today = store.hub_share_count_on("retro|", &today)?;
    let remaining = RETRO_DAILY_CAP.saturating_sub(used_today) as usize;
    let out = store
        .struggle_sessions(retro_min_errors(), retro_min_events(), &cutoff)?
        .into_iter()
        .filter(|s| s.last_result_ok)
        .map(|s| (retro_state_key(&s.session_id), s))
        .filter(|(k, _)| !already.contains(k))
        .take(remaining)
        .collect();
    Ok(out)
}

/// Engine에 넘길 프롬프트 — 정밀도의 선: 브리프의 사실·수치만 인용, 새 수치 발명 금지.
/// 원문(작업 미리보기)은 여기(Engine)까지만 간다.
pub fn retro_prompt(s: &crate::store::StruggleSession) -> (String, String) {
    let tools = s
        .error_tools
        .iter()
        .map(|(t, n)| format!("{t} {n}회"))
        .collect::<Vec<_>>()
        .join(", ");
    let mins = minutes_between(s.first_ts.as_deref(), s.last_ts.as_deref());
    let system = "너는 개발팀 지식 허브에 올릴 세션 회고를 쓰는 조수다. 아래 사실만 근거로 \
        한국어로 쓴다. 새로운 수치·파일명·경로를 지어내지 않는다. 회사·개인 식별 정보는 넣지 않는다. \
        JSON으로만 답한다: {\"title\": \"한 줄 제목(문제+해결 요지)\", \"summary\": \"2~3문장 — 무엇을 하다 어떤 시행착오를 겪었고 어떻게 마무리됐는지\"}"
        .to_string();
    let user = format!(
        "작업(첫 요청 미리보기): {}\n시행착오: 도구 오류 총 {}회 ({})\n마무리: 마지막 도구 실행은 정상(회복)\n규모: 이벤트 {}건, 약 {}분",
        s.first_prompt_preview.as_deref().unwrap_or("(미상)"),
        s.error_count,
        tools,
        s.total_events,
        mins,
    );
    (system, user)
}

/// Engine 응답에서 title/summary 추출 (관대한 파싱 — 첫 { .. } 블록).
pub fn parse_retro_reply(text: &str) -> Option<(String, String)> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    let title = v.get("title")?.as_str()?.trim().to_string();
    let summary = v.get("summary")?.as_str()?.trim().to_string();
    if title.is_empty() || summary.is_empty() {
        return None;
    }
    Some((title, summary))
}

/// 허브에 실을 steps — 결정론만 (원문 미리보기·경로·세션 id 금지).
pub fn retro_steps(s: &crate::store::StruggleSession) -> Vec<String> {
    let tools = s
        .error_tools
        .iter()
        .map(|(t, n)| format!("{t} {n}회"))
        .collect::<Vec<_>>()
        .join(", ");
    let mins = minutes_between(s.first_ts.as_deref(), s.last_ts.as_deref());
    vec![
        format!("시행착오: 도구 오류 총 {}회 ({tools})", s.error_count),
        "해결: 마지막 도구 실행 정상 — 회복 확인 (a-mate 세션 회고)".into(),
        format!("규모: 이벤트 {}건 · 약 {mins}분", s.total_events),
    ]
}

/// CLI·단일 스레드용 전 과정. Engine 없으면 발행 보류(품질 > 정시성).
pub fn run_retro_push(
    store: &crate::store::SqliteStore,
    cfg: &HubConfig,
    engine: &dyn crate::diary::engine::Engine,
) -> Result<Vec<(String, String)>> {
    let mut published = Vec::new();
    if !retro_enabled() {
        return Ok(published);
    }
    let candidates = select_retros(store, chrono::Utc::now())?;
    if candidates.is_empty() {
        return Ok(published);
    }
    let token = match cfg.token.clone().or(store.get_setting("knowledge_hub_token")?) {
        Some(t) => t,
        None => {
            let (agent_id, t) = HubClient::register(cfg)?;
            store.set_setting("knowledge_hub_agent_id", &agent_id)?;
            store.set_setting("knowledge_hub_token", &t)?;
            t
        }
    };
    let client = HubClient {
        base_url: cfg.base_url.clone(),
        api_key: cfg.api_key.clone(),
        token,
    };
    let now = chrono::Utc::now().to_rfc3339();
    for (key, s) in candidates {
        let (system, user) = retro_prompt(&s);
        let reply = match engine.generate(&system, &user) {
            Ok(o) => o.text,
            Err(e) => {
                eprintln!("[retro] engine 실패(보류): {e}");
                continue;
            }
        };
        let Some((title, summary)) = parse_retro_reply(&reply) else {
            eprintln!("[retro] 응답 파싱 실패(보류)");
            continue;
        };
        let issue_id = client.open_issue(&cfg.space_id, &format!("[a-mate 회고] {title}"))?;
        store.hub_mark_issue(&key, &issue_id, &now)?;
        if let Some(page_id) = client.resolve_issue(&issue_id, &summary, &retro_steps(&s))? {
            store.hub_mark_published(&key, &page_id, &now)?;
            published.push((s.session_id.clone(), page_id));
        }
    }
    Ok(published)
}

/// pull 방향(팀 지식 → 큐레이션 피드) 소스. 토큰이 없으면 None —
/// push 경로가 최초 register로 settings(knowledge_hub_token)를 채우면 그때부터 활성.
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
    /// 남이 이미 발행한 지식을 인용한 건 — (dedup_key, 인용한 page_id, reuse_id).
    /// README의 "다른 에이전트가 검색·인용해 재사용"이 실제로 일어난 증거다.
    pub cited: Vec<(String, String, String)>,
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

        // ③-a 발행 전에 검색: 이미 누가 풀어놨으면 중복 발행 대신 인용한다(스펙 §4).
        //     질의·매칭 모두 마커를 쓴다 — 허브가 발행 Page 제목을 summary로 만들기 때문에
        //     이슈 제목으로는 되찾을 수 없다(E2E로 확인). org 전체를 뒤져 교차 팀 재사용을 가능케 한다.
        //     검색 실패는 무해 — 빈 결과로 취급해 기존 발행 경로로 폴백한다.
        let citable = if content.marker.is_empty() {
            None
        } else {
            let hits = match client.search_knowledge(None, &content.marker, 5) {
                Ok(h) => h,
                Err(e) => {
                    report.warnings.push(format!("{}: search 실패(발행으로 폴백): {e}", f.dedup_key));
                    Vec::new()
                }
            };
            pick_citable(&hits, &content.marker, &cfg.user_id).cloned()
        };

        let issue_id = match client.open_issue(&cfg.space_id, &content.title) {
            Ok(id) => id,
            Err(e) => {
                report.warnings.push(format!("{}: open_issue 실패: {e}", f.dedup_key));
                continue;
            }
        };
        // open 성공 즉시 기록 — 이후 단계가 실패해도 다음 스캔이 재개(중복 이슈 방지)
        store.hub_mark_issue(&f.dedup_key, &issue_id, &now)?;

        match citable {
            // ③-b 재사용: 남이 발행한 같은 지식을 인용한다 — 이 순간 ReuseEvent가 생긴다.
            Some(hit) => match client.cite_knowledge(&issue_id, &hit.page_id, "a-mate: 같은 진단을 로컬에서 재확인") {
                Ok(reuse_id) => {
                    // 인용도 "이 finding은 처리됨"이므로 published로 기록 — 평생 1회 원칙 유지.
                    // page_id는 인용한 상대의 페이지 id.
                    store.hub_mark_published(&f.dedup_key, &hit.page_id, &now)?;
                    report.cited.push((f.dedup_key.clone(), hit.page_id.clone(), reuse_id));
                }
                Err(e) => report.warnings.push(format!("{}: cite 실패(다음 스캔 재개): {e}", f.dedup_key)),
            },
            // ③-c 신규 지식 발행 (기존 경로)
            None => match client.resolve_issue(&issue_id, &content.summary, &content.steps) {
                Ok(Some(page_id)) => {
                    store.hub_mark_published(&f.dedup_key, &page_id, &now)?;
                    report.published.push((f.dedup_key.clone(), page_id));
                }
                Ok(None) => report.warnings.push(format!("{}: resolve 응답에 page_id 없음", f.dedup_key)),
                Err(e) => report.warnings.push(format!("{}: resolve 실패(다음 스캔 재개): {e}", f.dedup_key)),
            },
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
            judgment: None,
        }
    }

    #[test]
    fn select_filters_whitelist_threshold_dedup_and_cap() {
        // R8은 est_tokens_saved=0 룰이라 occurrences 게이트로 선별된다.
        let findings = vec![
            row("R8", 0, 3, "new", "R8|alpha"),
            row("R7", 99999, 9, "new", "R7|x"),           // 화이트리스트 밖
            row("R6", 0, 9, "new", "R6|p"),               // 화이트리스트 밖(프롬프트 원문 보호)
            row("R8", 0, 1, "new", "R8|too-rare"),        // occurrences 미달
            row("R8", 0, 5, "dismissed", "R8|hidden"),    // 사용자 숨김
            row("R8", 0, 4, "new", "R8|beta"),
            row("R8", 0, 4, "new", "R8|already"),         // 이미 공유됨
            row("R8", 0, 7, "new", "R8|gamma"),
            row("R8", 0, 8, "new", "R8|over-cap"),        // 상한(3) 초과분
        ];
        let mut already = HashSet::new();
        already.insert("R8|already".to_string());
        let picked = select_shareable(&findings, &already, DEFAULT_MIN_TOKENS);
        let keys: Vec<&str> = picked.iter().map(|f| f.dedup_key.as_str()).collect();
        assert_eq!(keys, vec!["R8|alpha", "R8|beta", "R8|gamma"]);
    }

    /// 회귀 방지 — 2026-07-25에 발견된 사멸의 재발을 막는다.
    ///
    /// 화이트리스트가 은퇴한 룰(R1·R2·R10·R11·R12)만 가리켜 a-mate가 **영구적으로**
    /// 아무것도 발행하지 못했다. 룰을 은퇴시키면서 여기를 안 고치면 이 테스트가 깨진다.
    #[test]
    fn share_rules_are_active_and_renderable() {
        let active = crate::ops::registered_rule_ids();
        for rule in SHARE_RULES {
            assert!(
                active.contains(&rule),
                "SHARE_RULES의 '{rule}'이 등록된 룰이 아니다 (등록: {active:?}). \
                 룰을 은퇴시켰다면 SHARE_RULES도 함께 갱신할 것 — 안 그러면 발행이 영구 0건이 된다."
            );
        }
        assert!(!SHARE_RULES.is_empty(), "화이트리스트가 비면 공유가 영구 0건이 된다");
    }

    #[test]
    fn every_share_rule_has_a_renderer() {
        for rule in SHARE_RULES {
            let f = row(rule, 0, 3, "new", &format!("{rule}|x"));
            assert!(
                render_share(&f).is_some(),
                "'{rule}'은 공유 대상인데 render_share가 None을 반환한다"
            );
        }
    }

    #[test]
    fn render_r8_is_deterministic_and_has_no_personal_text() {
        let mut f = row("R8", 0, 4, "new", "R8|github");
        f.evidence = json!({
            "server": "github",
            "large_result_count": 12,
            "avg_chars": 40000,
            "max_chars": 90000,
            "approx_tokens_total": 120000,
            "approx_tokens_avg": 10000,
            "char_threshold": 20000,
            "window_days": 14,
        });
        let c = render_share(&f).expect("R8은 렌더된다");

        // 제목은 결정론.
        assert_eq!(c.title, share_title_r8("github"));
        assert_eq!(c.title, "[a-mate] MCP 'github' 대형 결과 반복 — 질의 범위 좁히기");

        // 마커는 summary에 심긴다 — 허브가 발행 Page 제목을 summary로 만들기 때문에,
        // 마커가 본문에 없으면 나중에 그 지식을 되찾아 인용할 수 없다.
        assert_eq!(c.marker, "[a-mate:R8:github]");
        assert!(c.summary.contains(&c.marker), "마커가 summary에 없으면 재사용 판별이 불가능하다");

        // 스크럽: 로컬 경로 슬러그·세션·프롬프트가 새어나가지 않는다.
        let blob = format!("{} {} {}", c.title, c.summary, c.steps.join(" "));
        assert!(!blob.contains("c--users"), "프로젝트 경로 슬러그 유출");
        assert!(!blob.contains("secret"), "로컬 경로 유출");
        assert!(blob.contains("github") && blob.contains("12"), "핵심 근거는 남아야 한다");
    }

    #[test]
    fn pick_citable_matches_marker_and_skips_own_pages() {
        let want = share_marker("R8", "github");
        // 실제 허브가 만드는 Page 제목 = summary(마커 포함)
        let page_title = |m: &str| format!("{m} MCP가 큰 결과를 반환했다…");
        let hit = |title: String, by: &str| HubSearchHit {
            page_id: format!("page-{by}"),
            title,
            created_by: Some(by.into()),
        };

        // 남이 쓴 같은 마커 → 인용
        let hits = vec![hit("무관한 글".into(), "bob"), hit(page_title(&want), "bob")];
        assert_eq!(pick_citable(&hits, &want, "me").unwrap().page_id, "page-bob");

        // 내 글만 있으면 인용하지 않는다 (자기 인용은 재사용이 아니다)
        let mine = vec![hit(page_title(&want), "me")];
        assert!(pick_citable(&mine, &want, "me").is_none());

        // 다른 대상(서버)이면 인용하지 않는다 (느슨한 매칭 금지)
        let other = vec![hit(page_title(&share_marker("R8", "gitlab")), "bob")];
        assert!(pick_citable(&other, &want, "me").is_none());

        // created_by가 없는 응답도 안전하게 인용 대상이 된다
        let anon = vec![HubSearchHit { page_id: "p1".into(), title: page_title(&want), created_by: None }];
        assert_eq!(pick_citable(&anon, &want, "me").unwrap().page_id, "p1");
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

    /// 배포 빌드는 .env를 로드하지 않으므로, 설정(store)만으로도 허브가 잡혀야 한다.
    #[test]
    fn config_resolves_from_store_before_env() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        store.set_setting("knowledge_hub_url", "https://hub.example.com/").unwrap();
        store.set_setting("knowledge_hub_space_id", "sw-innov").unwrap();
        store.set_setting("knowledge_hub_user", "alice").unwrap();

        let cfg = HubConfig::resolve(&store).expect("설정만으로 허브 구성이 잡혀야 한다");
        assert_eq!(cfg.base_url, "https://hub.example.com", "끝 슬래시는 정규화된다");
        assert_eq!(cfg.space_id, "sw-innov");
        assert_eq!(cfg.user_id, "alice");
    }

    #[test]
    fn config_off_switch_in_store_wins_over_env() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        store.set_setting("knowledge_hub_url", "https://hub.example.com").unwrap();
        store.set_setting("knowledge_hub_share", "off").unwrap();
        assert!(HubConfig::resolve(&store).is_none(), "설정에서 끄면 env로 되살아나지 않는다");
    }

    #[test]
    fn config_absent_env_is_none() {
        // 주의: 환경변수 전역 상태라 미설정 케이스만 검증 (설정 케이스는 E2E에서)
        std::env::remove_var("SPACE_A_HUB_URL");
        assert!(HubConfig::from_env().is_none());
    }

    // ── 세션 회고 — 선별 조건·스크럽·파싱 ──

    fn seed_session(store: &crate::store::SqliteStore, sess: &str, n_err: u64, last_ok: bool, n_pad: u64) {
        use crate::model::{EventKind, NormModel, NormalizedEvent, ResultStatus, TokenUsage, ToolKind};
        let mut evs = Vec::new();
        let mut off = 0u64;
        let mut push = |kind: EventKind, off: &mut u64| {
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "c--users-secret".into(),
                session_id: sess.into(), uuid: Some(format!("{sess}-u{off}")), parent_uuid: None,
                is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
                source_file: "s.jsonl".into(), source_offset: *off,
                msg_id: None,
                kind,
            });
            *off += 1;
        };
        // 패딩(규모) — assistant turns
        for _ in 0..n_pad {
            push(EventKind::AssistantTurn {
                model: NormModel::from_raw_id("claude-haiku-4-5"),
                usage: TokenUsage { input: 10, ..Default::default() },
                web_search: 0, web_fetch: 0,
            }, &mut off);
        }
        // 오류 call/result 쌍
        for i in 0..n_err {
            let tid = format!("{sess}-t{i}");
            push(EventKind::ToolCall {
                kind: ToolKind::Execute, raw_name: "Bash".into(),
                target: Some("C:/secret/build.sh".into()), tool_use_id: Some(tid.clone()),
            }, &mut off);
            push(EventKind::ToolResult { tool_use_id: tid, status: ResultStatus::Error, result_len: 0 }, &mut off);
        }
        // 마지막 결과
        let tid = format!("{sess}-tf");
        push(EventKind::ToolCall {
            kind: ToolKind::Execute, raw_name: "Bash".into(),
            target: None, tool_use_id: Some(tid.clone()),
        }, &mut off);
        push(EventKind::ToolResult {
            tool_use_id: tid,
            status: if last_ok { ResultStatus::Ok } else { ResultStatus::Error },
            result_len: 0,
        }, &mut off);
        store.upsert_events(&evs).unwrap();
    }

    #[test]
    fn retro_selects_only_recovered_struggles_and_steps_are_scrubbed() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        seed_session(&store, "sA", 4, true, 20);  // 고생 + 회복 → 선정
        seed_session(&store, "sB", 4, false, 20); // 회복 없음 → 제외
        seed_session(&store, "sC", 1, true, 20);  // 오류 미달 → 제외
        let picked = select_retros(&store, chrono::Utc::now()).unwrap();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].1.session_id, "sA");
        assert!(picked[0].1.error_tools.iter().any(|(t, n)| t == "Bash" && *n >= 4));
        // steps 스크럽: 경로·세션 id·프로젝트 슬러그 금지
        let steps = retro_steps(&picked[0].1).join(" ");
        assert!(steps.contains("Bash"));
        assert!(!steps.contains("secret"));
        assert!(!steps.contains("sA"));
    }

    #[test]
    fn retro_daily_cap_blocks_further_posts() {
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        seed_session(&store, "sA", 4, true, 20);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        store.hub_mark_published("retro|x1", "p1", &format!("{today}T01:00:00Z")).unwrap();
        store.hub_mark_published("retro|x2", "p2", &format!("{today}T02:00:00Z")).unwrap();
        assert!(select_retros(&store, chrono::Utc::now()).unwrap().is_empty(), "하루 2건 상한");
    }

    #[test]
    fn retro_reply_parsing_is_lenient() {
        let (t, s) = parse_retro_reply("네! {\"title\":\"포트 예약 이슈 해결\",\"summary\":\"요약.\"} 끝").unwrap();
        assert_eq!(t, "포트 예약 이슈 해결");
        assert_eq!(s, "요약.");
        assert!(parse_retro_reply("json 아님").is_none());
        assert!(parse_retro_reply("{\"title\":\"\",\"summary\":\"x\"}").is_none());
    }

    // ── 텔레메트리 브리프 — 파생 신호만, 원문·경로 없음 ──

    #[test]
    fn telemetry_brief_shape_and_derived_only() {
        use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage, ToolKind};
        let store = crate::store::SqliteStore::open_in_memory().unwrap();
        // MCP 호출 이벤트 2건 (jira) + 1건 (notion), 오늘 날짜
        let mk = |i: u64, server: &str| NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "c--users-secret-path".into(),
            session_id: format!("s{i}"), uuid: Some(format!("u{i}")), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-01T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: i,
            msg_id: None,
            kind: EventKind::ToolCall {
                kind: ToolKind::McpCall { server: server.into(), tool: "t".into() },
                raw_name: format!("mcp__{server}__t"),
                target: Some("우리 회사 기밀 파일 C:/secret/x.xlsx".into()),
                tool_use_id: None,
            },
        };
        // 빈 store → 빈 날 판정
        let empty = build_telemetry_brief(&store, "2026-07-01", "palen").unwrap();
        assert!(telemetry_is_empty(&empty), "활동 없으면 empty");

        store.upsert_events(&[mk(1, "jira"), mk(2, "jira"), mk(3, "notion")]).unwrap();
        store.rebuild_rollup().unwrap();
        // 2026-07-01T10:00Z의 로컬(KST) 날짜 = 2026-07-01 (19시)
        let brief = build_telemetry_brief(&store, "2026-07-01", "palen").unwrap();
        assert!(!telemetry_is_empty(&brief), "세션이 있으면 발행 대상");
        assert_eq!(brief["sessions"], 3);
        assert_eq!(brief["kind"], TELEMETRY_KIND);
        assert_eq!(brief["agent"], "palen");
        assert_eq!(brief["mcp_calls"]["jira"], 2);
        assert_eq!(brief["mcp_calls"]["notion"], 1);
        // 파생 신호 원칙: 프로젝트 경로·도구 target(파일 경로) 미포함
        let s = serde_json::to_string(&brief).unwrap();
        assert!(!s.contains("secret"), "경로/target 누출 금지: {s}");
        assert!(!s.contains("기밀"));
        // 상태 키 형식
        assert_eq!(telemetry_state_key("2026-07-01"), "telemetry|2026-07-01");
    }
}
