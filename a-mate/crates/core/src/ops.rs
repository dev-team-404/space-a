//! CLI와 Tauri 앱이 공유하는 상위 오퍼레이션. 출력(print) 없이 값을 반환한다.
use crate::adapter::SourceAdapter;
use crate::finding::Finding;
use crate::hosts::enumerate_hosts;
use crate::inventory::{collect_host_inventory, scan_plugin_inventory};
use crate::rules::r7_opus_trivial::R7OpusTrivial;
use crate::rules::RuleEngine;
use crate::store::{ingest_file, SqliteStore};
use anyhow::Result;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

pub struct IngestReport {
    pub files: usize,
    pub new_events: usize,
    pub warnings: Vec<String>,
}

/// 한 호스트의 수집 대상. `discover_work`가 만들고 호출자가 소비한다.
pub struct HostWork {
    pub adapter: crate::adapter::ClaudeCodeAdapter,
    pub files: Vec<std::path::PathBuf>,
}

/// 전 호스트 discover. **DB를 쓰지 않으므로 store 락 밖에서 호출할 수 있다** —
/// Tauri 파이프라인이 초기 스캔의 락 보유를 줄이려고 이 성질에 의존한다(X1).
/// 반환: (호스트별 작업 목록, 열거 실패 경고). 한 호스트가 실패해도 나머지는 진행한다.
pub fn discover_work() -> (Vec<HostWork>, Vec<String>) {
    let mut work = Vec::new();
    let mut warnings = Vec::new();
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        let files = match adapter.discover() {
            Ok(f) => f,
            Err(e) => {
                warnings.push(format!("host {} 파일 열거 실패: {e}", hs.host));
                Vec::new()
            }
        };
        work.push(HostWork { adapter, files });
    }
    (work, warnings)
}

pub fn run_ingest(store: &SqliteStore) -> Result<IngestReport> {
    run_ingest_with_progress(store, &mut |_, _| {})
}

/// 파일 단위 진행 콜백 `(done, total)`. CLI(`crates/core/src/main.rs`) 경로 — 호출자가 잡은 락을
/// 끝까지 유지한다. 락을 쪼개야 하는 Tauri 파이프라인은 `discover_work` + `store::ingest_file`을
/// 직접 조립한다(X1 설계 §A).
pub fn run_ingest_with_progress(
    store: &SqliteStore,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<IngestReport> {
    let (work, warnings) = discover_work();
    let mut report = IngestReport {
        files: work.iter().map(|w| w.files.len()).sum(),
        new_events: 0,
        warnings,
    };
    ingest_all(store, &work, &mut report, on_progress);
    store.rebuild_rollup()?;
    Ok(report)
}

fn ingest_all(
    store: &SqliteStore,
    work: &[HostWork],
    report: &mut IngestReport,
    on_progress: &mut dyn FnMut(usize, usize),
) {
    let total: usize = work.iter().map(|w| w.files.len()).sum();
    let mut done = 0;
    for w in work {
        for f in &w.files {
            match ingest_file(store, &w.adapter, f) {
                Ok(n) => report.new_events += n,
                Err(e) => report.warnings.push(format!("{} 수집 실패(건너뜀): {e}", f.display())),
            }
            done += 1;
            on_progress(done, total);
        }
    }
}

pub fn read_json_guarded(path: &Path) -> Option<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Some(serde_json::Value::Null),
        Ok(s) => serde_json::from_str(&s).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(serde_json::Value::Null),
        Err(_) => None,
    }
}

/// 포인터(source_file + 라인 시작 byte offset)가 가리키는 JSONL 한 줄을 파싱해 반환.
/// 프로즈(프롬프트/에러 전문)를 DB에 두지 않고 필요 시 원본에서 지연로드하기 위한 것(스펙 §3).
/// 파일 부재·파싱 실패·빈 줄이면 None (미리보기 fallback은 호출부 책임).
pub fn deref_jsonl_line(source_file: &str, offset: u64) -> Option<serde_json::Value> {
    let f = std::fs::File::open(source_file).ok()?;
    let mut reader = BufReader::new(f);
    reader.seek(SeekFrom::Start(offset)).ok()?;
    let mut line = String::new();
    let n = reader.read_line(&mut line).ok()?;
    if n == 0 {
        return None;
    }
    serde_json::from_str(line.trim_end_matches(['\n', '\r'])).ok()
}

