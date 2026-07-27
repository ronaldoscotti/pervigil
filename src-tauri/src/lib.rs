use tauri::Manager;

pub mod app;
pub mod config;
pub mod core;
pub mod io;
pub mod platform;
mod tray;



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
            app::snapshot,
            app::focus,
            app::set_notifications,
            app::set_dismiss_read,
            app::set_pinned,
            app::set_project_hidden,
            app::set_tray_strings,
            app::dismiss,
            app::open_settings,
            app::open_url,
            app::save_day_card,
            app::set_window_pinned
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
            // `None` is a user-interaction exit — the last window going away. `Some`
            // came from `AppHandle::exit`, which is our own Quit, and must be let
            // through. (The updater's relaunch needs no guard here: `prevent_exit`
            // already ignores itself at `RESTART_EXIT_CODE`.)
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
