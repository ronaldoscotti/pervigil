use std::io;

use super::focuser::{Reach, Runner, Strategy, WindowFocuser};

/// macOS focus. tmux selects the pane's window then the pane; iTerm2 is driven by
/// AppleScript on the session UUID; VS Code opens the folder; the clipboard floor
/// pipes the resume command to `pbcopy`.
pub struct MacFocuser<R: Runner> {
    runner: R,
}

impl<R: Runner> MacFocuser<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: Runner> WindowFocuser for MacFocuser<R> {
    fn focus(&self, strategy: &Strategy) -> io::Result<Reach> {
        match strategy {
            Strategy::Tmux { pane } => {
                // A pane id resolves to its window, so both targets are the pane.
                self.runner
                    .run("tmux", &["select-window", "-t", pane], None)?;
                self.runner
                    .run("tmux", &["select-pane", "-t", pane], None)?;
                Ok(Reach::Raised)
            }
            Strategy::Iterm { session } => {
                self.runner
                    .run("osascript", &["-e", &iterm_script(session)], None)?;
                Ok(Reach::Raised)
            }
            Strategy::VsCode { path } => {
                self.runner.run("code", &[path], None)?;
                Ok(Reach::Raised)
            }
            Strategy::Clipboard { resume } => {
                self.runner.run("pbcopy", &[], Some(resume))?;
                Ok(Reach::Copied)
            }
        }
    }
}

/// `$ITERM_SESSION_ID` is `w0t1p2:GUID`; iTerm2's AppleScript `id` is the GUID alone.
fn iterm_script(session: &str) -> String {
    let id = session.rsplit(':').next().unwrap_or(session);
    format!(
        r#"tell application "iTerm2"
  activate
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if id of s is "{id}" then
          select w
          select t
          select s
        end if
      end repeat
    end repeat
  end repeat
end tell"#
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// One recorded invocation: program, args, and any stdin.
    #[derive(Clone)]
    struct Call {
        program: String,
        args: Vec<String>,
        stdin: Option<String>,
    }

    #[derive(Default)]
    struct Recorder {
        calls: RefCell<Vec<Call>>,
    }

    impl Runner for Recorder {
        fn run(&self, program: &str, args: &[&str], stdin: Option<&str>) -> io::Result<()> {
            self.calls.borrow_mut().push(Call {
                program: program.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
                stdin: stdin.map(str::to_string),
            });
            Ok(())
        }
    }

    fn focus(strategy: Strategy) -> (Reach, Vec<Call>) {
        let focuser = MacFocuser::new(Recorder::default());
        let reach = focuser.focus(&strategy).unwrap();
        (reach, focuser.runner.calls.into_inner())
    }

    #[test]
    fn tmux_selects_the_window_then_the_pane() {
        let (reach, calls) = focus(Strategy::Tmux { pane: "%3".into() });

        assert_eq!(reach, Reach::Raised);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].args, vec!["select-window", "-t", "%3"]);
        assert_eq!(calls[1].args, vec!["select-pane", "-t", "%3"]);
    }

    #[test]
    fn vscode_opens_the_folder_path() {
        let (reach, calls) = focus(Strategy::VsCode {
            path: "/Users/x/proj".into(),
        });

        assert_eq!(reach, Reach::Raised);
        assert_eq!(calls[0].program, "code");
        assert_eq!(calls[0].args, vec!["/Users/x/proj"]);
    }

    #[test]
    fn the_clipboard_floor_pipes_the_resume_command_to_pbcopy() {
        let (reach, calls) = focus(Strategy::Clipboard {
            resume: "claude --resume s1".into(),
        });

        assert_eq!(reach, Reach::Copied);
        assert_eq!(calls[0].program, "pbcopy");
        assert_eq!(calls[0].stdin.as_deref(), Some("claude --resume s1"));
    }

    #[test]
    fn iterm_drives_applescript_on_the_bare_guid() {
        let (reach, calls) = focus(Strategy::Iterm {
            session: "w0t1p2:ABC-123".into(),
        });

        assert_eq!(reach, Reach::Raised);
        assert_eq!(calls[0].program, "osascript");
        let script = &calls[0].args[1];
        assert!(script.contains(r#"is "ABC-123""#), "script was {script}");
        assert!(!script.contains("w0t1p2"), "the tab prefix is not the id");
    }

    /// Round-trips the real `pbcopy`/`pbpaste`, proving [`SystemRunner`] works. Ignored
    /// by default because it clobbers the user's clipboard:
    /// `cargo test real_pbcopy -- --ignored`
    #[test]
    #[ignore]
    fn real_pbcopy_actually_sets_the_clipboard() {
        use super::super::focuser::SystemRunner;

        let focuser = MacFocuser::new(SystemRunner);
        focuser
            .focus(&Strategy::Clipboard {
                resume: "claude --resume roundtrip".into(),
            })
            .unwrap();

        let pasted = std::process::Command::new("pbpaste").output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&pasted.stdout),
            "claude --resume roundtrip"
        );
    }
}