pub fn run_inventory(store: &mut SqliteStore) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    for hs in enumerate_hosts() {
        let Some(claude_json) = read_json_guarded(&hs.claude_json()) else {
            warnings.push(format!("host {} claude.json 읽기 실패 — reconcile 스킵", hs.host));
            continue;
        };
        let Some(settings) = read_json_guarded(&hs.settings_json()) else {
            warnings.push(format!("host {} settings.json 읽기 실패 — reconcile 스킵", hs.host));
            continue;
        };
        let cache = hs.claude_root.join("plugins").join("cache");
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        // MCP 인벤토리는 completeness 게이트를 둔다: 중첩 .mcp.json 하나라도 못 읽으면 저장을
        // 건너뛴다(미완 인벤토리로 R1이 서버를 미사용으로 오탐하는 것 방지). 단, 이 게이트는 MCP
        // 인벤토리에만 적용하고 아래 독립 수집(플러그인·호스트 설정·개인 스킬)은 계속 진행한다 —
        // 하나의 .mcp.json 실패가 R7·R13 재료(모델/effort·인벤토리)까지 막지 않도록.
        if inv.complete {
            store.replace_host_inventory(&hs.host, &inv.entries)?;
        } else {
            warnings.push(format!("host {} 중첩 .mcp.json 읽기 실패 — MCP 인벤토리 스킵", hs.host));
        }
        let (plugins, plugins_complete) = scan_plugin_inventory(&settings, &cache);
        if !plugins_complete {
            warnings.push(format!("host {} 플러그인 스킬 스캔 실패 — R2 스킵", hs.host));
        } else {
            store.replace_plugin_inventory(&hs.host, &plugins)?;
        }
        // E — 설치 전수 스냅숏(enabledPlugins 맵, disabled 포함): ②(미설치 추천)의 부재 확인
        // 게이트. settings.json만으로 완전하므로 스킬 스캔 완전성과 무관하게 항상 갱신.
        let installed = settings
            .get("enabledPlugins")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if let Err(e) = store.set_installed_plugins(&hs.host, &installed) {
            warnings.push(format!("host {} 설치 스냅숏 실패: {e}", hs.host));
        }
        // v3: 호스트 설정 스냅숏 — R7 확장·R13 OutdatedModel 재료 (코칭 v3 §4.2)
        let default_model = settings.get("model").and_then(|v| v.as_str());
        let effort = settings.get("effortLevel").and_then(|v| v.as_str());
        if let Err(e) = store.replace_host_settings(
            &hs.host, default_model, effort, &chrono::Utc::now().to_rfc3339(),
        ) {
            warnings.push(format!("host {} 설정 스냅숏 실패: {e}", hs.host));
        }
        // v3: 개인 스킬 인벤토리 — 사용자 스코프 + 프로젝트 스코프(.claude/skills, 로컬 존재 cwd만)
        let mut personal =
            crate::inventory::scan_personal_skills(&hs.claude_root.join("skills"), "user");
        let cwds = match store.session_cwds(&hs.host) {
            Ok(c) => c,
            Err(e) => {
                warnings.push(format!("host {} 세션 cwd 조회 실패 — 프로젝트 스킬 스캔 스킵: {e}", hs.host));
                Vec::new()
            }
        };
        for cwd in cwds {
            // WSL 세션 cwd(/home/...)는 Windows에서 못 여니 distro UNC로 변환해서 스캔.
            let p = hs.resolve_cwd(&cwd).join(".claude").join("skills");
            personal.extend(crate::inventory::scan_personal_skills(&p, "project"));
        }
        if let Err(e) = store.replace_personal_skills(&hs.host, &personal) {
            warnings.push(format!("host {} 개인 스킬 스캔 실패: {e}", hs.host));
        }
    }
    Ok(warnings)
}

/// 지금 실제로 돌아가는 룰 집합 — 단일 원본.
///
/// 별도 함수인 이유: 허브 공유 화이트리스트(`hub::SHARE_RULES`)가 은퇴한 룰만 가리켜
/// 발행이 영구 0건이 된 사고가 있었다(2026-07-25). 등록 룰을 코드로 조회할 수 있어야
/// "화이트리스트 ⊆ 등록 룰" 불변식을 테스트로 고정할 수 있다.
pub fn registered_rules() -> Vec<Box<dyn crate::rules::Rule>> {
    vec![
        // R6(반복 지시 → 스킬/커맨드화)은 v3 은퇴 대상 아님 — 킥오프 차별점 신규 등록
        Box::new(crate::rules::r6_repeated_prompts::R6RepeatedPrompts::default()),
        Box::new(R7OpusTrivial::default()),
        // R8(MCP 대형 결과) — result_len 수집 승격, 2026-07-19
        Box::new(crate::rules::r8_mcp_large_result::R8McpLargeResult::default()),
        // R10·R11 은퇴(코드 보존 — detect_bursts는 R7 후보 제외에 계속 사용). F(R24)는 완전 제거 — 전용 세그먼터도 삭제(purge 참조)
    ]
}

