mod commands;

use agent_mentor::store::SqliteStore;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub store: Mutex<SqliteStore>,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = SqliteStore::open(&dir.join("agent-mentor.db"))?;
            app.manage(AppState { store: Mutex::new(store) });
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
        ])
        .run(tauri::generate_context!())
        .expect("tauri 실행 실패");
}
