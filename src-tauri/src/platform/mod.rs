pub mod focuser;
#[cfg(target_os = "macos")]
pub mod focuser_macos;
pub mod liveness;

/// A GUI-launched macOS `.app` inherits only the minimal system PATH
/// (`/usr/bin:/bin:/usr/sbin:/sbin`), not the user's shell PATH. `code`, `tmux` and
/// friends live in `/usr/local/bin` or `/opt/homebrew/bin`, so without restoring them
/// the focuser finds no tools and every session degrades to the clipboard floor.
#[cfg(target_os = "macos")]
pub fn restore_tool_path() {
    // ponytail: covers the standard install dirs; a tool in a custom dir still
    // degrades honestly to copy. Query a login shell only if that ever bites.
    const TOOL_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];
    let current = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", prepend_missing(&current, TOOL_DIRS));
}

/// Prepend the `dirs` not already in `path`, keeping the existing entries and order.
#[cfg(target_os = "macos")]
fn prepend_missing(path: &str, dirs: &[&str]) -> String {
    let present: std::collections::HashSet<&str> =
        path.split(':').filter(|entry| !entry.is_empty()).collect();
    let mut parts: Vec<&str> = dirs
        .iter()
        .copied()
        .filter(|dir| !present.contains(dir))
        .collect();
    if !path.is_empty() {
        parts.push(path);
    }
    parts.join(":")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::prepend_missing;

    #[test]
    fn an_empty_path_becomes_just_the_dirs() {
        assert_eq!(prepend_missing("", &["/a", "/b"]), "/a:/b");
    }

    #[test]
    fn missing_dirs_are_prepended_and_existing_entries_kept() {
        assert_eq!(
            prepend_missing("/usr/bin:/bin", &["/opt/homebrew/bin", "/usr/local/bin"]),
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
        );
    }

    #[test]
    fn a_dir_already_on_path_is_not_duplicated() {
        assert_eq!(
            prepend_missing("/usr/local/bin:/usr/bin", &["/usr/local/bin"]),
            "/usr/local/bin:/usr/bin"
        );
    }
}