/// 등록된 룰의 id 목록 (표시·검증용).
pub fn registered_rule_ids() -> Vec<&'static str> {
    registered_rules().iter().map(|r| r.id()).collect()
}

pub fn run_rules(store: &SqliteStore) -> Result<Vec<Finding>> {
    // R5(반복 읽기) 발화 보류(2026-07-10 사용자 판정): 반복 Read는 에이전트 동작이라
    // 사용자가 행동할 레버가 없음 — 등록 해제·전 스코프 카드 정리, 룰 코드·테스트는 보존.
    store.delete_findings_by_rule_and_scope("R5", "session")?;
    store.delete_findings_by_rule_and_scope("R5", "project")?;
    // 코칭 v3 처분(스펙 §3.1): R1·R2·R9·R12 은퇴 — 잔소리 단독 카드 폐지,
    // 탐지 신호는 R13(환경 큐레이션, PR③)이 흡수. 룰 코드·테스트는 보존.
    store.delete_findings_by_rule_and_scope("R1", "host")?;
    store.delete_findings_by_rule_and_scope("R1", "project")?;
    store.delete_findings_by_rule_and_scope("R2", "host")?;
    store.delete_findings_by_rule_and_scope("R9", "session")?;
    store.delete_findings_by_rule_and_scope("R12", "project")?;
    // 코칭 가치 재설계: R10(관찰 카드)·R11(권한 마찰) 은퇴 — 코드·테스트는 보존.
    store.delete_findings_by_rule_and_scope("R10", "project")?;
    store.delete_findings_by_rule_and_scope("R11", "project")?;
    // 코칭 가치 재설계 후속(2026-07-23): F(R24 컨텍스트 위생) 은퇴 — 룰·에피소드 세그먼터 완전 제거.
    // 8자↑ 프롬프트=새 작업 경계가 후속질문을 작업전환으로 오인 → 결정론으로 정확도 확보 불가(스펙 참조).
    store.delete_findings_by_rule_and_scope("R24", "project")?;
    // 하니스 주입 프롬프트(크론 `<scheduled-task …>`, 슬래시 커맨드 로그)는 R6 대상이 아니다.
    // 수집 시점 필터(normalize)는 신규 수집분만 막으므로, 이미 쌓인 행과 그 카드는 여기서 정리한다.
    // 안 그러면 기존 사용자는 계속 오탐 카드를 본다 (실측 2026-07-25: R6 3건 전부 오탐).
    let purged = store.purge_harness_injected_prompts()?;
    if purged > 0 {
        log::debug!("R6: 하니스 주입 프롬프트 {purged}행 정리");
    }
    let engine = RuleEngine::new(registered_rules());
    let findings = engine.run(store)?;
    let now = chrono::Utc::now().to_rfc3339();
    let tx = store.conn.unchecked_transaction()?;
    for f in &findings {
        store.upsert_finding(f, &now)?;
    }
    tx.commit()?;
    // R6 앵커 키 이동/병합·관찰창 이탈로 생긴 유령 활성 카드 정리 (A 느슨한 묶기 부작용 방지).
    let r6_keys: Vec<String> = findings
        .iter()
        .filter(|f| f.rule_id == "R6" && f.scope_kind == "pattern")
        .map(|f| f.dedup_key.clone())
        .collect();
    store.prune_stale_r6_patterns(&r6_keys)?;
    Ok(findings)
}

/// 콘텐츠 큐레이션 — 프로필 감지 → (내장 팁 + 피드) 스코어링·정렬 → persist → 노출 목록 반환.
/// 결정론: `feed_items`는 호출부(가장자리)가 네트워크로 미리 가져와 넘긴다(테스트는 빈 벡터).
/// (킥오프 `docs/brainstorming/2026-07-14-content-curation-kickoff.md` §How)
pub fn run_curation(
    store: &SqliteStore,
    feed_items: Vec<crate::content::ContentItem>,
    catalog: &[crate::content::CatalogEntry],
    now_ts: &str,
) -> Result<Vec<crate::store::ContentRow>> {
    use crate::content::{rank, BuiltinTipsSource, ContentSource, CONTENT_COOLDOWN_DAYS};
    let profile = crate::profile::detect_profile(store)?;
    let mut items = BuiltinTipsSource.fetch()?;
    // 개인 실전 레슨 — 내 로그의 최근 사건에서 (조건 미충족이면 빈 벡터, 억지 레슨 없음)
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    items.extend(crate::content::personal_lessons(store, &today, &yesterday));
    // E — plugin 추천 (결정론: 캐시된 work-kind + 인벤토리 + 카탈로그. LLM 판정은 파이프라인
    // 별도 스텝이 캐시해 둠 — 엔진 미설정이면 캐시가 비어 자연 침묵)
    items.extend(crate::plugin_reco::plugin_reco_items(store, catalog, now_ts)?);
    items.extend(feed_items);
    let ranked = rank(items, &profile);
    store.replace_content_items(&ranked, now_ts)?;
    store.list_content(now_ts, CONTENT_COOLDOWN_DAYS, false)
}

