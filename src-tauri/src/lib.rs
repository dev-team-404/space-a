#[cfg_attr(test, allow(dead_code, unused_imports))]
mod commands;
#[cfg_attr(test, allow(dead_code))]
mod geometry;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod pipeline;
#[cfg_attr(test, allow(dead_code, unused_imports))]
mod tray;

use agent_mentor::store::SqliteStore;
use std::sync::Mutex;

pub struct AppState {
    pub store: Mutex<SqliteStore>,
    pub scan_tx: std::sync::mpsc::Sender<pipeline::PipelineMsg>,
}

pub fn run() {
    #[cfg(not(test))]
    {
        use tauri::Manager;
        tauri::Builder::default()
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent, // Windows에선 무시되는 인자
                None,
            ))
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    if window.label() == "chat" {
                        let _ = window.hide(); // 상주: destroy 대신 hide (스펙 §3)
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
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
                commands::get_summary,
                commands::list_findings,
                commands::list_diary_dates,
                commands::get_diary,
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
            ])
            .run(tauri::generate_context!())
            .expect("tauri 실행 실패");
    }
}
