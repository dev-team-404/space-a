#[cfg_attr(test, allow(dead_code, unused_imports))]
mod commands;
#[cfg_attr(test, allow(dead_code))]
mod geometry;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod pipeline;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod tray;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod visit;

use agent_mentor::diary::engine::OpenAiCompatEngine;
use agent_mentor::store::SqliteStore;
use std::sync::Mutex;

pub struct AppState {
    pub store: Mutex<SqliteStore>,
    pub scan_tx: std::sync::mpsc::Sender<pipeline::PipelineMsg>,
    /// 마스코트 말풍선/메뉴 열림 여부 — 클릭 통과 폴러가 소비 (setup의 폴러 주석 참조)
    pub mascot_expanded: std::sync::atomic::AtomicBool,
}

/// 마스코트 창 논리 크기(px). 창은 이 크기로 **상시 고정** — 확장/접힘을 리사이즈로
/// 구현하면 창 원점 이동 + WebView 비동기 리페인트 때문에 로봇이 튀어 보이는
/// 깜빡임이 생긴다. 접힘 상태의 여백은 클릭 통과로 처리한다.
pub(crate) const MASCOT_W: f64 = 320.0;
pub(crate) const MASCOT_H: f64 = 230.0;
/// 로봇 상호작용 영역(우하단, 논리 px) — 이 밖의 투명 여백은 접힘 상태에서 클릭 통과
pub(crate) const ROBOT_SIDE: f64 = 160.0;