/// 피드 소스(T1 changelog 등)를 네트워크로 가져온다 — 부수효과는 가장자리. 실패는 하드
/// 에러 아님(빈 벡터 후 계속 — 관대한 파싱 원칙). Tauri 파이프라인이 락 밖에서 호출.
/// 외부 문서(code.claude.com) 도달성 — 내부망 감지용 3초 HEAD. (동료 이슈: 내부망 링크)
pub fn probe_docs_reachable() -> bool {
    ureq::head("https://code.claude.com/docs/en/overview")
        .timeout(std::time::Duration::from_secs(3))
        .call()
        .is_ok()
}

pub fn fetch_feed_items(
    hub: Option<crate::content::HubKnowledgeSource>,
) -> Vec<crate::content::ContentItem> {
    use crate::content::{BorisTipsSource, ClaudeChangelogSource, ContentSource};
    // 여러 소스를 각각 관대하게 fetch — 하나가 실패해도 나머지는 계속.
    let mut items = Vec::new();
    match ClaudeChangelogSource::default().fetch() {
        Ok(mut v) => items.append(&mut v),
        Err(e) => eprintln!("[curation] changelog 피드 fetch 실패(계속): {e}"),
    }
    match BorisTipsSource::default().fetch() {
        Ok(mut v) => items.append(&mut v),
        Err(e) => eprintln!("[curation] boris 피드 fetch 실패(계속): {e}"),
    }
    // 팀 지식(pull) — 허브 설정+토큰이 있을 때만 (hub::pull_source)
    if let Some(src) = hub {
        match src.fetch() {
            Ok(mut v) => items.append(&mut v),
            Err(e) => eprintln!("[curation] 팀 지식 피드 fetch 실패(계속): {e}"),
        }
    }
    items
}

