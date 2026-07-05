use crate::{pipeline::PipelineMsg, AppState};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tauri_plugin_autostart::ManagerExt;

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let mascot_on = {
        let state = app.state::<AppState>();
        let guard = state.store.lock().ok();
        guard
            .and_then(|s| s.get_setting("mascot_visible").ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(true)
    };
    let mascot = CheckMenuItem::with_id(app, "mascot", "마스코트 표시", true, mascot_on, None::<&str>)?;
    let realtime_on = {
        let state = app.state::<AppState>();
        let guard = state.store.lock().ok();
        guard
            .and_then(|s| s.get_setting("realtime_advice").ok().flatten())
            .map(|v| v == "on")
            .unwrap_or(false)
    };
    let realtime = CheckMenuItem::with_id(app, "realtime", "실시간 조언", true, realtime_on, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "지금 스캔", true, None::<&str>)?;
    let auto_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "시작 시 실행", true, auto_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &mascot, &realtime, &scan, &autostart, &quit])?;

    let autostart_item = autostart.clone();
    let mascot_item = mascot.clone();
    let realtime_item = realtime.clone();
    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| tauri::image::Image::new_owned(vec![0, 0, 0, 0], 1, 1)),
        )
        .tooltip("Agent Mentor")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id().as_ref() {
            "open" => show_chat(app),
            "mascot" => {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("mascot") {
                    let was_visible = w.is_visible().unwrap_or(false);
                    let _ = if was_visible { w.hide() } else { w.show() };
                    let _ = mascot_item.set_checked(!was_visible);
                    if let Ok(store) = app.state::<AppState>().store.lock() {
                        let _ = store.set_setting("mascot_visible", if was_visible { "false" } else { "true" });
                    }
                }
            }
            "realtime" => {
                use tauri::Emitter;
                let cur = realtime_item.is_checked().unwrap_or(false);
                let next = !cur;
                let _ = realtime_item.set_checked(next);
                if let Ok(store) = app.state::<AppState>().store.lock() {
                    let _ = store.set_setting("realtime_advice", if next { "on" } else { "off" });
                }
                let _ = app.emit("settings:changed", ());
            }
            "scan" => {
                let _ = app.state::<AppState>().scan_tx.send(PipelineMsg::RunNow);
            }
            "autostart" => {
                let al = app.autolaunch();
                let cur = al.is_enabled().unwrap_or(false);
                let _ = if cur { al.disable() } else { al.enable() };
                let _ = autostart_item.set_checked(al.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_chat(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn toggle_chat(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("chat") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_chat(app);
        }
    }
}

fn show_chat(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("chat") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
