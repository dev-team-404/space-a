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
            // 팀 지식(pull)은 허브 설정+토큰(env 또는 settings 보존분)이 있을 때만
            let hub_src = agent_mentor::hub::HubConfig::resolve(store)
                .and_then(|cfg| {
                    let stored = store.get_setting("knowledge_hub_token").ok().flatten();
                    agent_mentor::hub::pull_source(&cfg, stored)
                });
            // CLI는 사용자가 직접 부르는 1회성 실행이라 TTL을 적용하지 않는다(전 소스 참여).
            let (n, _) = ops::fetch_feed_items(hub_src, &ops::FeedPlan::all());
            eprintln!("feed: 네트워크에서 {}건", n.len());
            n
        }
    };
    // 로컬 공지(⑥ 스펙 §6.1) — 네트워크 0, 파일 읽기. 파이프라인과 같은 재료를 CLI에도.
    let mut feed = feed;
    let announcements = agent_mentor::announcements::collect_local_announcements();
    eprintln!("feed: 로컬 공지 {}건", announcements.len());
    feed.extend(agent_mentor::announcements::announcement_items(&announcements));
    let now = chrono::Utc::now().to_rfc3339();
    // E — 마켓플레이스 카탈로그(② 미설치 추천 재료)도 파이프라인과 동일하게 fetch(실패 시 빈).
    let (catalog, _) = ops::fetch_marketplace_catalog(&ops::FeedPlan::all());
    eprintln!("catalog: 공식 마켓플레이스 {}건", catalog.len());
    let visible = ops::run_curation(store, feed, &catalog, &now, &ops::FeedPlan::all())?;
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

/// 인정 루프 — 내가 발행한 지식을 남이 인용했는지 확인하고 축하 문구를 만든다.
/// 앱의 `maybe_poll_reuse`와 같은 경로(조회 → diff → 노트 → 문구)를 헤드리스로 검증한다.
fn cmd_reuse(store: &SqliteStore) -> Result<()> {
    let Some(cfg) = agent_mentor::hub::HubConfig::resolve(store) else {
        println!("reuse: 허브 미설정 — 건너뜀");
        return Ok(());
    };
    let Some(token) = cfg.token.clone().or(store.get_setting("knowledge_hub_token")?) else {
        println!("reuse: 토큰 없음 — 먼저 hub-share로 등록하세요");
        return Ok(());
    };
    let mine = store.hub_published_page_ids()?;
    println!("reuse: 내 발행분 {}건 — {:?}", mine.len(), mine);
    if mine.is_empty() {
        return Ok(());
    }
    let client = agent_mentor::hub::HubClient {
        base_url: cfg.base_url.clone(),
        api_key: cfg.api_key.clone(),
        token,
    };
    let rows = match client.reuse_events(200)? {
        Some(r) => r,
        None => {
            println!("reuse: 허브에 /reuse-events 없음(구버전 배포) — 건너뜀");
            return Ok(());
        }
    };
    let cursor = store.get_setting("inbound_reuse_cursor")?.filter(|v| !v.is_empty());
    println!("reuse: 서버 이벤트 {}건, 커서 {:?}", rows.len(), cursor);

    let (fresh, next) =
        agent_mentor::inbound::select_new_reuses(&rows, &mine, &cfg.user_id, cursor.as_deref());
    let notes: Vec<_> = fresh.iter().filter_map(agent_mentor::hub::to_reuse_note).collect();
    match agent_mentor::hub::render_reuse_praise(&notes) {
        Some(msg) => {
            for n in &notes {
                println!("  축하 대상: page {} · {} 팀 · cross_team={}", n.page_id, n.space, n.cross_team);
            }
            println!("🤖 {msg}");
        }
        None => println!("reuse: 새 재사용 없음 (조용히 넘어감)"),
    }
    if let Some(next) = next {
        store.set_setting("inbound_reuse_cursor", &next)?;
        println!("reuse: 커서 전진 → {next}");
    }
    Ok(())
}

