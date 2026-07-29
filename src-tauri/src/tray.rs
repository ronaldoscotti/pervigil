//! The tray, applied: assets, menu, clicks, and the clock that keeps it fresh.
//! What the tray *says* is decided in [`crate::core::tray`] and only drawn here.
//! Every ignored result below is a window-server call the user cannot act on.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::Local;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Theme};

use crate::app::{self, App, Span};
use crate::core::tray::TrayView;

pub(crate) const TRAY_ID: &str = "specola";

/// Menu ids for a session row, so they can never collide with the fixed entries.
const SESSION: &str = "session:";
const OPEN: &str = "open";
const QUIT: &str = "quit";

/// When the panel's own poll last drove a snapshot. The ticker stands down while
/// that is recent: exactly one clock runs, and which one is decided by observing
/// whether the webview is still ticking rather than by tracking whether the window
/// is visible.
///
/// Visibility was the wrong question. Hiding a window does not stop its WebView, so
/// a hidden panel keeps polling; and a *throttled* WebView stops polling without any
/// window event to tell us. A lease answers both: the webview holds the clock while
/// it is actually calling, and loses it by going quiet, whatever the reason.
static POLLED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// How long the panel's poll holds the clock. Comfortably above its 1s period, so a
/// slow tick does not hand the clock over and briefly double it.
const LEASE: Duration = Duration::from_secs(3);

/// Called by the `snapshot` command: the panel just drove a tick, so it is the clock.
pub(crate) fn panel_polled() {
    *POLLED_AT.lock().expect("polled lock") = Some(Instant::now());
}

fn panel_holds_the_clock() -> bool {
    POLLED_AT
        .lock()
        .expect("polled lock")
        .is_some_and(|at| at.elapsed() < LEASE)
}

/// The signature of the menu currently on screen. Rebuilding a menu closes it under
/// the user's cursor, so it happens only when the structure actually changed.
static SHOWN: Mutex<String> = Mutex::new(String::new());

macro_rules! icon_table {
    ($($name:literal),* $(,)?) => {
        &[$(
            ($name, include_bytes!(concat!("../icons/tray/", $name, "-light.png")) as &[u8],
                    include_bytes!(concat!("../icons/tray/", $name, "-dark.png")) as &[u8]),
        )*]
    };
}

/// `(stem, black ink, white ink)`. Embedded rather than bundled as resources: there
/// is then no path where the packaged app finds an empty directory `tauri dev` found
/// full.
type IconRow = (&'static str, &'static [u8], &'static [u8]);
static ICONS: &[IconRow] =
    icon_table!["bare", "1", "2", "3", "4", "5", "6", "7", "8", "9", "overflow"];

/// Show the panel. The clock needs no bookkeeping here — the panel takes it back by
/// polling, and the ticker steps aside the moment it does.
pub(crate) fn show_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.show();
        let _ = panel.set_focus();
    }
}

pub(crate) fn hide_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.hide();
    }
}

pub(crate) fn build(app: &AppHandle) -> tauri::Result<()> {
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(image(app, &crate::core::tray::IconKey::Bare.asset())?)
        .on_menu_event(on_menu_event)
        .build(app)?;

    #[cfg(target_os = "macos")]
    let _ = tray.set_icon_as_template(true);
    // Either button opens the menu, and the panel is one item inside it. Right-click
    // cannot be made to do something else: `tray-icon` has a switch for the
    // right-click menu, Tauri 2.11 exposes only the left-click one, so the menu pops
    // on right-click no matter what we hang off the click event.
    let _ = tray.set_show_menu_on_left_click(true);

    apply(app);
    spawn_ticker(app.clone());
    Ok(())
}

/// Draw the current view. Called on every tick, and after anything that could have
/// changed it.
pub(crate) fn apply(app: &AppHandle) {
    let view = app.state::<App>().tray();
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    if let Ok(icon) = image(app, &view.icon.asset()) {
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_with_as_template(Some(icon), true);
        #[cfg(not(target_os = "macos"))]
        let _ = tray.set_icon(Some(icon));
    }

    // Unsupported on Linux; the menu's summary line carries the same information.
    #[cfg(not(target_os = "linux"))]
    let _ = tray.set_tooltip(Some(&view.tooltip));

    let mut shown = SHOWN.lock().expect("shown lock");
    if *shown != view.signature {
        if let Ok(menu) = menu(app, &view) {
            if tray.set_menu(Some(menu)).is_ok() {
                *shown = view.signature.clone();
            }
        }
    }
}

fn menu(app: &AppHandle, view: &TrayView) -> tauri::Result<Menu<tauri::Wry>> {
    let words = app.state::<App>().words();
    let menu = Menu::new(app)?;

    menu.append(
        &MenuItemBuilder::with_id("summary", &view.summary)
            .enabled(false)
            .build(app)?,
    )?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    for item in &view.items {
        let id = format!("{SESSION}{}", item.id);
        menu.append(&MenuItemBuilder::with_id(id, &item.label).build(app)?)?;
    }
    if !view.items.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    menu.append(&MenuItemBuilder::with_id(OPEN, &words.open).build(app)?)?;
    menu.append(&MenuItemBuilder::with_id(QUIT, &words.quit).build(app)?)?;
    Ok(menu)
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        OPEN => show_panel(app),
        QUIT => app.exit(0),
        _ if id.starts_with(SESSION) => jump(app, &id[SESSION.len()..]),
        _ => {}
    }
}

/// A tray click that cannot raise a window falls back to the clipboard. The panel
/// reports that with a toast and a hidden panel has none, so a silent copy would be
/// indistinguishable from a dead click — say so, or open the panel so its toast can.
fn jump(app: &AppHandle, id: &str) {
    let state = app.state::<App>();
    let outcome = state.focus(id);
    if outcome.raised {
        return;
    }

    if state.notifications_on() {
        app::fire(
            app,
            vec![app::Notice {
                title: outcome.label,
                body: outcome.resume.unwrap_or_default(),
            }],
        );
    } else {
        show_panel(app);
    }
}

fn image(app: &AppHandle, stem: &str) -> tauri::Result<Image<'static>> {
    let dark_background = app
        .get_webview_window("main")
        .and_then(|window| window.theme().ok())
        .is_none_or(|theme| theme == Theme::Dark);

    let row = ICONS
        .iter()
        .find(|(name, ..)| *name == stem)
        .unwrap_or(&ICONS[0]);
    let bytes = if dark_background { row.2 } else { row.1 };
    Image::from_bytes(bytes)
}

/// One thread for the app's life, idle whenever the panel's poll is doing the job.
/// Two clocks would fight over `App::targets`, which the next click depends on.
fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        if panel_holds_the_clock() {
            continue;
        }
        let state = app.state::<App>();
        state.snapshot(Span::Today, Local::now());
        app::fire(&app, state.take_pending());
        apply(&app);
    });
}
