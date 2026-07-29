use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::event::{SessionId, Timestamp};
use crate::core::session::{DismissMode, ViewPrefs};

/// User settings, persisted to `~/.specola/config.json`. Short and opinionated
/// (spec §2): what the panel notifies about, which projects it shows, and the
/// pin/dismiss state that can't live in the append-only event log. `BTree*` so the
/// file serializes in a stable order and diffs cleanly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub notifications: bool,
    pub hidden_projects: BTreeSet<String>,
    pub pinned: BTreeSet<SessionId>,
    pub dismissed: BTreeMap<SessionId, Timestamp>,
    /// When true, the dismiss action marks a session read (demotes it to idle) instead
    /// of hiding it.
    pub dismiss_read: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Surfacing what's blocked on you is the product's whole point; on unless
            // the user turns it off.
            notifications: true,
            hidden_projects: BTreeSet::new(),
            pinned: BTreeSet::new(),
            dismissed: BTreeMap::new(),
            dismiss_read: false,
        }
    }
}

impl Config {
    /// Never fails: a missing or corrupt file yields defaults, because settings must
    /// not be able to stop the panel from opening.
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// The pin/dismiss data `fold` consumes. Config keeps the persisted copy; `fold`
    /// stays pure by taking it as input.
    pub fn view_prefs(&self) -> ViewPrefs {
        ViewPrefs {
            pinned: self.pinned.iter().cloned().collect(),
            dismissed: self.dismissed.clone().into_iter().collect(),
            dismiss_mode: if self.dismiss_read {
                DismissMode::Read
            } else {
                DismissMode::Hide
            },
        }
    }

    /// A project is shown unless the user hid it. The lane still counts every session
    /// (spec item 4) — visibility filters the list only.
    pub fn shows(&self, project: &str) -> bool {
        !self.hidden_projects.contains(project)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("specola-cfg-{name}-{nanos}/config.json"))
    }

    #[test]
    fn a_fresh_config_has_notifications_on_and_nothing_hidden() {
        let config = Config::default();

        assert!(config.notifications);
        assert!(config.hidden_projects.is_empty());
        assert!(config.shows("anything"));
    }

    #[test]
    fn a_missing_file_loads_the_defaults() {
        let config = Config::load(Path::new("/no/such/config.json"));

        assert_eq!(config, Config::default());
    }

    #[test]
    fn a_corrupt_file_loads_the_defaults_rather_than_panicking() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();

        assert_eq!(Config::load(&path), Config::default());

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_saved_config_round_trips() {
        let path = temp_path("roundtrip");
        let config = Config {
            notifications: false,
            hidden_projects: BTreeSet::from(["secret-project".into()]),
            pinned: BTreeSet::from(["s1".into()]),
            dismissed: BTreeMap::from([("s2".into(), 1_700)]),
            dismiss_read: true,
        };

        config.save(&path).unwrap();

        assert_eq!(Config::load(&path), config);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn an_old_file_missing_a_field_keeps_the_default_for_it() {
        let path = temp_path("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"pinned":["s1"]}"#).unwrap();

        let config = Config::load(&path);

        assert!(config.notifications, "the absent field keeps its default");
        assert!(config.pinned.contains("s1"));

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn view_prefs_carry_pin_and_dismiss_into_fold() {
        let mut config = Config::default();
        config.pinned.insert("s1".into());
        config.dismissed.insert("s2".into(), 400);

        let prefs = config.view_prefs();

        assert!(prefs.pinned.contains("s1"));
        assert_eq!(prefs.dismissed.get("s2"), Some(&400));
    }

    /// A settings write that fails must say so. Here the parent is an existing
    /// *file*, so no directory can be created under it.
    #[test]
    fn a_save_that_cannot_write_reports_the_error() {
        let blocker = temp_path("unwritable");
        std::fs::create_dir_all(blocker.parent().unwrap()).unwrap();
        std::fs::write(&blocker, "not a directory").unwrap();

        let result = Config::default().save(&blocker.join("config.json"));

        assert!(result.is_err(), "a lost setting must not read as saved");

        std::fs::remove_dir_all(blocker.parent().unwrap()).ok();
    }

    #[test]
    fn a_hidden_project_is_not_shown() {
        let mut config = Config::default();
        config.hidden_projects.insert("hidden".into());

        assert!(!config.shows("hidden"));
        assert!(config.shows("visible"));
    }
}
