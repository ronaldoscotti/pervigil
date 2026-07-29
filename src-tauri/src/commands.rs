//! The Tauri command layer: everything the webview can call, and nothing else. Each
//! one is a thin wrapper, which is what keeps `app` and `core` testable without a
//! running app.

use chrono::Local;

use crate::app::{App, FocusOutcome, Snapshot};
use crate::core::event::Timestamp;
use crate::core::notify::Notice;
use crate::core::span::Span;
use crate::core::tray::TrayStrings;

#[tauri::command]
pub fn snapshot(span: Span, app: tauri::State<'_, App>, handle: tauri::AppHandle) -> Snapshot {
    // Stamped either side of the work: the first claims the clock, the second keeps
    // it through a tick that ran longer than the lease.
    crate::tray::panel_polled();
    let snapshot = app.snapshot(span, Local::now());
    crate::tray::apply(&handle);
    fire(&handle, app.take_pending());
    crate::tray::panel_polled();
    snapshot
}

/// The panel owns the ten locales; the tray is built in Rust. This is how the words
/// get there — at startup, and again whenever the language changes.
#[tauri::command]
pub fn set_tray_strings(words: TrayStrings, app: tauri::State<'_, App>) {
    app.set_strings(words);
}

#[tauri::command]
pub fn focus(id: String, app: tauri::State<'_, App>) -> FocusOutcome {
    app.focus(&id)
}

/// The settings commands return the persistence error so the panel can say the
/// change did not stick, rather than showing a setting the disk never took.
#[tauri::command]
pub fn set_notifications(on: bool, app: tauri::State<'_, App>) -> Result<(), String> {
    app.set_notifications(on).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_dismiss_read(on: bool, app: tauri::State<'_, App>) -> Result<(), String> {
    app.set_dismiss_read(on).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_pinned(id: String, pinned: bool, app: tauri::State<'_, App>) -> Result<(), String> {
    app.set_pinned(&id, pinned).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_project_hidden(
    project: String,
    hidden: bool,
    app: tauri::State<'_, App>,
) -> Result<(), String> {
    app.set_project_hidden(&project, hidden)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dismiss(id: String, app: tauri::State<'_, App>) -> Result<(), String> {
    app.dismiss(&id, Local::now().timestamp().max(0) as Timestamp)
        .map_err(|e| e.to_string())
}

/// Open `~/.claude/settings.json` in the user's default editor for JSON — specola
/// never edits it, so the fastest honest help is to take you there.
#[tauri::command]
pub fn open_settings(app: tauri::State<'_, App>) {
    let path = app.settings_path();
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(path).spawn();
}

/// Open a URL in the default browser — used by the About panel's website link.
/// Restricted to http(s) so this webview-exposed command can't be coerced into
/// opening a local file or app.
#[tauri::command]
pub fn open_url(url: String) {
    if !allowed_url(&url) {
        return;
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(url).spawn();
}

/// Save the shared day card to Downloads and reveal it — the reliable path when the
/// webview can't write an image to the clipboard. Returns the saved path.
#[tauri::command]
pub fn save_day_card(bytes: Vec<u8>, app: tauri::State<'_, App>) -> Result<String, String> {
    let dir = app.home().join("Downloads");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("specola-day.png");
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn();
    Ok(path.to_string_lossy().into_owned())
}

fn allowed_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Toggle the panel's always-on-top. A watch instrument you keep in view, but yours
/// to unpin when it's in the way.
#[tauri::command]
pub fn set_window_pinned(pinned: bool, window: tauri::WebviewWindow) {
    let _ = window.set_always_on_top(pinned);
}

/// Show the queued notices. Best-effort: a notification that won't show must never
/// disturb the panel.
pub(crate) fn fire(handle: &tauri::AppHandle, notices: Vec<Notice>) {
    use tauri_plugin_notification::NotificationExt;

    for notice in notices {
        let _ = handle
            .notification()
            .builder()
            .title(notice.title)
            .body(notice.body)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_allows_only_web_schemes() {
        assert!(allowed_url("https://ronaldoscotti.com"));
        assert!(allowed_url("http://example.com"));
        assert!(!allowed_url("file:///etc/passwd"));
        assert!(!allowed_url("/Applications/Calculator.app"));
        assert!(!allowed_url("-a Calculator"));
    }
}
