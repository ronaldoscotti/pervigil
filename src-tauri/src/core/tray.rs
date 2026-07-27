//! What the tray shows, decided without a Tauri runtime, a clock, or the disk.
//!
//! The tray is applied from this, never computed at the call site: everything that
//! could be wrong here is testable, and everything downstream is a redraw.

use serde::Deserialize;

use super::session::{project, Session, SessionState};

/// The tray's words. The panel owns ten locales and the tray is built in Rust, so
/// the frontend pushes these down once at startup and again on a language change.
/// The defaults are English and are what the tray uses until it does — a tray that
/// reads correctly before the webview has loaded, rather than a blank one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStrings {
    /// `{n}` is replaced by the count.
    pub waiting: String,
    pub nothing: String,
    /// `{cost}` is replaced by the formatted figure.
    pub today: String,
    pub open: String,
    pub quit: String,
}

impl Default for TrayStrings {
    fn default() -> Self {
        Self {
            waiting: "{n} waiting".into(),
            nothing: "nothing waiting".into(),
            today: "${cost} today".into(),
            open: "Open Pervigil".into(),
            quit: "Quit".into(),
        }
    }
}

/// Which generated asset to display. `Overflow` is the `9+` artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKey {
    Bare,
    Count(u8),
    Overflow,
}

impl IconKey {
    /// The generated file's stem, minus the `-light` / `-dark` variant.
    pub fn asset(self) -> String {
        match self {
            IconKey::Bare => "bare".to_string(),
            IconKey::Count(n) => n.to_string(),
            IconKey::Overflow => "overflow".to_string(),
        }
    }
}

/// One clickable row in the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    pub id: String,
    pub label: String,
}

/// Everything the tray needs to draw itself.
#[derive(Debug, Clone, PartialEq)]
pub struct TrayView {
    pub icon: IconKey,
    pub tooltip: String,
    pub summary: String,
    pub items: Vec<TrayItem>,
    /// Changes only when something the menu actually draws changes. Nothing that
    /// moves every tick reaches the menu at all — see `tooltip`.
    pub signature: String,
}

/// The menu lists at most this many sessions. The summary always states the true
/// count, so a cap never hides the number.
const MENU_CAP: usize = 9;

/// The count the badge can still draw as a digit. Above it the icon says `9+` while
/// the summary and the tooltip keep the real figure.
const BADGE_MAX: usize = 9;

