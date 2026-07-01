use agent_mentor::adapter::{ClaudeCodeAdapter, SourceAdapter};
use agent_mentor::diary::engine::{Engine, MockEngine, OpenAiCompatEngine};
use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
use agent_mentor::inventory::parse_claude_json;
use agent_mentor::rules::r1_unused_mcp::R1UnusedMcp;
use agent_mentor::rules::r5_repeated_read::R5RepeatedRead;
use agent_mentor::rules::RuleEngine;
use agent_mentor::store::{ingest_file, SqliteStore};
use anyhow::Result;
use std::path::PathBuf;

fn db() -> Result<SqliteStore> {
    SqliteStore::open(std::path::Path::new("./agent-mentor.db"))
}

fn cmd_ingest(store: &SqliteStore) -> Result<()> {
    let adapter = ClaudeCodeAdapter::windows();
    let files = adapter.discover()?;
    let mut total = 0usize;
    for f in &files {
        total += ingest_file(store, &adapter, f)?;
    }
    store.rebuild_rollup()?;
    println!("ingested {total} new events from {} files", files.len());
    Ok(())
}

fn cmd_inventory(store: &SqliteStore) -> Result<()> {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    let path = PathBuf::from(home).join(".claude.json");
    let raw = std::fs::read_to_string(&path)?;
    let json: serde_json::Value = serde_json::from_str(&raw)?;
    let parsed = parse_claude_json(&json);
    let mut servers = 0;
    for (project, srvs) in &parsed {
        servers += srvs.len();
        store.upsert_inventory("Windows", project, srvs)?;
    }
    println!("inventory: {} projects, {servers} servers", parsed.len());
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
