//! CLI와 Tauri 앱이 공유하는 상위 오퍼레이션. 출력(print) 없이 값을 반환한다.
use crate::adapter::SourceAdapter;
use crate::finding::Finding;
use crate::hosts::enumerate_hosts;
use crate::inventory::{collect_host_inventory, scan_plugin_inventory};
use crate::rules::r7_opus_trivial::R7OpusTrivial;
use crate::rules::r10_automation_burst::R10AutomationBurst;
use crate::rules::r11_permission_friction::R11PermissionFriction;
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

pub fn run_ingest(store: &SqliteStore) -> Result<IngestReport> {
    run_ingest_with_progress(store, &mut |_, _| {})
}

/// 파일 단위 진행 콜백 `(done, total)` — Tauri 쪽에서 scan:progress emit에 사용 (스펙 §7).
pub fn run_ingest_with_progress(
    store: &SqliteStore,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<IngestReport> {
    let mut report = IngestReport { files: 0, new_events: 0, warnings: Vec::new() };
    // 1) 전 호스트 discover 먼저 — total을 알아야 진행률이 됨
    let mut work: Vec<(crate::adapter::ClaudeCodeAdapter, Vec<std::path::PathBuf>)> = Vec::new();
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        let files = match adapter.discover() {
            Ok(f) => f,
            Err(e) => {
                report.warnings.push(format!("host {} 파일 열거 실패: {e}", hs.host));
                Vec::new()
            }
        };
        report.files += files.len();
        work.push((adapter, files));
    }
    // 2) 파일 단위 수집 + 진행 보고
    ingest_all(store, &work, &mut report, on_progress);
    store.rebuild_rollup()?;
    Ok(report)
}

fn ingest_all(
    store: &SqliteStore,
    work: &[(crate::adapter::ClaudeCodeAdapter, Vec<std::path::PathBuf>)],
    report: &mut IngestReport,
    on_progress: &mut dyn FnMut(usize, usize),
) {
    let total: usize = work.iter().map(|(_, files)| files.len()).sum();
    let mut done = 0;
    for (adapter, files) in work {
        for f in files {
            match ingest_file(store, adapter, f) {
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

pub fn run_rules(store: &SqliteStore) -> Result<Vec<Finding>> {
    // 코칭 v2 이행: 세션 스코프 R7은 폐기 — 프로젝트 집계(R7 v2)가 대체 (스펙 §3)
    store.delete_findings_by_rule_and_scope("R7", "session")?;
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
    let engine = RuleEngine::new(vec![
        // R6(반복 지시 → 스킬/커맨드화)은 v3 은퇴 대상 아님 — 킥오프 차별점 신규 등록
        Box::new(crate::rules::r6_repeated_prompts::R6RepeatedPrompts::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(R10AutomationBurst::default()),
        Box::new(R11PermissionFriction::default()),
    ]);
    let findings = engine.run(store)?;
    let now = chrono::Utc::now().to_rfc3339();
    let tx = store.conn.unchecked_transaction()?;
    for f in &findings {
        store.upsert_finding(f, &now)?;
    }
    tx.commit()?;
    Ok(findings)
}

/// 콘텐츠 큐레이션 — 프로필 감지 → (내장 팁 + 피드) 스코어링·정렬 → persist → 노출 목록 반환.
/// 결정론: `feed_items`는 호출부(가장자리)가 네트워크로 미리 가져와 넘긴다(테스트는 빈 벡터).
/// (킥오프 `docs/brainstorming/2026-07-14-content-curation-kickoff.md` §How)
pub fn run_curation(
    store: &SqliteStore,
    feed_items: Vec<crate::content::ContentItem>,
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
            kind: EventKind::AssistantTurn {
                model: NormModel::from_raw_id(model),
                usage: TokenUsage::default(), web_search: 0, web_fetch: 0,
            },
        }]).unwrap();
    }

    #[test]
    fn run_curation_persists_and_returns_frontier_first() {
        let store = SqliteStore::open_in_memory().unwrap();
        // 전부 opus → 프론티어 = 모델 리터러시
        opus_turn(&store, "s1", "u1", "claude-opus-4-8");
        opus_turn(&store, "s1", "u2", "claude-opus-4-8");
        let visible = run_curation(&store, vec![], "2026-07-14T10:00:00Z").unwrap();
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
        run_curation(&store, vec![], "2026-07-14T10:00:00Z").unwrap();
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
        let visible = run_curation(&store, vec![], "2026-07-14T10:00:00Z").unwrap();
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
        // v2 폐기분(R7 session·R5 전 스코프) + v3 은퇴분(R1·R2·R9·R12) + 생존 R11
        store.upsert_finding(&mk("R7", "session", "R7|s1"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "session", "R5|s1|a.md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "project", "R5|W|proj|cross_session_claude_md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "host", "R1|W|ctx"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "project", "R1|W|proj|pw"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R2", "host", "R2|W|superpowers@mp"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R9", "session", "R9|s9"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R12", "project", "R12|W|proj"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R11", "project", "R11|W|proj"), "2026-07-06T00:00:00Z").unwrap();

        run_rules(&store).unwrap();
        assert_eq!(store.count_findings().unwrap(), 1, "R11만 생존해야 함");
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
        let work = vec![(adapter, files)];
        let mut seen = Vec::new();
        let mut report = IngestReport { files: 2, new_events: 0, warnings: Vec::new() };
        ingest_all(&store, &work, &mut report, &mut |done, total| seen.push((done, total)));

        assert_eq!(seen, vec![(1, 2), (2, 2)]); // 파일 단위 단조 증가
        assert_eq!(report.new_events, 2);
        assert!(report.warnings.is_empty());
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