/// a-hub 지식 공유 — 유의미 finding을 이슈→해결로 발행 (SPACE_A_HUB_URL 미설정 시 no-op).
fn cmd_hub_share(store: &SqliteStore) -> Result<()> {
    let Some(cfg) = agent_mentor::hub::HubConfig::resolve(store) else {
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
    for (key, page, reuse) in &report.cited {
        println!("cited(재사용): {key} → 기존 page {page} (reuse {reuse})");
    }
    println!(
        "hub-share: {}건 발행, {}건 인용(재사용), {}건 재개, {}건 보류/비대상",
        report.published.len(),
        report.cited.len(),
        report.resumed,
        report.skipped
    );
    Ok(())
}

/// 텔레메트리(#46) — 지정 날짜(기본: 어제)의 파생 신호를 허브 전용 공간에 발행.
fn cmd_telemetry(store: &SqliteStore, date: Option<String>) -> Result<()> {
    let Some(cfg) = agent_mentor::hub::HubConfig::resolve(store) else {
        println!("telemetry: SPACE_A_HUB_URL 미설정 — 건너뜀");
        return Ok(());
    };
    let date = date.unwrap_or_else(|| {
        (chrono::Local::now() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string()
    });
    match agent_mentor::hub::run_telemetry_push(store, &cfg, &date)? {
        agent_mentor::hub::TelemetryOutcome::Published { date, page_id } => {
            println!("telemetry published: {date} → page {page_id}");
        }
        agent_mentor::hub::TelemetryOutcome::AlreadySent => {
            println!("telemetry: {date} 이미 발행됨 (하루 1회)");
        }
        agent_mentor::hub::TelemetryOutcome::Skipped(why) => {
            println!("telemetry skipped: {why}");
        }
    }
    Ok(())
}

/// 세션 회고(스펙 2026-07-19) — "고생 끝 해결" 세션을 로컬 생성 요약으로 발행. Engine 필수.
fn cmd_retro(store: &SqliteStore) -> Result<()> {
    let Some(cfg) = agent_mentor::hub::HubConfig::resolve(store) else {
        println!("retro: SPACE_A_HUB_URL 미설정 — 건너뜀");
        return Ok(());
    };
    let Some(engine) = OpenAiCompatEngine::from_env() else {
        println!("retro: 엔진 미설정 — 발행 보류 (품질 > 정시성)");
        return Ok(());
    };
    let published = agent_mentor::hub::run_retro_push(store, &cfg, &engine)?;
    for (sess, page) in &published {
        println!("retro published: {} → page {page}", &sess[..8.min(sess.len())]);
    }
    println!("retro: {}건 발행", published.len());
    Ok(())
}

/// R6 반복 지시 → SKILL.md 초안 (킥오프 3대 차별점). R6 finding마다 초안을 출력.
/// 엔진(AGENT_MENTOR_ENGINE_URL) 설정 시 LLM 생성, 아니면 결정론 골격.
fn cmd_skill_draft(store: &SqliteStore) -> Result<()> {
    use agent_mentor::rules::r6_repeated_prompts::R6RepeatedPrompts;
    use agent_mentor::rules::Rule;
    let findings = R6RepeatedPrompts::default().evaluate(store)?;
    if findings.is_empty() {
        println!("R6 반복 지시 패턴 없음 (같은 지시가 ≥3 세션에서 반복, 최근 14일). 초안 대상 없음.");
        return Ok(());
    }
    let engine = OpenAiCompatEngine::from_env();
    if engine.is_some() {
        println!("engine: {}\n", engine.as_ref().unwrap().name());
    } else {
        println!("engine: (미설정) — 결정론 골격으로 생성\n");
    }
    for f in &findings {
        let host = f.scope_host.clone().unwrap_or_default();
        let rep = f.evidence["repeated_prompt"].as_str().unwrap_or_default();
        let ctx = agent_mentor::skill_draft::gather_context_for_finding(store, &host, rep, &f.evidence)?;
        let draft = agent_mentor::skill_draft::build_draft(
            &ctx,
            engine.as_ref().map(|e| e as &dyn Engine),
        );
        println!("═══ 반복 지시: \"{}\" ({}개 세션) → skills/{}/SKILL.md ═══",
            rep, ctx.session_count, draft.slug);
        println!("{}\n", draft.markdown);
    }
    Ok(())
}

/// Tier 2 질적 코칭(채팅) — 질문 의도 분류 → 코칭 브리프 → 엔진 답변. 검증용.
fn cmd_coach_chat(store: &SqliteStore, question: &str) -> Result<()> {
    use agent_mentor::chat::{
        assemble_coaching_brief, build_coaching_system_prompt, classify_intent, ChatIntent,
    };
    use agent_mentor::diary::engine::ChatMessage;
    let intent = classify_intent(question);
    println!("질문: \"{question}\"");
    println!("→ 분류된 의도(티어): {}\n", intent.key());
    match intent {
        ChatIntent::Coaching => {
            let brief = assemble_coaching_brief(store)?;
            let system = build_coaching_system_prompt(&brief);
            println!("─── Tier 2 코칭 브리프(시스템 프롬프트) ───\n{system}\n");
            match OpenAiCompatEngine::from_env() {
                Some(eng) => {
                    println!("engine: {}", eng.name());
                    let out = eng.chat(
                        &system,
                        &[ChatMessage { role: "user".into(), content: question.into(), ..Default::default() }],
                    )?;
                    println!("\n─── 튜터 답변 ───\n{}", out.text);
                }
                None => println!("(엔진 미설정 — 브리프까지만 확인)"),
            }
        }
        _ => println!("(이 데모는 코칭 티어 전용 — '깊게 봐줘/약점/개선' 류 질문을 넣어보세요)"),
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("all");
    let mut store = db()?;
    match cmd {
        "coach-chat" => {
            let q = args.get(2).map(|s| s.as_str()).unwrap_or("이번 주 깊게 봐줘");
            cmd_coach_chat(&store, q)?;
        }
        "occ-sprite" => {
            // 방 점유자 스프라이트 생성 검증: seed → PNG 파일
            let seed = args.get(2).map(|s| s.as_str()).unwrap_or("demo|guest");
            match agent_mentor::sprite::SpriteConfig::from_env() {
                Some(cfg) => {
                    let png = agent_mentor::sprite::sprite_for_seed(&cfg, seed)?;
                    let name = format!("occ-{}.png", agent_mentor::sprite::seed_cache_name(seed));
                    std::fs::write(&name, &png)?;
                    println!("occ-sprite: seed={seed} → {name} ({} bytes)", png.len());
                }
                None => println!("occ-sprite: 이미지 모델 미설정 (AGENT_MENTOR_ENGINE_URL 필요)"),
            }
        }
        "ingest" => cmd_ingest(&store)?,
        "inventory" => cmd_inventory(&mut store)?,
        "rules" => cmd_rules(&store)?,
        "curate" => cmd_curate(&store)?,
        "diary" => cmd_diary(&store, args.get(2).cloned())?,
        "skill-draft" => cmd_skill_draft(&store)?,
        "hub-share" => cmd_hub_share(&store)?,
        "reuse" => cmd_reuse(&store)?,
        "telemetry" => cmd_telemetry(&store, args.get(2).cloned())?,
        "retro" => cmd_retro(&store)?,
        "all" => {
            cmd_ingest(&store)?;
            cmd_inventory(&mut store)?;
            cmd_rules(&store)?;
            cmd_curate(&store)?;
            cmd_diary(&store, None)?;
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: agent-mentor [ingest|inventory|rules|curate|diary [date]|skill-draft|coach-chat <질문>|hub-share|telemetry [date]|all]");
        }
    }
    Ok(())
}

