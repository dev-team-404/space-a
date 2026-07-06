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
use crate::rules::RuleEngine;
use crate::store::{ingest_file, SqliteStore};
use anyhow::Result;
use std::path::Path;

pub struct IngestReport {
    pub files: usize,
    pub new_events: usize,
    pub warnings: Vec<String>,
}

pub fn run_ingest(store: &SqliteStore) -> Result<IngestReport> {
    let mut report = IngestReport { files: 0, new_events: 0, warnings: Vec::new() };
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
        for f in &files {
            match ingest_file(store, &adapter, f) {
                Ok(n) => report.new_events += n,
                Err(e) => report.warnings.push(format!("{} 수집 실패(건너뜀): {e}", f.display())),
            }
        }
    }
    store.rebuild_rollup()?;
    Ok(report)
}

pub fn read_json_guarded(path: &Path) -> Option<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Some(serde_json::Value::Null),
        Ok(s) => serde_json::from_str(&s).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(serde_json::Value::Null),
        Err(_) => None,
    }
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
    let engine = RuleEngine::new(vec![
        Box::new(R5RepeatedRead::default()),
        Box::new(R1UnusedMcp::default()),
        Box::new(R2UnusedPluginSkills::default()),
        Box::new(R7OpusTrivial::default()),
        Box::new(R9WebOveruse::default()),
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
}
