use crate::{pipeline::PipelineMsg, AppState};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tauri_plugin_autostart::ManagerExt;

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let mascot_on = {
        let state = app.state::<AppState>();
        let guard = state.lock_store().ok();
        guard
            .and_then(|s| s.get_setting("mascot_visible").ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(true)
    };
    let mascot = CheckMenuItem::with_id(app, "mascot", "마스코트 표시", true, mascot_on, None::<&str>)?;
    let reset_mascot = MenuItem::with_id(app, "reset_mascot", "마스코트 위치 초기화", true, None::<&str>)?;
    let realtime_on = {
        let state = app.state::<AppState>();
        let guard = state.lock_store().ok();
        guard
            .and_then(|s| s.get_setting("realtime_advice").ok().flatten())
            .map(|v| v == "on")
            .unwrap_or(false)
    };
    let realtime = CheckMenuItem::with_id(app, "realtime", "실시간 조언", true, realtime_on, None::<&str>)?;
    let protect_on = {
        let state = app.state::<AppState>();
        let guard = state.lock_store().ok();
        guard
            .and_then(|s| s.get_setting("content_protected").ok().flatten())
            .map(|v| v == "true")
            .unwrap_or(false)
    };
    let protect = CheckMenuItem::with_id(app, "protect", "화면 캡처 보호", true, protect_on, None::<&str>)?;
    let chatter_level = {
        let state = app.state::<AppState>();
        let guard = state.lock_store().ok();
        let raw = guard
            .and_then(|s| s.get_setting("chatter_level").ok().flatten())
            .unwrap_or_else(|| "low".into());
        // 알 수 없는 값은 low로 정규화 (Mascot.svelte 기본값과 일치)
        if raw == "normal" || raw == "off" { raw } else { "low".to_string() }
    };
    let chatter_normal =
        CheckMenuItem::with_id(app, "chatter_normal", "자주", true, chatter_level == "normal", None::<&str>)?;
    let chatter_low =
        CheckMenuItem::with_id(app, "chatter_low", "가끔", true, chatter_level == "low", None::<&str>)?;
    let chatter_off =
        CheckMenuItem::with_id(app, "chatter_off", "안 함", true, chatter_level == "off", None::<&str>)?;
    let chatter_menu = SubmenuBuilder::with_id(app, "chatter", "잡담")
        .items(&[&chatter_normal, &chatter_low, &chatter_off])
        .build()?;
    let scan = MenuItem::with_id(app, "scan", "지금 스캔", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "설정", true, None::<&str>)?;
    let check_update = MenuItem::with_id(app, "check_update", "업데이트 확인", true, None::<&str>)?;
    let auto_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "시작 시 실행", true, auto_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &mascot, &reset_mascot, &realtime, &protect, &chatter_menu, &scan, &settings, &check_update, &autostart, &quit])?;

    let autostart_item = autostart.clone();
    let mascot_item = mascot.clone();
    let realtime_item = realtime.clone();
    let protect_item = protect.clone();
    let chatter_normal_item = chatter_normal.clone();
    let chatter_low_item = chatter_low.clone();
    let chatter_off_item = chatter_off.clone();
    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| tauri::image::Image::new_owned(vec![0, 0, 0, 0], 1, 1)),
        )
        // 툴팁에 버전 — 자동 업데이트가 적용됐는지 확인할 가장 싼 경로
        .tooltip(format!("Agent Mentor {}", app.package_info().version))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id().as_ref() {
            "open" => show_chat(app),
            "mascot" => {
                use tauri::Manager;
                if let Some(w) = app.get_webview_window("mascot") {
                    let was_visible = w.is_visible().unwrap_or(false);
                    if was_visible {
                        if let Err(error) = w.hide() {
                            log::error!("마스코트 창 숨김 실패: {error}");
                        }
                    } else {
                        crate::show_mascot(app, false);
                    }
                    let _ = mascot_item.set_checked(!was_visible);
                    if let Ok(store) = app.state::<AppState>().lock_store() {
                        let _ = store.set_setting("mascot_visible", if was_visible { "false" } else { "true" });
                    }
                }
            }
            "reset_mascot" => {
                crate::show_mascot(app, true);
                let _ = mascot_item.set_checked(true);
                if let Ok(store) = app.state::<AppState>().lock_store() {
                    let _ = store.set_setting("mascot_visible", "true");
                }
            }
            "realtime" => {
                use tauri::Emitter;
                // 주의: muda CheckMenuItem은 클릭 시 checked를 자동 토글하므로
                // is_checked()는 이미 새 값 — 설정(store)을 소스오브트루스로 파생한다.
                if let Ok(store) = app.state::<AppState>().lock_store() {
                    let cur = store
                        .get_setting("realtime_advice")
                        .ok()
                        .flatten()
                        .map(|v| v == "on")
                        .unwrap_or(false);
                    let next = !cur;
                    let _ = store.set_setting("realtime_advice", if next { "on" } else { "off" });
                    let _ = realtime_item.set_checked(next);
                }
                let _ = app.emit("settings:changed", ());
            }
            "protect" => {
                // muda CheckMenuItem은 클릭 시 checked 자동 토글 — store를 소스오브트루스로.
                let next = {
                    let state = app.state::<AppState>();
                    let Ok(store) = state.lock_store() else { return };
                    let cur = store
                        .get_setting("content_protected")
                        .ok()
                        .flatten()
                        .map(|v| v == "true")
                        .unwrap_or(false);
                    let next = !cur;
                    let _ = store.set_setting("content_protected", if next { "true" } else { "false" });
                    next
                }; // 락 해제 후 창 적용
                let _ = protect_item.set_checked(next);
                crate::apply_content_protection(app, next);
            }
            "chatter_normal" | "chatter_low" | "chatter_off" => {
                use tauri::Emitter;
                let level = match e.id().as_ref() {
                    "chatter_normal" => "normal",
                    "chatter_off" => "off",
                    _ => "low",
                };
                if let Ok(store) = app.state::<AppState>().lock_store() {
                    let _ = store.set_setting("chatter_level", level);
                }
                // 수동 라디오: muda 자동 토글을 덮어써 선택 항목만 체크 (store가 소스오브트루스)
                let _ = chatter_normal_item.set_checked(level == "normal");
                let _ = chatter_low_item.set_checked(level == "low");
                let _ = chatter_off_item.set_checked(level == "off");
                let _ = app.emit("settings:changed", ());
            }
            "scan" => {
                let _ = app.state::<AppState>().scan_tx.send(PipelineMsg::RunNow);
            }
            "settings" => show_settings(app),
            "check_update" => {
                use tauri::Emitter;
                show_chat(app);
                let _ = app.emit("update:check", ());
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

/// 설정 = 미니홈피 창의 설정 탭(연결 그룹). 별도 설정 창은 없다.
fn show_settings(app: &AppHandle) {
    use tauri::Emitter;
    show_chat(app);
    if let Err(error) = app.emit(
        "chat:goto-tab",
        crate::commands::GotoTabPayload { tab: "settings".into(), target: Some("conn".into()) },
    ) {
        log::error!("설정 탭 이동 이벤트 전송 실패: {error}");
    }
}
