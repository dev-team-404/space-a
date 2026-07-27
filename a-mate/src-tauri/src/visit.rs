//! P3 — 방문 직후 자동 방명록 글루 (스펙 §2). 순수 로직은 agent_mentor::visit,
//! 여기는 설정 스냅샷(락) → 락 밖 네트워크·LLM 오케스트레이션만.

use agent_mentor::store::SqliteStore;

/// life_goto 성공 후 백그라운드 스레드에서 호출. 모든 실패는 warn+skip — 방문 자체에
/// 영향 없음 (스펙 §6). 미래 자율 방문 기능이 그대로 호출하는 재사용 진입점 (스펙 §8).
/// 동시 호출(연타 방 이동)은 전역 뮤텍스로 직렬화 — 뒤 호출의 GET이 앞 호출의 게시를
/// 보게 되어 쿨다운이 자연 적용된다.
pub fn maybe_sign_guestbook(store_mutex: &std::sync::Mutex<SqliteStore>, life_id: &str) {
    static SIGNING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _serial = match SIGNING.lock() {
        Ok(g) => g,
        Err(e) => { log::warn!("방문 방명록: 직렬화 락 오염(skip): {e}"); return; }
    };

    // ① 락: 토글·엔진·hub 설정·페르소나·vibe 스냅샷 → 즉시 해제 (G3 maybe_reply_guestbook 선례)
    let (enabled, engine, url, token, api_key, my_life_id, agent_id, title, user_name, mbti, vibe, is_weekend) =
        match store_mutex.lock() {
            Ok(store) => {
                let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
                // 토글 기본 on — 'false'로 저장된 경우에만 off (프론트 visitGuestbookEnabled와 동일 규칙)
                let enabled = store.get_setting("visit_guestbook_enabled").ok().flatten()
                    .map(|v| v != "false").unwrap_or(true);
                let now = chrono::Local::now();
                let today = now.format("%Y-%m-%d").to_string();
                let (session_count, tokens_today) = crate::commands::chat_context_inner(&store)
                    .map(|c| (c.session_count, c.tok_input + c.tok_output)).unwrap_or((0, 0));
                let work = agent_mentor::diary::collect_work_context(&store, &today, now.date_naive());
                let vibe = agent_mentor::mascot::owner_vibe(session_count, tokens_today, &work);
                let is_weekend = matches!(
                    chrono::Datelike::weekday(&now.date_naive()),
                    chrono::Weekday::Sat | chrono::Weekday::Sun
                );
                (
                    enabled,
                    crate::resolve_engine(&store),
                    get("hub_url"), get("hub_token"), get("hub_api_key"),
                    get("hub_life_id"), get("hub_agent_id"),
                    crate::commands::owner_title(&store), get("user_name"), get("user_mbti"),
                    vibe, is_weekend,
                )
            }
            Err(e) => { log::warn!("store lock poisoned: {e}"); return; }
        };
    if !enabled || life_id == my_life_id { return; } // 토글 off / 자기 방
    let Some(engine) = engine else { return; }; // 엔진 미설정 = 조용히 no-op
    if url.trim().is_empty() || token.is_empty() || agent_id.is_empty() { return; }

    // ② 락 없이 네트워크: 조회(쿨다운·이유 판정) → 주인 이름 → 생성 → 게시
    let client = agent_mentor::life_client::LifeClient {
        base_url: url,
        token,
        api_key: { let k = api_key.trim(); (!k.is_empty()).then(|| k.to_string()) },
    };
    let entries = match client.guestbook(life_id) {
        Ok(v) => v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default(),
        Err(e) => { log::warn!("방문 방명록: 조회 실패(보수적 skip): {e}"); return; }
    };
    let Some(reason) = agent_mentor::visit::visit_sign_decision(
        &entries, &agent_id, chrono::Utc::now(), is_weekend, vibe)
    else {
        return; // 쿨다운 중이거나 이유 없음 — 조용히 방문만
    };
    // 방 주인 이름 — 실패해도 이름 없이 진행 (스펙 §6)
    let owner_name = client.life_state(life_id).ok()
        .and_then(|v| v.get("owner_name").and_then(|n| n.as_str()).map(str::to_string));
    let mbti = agent_mentor::mascot::normalize_mbti(&mbti);
    let line = match agent_mentor::visit::compute_visit_guestbook(
        &engine, &title, mbti.as_deref(), reason, owner_name.as_deref())
    {
        Ok(l) => l,
        Err(e) => { log::warn!("방문 방명록: 생성 실패(skip): {e}"); return; }
    };
    let author = agent_mentor::mascot::bot_author_name(&user_name);
    if let Err(e) = client.add_guestbook(life_id, &line, author.as_deref(), None, Some("bot")) {
        log::warn!("방문 방명록: 게시 실패(skip): {e}");
    }
}
