//! P3 방문 방명록 + 묶음 ② 방문 소재 스냅샷 글루 (스펙 §3). 순수 로직은 agent_mentor::visit,
//! 여기는 설정 스냅샷(락) → 락 밖 네트워크·LLM → 짧은 락 기록만.

use agent_mentor::life_client::LifeClient;
use agent_mentor::mascot::OwnerVibe;
use agent_mentor::store::{LifeVisit, SqliteStore};
use agent_mentor::visit::{SignReason, VISIT_RETENTION_DAYS};

/// 수동 방 이동인가, 봇이 스스로 간 자율 방문인가 (스펙 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitMode {
    Manual,
    Auto,
}

impl VisitMode {
    fn as_str(self) -> &'static str {
        match self {
            VisitMode::Manual => "manual",
            VisitMode::Auto => "auto",
        }
    }
}

/// 방문 순간 확보한 소재 — life_visits 1행이 된다 (스펙 §2).
struct VisitSnapshot {
    life_id: String,
    owner_name: Option<String>,
    design_json: Option<String>,
    diary_excerpt_json: String,
}

/// 게이트를 통과해 문구까지 만든 상태. 게시 직전 재판정에 필요한 값을 함께 든다.
struct SignPlan {
    line: String,
    author: Option<String>,
    agent_id: String,
    is_weekend: bool,
    vibe: OwnerVibe,
    mode: VisitMode,
}

/// prepare_visit 결과 — commit_visit이 소비한다.
pub struct VisitPrep {
    client: LifeClient,
    mode: VisitMode,
    snapshot: VisitSnapshot,
    sign: Option<SignPlan>,
}

/// 방명록을 남길 계획이 있나(자율 방문은 없으면 이동을 취소한다 — 스펙 §4 ④).
pub fn visit_has_sign(prep: &VisitPrep) -> bool {
    prep.sign.is_some()
}

/// 동시 호출(연타 방 이동·자율 방문 동시 발동) 직렬화 — 뒤 호출의 GET이 앞 호출의 게시를
/// 보게 되어 쿨다운이 자연 적용된다.
static SIGNING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// life_goto 성공 후(수동) 백그라운드 스레드에서 호출되는 기존 진입점. 준비→커밋을 잇는 래퍼.
/// 모든 실패는 warn+skip — 방문 자체에 영향 없음.
pub fn maybe_sign_guestbook(store_mutex: &std::sync::Mutex<SqliteStore>, life_id: &str) {
    let _serial = match SIGNING.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("방문: 직렬화 락 오염(skip): {e}");
            return;
        }
    };
    if let Some(prep) = prepare_visit(store_mutex, life_id, VisitMode::Manual) {
        commit_visit(store_mutex, prep);
    }
}

/// ① 짧은 락으로 설정·페르소나 스냅샷 → ② 락 밖 GET(방명록·방 상태·상대 공개 일기)
/// → ③ 게이트 판정 → ④ 락 밖 LLM 문구 생성. 실패는 warn+skip(None).
/// 자기 방·hub 미연결은 조용히 None.
pub fn prepare_visit(
    store_mutex: &std::sync::Mutex<SqliteStore>,
    life_id: &str,
    mode: VisitMode,
) -> Option<VisitPrep> {
    // ① 락: 토글·엔진·hub 설정·페르소나·vibe 스냅샷 → 즉시 해제
    let (
        sign_enabled,
        engine,
        url,
        token,
        api_key,
        my_life_id,
        agent_id,
        title,
        user_name,
        mbti,
        vibe,
        is_weekend,
    ) = match store_mutex.lock() {
        Ok(store) => {
            let get = |k: &str| store.get_setting(k).ok().flatten().unwrap_or_default();
            // 토글 기본 on — 'false'로 저장된 경우에만 off (프론트 visitGuestbookEnabled와 동일 규칙)
            let sign_enabled = store
                .get_setting("visit_guestbook_enabled")
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true);
            let now = chrono::Local::now();
            let today = now.format("%Y-%m-%d").to_string();
            let (session_count, tokens_today) = crate::commands::chat_context_inner(&store)
                .map(|c| (c.session_count, c.tok_input + c.tok_output))
                .unwrap_or((0, 0));
            let work = agent_mentor::diary::collect_work_context(&store, &today, now.date_naive());
            let vibe = agent_mentor::mascot::owner_vibe(session_count, tokens_today, &work);
            let is_weekend = matches!(
                chrono::Datelike::weekday(&now.date_naive()),
                chrono::Weekday::Sat | chrono::Weekday::Sun
            );
            (
                sign_enabled,
                crate::resolve_engine(&store),
                get("hub_url"),
                get("hub_token"),
                get("hub_api_key"),
                get("hub_life_id"),
                get("hub_agent_id"),
                crate::commands::owner_title(&store),
                get("user_name"),
                get("user_mbti"),
                vibe,
                is_weekend,
            )
        }
        Err(e) => {
            log::warn!("store lock poisoned: {e}");
            return None;
        }
    };
    if life_id == my_life_id {
        return None; // 자기 방
    }
    if url.trim().is_empty() || token.is_empty() || agent_id.is_empty() {
        return None; // hub 미연결
    }

    let client = LifeClient {
        base_url: url,
        token,
        api_key: {
            let k = api_key.trim();
            (!k.is_empty()).then(|| k.to_string())
        },
    };

    // ② 락 없이 GET — 방 상태(주인 이름·꾸밈)와 상대 공개 일기는 실패해도 소재만 비운다.
    // 공개범위 판정은 서버가 단일 지점에서 수행한다(볼 수 없으면 빈 배열 — ADR 0025).
    let state = client.life_state(life_id).ok();
    let owner_name = state
        .as_ref()
        .and_then(|v| v.get("owner_name").and_then(|n| n.as_str()).map(str::to_string));
    let design_json = state
        .as_ref()
        .and_then(|v| v.get("design"))
        .and_then(|d| serde_json::to_string(d).ok());
    let excerpts = client
        .diaries(life_id)
        .ok()
        .and_then(|v| v.get("diaries").and_then(|d| d.as_array()).cloned())
        .map(|rows| agent_mentor::visit::visit_diary_excerpts(&rows))
        .unwrap_or_default();
    let diary_excerpt_json = serde_json::to_string(
        &excerpts
            .iter()
            .map(|(date, excerpt)| serde_json::json!({"date": date, "excerpt": excerpt}))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());
    let snapshot = VisitSnapshot {
        life_id: life_id.to_string(),
        owner_name: owner_name.clone(),
        design_json,
        diary_excerpt_json,
    };

    // ③ 게이트 — 토글 off나 엔진 미설정이면 소재만 남기고 방명록은 건너뛴다
    let sign = 'sign: {
        if !sign_enabled {
            break 'sign None;
        }
        let Some(engine) = engine else { break 'sign None };
        let entries = match client.guestbook(life_id) {
            Ok(v) => v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default(),
            Err(e) => {
                log::warn!("방문 방명록: 조회 실패(보수적 skip): {e}");
                break 'sign None;
            }
        };
        let now = chrono::Utc::now();
        let reason = match mode {
            VisitMode::Manual => {
                agent_mentor::visit::visit_sign_decision(&entries, &agent_id, now, is_weekend, vibe)
            }
            // 자율 방문은 이유 게이트 없이 쿨다운만 (스펙 §3.1)
            VisitMode::Auto => agent_mentor::visit::visit_cooldown_ok(&entries, &agent_id, now)
                .then_some(SignReason::PlayVisit),
        };
        let Some(reason) = reason else { break 'sign None };

        // ④ 락 밖 LLM
        let mbti = agent_mentor::mascot::normalize_mbti(&mbti);
        match agent_mentor::visit::compute_visit_guestbook(
            &engine,
            &title,
            mbti.as_deref(),
            reason,
            owner_name.as_deref(),
        ) {
            Ok(line) => Some(SignPlan {
                line,
                author: agent_mentor::mascot::bot_author_name(&user_name),
                agent_id,
                is_weekend,
                vibe,
                mode,
            }),
            Err(e) => {
                log::warn!("방문 방명록: 생성 실패(skip): {e}");
                None
            }
        }
    };

    Some(VisitPrep { client, mode, snapshot, sign })
}

