use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

pub mod app;
pub mod config;
pub mod core;
pub mod io;
pub mod platform;

pub(crate) const TRAY_ID: &str = "pervigil";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    platform::restore_tool_path();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app::App::new())
        .invoke_handler(tauri::generate_handler![
            app::snapshot,
            app::focus,
            app::set_notifications,
            app::set_pinned,
            app::set_project_hidden,
            app::dismiss,
            app::open_settings,
            app::open_url,
            app::set_window_pinned
        ])
        .setup(|app| {
            let mut tray = TrayIconBuilder::with_id(TRAY_ID).on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    if let Some(panel) = tray.app_handle().get_webview_window("main") {
                        let _ = panel.show();
                        let _ = panel.set_focus();
                    }
                }
            });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
