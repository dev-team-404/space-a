use agent_mentor::diary::engine::{Engine, MockEngine, OpenAiCompatEngine};
use agent_mentor::diary::{assemble_brief, generate_diary, DiaryConfig};
use agent_mentor::ops;
use agent_mentor::store::SqliteStore;
use anyhow::Result;

fn db() -> Result<SqliteStore> {
    SqliteStore::open(std::path::Path::new("./agent-mentor.db"))
}

fn cmd_ingest(store: &SqliteStore) -> Result<()> {
    let r = ops::run_ingest(store)?;
    for w in &r.warnings {
        eprintln!("warn: {w}");
    }
    println!("ingested {} new events from {} files across all hosts", r.new_events, r.files);
    Ok(())
}

// read_json_guarded moved to ops module

fn cmd_inventory(store: &mut SqliteStore) -> Result<()> {
    let warnings = ops::run_inventory(store)?;
    for w in &warnings {
        eprintln!("warn: {w}");
    }
    println!("inventory reconciled across all hosts");
    Ok(())
}

fn cmd_rules(store: &SqliteStore) -> Result<()> {
    let findings = ops::run_rules(store)?;
    for f in &findings {
        println!(
            "[{}] {} scope={} ~{}토큰 절약  {}",
            f.severity.as_str(), f.rule_id, f.scope_ref, f.est_tokens_saved, f.evidence
        );
    }
    println!("total {} findings", findings.len());
    Ok(())
}

fn cmd_diary(store: &SqliteStore, date: Option<String>) -> Result<()> {
    let date = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
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

