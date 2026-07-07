//! CLI와 Tauri 앱이 공유하는 상위 오퍼레이션. 출력(print) 없이 값을 반환한다.
use crate::adapter::SourceAdapter;
use crate::finding::Finding;
use crate::hosts::enumerate_hosts;
use crate::inventory::{collect_host_inventory, scan_plugin_inventory};
use crate::rules::r1_unused_mcp::R1UnusedMcp;
use crate::rules::r2_unused_plugins::R2UnusedPluginSkills;
use crate::rules::r5_repeated_read::R5RepeatedRead;
use crate::rules::r7_opus_trivial::R7OpusTrivial;
use crate::rules::r9_web_overuse::R9WebOveruse;
use crate::rules::r10_automation_burst::R10AutomationBurst;
use crate::rules::r11_permission_friction::R11PermissionFriction;
use crate::rules::r12_unused_skills::R12UnusedSkills;
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
        if !inv.complete {
            warnings.push(format!("host {} 중첩 .mcp.json 읽기 실패 — reconcile 스킵", hs.host));
            continue;
        }
        store.replace_host_inventory(&hs.host, &inv.entries)?;
        let (plugins, plugins_complete) = scan_plugin_inventory(&settings, &cache);
        if !plugins_complete {
            warnings.push(format!("host {} 플러그인 스킬 스캔 실패 — R2 스킵", hs.host));
        } else {
            store.replace_plugin_inventory(&hs.host, &plugins)?;
        }
    }
    Ok(warnings)
}

pub fn run_rules(store: &SqliteStore) -> Result<Vec<Finding>> {
    // 코칭 v2 이행: 세션 스코프 R7은 폐기 — 프로젝트 집계(R7 v2)가 대체 (스펙 §3)
    store.delete_findings_by_rule_and_scope("R7", "session")?;
    // 옛 R5 세션 카드 정리(스팸 재발분). 새 R5는 scope_kind="project"+subtype dedup이라 안 걸리고 공존(스펙 §6.3).
    store.delete_findings_by_rule_and_scope("R5", "session")?;
    let engine = RuleEngine::new(vec![
        Box::new(R1UnusedMcp::default()),
        Box::new(R2UnusedPluginSkills::default()),
        Box::new(R5RepeatedRead::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(R9WebOveruse::default()),
        Box::new(R10AutomationBurst::default()),
        Box::new(R11PermissionFriction::default()),
        Box::new(R12UnusedSkills::default()),
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

    #[test]
    fn run_rules_purges_deprecated_session_findings() {
        use crate::finding::{Finding, Severity};
        let store = SqliteStore::open_in_memory().unwrap();
        let mk = |rule: &str, kind: &str, key: &str| Finding {
            rule_id: rule.into(), severity: Severity::Suggest,
            scope_host: None, scope_project: None,
            scope_kind: kind.into(), scope_ref: "x".into(),
            evidence: serde_json::json!({}), est_tokens_saved: 0,
            prescription: None, dedup_key: key.into(),
        };
        // 폐기된 세션 스코프 카드 2종(R7 session·R5 session) + 유지될 R1 host 1종
        store.upsert_finding(&mk("R7", "session", "R7|s1"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R5", "session", "R5|s1|a.md"), "2026-07-06T00:00:00Z").unwrap();
        store.upsert_finding(&mk("R1", "host", "R1|W|ctx"), "2026-07-06T00:00:00Z").unwrap();

        run_rules(&store).unwrap();
        assert_eq!(store.count_findings().unwrap(), 1); // R1만 생존(R7·R5 세션 카드 삭제)
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
