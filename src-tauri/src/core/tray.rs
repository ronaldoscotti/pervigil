//! What the tray shows, decided without a Tauri runtime, a clock, or the disk.
//!
//! The tray is applied from this, never computed at the call site: everything that
//! could be wrong here is testable, and everything downstream is a redraw.

use super::session::{project, Session, SessionState};

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
    /// Changes only when the menu's *structure* does. The cost is deliberately
    /// excluded: it moves on almost every tick, and rebuilding a macOS menu closes
    /// it under the user's cursor.
    pub signature: String,
}

/// The menu lists at most this many sessions. The summary always states the true
/// count, so a cap never hides the number.
const MENU_CAP: usize = 9;

/// The count the badge can still draw as a digit. Above it the icon says `9+` while
/// the summary and the tooltip keep the real figure.
const BADGE_MAX: usize = 9;

pub fn tray_view(sessions: &[Session], today_cost: f64) -> TrayView {
    let waiting: Vec<&Session> = sessions
        .iter()
        .filter(|session| session.state == SessionState::WaitingOnYou)
        .collect();
    let count = waiting.len();

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
        tooltip: match count {
            0 => "Pervigil — nothing waiting".to_string(),
            n => format!("Pervigil — {n} waiting"),
        },
        summary: format!("{count} waiting · ${today_cost:.2} today"),
        signature: items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join("\u{1f}"),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let view = tray_view(&[idle("a")], 0.0);

        assert_eq!(view.icon, IconKey::Bare);
        assert_eq!(view.tooltip, "Pervigil — nothing waiting");
        assert!(view.items.is_empty());
    }

    #[test]
    fn only_waiting_sessions_reach_the_icon_and_the_menu() {
        let view = tray_view(&[waiting("a"), idle("b"), waiting("c")], 4.2);

        assert_eq!(view.icon, IconKey::Count(2));
        assert_eq!(view.summary, "2 waiting · $4.20 today");
        assert_eq!(view.items.len(), 2, "an idle session is not a menu item");
    }

    #[test]
    fn a_menu_row_names_its_project_and_its_title() {
        let view = tray_view(&[waiting("pervigil")], 0.0);

        assert_eq!(view.items[0].label, "pervigil — do the pervigil thing");
        assert_eq!(view.items[0].id, "pervigil");
    }

    #[test]
    fn a_session_with_no_title_still_names_its_project() {
        let mut bare = waiting("pervigil");
        bare.title = None;

        let view = tray_view(&[bare], 0.0);

        assert_eq!(view.items[0].label, "pervigil");
    }

    #[test]
    fn above_nine_the_icon_overflows_but_the_count_is_never_hidden() {
        let many: Vec<Session> = (0..12).map(|n| waiting(&n.to_string())).collect();

        let view = tray_view(&many, 0.0);

        assert_eq!(view.icon, IconKey::Overflow);
        assert_eq!(view.tooltip, "Pervigil — 12 waiting");
        assert!(view.summary.starts_with("12 waiting"));
        assert_eq!(view.items.len(), 9, "the menu caps; the summary does not");
    }

    #[test]
    fn exactly_nine_still_gets_a_digit() {
        let nine: Vec<Session> = (0..9).map(|n| waiting(&n.to_string())).collect();

        assert_eq!(tray_view(&nine, 0.0).icon, IconKey::Count(9));
    }

    #[test]
    fn the_signature_ignores_cost_so_a_rebuild_never_closes_an_open_menu() {
        let sessions = [waiting("a")];

        let cheap = tray_view(&sessions, 1.00);
        let dear = tray_view(&sessions, 99.00);

        assert_eq!(cheap.signature, dear.signature);
        assert_ne!(cheap.summary, dear.summary, "but the line itself moved");
    }

    #[test]
    fn the_signature_changes_when_the_waiting_set_does() {
        let before = tray_view(&[waiting("a")], 0.0);
        let after = tray_view(&[waiting("b")], 0.0);

        assert_ne!(before.signature, after.signature);
    }
}