/// ⑤ 게시 직전 재확인 후 POST → ⑥ 짧은 락으로 방문 1행 기록. 반환값 = 방명록을 남겼나.
/// 방명록이 skip돼도 방문 기록·스냅샷은 남긴다 — P1·P2 소재는 방명록과 무관하다.
pub fn commit_visit(store_mutex: &std::sync::Mutex<SqliteStore>, prep: VisitPrep) -> bool {
    let VisitPrep { client, mode, snapshot, sign } = prep;
    let mut signed = false;
    if let Some(plan) = sign {
        // 생성(수 초~수십 초) 동안 내가 이 방에 글을 남겼을 수 있다 — 재조회로 쿨다운·이유 재판정
        match client.guestbook(&snapshot.life_id) {
            Ok(v) => {
                let fresh =
                    v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                let now = chrono::Utc::now();
                let still_ok = match plan.mode {
                    VisitMode::Manual => agent_mentor::visit::visit_sign_decision(
                        &fresh,
                        &plan.agent_id,
                        now,
                        plan.is_weekend,
                        plan.vibe,
                    )
                    .is_some(),
                    VisitMode::Auto => {
                        agent_mentor::visit::visit_cooldown_ok(&fresh, &plan.agent_id, now)
                    }
                };
                if still_ok {
                    match client.add_guestbook(
                        &snapshot.life_id,
                        &plan.line,
                        plan.author.as_deref(),
                        None,
                        Some("bot"),
                    ) {
                        Ok(_) => signed = true,
                        Err(e) => log::warn!("방문 방명록: 게시 실패(skip): {e}"),
                    }
                }
            }
            Err(e) => log::warn!("방문 방명록: 게시 전 재확인 실패(skip): {e}"),
        }
    }

    // ⑥ 짧은 락: 방문 기록 + 보존 기간 초과 정리(방문에 편승 — 스펙 §2)
    let visit = LifeVisit {
        life_id: snapshot.life_id,
        visited_at: chrono::Utc::now().to_rfc3339(),
        kind: mode.as_str().to_string(),
        owner_name: snapshot.owner_name,
        design_json: snapshot.design_json,
        diary_excerpt_json: snapshot.diary_excerpt_json,
        signed,
    };
    match store_mutex.lock() {
        Ok(store) => {
            if let Err(e) = store.record_life_visit(&visit) {
                log::warn!("방문 기록 실패: {e}");
            }
            let cutoff =
                (chrono::Utc::now() - chrono::Duration::days(VISIT_RETENTION_DAYS)).to_rfc3339();
            if let Err(e) = store.prune_life_visits(&cutoff) {
                log::warn!("방문 기록 정리 실패: {e}");
            }
        }
        Err(e) => log::warn!("store lock poisoned: {e}"),
    }
    signed
}
