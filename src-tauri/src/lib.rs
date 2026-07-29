use tauri::Manager;

pub mod app;
pub mod commands;
pub mod config;
pub mod io;
pub mod platform;
mod tray;

/// The pure core, a crate of its own — re-exported so `crate::core::…` still reads as
/// the heart of this app rather than as a third-party dependency.
pub use specola_core as core;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    platform::restore_tool_path();

    tauri::Builder::default()
        // Must be the first plugin: a second launch focuses the running panel
        // instead of opening another window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_panel(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(app::App::new())
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::focus,
            commands::set_notifications,
            commands::set_dismiss_read,
            commands::set_pinned,
            commands::set_project_hidden,
            commands::set_tray_strings,
            commands::dismiss,
            commands::open_settings,
            commands::open_url,
            commands::save_day_card,
            commands::set_window_pinned
        ])
        // Closing the panel hides it; the tray's Quit is the only real exit.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                tray::hide_panel(window.app_handle());
            }
        })
        .setup(|app| {
            tray::build(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // `Some` came from `AppHandle::exit` — our own Quit — and must be let
            // through.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_panel(app),
            _ => {
                let _ = app;
            }
        });
}
