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
        let files = adapter.discover().unwrap_or_default();
        file_count += files.len();
        for f in &files {
            total += ingest_file(store, &adapter, f)?;
        }
        println!("  [{}] {} files", hs.host, files.len());
    }
    store.rebuild_rollup()?;
    println!("ingested {total} new events from {file_count} files across all hosts");
    Ok(())
}

fn cmd_inventory(store: &SqliteStore) -> Result<()> {
    let mut total_servers = 0usize;
    for hs in enumerate_hosts() {
        let claude_json = std::fs::read_to_string(hs.claude_json())
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .unwrap_or(serde_json::Value::Null);
        let settings = std::fs::read_to_string(hs.settings_json())
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .unwrap_or(serde_json::Value::Null);
        let cache = hs.claude_root.join("plugins").join("cache");

        for (project, servers) in collect_host_inventory(&claude_json, &settings, &cache) {
            total_servers += servers.len();
            store.upsert_inventory(&hs.host, &project, &servers)?;
        }
    }
    println!("inventory: {total_servers} servers across all hosts");
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
    let store = db()?;
    match cmd {
        "ingest" => cmd_ingest(&store)?,
        "inventory" => cmd_inventory(&store)?,
        "rules" => cmd_rules(&store)?,
        "diary" => cmd_diary(&store, args.get(2).cloned())?,
        "all" => {
            cmd_ingest(&store)?;
            cmd_inventory(&store)?;
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
