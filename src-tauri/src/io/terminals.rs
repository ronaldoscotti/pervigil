use std::collections::HashMap;
use std::path::Path;

use crate::core::terminal::Terminal;

const DIR: &str = ".pervigil/terminals";

/// Persist a session's latest terminal, one small file per session. The shim
/// overwrites it on every hook, so a session that never fired `SessionStart` (hooks
/// installed after it began) still gets its terminal captured on its next event —
/// which is what click-to-focus needs. Kept out of the event log, which stays a
/// record of *state*; this is just the latest focus target.
pub fn write(home: &Path, id: &str, term: &Terminal) -> std::io::Result<()> {
    let dir = home.join(DIR);
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string(term).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(format!("{id}.json")), json)
}

/// Every captured terminal, by session id. A missing dir or a corrupt file is simply
/// skipped — focus degrades to the clipboard, never errors.
pub fn read_all(home: &Path) -> HashMap<String, Terminal> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(home.join(DIR)) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        if let (Some(id), Ok(text)) = (
            path.file_stem().and_then(|s| s.to_str()),
            std::fs::read_to_string(&path),
        ) {
            if let Ok(term) = serde_json::from_str::<Terminal>(&text) {
                map.insert(id.to_string(), term);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_home(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("pervigil-term-{name}-{nanos}"))
    }

    #[test]
    fn a_written_terminal_reads_back_by_id() {
        let home = temp_home("roundtrip");
        let term = Terminal {
            program: Some("vscode".into()),
            ..Default::default()
        };

        write(&home, "s1", &term).unwrap();
        let all = read_all(&home);

        assert_eq!(all.get("s1"), Some(&term));

        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn the_latest_write_wins() {
        let home = temp_home("latest");
        write(
            &home,
            "s1",
            &Terminal {
                tmux_pane: Some("%1".into()),
                ..Default::default()
            },
        )
        .unwrap();
        write(
            &home,
            "s1",
            &Terminal {
                tmux_pane: Some("%9".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(read_all(&home)["s1"].tmux_pane.as_deref(), Some("%9"));

        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_missing_dir_is_empty_not_an_error() {
        assert!(read_all(Path::new("/no/such/home")).is_empty());
    }
}
