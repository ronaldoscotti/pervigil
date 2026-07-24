use std::io;

use serde::Serialize;

use crate::core::terminal::Terminal;

/// What the running platform can actually do — detected once, then fed into the pure
/// [`select`] so tier choice stays testable without shelling out.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caps {
    pub tmux: bool,
    pub code: bool,
    pub iterm: bool,
}

/// The best-available way to reach one session, chosen per spec §5. Every session
/// resolves to one — [`Strategy::Clipboard`] is the universal floor, so the feature
/// never fully fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Strategy {
    /// tmux pane id, e.g. `%3` — exact.
    Tmux { pane: String },
    /// iTerm2 session UUID — tab-level.
    Iterm { session: String },
    /// `code <path>` — folder-level window.
    VsCode { path: String },
    /// Copy the resume command; always works, raises nothing.
    Clipboard { resume: String },
}

impl Strategy {
    /// The verb the row shows, so a click never surprises: precise tiers say "jump",
    /// the floor says "copy".
    pub fn label(&self) -> &'static str {
        match self {
            Strategy::Tmux { .. } => "Jump to pane",
            Strategy::Iterm { .. } => "Focus tab",
            Strategy::VsCode { .. } => "Open in VS Code",
            Strategy::Clipboard { .. } => "Copy resume command",
        }
    }
}

/// What a focus attempt actually did — the honest distinction the UI reports back:
/// a window was raised, or we could only copy the resume command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Raised,
    Copied,
}

/// Runs one external command. Behind a trait so [`WindowFocuser`] argv-building is
/// testable without spawning a terminal or mutating the real clipboard.
pub trait Runner {
    fn run(&self, program: &str, args: &[&str], stdin: Option<&str>) -> io::Result<()>;
}

/// Raises the window/pane/folder a [`Strategy`] names, per platform.
pub trait WindowFocuser {
    fn focus(&self, strategy: &Strategy) -> io::Result<Reach>;
}

/// Spawns real processes. `code`/`osascript`/`tmux` are fire-and-forget; `pbcopy`
/// needs the resume string on its stdin.
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], stdin: Option<&str>) -> io::Result<()> {
        use std::process::{Command, Stdio};

        let mut command = Command::new(program);
        command.args(args);
        command.stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });

        let mut child = command.spawn()?;
        if let Some(input) = stdin {
            use io::Write;
            child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("no stdin"))?
                .write_all(input.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }
}

/// The focuser for the running platform. macOS is the tested target; elsewhere focus
/// is honestly unsupported for now rather than faked.
pub fn platform() -> Box<dyn WindowFocuser + Send + Sync> {
    #[cfg(target_os = "macos")]
    {
        Box::new(super::focuser_macos::MacFocuser::new(SystemRunner))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(Unsupported)
    }
}

/// Detected once at startup; `iterm` is a platform fact (osascript ships with macOS),
/// the rest are `PATH` lookups.
impl Caps {
    pub fn detect() -> Caps {
        Caps::from_lookup(on_path, cfg!(target_os = "macos"))
    }

    fn from_lookup(has: impl Fn(&str) -> bool, macos: bool) -> Caps {
        Caps {
            tmux: has("tmux"),
            code: has("code"),
            iterm: macos,
        }
    }
}

fn on_path(binary: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(binary).is_file()))
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
struct Unsupported;

#[cfg(not(target_os = "macos"))]
impl WindowFocuser for Unsupported {
    fn focus(&self, _strategy: &Strategy) -> io::Result<Reach> {
        Err(io::Error::other(
            "window focus is not supported on this platform yet",
        ))
    }
}

