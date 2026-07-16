#[cfg_attr(test, allow(dead_code, unused_imports))]
mod commands;
#[cfg_attr(test, allow(dead_code))]
mod geometry;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod pipeline;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod tray;

use agent_mentor::diary::engine::OpenAiCompatEngine;
use agent_mentor::store::SqliteStore;
use std::sync::Mutex;

pub struct AppState {
    pub store: Mutex<SqliteStore>,
    pub scan_tx: std::sync::mpsc::Sender<pipeline::PipelineMsg>,
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
        let api_key = store.get_setting("engine_key").ok().flatten().unwrap_or_default();
        let model = store
            .get_setting("engine_model")
            .ok()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        return Some(OpenAiCompatEngine { base_url, api_key, model });
    }
    OpenAiCompatEngine::from_env()
}

/// content_protected 설정을 두 창(chat·mascot)에 적용 — 화면 캡처/녹화에서 제외 (스펙 §7).
pub(crate) fn apply_content_protection(app: &tauri::AppHandle, on: bool) {
    use tauri::Manager;
    for label in ["chat", "mascot", "settings"] {
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

            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // 상주: destroy 대신 hide (스펙 §3). settings도 동일 — destroy되면 트레이에서 재오픈 불가
                    if matches!(window.label(), "chat" | "settings") {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
            })
            .setup(|app| {
                let dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&dir)?;
                let store = SqliteStore::open(&dir.join("agent-mentor.db"))?;
                let (tx, rx) = std::sync::mpsc::channel();
                app.manage(AppState { store: Mutex::new(store), scan_tx: tx.clone() });
                pipeline::start(app.handle().clone(), rx, tx);
                tray::setup_tray(app.handle())?;
                // mascot 창: 설정 보고 표시 + 위치 복원
                {
                    let state = app.state::<AppState>();
                    let (visible, pos) = {
                        let store = state.store.lock().map_err(|_| anyhow::anyhow!("store lock"))?;
                        (
                            store.get_setting("mascot_visible")?.map(|v| v == "true").unwrap_or(true),
                            store.get_setting("mascot_pos")?,
                        )
                    };
                    if let Some(w) = app.get_webview_window("mascot") {
                        let mut restored = false;
                        if let Some(p) = pos {
                            if let Some((x, y)) = p.split_once(',') {
                                if let (Ok(x), Ok(y)) = (x.parse::<i32>(), y.parse::<i32>()) {
                                    let monitors: Vec<(i32, i32, i32, i32)> = w
                                        .available_monitors()
                                        .map(|ms| ms.iter().map(|m| {
                                            let p = m.position();
                                            let s = m.size();
                                            (p.x, p.y, s.width as i32, s.height as i32)
                                        }).collect())
                                        .unwrap_or_default();
                                    let scale = w.scale_factor().unwrap_or(1.0);
                                    let side = (160.0 * scale) as i32;
                                    if geometry::sanitize_pos(x, y, side, side, &monitors) {
                                        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                                        restored = true;
                                    }
                                }
                            }
                        }
                        if !restored {
                            if let Ok(Some(mon)) = w.primary_monitor() {
                                let size = mon.size();
                                let mpos = mon.position();
                                // 창 160×160 + 여백 16px, 작업표시줄(대략 하단 48px) 위 (스펙 §1)
                                let x = mpos.x + size.width as i32 - 160 - 16;
                                let y = mpos.y + size.height as i32 - 160 - 64;
                                let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
                            }
                        }
                        if visible {
                            let _ = w.show();
                        }
                    }
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
                commands::get_settings,
                commands::set_setting,
                commands::run_scan_now,
                commands::open_chat_tab,
                commands::set_finding_status,
                commands::get_week_summary,
                commands::get_model_mix,
                commands::get_today_occasions,
                commands::get_session_transcript,
                commands::sessions_ctx,
                commands::chat_status,
                commands::chat_send,
                commands::engine_settings_get,
                commands::engine_settings_set,
                commands::engine_test,
                commands::coach_tip,
                commands::list_content,
                commands::set_content_status,
            ])
            .run(tauri::generate_context!())
            .expect("tauri 실행 실패");
    }
}
