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

/// AX 튜터 — 역량 프로필 감지 + 커리큘럼 팁/소식 큐레이션. (ingest 후 실행)
fn cmd_curate(store: &SqliteStore) -> Result<()> {
    let profile = agent_mentor::profile::detect_profile(store)?;
    println!("== 역량 프로필 (총 이벤트 {}) ==", profile.total_events);
    for d in &profile.dims {
        println!("  {:16} {:11}  {}", d.dimension.key(), format!("{:?}", d.mastery), d.evidence);
    }
    println!("  → 프론티어(지금 배울 것): {:?}", profile.frontier().map(|d| d.key()));
    println!("  → 관심 태그: {:?}", profile.active_tags);

    // 피드: 파일 지정(AGENT_MENTOR_CHANGELOG_FILE)이면 그걸, 아니면 실제 네트워크 fetch(실패 시 빈).
    let feed = match std::env::var("AGENT_MENTOR_CHANGELOG_FILE") {
        Ok(path) => {
            let md = std::fs::read_to_string(&path).unwrap_or_default();
            let n = agent_mentor::content::ClaudeChangelogSource::default().parse_markdown(&md);
            eprintln!("feed: changelog 파일에서 {}건", n.len());
            n
        }
        Err(_) => {
            let n = ops::fetch_feed_items();
            eprintln!("feed: 네트워크에서 {}건", n.len());
            n
        }
    };
    let now = chrono::Utc::now().to_rfc3339();
    let visible = ops::run_curation(store, feed, &now)?;
    println!("\n== AX 튜터가 지금 보여줄 것 (노출 {}건, content_items에 persist) ==", visible.len());
    for r in &visible {
        println!("  [{:>4}] {:16} {}", r.score, r.dimension.as_deref().unwrap_or("news"), r.title);
    }
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

/// a-hub 지식 공유 — 유의미 finding을 이슈→해결로 발행 (SPACE_A_HUB_URL 미설정 시 no-op).
fn cmd_hub_share(store: &SqliteStore) -> Result<()> {
    let Some(cfg) = agent_mentor::hub::HubConfig::from_env() else {
        println!("hub-share: SPACE_A_HUB_URL 미설정 (또는 SPACE_A_SHARE=off) — 건너뜀");
        return Ok(());
    };
    let report = agent_mentor::hub::run_share(store, &cfg)?;
    for w in &report.warnings {
        eprintln!("warn: {w}");
    }
    for (key, page) in &report.published {
        println!("published: {key} → page {page}");
    }
    println!(
        "hub-share: {}건 발행({}건 재개), {}건 보류/비대상",
        report.published.len(),
        report.resumed,
        report.skipped
    );
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
        "curate" => cmd_curate(&store)?,
        "diary" => cmd_diary(&store, args.get(2).cloned())?,
        "hub-share" => cmd_hub_share(&store)?,
        "all" => {
            cmd_ingest(&store)?;
            cmd_inventory(&mut store)?;
            cmd_rules(&store)?;
            cmd_curate(&store)?;
            cmd_diary(&store, None)?;
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: agent-mentor [ingest|inventory|rules|curate|diary [date]|hub-share|all]");
        }
    }
    Ok(())
}