/// Pick the highest tier the captured hint *and* the platform both support. tmux is
/// innermost so it wins; then iTerm2, then VS Code; then the clipboard floor. A tier
/// whose binary is missing is skipped, never attempted — a guessed raise is worse
/// than an honest copy.
pub fn select(term: Option<&Terminal>, cwd: &str, id: &str, caps: Caps) -> Strategy {
    if let Some(term) = term {
        if let (true, Some(pane)) = (caps.tmux, &term.tmux_pane) {
            return Strategy::Tmux { pane: pane.clone() };
        }
        if let (true, "iTerm.app", Some(session)) = (
            caps.iterm,
            term.program.as_deref().unwrap_or(""),
            &term.iterm_session,
        ) {
            return Strategy::Iterm {
                session: session.clone(),
            };
        }
        if caps.code && term.program.as_deref() == Some("vscode") {
            return Strategy::VsCode {
                path: cwd.to_string(),
            };
        }
    }

    Strategy::Clipboard {
        resume: format!("claude --resume {id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Caps {
        Caps {
            tmux: true,
            code: true,
            iterm: true,
        }
    }

    fn term(program: &str) -> Terminal {
        Terminal {
            program: Some(program.into()),
            ..Default::default()
        }
    }

    #[test]
    fn tmux_wins_because_it_is_the_innermost_context() {
        let hint = Terminal {
            program: Some("iTerm.app".into()),
            tmux_pane: Some("%3".into()),
            iterm_session: Some("uuid".into()),
        };

        let strategy = select(Some(&hint), "/p", "s1", all());

        assert_eq!(strategy, Strategy::Tmux { pane: "%3".into() });
    }

    #[test]
    fn iterm_is_chosen_when_there_is_no_tmux() {
        let hint = Terminal {
            program: Some("iTerm.app".into()),
            iterm_session: Some("uuid".into()),
            ..Default::default()
        };

        let strategy = select(Some(&hint), "/p", "s1", all());

        assert_eq!(
            strategy,
            Strategy::Iterm {
                session: "uuid".into()
            }
        );
    }

    #[test]
    fn vscode_focuses_the_folder() {
        let strategy = select(Some(&term("vscode")), "/Users/x/proj", "s1", all());

        assert_eq!(
            strategy,
            Strategy::VsCode {
                path: "/Users/x/proj".into()
            }
        );
    }

    #[test]
    fn an_unknown_terminal_falls_to_the_clipboard_floor() {
        let strategy = select(Some(&term("Ghostty")), "/p", "s1", all());

        assert_eq!(
            strategy,
            Strategy::Clipboard {
                resume: "claude --resume s1".into()
            }
        );
    }

    #[test]
    fn no_hint_at_all_is_the_clipboard_floor() {
        let strategy = select(None, "/p", "abc", all());

        assert_eq!(
            strategy,
            Strategy::Clipboard {
                resume: "claude --resume abc".into()
            }
        );
    }

    #[test]
    fn a_tier_whose_binary_is_missing_is_skipped_not_guessed() {
        let hint = Terminal {
            tmux_pane: Some("%3".into()),
            ..Default::default()
        };
        let no_tmux = Caps {
            tmux: false,
            ..all()
        };

        // tmux is the only viable precise tier here; without the binary we must not
        // fall sideways into VS Code — we copy.
        let strategy = select(Some(&hint), "/p", "s1", no_tmux);

        assert!(matches!(strategy, Strategy::Clipboard { .. }));
    }

    #[test]
    fn vscode_without_the_cli_degrades_to_copy() {
        let no_code = Caps {
            code: false,
            ..all()
        };

        let strategy = select(Some(&term("vscode")), "/p", "s1", no_code);

        assert!(matches!(strategy, Strategy::Clipboard { .. }));
    }

    #[test]
    fn caps_reflect_the_binaries_on_path_and_the_platform() {
        let only_code = Caps::from_lookup(|bin| bin == "code", true);

        assert!(only_code.code);
        assert!(!only_code.tmux);
        assert!(only_code.iterm, "osascript ships with macOS");
    }

    #[test]
    fn iterm_is_never_capable_off_macos() {
        let caps = Caps::from_lookup(|_| true, false);

        assert!(!caps.iterm);
    }
}
