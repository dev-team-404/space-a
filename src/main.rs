use agent_mentor::adapter::SourceAdapter;
use agent_mentor::diary::engine::{Engine, MockEngine, OpenAiCompatEngine};
use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
use agent_mentor::hosts::enumerate_hosts;
use agent_mentor::inventory::collect_host_inventory;
use agent_mentor::rules::r1_unused_mcp::R1UnusedMcp;
use agent_mentor::rules::r5_repeated_read::R5RepeatedRead;
use agent_mentor::rules::RuleEngine;
use agent_mentor::store::{ingest_file, SqliteStore};
use anyhow::Result;

fn db() -> Result<SqliteStore> {
    SqliteStore::open(std::path::Path::new("./agent-mentor.db"))
}

fn cmd_ingest(store: &SqliteStore) -> Result<()> {
    let mut total = 0usize;
    let mut file_count = 0usize;
    for hs in enumerate_hosts() {
        let adapter = hs.adapter();
        // 관대한 수집: discover/파일 단위 실패는 로깅 후 계속(WSL UNC 경로는 절전·잠금으로
        // 일시 실패할 수 있음). 파일 하나의 실패가 전 호스트 수집을 중단시키지 않는다.
        let files = match adapter.discover() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("warn: host {} 파일 열거 실패: {e}", hs.host);
                Vec::new()
            }
        };
        file_count += files.len();
        for f in &files {
            match ingest_file(store, &adapter, f) {
                Ok(n) => total += n,
                Err(e) => eprintln!("warn: {} 수집 실패(건너뜀): {e}", f.display()),
            }
        }
        println!("  [{}] {} files", hs.host, files.len());
    }
    store.rebuild_rollup()?;
    println!("ingested {total} new events from {file_count} files across all hosts");
    Ok(())
}

/// 설정 파일을 읽어 reconcile 안전성을 판정.
/// 읽기+파싱 성공 → Some(Value); 파일 부재(NotFound) → Some(Null)(정당한 빈 설정);
/// 존재하나 IO/파싱 실패 → None(그 호스트 reconcile 스킵, 기존 행 유지).
fn read_json_guarded(path: &std::path::Path) -> Option<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(serde_json::Value::Null),
        Err(_) => None,
    }
}

fn cmd_inventory(store: &mut SqliteStore) -> Result<()> {
    let mut total_servers = 0usize;
    for hs in enumerate_hosts() {
        // 읽기 가드: claude.json·settings.json 중 하나라도 읽기 실패(존재하나 IO/파싱)면
        // 그 호스트는 reconcile 스킵(기존 인벤토리 유지) — 일시 실패로 인한 오삭제 방지.
        let Some(claude_json) = read_json_guarded(&hs.claude_json()) else {
            eprintln!("warn: host {} claude.json 읽기 실패 — 인벤토리 유지, reconcile 스킵", hs.host);
            continue;
        };
        let Some(settings) = read_json_guarded(&hs.settings_json()) else {
            eprintln!("warn: host {} settings.json 읽기 실패 — 인벤토리 유지, reconcile 스킵", hs.host);
            continue;
        };
        let cache = hs.claude_root.join("plugins").join("cache");
        let inv = collect_host_inventory(&claude_json, &settings, &cache);
        total_servers += inv.iter().map(|(_, servers)| servers.len()).sum::<usize>();
        // 원자 교체: 이 호스트의 기존 행 삭제 후 현재셋 삽입 → stale 제거.
        store.replace_host_inventory(&hs.host, &inv)?;
    }
    println!("inventory: {total_servers} servers across all hosts (reconciled)");
    Ok(())
}

fn cmd_rules(store: &SqliteStore) -> Result<()> {
    let engine = RuleEngine::new(vec![
        Box::new(R5RepeatedRead::default()),
        Box::new(R1UnusedMcp::default()),
    ]);
    let findings = engine.run(store)?;
    let now = chrono::Utc::now().to_rfc3339();
    for f in &findings {
        store.upsert_finding(f, &now)?;
        println!(
            "[{}] {} scope={} ~{}토큰 절약  {}",
            f.severity.as_str(), f.rule_id, f.scope_ref, f.est_tokens_saved, f.evidence
        );
    }
    println!("total {} findings", findings.len());
    Ok(())
}

fn cmd_diary(store: &SqliteStore, date: Option<String>) -> Result<()> {
    let date = date.unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let cfg = DiaryConfig::default();
    let brief = assemble_brief(store, "Windows", &date, &cfg)?;

    let engine: Box<dyn Engine> = match OpenAiCompatEngine::from_env() {
        Some(e) => {
            println!("engine: {}", e.name());
            Box::new(e)
        }
        None => {
            println!("engine: mock (set AGENT_MENTOR_ENGINE_URL for real engine)");
            Box::new(MockEngine {
                canned: format!(
                    "오늘 {honorific}은 나를 {n}개 세션 굴렸다. 브리프 기반 요약이다.",
                    honorific = cfg.honorific,
                    n = brief.totals.session_count
                ),
            })
        }
    };
    let out = generate_diary(store, engine.as_ref(), &brief, &cfg)?;
    println!("diary → {} (~{} 토큰)", out.path.display(), out.tokens_used);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("all");
    let mut store = db()?;
    match cmd {
        "ingest" => cmd_ingest(&store)?,
        "inventory" => cmd_inventory(&mut store)?,
        "rules" => cmd_rules(&store)?,
        "diary" => cmd_diary(&store, args.get(2).cloned())?,
        "all" => {
            cmd_ingest(&store)?;
            cmd_inventory(&mut store)?;
            cmd_rules(&store)?;
            cmd_diary(&store, None)?;
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: agent-mentor [ingest|inventory|rules|diary [date]|all]");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