/// E 카탈로그 fetch — 부수효과는 가장자리(파이프라인이 락 밖에서 호출). 실패는 관대:
/// 빈 벡터 = ② 추천만 침묵(에러 아님), ①·나머지 큐레이션은 계속 (스펙 §6).
pub fn fetch_marketplace_catalog() -> Vec<crate::content::CatalogEntry> {
    match crate::content::MarketplaceCatalogSource::default().fetch() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[curation] 마켓플레이스 카탈로그 fetch 실패(계속): {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteStore;

    #[test]
    fn run_rules_on_empty_store_is_ok() {
        let store = SqliteStore::open_in_memory().unwrap();
        let findings = run_rules(&store).unwrap();
        assert!(findings.is_empty());
        assert_eq!(store.count_findings().unwrap(), 0);
    }

    fn opus_turn(store: &SqliteStore, sid: &str, uuid: &str, model: &str) {
        use crate::model::*;
        store.upsert_events(&[NormalizedEvent {
            source_agent: "claude-code".into(), schema_version: "t".into(),
            host: "Windows".into(), project_id: "d--proj".into(),
            session_id: sid.into(), uuid: Some(uuid.into()), parent_uuid: None,
            is_sidechain: false, ts: Some("2026-07-14T10:00:00Z".into()),
            source_file: "s.jsonl".into(), source_offset: 0,
            msg_id: None,
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
    }

    #[test]
    fn run_curation_surfaces_plugin_reco_and_preserves_dismissal() {
        use crate::content::CatalogEntry;
        use crate::model::{EventKind, NormModel, NormalizedEvent, TokenUsage};
        let store = SqliteStore::open_in_memory().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        // frontend_ui 판정 세션 2개 시드 (프롬프트+턴+판정 캐시)
        for i in 0..2 {
            let sid = format!("fe{i}");
            store.upsert_events(&[
                NormalizedEvent {
                    source_agent: "claude-code".into(), schema_version: "t".into(),
                    host: "Windows".into(), project_id: "d--proj".into(),
                    session_id: sid.clone(), uuid: Some(format!("{sid}-p")), parent_uuid: None,
                    is_sidechain: false, ts: Some(now.clone()),
                    source_file: format!("{sid}.jsonl"), source_offset: 0, msg_id: None,
                    kind: EventKind::UserPrompt { preview: "버튼 컴포넌트 스타일 다듬어줘".into(), is_command: false },
                },
                NormalizedEvent {
                    source_agent: "claude-code".into(), schema_version: "t".into(),
                    host: "Windows".into(), project_id: "d--proj".into(),
                    session_id: sid.clone(), uuid: Some(format!("{sid}-t")), parent_uuid: None,
                    is_sidechain: false, ts: Some(now.clone()),
                    source_file: format!("{sid}.jsonl"), source_offset: 1, msg_id: None,
                    kind: EventKind::AssistantTurn {
                        model: NormModel::from_raw_id("claude-sonnet-4-6"),
                        usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
                    },
                },
            ]).unwrap();
            store.set_session_work_kinds(&sid, "Windows", "d--proj", &now,
                Some(&["frontend_ui".to_string()]), 1, &now).unwrap();
        }
        let catalog = vec![CatalogEntry {
            name: "frontend-design".into(), description: "".into(),
            category: None, homepage: None,
        }];
        // ② 게이트: 설치 전수 스냅숏(스캔됨·설치 0개)으로 부재 확인
        store.set_installed_plugins("Windows", &serde_json::json!({})).unwrap();
        // ② 카드가 큐레이션 노출 목록에 오른다 (personal 스코어 → 상단권)
        let visible = run_curation(&store, vec![], &catalog, &now).unwrap();
        let reco = visible.iter().find(|r| r.id.starts_with("plugin-reco-"))
            .expect("plugin 추천 카드 노출");
        assert!(reco.trigger_tags.iter().any(|t| t == "personal"));
        // dismiss → 재큐레이션에도 다시 안 뜬다 (기존 dismissal 인프라 재사용 검증)
        let reco_id = reco.id.clone();
        store.set_content_status(&reco_id, "dismissed", &now).unwrap();
        let again = run_curation(&store, vec![], &catalog, &now).unwrap();
        assert!(again.iter().all(|r| r.id != reco_id), "dismiss된 추천은 재노출 금지");
    }

    #[test]
    fn run_curation_persists_and_returns_frontier_first() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 전부 opus → 프론티어 = 모델 리터러시
        opus_turn(&store, "s1", "u1", "claude-opus-4-8");
        opus_turn(&store, "s1", "u2", "claude-opus-4-8");
        let visible = run_curation(&store, vec![], &[], "2026-07-14T10:00:00Z").unwrap();
        assert!(!visible.is_empty());
        assert_eq!(visible[0].dimension.as_deref(), Some("model_literacy"));
        // persist 확인: 전체(숨김 포함) 목록엔 마스터 축 팁도 저장돼 있음
        let all = store.list_content("2026-07-14T10:00:00Z", 14.0, true).unwrap();
        assert!(all.len() > visible.len());
    }

    #[test]
    fn mcp_tip_is_grounded_in_user_finding() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        opus_turn(&store, "s1", "u1", "claude-opus-4-8");
        // 사용자의 실제 근거: 미사용 MCP finding(R1)
        store.upsert_finding(&Finding {
            rule_id: "R1".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "host".into(), scope_ref: "chrome-devtools".into(),
            evidence: serde_json::json!({"server":"chrome-devtools","resident_tokens_total":432657}),
            est_tokens_saved: 2500, prescription: None, dedup_key: "R1|chrome-devtools".into(),
        }, "2026-07-14T10:00:00Z").unwrap();
        run_curation(&store, vec![], &[], "2026-07-14T10:00:00Z").unwrap();
        let all = store.list_content("2026-07-14T10:00:00Z", 14.0, true).unwrap();
        let mcp = all.iter().find(|r| r.trigger_tags.iter().any(|t| t == "mcp")).unwrap();
        let p = mcp.personal.as_ref().expect("MCP 팁에 '당신 로그' 근거가 있어야 함");
        assert!(p.contains("chrome-devtools"), "서버명 포함해야: {p}");
        assert!(p.contains("MCP"), "MCP 언급해야: {p}");
        assert!(p.contains("2500") || p.contains("2,500"), "절약 토큰 포함해야: {p}");
    }

    #[test]
    fn dismissed_tip_cools_down_its_dimension() {
        let store = SqliteStore::open_in_memory().unwrap();
        opus_turn(&store, "s1", "u1", "claude-opus-4-8");
        opus_turn(&store, "s1", "u2", "claude-opus-4-8");
        let visible = run_curation(&store, vec![], &[], "2026-07-14T10:00:00Z").unwrap();
        let top = visible[0].id.clone(); // model_literacy 팁
        // 닫으면 같은 축 형제도 쿨다운 기간 동안 억제
        store.set_content_status(&top, "dismissed", "2026-07-14T10:05:00Z").unwrap();
        let after = store.list_content("2026-07-15T10:00:00Z", 14.0, false).unwrap();
        assert!(after.iter().all(|r| r.dimension.as_deref() != Some("model_literacy")),
            "닫은 축(model_literacy)은 쿨다운 동안 조용해야 함");
        // 쿨다운 경과 후엔 다시 노출
        let later = store.list_content("2026-08-01T10:00:00Z", 14.0, false).unwrap();
        assert!(later.iter().any(|r| r.dimension.as_deref() == Some("model_literacy")));
    }

    #[test]
    fn run_rules_purges_retired_rule_findings() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |rule: &str, kind: &str, key: &str| Finding {
            rule_id: rule.into(), severity: Severity::Suggest,
            scope_host: None, scope_project: None,
            scope_kind: kind.into(), scope_ref: "x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        // v2 폐기분(R5 전 스코프) + v3 은퇴분(R1·R2·R9·R12·R11)
        // (R7 session은 더 이상 run_rules가 사전 정리하지 않음 — 판정 캐시 보존, 아래 별도 테스트)
        store.upsert_finding(&mk("R5", "session", "R5|s1|a.md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "project", "R5|W|proj|cross_session_claude_md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "host", "R1|W|ctx"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "project", "R1|W|proj|pw"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R2", "host", "R2|W|superpowers@mp"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R9", "session", "R9|s9"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R12", "project", "R12|W|proj"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R10", "project", "R10|W|proj"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R11", "project", "R11|W|proj"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R24", "project", "R24|W|proj"), "2026-07-06T00:00:00Z").unwrap();

        run_rules(&store).unwrap();
        // 은퇴 룰(R10·R11·R24 포함)·구폐기분 전부 purge. R7 세션 후보는 이 테스트에 시드 안 함.
        for key in ["R10|W|proj", "R11|W|proj", "R24|W|proj"] {
            let n: i64 = store.conn.query_row(
                "SELECT COUNT(*) FROM findings WHERE dedup_key=?1", [key], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{key} purge되어야");
        }
    }

    #[test]
    fn run_rules_prunes_stale_active_r6_patterns() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |key: &str| Finding {
            rule_id: "R6".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: None,
            scope_kind: "pattern".into(), scope_ref: "pattern:x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        // prompt_events 없음 → R6 evaluate는 아무 후보도 방출하지 않는다(emit set 비어 있음).
        // 앵커 이동/병합으로 방출되지 않게 된 활성 카드는 정리, 판정 캐시(rejected)는 보존.
        store.upsert_finding(&mk("R6|Windows|stale_new"), "2026-07-06T00:00:00Z").unwrap(); // init 'pending'
        store.set_judgment("R6|Windows|stale_new", Some("new"), &serde_json::json!({})).unwrap();
        store.upsert_finding(&mk("R6|Windows|stale_rej"), "2026-07-06T00:00:00Z").unwrap();
        store.set_judgment("R6|Windows|stale_rej", Some("rejected"), &serde_json::json!({})).unwrap();

        run_rules(&store).unwrap();

        assert!(store.find_finding("R6|Windows|stale_new").unwrap().is_none(),
            "미방출 활성(new) R6 카드는 정리돼야");
        assert!(store.find_finding("R6|Windows|stale_rej").unwrap().is_some(),
            "rejected 판정 캐시는 보존돼야(재판정 금지)");
    }

    #[test]
    fn run_rules_creates_no_new_row_for_dismissed_r6_after_anchor_drift() {
        // §5.4 영속성 레벨 검증 — 무시된 묶음의 앵커가 바뀌어도 새 행이 생기면 안 된다.
        // (룰 레벨 침묵은 r6_repeated_prompts 테스트가, 여기서는 upsert·prune까지 통과시킨다.)
        use crate::model::{EventKind, NormalizedEvent};
        let store = SqliteStore::open_in_memory().unwrap();
        let seed = |sess: &str, prompt: &str, min: i64| {
            store.upsert_events(&[NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "p".into(),
                session_id: sess.into(), uuid: Some(format!("{sess}-u")), parent_uuid: None,
                is_sidechain: false,
                ts: Some((chrono::Utc::now() - chrono::Duration::minutes(min)).to_rfc3339()),
                source_file: "s.jsonl".into(), source_offset: 0, msg_id: None,
                kind: EventKind::UserPrompt { preview: prompt.into(), is_command: false },
            }]).unwrap();
        };
        seed("s1", "pr 리뷰 코멘트 종합 검토해서 조치해줘", 0);
        seed("s2", "pr 리뷰 코멘트 종합 검토하고 반영해줘", 1);
        seed("s3", "pr 리뷰 코멘트 종합 검토 후 조치", 2);

        let first = run_rules(&store).unwrap();
        let key = first.iter().find(|f| f.rule_id == "R6").unwrap().dedup_key.clone();
        assert!(store.set_finding_status(&key, "dismissed").unwrap());

        // 사전순으로 더 앞서는 변형 → 앵커(=dedup_key)가 바뀐다
        seed("s4", "aa 리뷰 코멘트 종합 검토해서 조치하자", 3);
        run_rules(&store).unwrap();

        let rows = store.list_findings_current(true).unwrap();
        let r6: Vec<_> = rows.iter().filter(|r| r.rule_id == "R6").collect();
        assert_eq!(r6.len(), 1, "억제된 묶음이 새 행을 만들면 안 됨");
        assert_eq!(r6[0].dedup_key, key);
        assert_eq!(r6[0].status, "dismissed", "무시 기록은 그대로 보존");
    }

    #[test]
    fn run_rules_preserves_judged_r7_session_findings() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let f = Finding {
            rule_id: "R7".into(), severity: Severity::Suggest,
            scope_host: Some("Windows".into()), scope_project: Some("p".into()),
            scope_kind: "session".into(), scope_ref: "s1".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: "R7|sess|Windows|s1".into(),
        };
        store.upsert_finding(&f, "2026-07-06T00:00:00Z").unwrap();
        store.set_judgment("R7|sess|Windows|s1", Some("confirmed"), &serde_json::json!({"attempts": 1})).unwrap();

        run_rules(&store).unwrap(); // 이벤트 없음 → R7 evaluate가 새 후보를 만들지 않음

        assert!(
            store.find_finding("R7|sess|Windows|s1").unwrap().is_some(),
            "판정 캐시가 있는 R7 세션 후보를 run_rules가 지우면 안 됨"
        );
    }

    #[test]
    fn run_rules_does_not_surface_r24_after_retirement() {
        use crate::model::*;
        let store = SqliteStore::open_in_memory().unwrap();
        let recent = |h: i64| (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339();
        // 6개 에피소드, 전부 60k 상속 — 은퇴 전이라면 R24 발화 조건이었다.
        let mut evs = Vec::new();
        for i in 0..6u64 {
            let ts = recent(20 - i as i64);
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(),
                session_id: "s1".into(), uuid: Some(format!("p{i}")), parent_uuid: None,
                is_sidechain: false, ts: Some(ts.clone()), source_file: "s.jsonl".into(),
                source_offset: i * 2, msg_id: None,
                kind: EventKind::UserPrompt { preview: format!("에피소드 {i} 실질 작업 지시 문장"), is_command: false },
            });
            evs.push(NormalizedEvent {
                source_agent: "claude-code".into(), schema_version: "t".into(),
                host: "Windows".into(), project_id: "d--proj".into(),
                session_id: "s1".into(), uuid: Some(format!("a{i}")), parent_uuid: None,
                is_sidechain: false, ts: Some(ts), source_file: "s.jsonl".into(),
                source_offset: i * 2 + 1, msg_id: None,
                kind: EventKind::AssistantTurn {
                    model: NormModel::from_raw_id("claude-opus-4-8"),
                    usage: TokenUsage { input: 60_000, ..Default::default() },
                    web_search: 0, web_fetch: 0,
                },
            });
        }
        store.upsert_events(&evs).unwrap();

        let findings = run_rules(&store).unwrap();
        // F(R24) 은퇴: 발화 조건 데이터여도 어떤 카드도 나오지 않아야 한다.
        assert!(
            !findings.iter().any(|f| f.rule_id == "R24"),
            "R24는 은퇴 — run_rules가 카드를 내지 않는다"
        );
        let n: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM findings WHERE rule_id = 'R24'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "R24 finding 행이 없어야");
    }

    #[test]
    fn ingest_all_reports_monotonic_progress() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let mut files = Vec::new();
        for (name, sid) in [("a.jsonl", "s1"), ("b.jsonl", "s2")] {
            let file = proj.join(name);
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"{sid}","uuid":"{sid}-u","timestamp":"2026-07-01T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":2}}}}}}"#).unwrap();
            files.push(file);
        }

        let store = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        let work = vec![HostWork { adapter, files }];
        let mut seen = Vec::new();
        let mut report = IngestReport { files: 2, new_events: 0, warnings: Vec::new() };
        ingest_all(&store, &work, &mut report, &mut |done, total| seen.push((done, total)));

        assert_eq!(seen, vec![(1, 2), (2, 2)]); // 파일 단위 단조 증가
        assert_eq!(report.new_events, 2);
        assert!(report.warnings.is_empty());
    }

    /// 파이프라인이 락을 파일 단위로 놓고 잡으려면, 파일마다 따로 수집해도 일괄 수집과
    /// 같은 결과가 나와야 한다. offset이 어긋나면 다음 스캔이 중복 수집하거나 빠뜨린다.
    #[test]
    fn per_file_ingest_matches_ingest_all() {
        use crate::adapter::ClaudeCodeAdapter;
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--Users-jibin");
        std::fs::create_dir_all(&proj).unwrap();
        let mut files = Vec::new();
        for (name, sid) in [("a.jsonl", "s1"), ("b.jsonl", "s2"), ("c.jsonl", "s3")] {
            let file = proj.join(name);
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, r#"{{"type":"assistant","sessionId":"{sid}","uuid":"{sid}-u","timestamp":"2026-07-01T10:00:00Z","message":{{"model":"claude-opus-4-8","usage":{{"input_tokens":1,"output_tokens":2}}}}}}"#).unwrap();
            files.push(file);
        }

        // A: ingest_all 한 번 (CLI 경로)
        let bulk = SqliteStore::open_in_memory().unwrap();
        let work = vec![HostWork {
            adapter: ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() },
            files: files.clone(),
        }];
        let mut report = IngestReport { files: files.len(), new_events: 0, warnings: Vec::new() };
        ingest_all(&bulk, &work, &mut report, &mut |_, _| {});

        // B: 파일마다 따로 (파이프라인이 쓸 경로 — 락을 파일 단위로 놓는다)
        let chunked = SqliteStore::open_in_memory().unwrap();
        let adapter = ClaudeCodeAdapter { root: dir.path().into(), host: "Windows".into() };
        let mut chunked_events = 0usize;
        for f in &files {
            chunked_events += crate::store::ingest_file(&chunked, &adapter, f).unwrap();
        }

        assert_eq!(
            report.new_events, chunked_events,
            "파일 단위 수집이 일괄 수집과 같은 이벤트 수를 낸다"
        );
        assert_eq!(chunked_events, 3);

        // ingest_state(파일별 재개 offset)가 같아야 한다
        let offsets = |s: &SqliteStore| -> Vec<(String, i64)> {
            let mut stmt = s
                .conn
                .prepare("SELECT source_file, last_offset FROM ingest_state ORDER BY source_file")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(offsets(&bulk), offsets(&chunked), "ingest_state 재개 지점이 같다");
        assert_eq!(offsets(&chunked).len(), 3);
    }

    #[test]
    fn read_json_guarded_valid_absent_broken() {
        let dir = tempfile::tempdir().unwrap();

        // 유효 JSON → Some(Value)
        let valid = dir.path().join("valid.json");
        std::fs::write(&valid, r#"{"a":1}"#).unwrap();
        assert_eq!(read_json_guarded(&valid), Some(serde_json::json!({"a":1})));

        // 파일 부재 → Some(Null) (정당한 빈 설정)
        let absent = dir.path().join("nope.json");
        assert_eq!(read_json_guarded(&absent), Some(serde_json::Value::Null));

        // 존재하나 깨진 JSON → None (스킵)
        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "{not json").unwrap();
        assert_eq!(read_json_guarded(&broken), None);

        // 존재하나 비어 있음/공백만 → Some(Null) (정당한 빈 설정)
        let empty = dir.path().join("empty.json");
        std::fs::write(&empty, "   \n").unwrap();
        assert_eq!(read_json_guarded(&empty), Some(serde_json::Value::Null));
    }

    #[test]
    fn deref_jsonl_line_roundtrip_and_missing() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s1.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        let l1 = r#"{"type":"user","sessionId":"s1","message":{"content":"first prompt full text"}}"#;
        let l2 = r#"{"type":"user","sessionId":"s1","message":{"content":"second"}}"#;
        writeln!(f, "{l1}").unwrap();
        let off2 = (l1.len() + 1) as u64; // 개행 포함
        writeln!(f, "{l2}").unwrap();
        f.flush().unwrap();
        let p = path.to_string_lossy();

        let v0 = super::deref_jsonl_line(&p, 0).unwrap();
        assert_eq!(v0["message"]["content"], "first prompt full text");
        let v2 = super::deref_jsonl_line(&p, off2).unwrap();
        assert_eq!(v2["message"]["content"], "second");

        // 파일 부재 → None
        assert!(super::deref_jsonl_line("C:\\nope\\missing.jsonl", 0).is_none());
    }
}