pub fn tray_view(sessions: &[Session], today_cost: f64, words: &TrayStrings) -> TrayView {
    let waiting: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.state == SessionState::WaitingOnYou)
        .collect();
    let count = waiting.len();
    let counted = match count {
        0 => words.nothing.clone(),
        n => words.waiting.replace("{n}", &n.to_string()),
    };

    let items: Vec<TrayItem> = waiting
        .iter()
        .take(MENU_CAP)
        .map(|session| TrayItem {
            id: session.id.clone(),
            label: match session.title.as_deref().filter(|t| !t.is_empty()) {
                Some(title) => format!("{} — {title}", project(&session.cwd)),
                None => project(&session.cwd),
            },
        })
        .collect();

    TrayView {
        icon: match count {
            0 => IconKey::Bare,
            n if n <= BADGE_MAX => IconKey::Count(n as u8),
            _ => IconKey::Overflow,
        },
        // The cost lives in the tooltip, not the summary. Updating menu text means
        // rebuilding the menu, and a rebuild closes it under the user's cursor — so a
        // figure that moves every tick would either close menus or, kept out of the
        // signature, quietly freeze. The tooltip is rewritten every tick for free.
        tooltip: format!(
            "Pervigil — {counted} · {}",
            words.today.replace("{cost}", &format!("{today_cost:.2}"))
        ),
        summary: counted.clone(),
        signature: [counted.as_str(), words.open.as_str(), words.quit.as_str()]
            .into_iter()
            .chain(items.iter().flat_map(|item| [item.id.as_str(), item.label.as_str()]))
            .collect::<Vec<_>>()
            .join("\u{1f}"),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> TrayStrings {
        TrayStrings::default()
    }

    fn session(id: &str, state: SessionState) -> Session {
        Session {
            id: id.into(),
            cwd: format!("/Users/x/work/{id}"),
            pid: Some(1),
            state,
            since: 0,
            last_active: 0,
            title: Some(format!("do the {id} thing")),
            git_branch: None,
            terminal: None,
        }
    }

    fn waiting(id: &str) -> Session {
        session(id, SessionState::WaitingOnYou)
    }

    fn idle(id: &str) -> Session {
        session(id, SessionState::Idle)
    }

    #[test]
    fn nothing_waiting_shows_the_bare_icon() {
        let view = tray_view(&[idle("a")], 0.0, &en());

        assert_eq!(view.icon, IconKey::Bare);
        assert_eq!(view.tooltip, "Pervigil — nothing waiting · $0.00 today");
        assert!(view.items.is_empty());
    }

    #[test]
    fn only_waiting_sessions_reach_the_icon_and_the_menu() {
        let view = tray_view(&[waiting("a"), idle("b"), waiting("c")], 4.2, &en());

        assert_eq!(view.icon, IconKey::Count(2));
        assert_eq!(view.summary, "2 waiting");
        assert_eq!(view.tooltip, "Pervigil — 2 waiting · $4.20 today");
        assert_eq!(view.items.len(), 2, "an idle session is not a menu item");
    }

    #[test]
    fn a_menu_row_names_its_project_and_its_title() {
        let view = tray_view(&[waiting("pervigil")], 0.0, &en());

        assert_eq!(view.items[0].label, "pervigil — do the pervigil thing");
        assert_eq!(view.items[0].id, "pervigil");
    }

    #[test]
    fn a_session_with_no_title_still_names_its_project() {
        let mut bare = waiting("pervigil");
        bare.title = None;

        let view = tray_view(&[bare], 0.0, &en());

        assert_eq!(view.items[0].label, "pervigil");
    }

    #[test]
    fn above_nine_the_icon_overflows_but_the_count_is_never_hidden() {
        let many: Vec<Session> = (0..12).map(|n| waiting(&n.to_string())).collect();

        let view = tray_view(&many, 0.0, &en());

        assert_eq!(view.icon, IconKey::Overflow);
        assert_eq!(view.tooltip, "Pervigil — 12 waiting · $0.00 today");
        assert!(view.summary.starts_with("12 waiting"));
        assert_eq!(view.items.len(), 9, "the menu caps; the summary does not");
    }

    #[test]
    fn exactly_nine_still_gets_a_digit() {
        let nine: Vec<Session> = (0..9).map(|n| waiting(&n.to_string())).collect();

        assert_eq!(tray_view(&nine, 0.0, &en()).icon, IconKey::Count(9));
    }

    #[test]
    fn the_signature_ignores_cost_so_a_rebuild_never_closes_an_open_menu() {
        let sessions = [waiting("a")];

        let cheap = tray_view(&sessions, 1.00, &en());
        let dear = tray_view(&sessions, 99.00, &en());

        assert_eq!(cheap.signature, dear.signature);
        assert_ne!(cheap.tooltip, dear.tooltip, "but the tooltip still moved");
    }

    #[test]
    fn the_words_come_from_the_caller_so_the_tray_speaks_the_panels_language() {
        let pt = TrayStrings {
            waiting: "{n} esperando por você".into(),
            nothing: "nada esperando".into(),
            today: "R${cost} hoje".into(),
            ..TrayStrings::default()
        };

        let busy = tray_view(&[waiting("a"), waiting("b")], 4.2, &pt);
        let quiet = tray_view(&[idle("a")], 0.0, &pt);

        assert_eq!(busy.tooltip, "Pervigil — 2 esperando por você · R$4.20 hoje");
        assert_eq!(quiet.tooltip, "Pervigil — nada esperando · R$0.00 hoje");
    }

    /// The menu is rebuilt only when the signature moves, so a language change that
    /// left it alone would leave yesterday's words on screen until a session did
    /// something. Every word the menu draws is in the signature; only the cost is not.
    #[test]
    fn the_signature_changes_when_the_language_does() {
        let sessions = [waiting("a")];
        let pt = TrayStrings {
            waiting: "{n} esperando".into(),
            open: "Abrir Pervigil".into(),
            quit: "Sair".into(),
            ..TrayStrings::default()
        };

        assert_ne!(
            tray_view(&sessions, 0.0, &en()).signature,
            tray_view(&sessions, 0.0, &pt).signature
        );
    }

    #[test]
    fn the_signature_changes_when_the_waiting_set_does() {
        let before = tray_view(&[waiting("a")], 0.0, &en());
        let after = tray_view(&[waiting("b")], 0.0, &en());

        assert_ne!(before.signature, after.signature);
    }
}
