pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("tauri 실행 실패");
}