fn parse_mascot_pos(value: &str) -> Option<(i32, i32)> {
    let (x, y) = value.split_once(',')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

/// 저장 좌표를 현재 디스플레이 구성에 맞게 검증하고 필요하면 주 모니터로 복구한다.
/// 복구 결과와 디스플레이 지문은 즉시 저장해 다음 실행도 같은 위치를 사용한다.
pub(crate) fn place_mascot(app: &tauri::AppHandle, force_default: bool) -> anyhow::Result<()> {
    use tauri::Manager;

    let window = app
        .get_webview_window("mascot")
        .ok_or_else(|| anyhow::anyhow!("mascot window not found"))?;
    let monitors = window.available_monitors()?;
    let bounds: Vec<(i32, i32, i32, i32)> = monitors
        .iter()
        .map(|monitor| {
            let pos = monitor.position();
            let size = monitor.size();
            (pos.x, pos.y, size.width as i32, size.height as i32)
        })
        .collect();
    let layout = geometry::display_layout_signature(
        &monitors
            .iter()
            .map(|monitor| {
                let pos = monitor.position();
                let size = monitor.size();
                (
                    pos.x,
                    pos.y,
                    size.width as i32,
                    size.height as i32,
                    (monitor.scale_factor() * 1000.0).round() as u32,
                )
            })
            .collect::<Vec<_>>(),
    );
    let (saved_pos, saved_layout) = {
        let state = app.state::<AppState>();
        let store = state
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock"))?;
        (
            store.get_setting("mascot_pos")?,
            store.get_setting("mascot_display_layout")?,
        )
    };

    let scale = window.scale_factor().unwrap_or(1.0);
    let (width, height) = ((MASCOT_W * scale) as i32, (MASCOT_H * scale) as i32);
    let robot = (ROBOT_SIDE * scale) as i32;
    let restored = (!force_default)
        .then(|| {
            let pos = parse_mascot_pos(saved_pos.as_deref()?)?;
            (saved_layout.as_deref() == Some(layout.as_str())
                && geometry::sanitize_pos(pos.0, pos.1, width, height, robot, &bounds))
            .then_some(pos)
        })
        .flatten();
    let position = if let Some(pos) = restored {
        pos
    } else {
        let primary = window
            .primary_monitor()?
            .ok_or_else(|| anyhow::anyhow!("primary monitor not found"))?;
        let size = primary.size();
        let origin = primary.position();
        let primary_scale = primary.scale_factor();
        (
            origin.x + size.width as i32
                - (MASCOT_W * primary_scale) as i32
                - (16.0 * primary_scale) as i32,
            origin.y + size.height as i32
                - (MASCOT_H * primary_scale) as i32
                - (64.0 * primary_scale) as i32,
        )
    };

    window.set_position(tauri::PhysicalPosition::new(position.0, position.1))?;
    let state = app.state::<AppState>();
    let store = state
        .store
        .lock()
        .map_err(|_| anyhow::anyhow!("store lock"))?;
    store.set_setting("mascot_pos", &format!("{},{}", position.0, position.1))?;
    store.set_setting("mascot_display_layout", &layout)?;
    Ok(())
}

pub(crate) fn show_mascot(app: &tauri::AppHandle, force_default: bool) {
    use tauri::Manager;

    if let Err(error) = place_mascot(app, force_default) {
        log::error!("마스코트 위치 복구 실패: {error}");
    }
    match app.get_webview_window("mascot") {
        Some(window) => {
            if let Err(error) = window.show() {
                log::error!("마스코트 창 표시 실패: {error}");
            }
        }
        None => log::error!("마스코트 창 표시 실패: window not found"),
    }
}

/// 엔진 해석 우선순위: 설정 UI(store) → .env — 설정 창에서 지정한 값이 있으면 그것을 쓰고,
/// 없으면 기존 AGENT_MENTOR_ENGINE_* 환경변수로 폴백한다 (지빈의 .env 워크플로 보존).
/// 호출자는 락을 짧게 잡고(네트워크 전 해제 규율) 이 함수에 &SqliteStore만 넘긴다.
pub(crate) fn resolve_engine(store: &SqliteStore) -> Option<OpenAiCompatEngine> {
    let url = store
        .get_setting("engine_url")
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(base_url) = url {
        let api_key = store
            .get_setting("engine_key")
            .ok()
            .flatten()
            .unwrap_or_default();
        let model = store
            .get_setting("engine_model")
            .ok()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        return Some(OpenAiCompatEngine {
            base_url,
            api_key,
            model,
        });
    }
    OpenAiCompatEngine::from_env()
}

/// 캐릭터 이미지 생성용 설정 해석 — 설정창 저장값(image_*) → env 순.
/// 텍스트 엔진과 **분리**된다(사내 LM Studio는 이미지 생성을 못 하므로, 이미지만 OpenRouter 등으로).
pub(crate) fn resolve_sprite_cfg(store: &SqliteStore) -> Option<agent_mentor::sprite::SpriteConfig> {
    let get = |k: &str| store.get_setting(k).ok().flatten();
    agent_mentor::sprite::SpriteConfig::resolve(
        get("image_url").as_deref(),
        get("image_key").as_deref(),
        get("image_model").as_deref(),
    )
}

/// content_protected 설정을 두 창(chat·mascot)에 적용 — 화면 캡처/녹화에서 제외 (스펙 §7).
pub(crate) fn apply_content_protection(app: &tauri::AppHandle, on: bool) {
    use tauri::Manager;
    for label in ["chat", "mascot"] {
        if let Some(w) = app.get_webview_window(label) {
            if let Err(e) = w.set_content_protected(on) {
                log::warn!("content_protected({label}) 적용 실패: {e}");
            }
        }
    }
}

pub fn run() {
    #[cfg(not(test))]
    {
        // dev 빌드에서만 프로젝트 루트 .env 자동 로드 — 엔진 env(AGENT_MENTOR_ENGINE_*) 편의.
        // dotenv()는 cwd와 상위 디렉터리를 탐색하므로 tauri dev의 cwd(src-tauri)에서도 루트 .env를 찾음.
        // release(배포) 빌드는 로드하지 않음 — cwd .env 의존 방지.
        #[cfg(debug_assertions)]
        let _ = dotenvy::dotenv();

        use tauri::Manager;
        tauri::Builder::default()
            .plugin(
                tauri_plugin_log::Builder::new()
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: Some("agent-mentor".into()),
                        }),
                    ])
                    .level(log::LevelFilter::Info)
                    .max_file_size(512_000)
                    .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                    .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                    .build(),
            )
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent, // Windows에선 무시되는 인자
                None,
            ))
            .plugin(tauri_plugin_opener::init()) // 공식 가이드 링크를 시스템 브라우저로 열기
            .plugin(tauri_plugin_updater::Builder::new().build()) // 인앱 자동 업데이트
            .plugin(tauri_plugin_process::init()) // 업데이트 후 재시작
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // 상주: destroy 대신 hide (스펙 §3) — destroy되면 트레이에서 재오픈 불가
                    if window.label() == "chat" {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
            })
            .setup(|app| {
                // 테스트용 오버라이드: 한 PC에서 두 인스턴스를 돌릴 때 데이터 디렉터리 분리
                // (docs/design/life-visit.md §5) — 미설정이면 기존 경로 그대로.
                let dir = match std::env::var("AGENT_MENTOR_DATA_DIR") {
                    Ok(d) if !d.trim().is_empty() => std::path::PathBuf::from(d),
                    _ => app.path().app_data_dir()?,
                };
                std::fs::create_dir_all(&dir)?;
                let store = SqliteStore::open(&dir.join("agent-mentor.db"))?;
                let (tx, rx) = std::sync::mpsc::channel();
                app.manage(AppState {
                    store: Mutex::new(store),
                    scan_tx: tx.clone(),
                    mascot_expanded: std::sync::atomic::AtomicBool::new(false),
                });
                pipeline::start(app.handle().clone(), rx, tx);
                tray::setup_tray(app.handle())?;
                // 방 방문: 앱 시작 시 내 에이전트는 항상 자기 방에서 출발한다
                // (서버는 마지막 위치를 기억하지만, 세션 시작의 기본값은 내 방 — 설계 §3)
                {
                    let state = app.state::<AppState>();
                    let cfg = state.store.lock().ok().map(|s| {
                        let get = |k: &str| s.get_setting(k).ok().flatten().unwrap_or_default();
                        (
                            get("hub_url"),
                            get("hub_token"),
                            get("hub_life_id"),
                            get("hub_api_key"),
                        )
                    });
                    if let Some((url, token, life_id, api_key)) = cfg {
                        if !url.trim().is_empty() && !token.is_empty() && !life_id.is_empty() {
                            let api_key = {
                                let k = api_key.trim();
                                if k.is_empty() {
                                    None
                                } else {
                                    Some(k.to_string())
                                }
                            };
                            std::thread::spawn(move || {
                                let client = agent_mentor::life_client::LifeClient {
                                    base_url: url,
                                    token,
                                    api_key,
                                };
                                if let Err(e) = client.enter(&life_id, None) {
                                    log::warn!("시작 시 내 방 입장 실패(무시): {e}");
                                }
                            });
                        }
                    }
                }
                // mascot 창: 설정을 보고 현재 디스플레이 구성에 맞게 위치 복원
                {
                    let visible = app
                        .state::<AppState>()
                        .store
                        .lock()
                        .map_err(|_| anyhow::anyhow!("store lock"))?
                        .get_setting("mascot_visible")?
                        .map(|value| value == "true")
                        .unwrap_or(true);
                    if visible {
                        show_mascot(app.handle(), false);
                    } else if let Err(error) = place_mascot(app.handle(), false) {
                        log::error!("마스코트 시작 위치 복구 실패: {error}");
                    }
                }
                // 마스코트 클릭 통과 폴러 — 창은 상시 확장 크기라 접힘 상태의 투명 여백이
                // 뒤 앱의 클릭을 막는다. 전역 커서를 폴링해 로봇 영역(우하단 160×160) 밖이면
                // ignore_cursor_events를 켠다. 말풍선/메뉴 열림(mascot_expanded) 중엔 항상 상호작용.
                {
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        let mut ignoring: Option<bool> = None;
                        loop {
                            std::thread::sleep(std::time::Duration::from_millis(80));
                            let Some(w) = handle.get_webview_window("mascot") else {
                                continue;
                            };
                            if !w.is_visible().unwrap_or(false) {
                                continue;
                            }
                            let expanded = handle
                                .state::<AppState>()
                                .mascot_expanded
                                .load(std::sync::atomic::Ordering::Relaxed);
                            let interactive = expanded
                                || match (
                                    handle.cursor_position(),
                                    w.outer_position(),
                                    w.scale_factor(),
                                ) {
                                    (Ok(c), Ok(p), Ok(s)) => {
                                        let side = ROBOT_SIDE * s;
                                        let rx = p.x as f64 + MASCOT_W * s - side;
                                        let ry = p.y as f64 + MASCOT_H * s - side;
                                        c.x >= rx && c.x < rx + side && c.y >= ry && c.y < ry + side
                                    }
                                    _ => true, // 판단 불가 시 상호작용 가능 쪽으로 (클릭을 잃지 않게)
                                };
                            let want_ignore = !interactive;
                            if ignoring != Some(want_ignore)
                                && w.set_ignore_cursor_events(want_ignore).is_ok()
                            {
                                ignoring = Some(want_ignore);
                            }
                        }
                    });
                }
                // content_protected 설정을 시작 시 실제 적용 (스펙 §7)
                // 설정 읽기 실패(락 poison·일시 잠금)로 시작이 죽지 않도록 안전 폴백 — 기본 미보호 (에러 철학 §9)
                {
                    let state = app.state::<AppState>();
                    let on = state
                        .store
                        .lock()
                        .ok()
                        .and_then(|store| store.get_setting("content_protected").ok().flatten())
                        .map(|v| v == "true")
                        .unwrap_or(false);
                    if on {
                        apply_content_protection(app.handle(), true);
                    }
                }
                log::info!("Agent Mentor 시작 — 파이프라인·트레이 초기화 완료");
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                commands::get_summary,
                commands::list_findings,
                commands::list_diary_dates,
                commands::get_diary,
                commands::get_daily_line,
                commands::get_chatter_pool,
                commands::get_mascot_seed,
                commands::get_sprite,
                commands::get_face_icon,
                commands::get_daily_cut,
                commands::get_occupant_sprite,
                commands::request_occupant_sprite,
                commands::image_settings_get,
                commands::image_settings_set,
                commands::image_test,
                commands::profile_get,
                commands::profile_set,
                commands::memory_list,
                commands::memory_add,
                commands::memory_update,
                commands::memory_delete,
                commands::mascot_preview,
                commands::mascot_commit,
                commands::generate_skill_draft,
                commands::save_skill_draft,
                commands::get_settings,
                commands::set_setting,
                commands::run_scan_now,
                commands::open_chat_tab,
                commands::set_finding_status,
                commands::get_week_summary,
                commands::get_profile,
                commands::get_model_mix,
                commands::get_today_occasions,
                commands::get_session_transcript,
                commands::sessions_ctx,
                commands::chat_status,
                commands::chat_send,
                commands::engine_settings_get,
                commands::engine_settings_set,
                commands::knowledge_hub_settings_get,
                commands::knowledge_hub_settings_set,
                commands::knowledge_hub_share_set,
                commands::theme_get,
                commands::theme_set,
                commands::engine_test,
                commands::coach_tip,
                commands::list_content,
                commands::set_content_status,
                commands::hub_settings_get,
                commands::hub_connect,
                commands::hub_disconnect,
                commands::life_view,
                commands::life_capabilities,
                commands::life_list,
                commands::life_goto,
                commands::life_move_cell,
                commands::life_save_design,
                commands::life_people,
                commands::life_set_friend,
                commands::life_set_content_visibility,
                commands::life_content_access,
                commands::life_set_diary_visibility,
                commands::life_diaries,
                commands::life_guestbook,
                commands::life_add_guestbook,
                commands::life_delete_guestbook,
                commands::life_set_bubble,
                commands::life_sync_mascot_image,
                commands::life_mascot_image,
                commands::robot_spec_for_seed,
                commands::mascot_set_expanded,
            ])
            .run(tauri::generate_context!())
            .expect("tauri 실행 실패");
    }
}
