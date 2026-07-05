#[cfg_attr(test, allow(dead_code, unused_imports))]
mod commands;
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
            ])
            .run(tauri::generate_context!())
            .expect("tauri 실행 실패");
    }
}
