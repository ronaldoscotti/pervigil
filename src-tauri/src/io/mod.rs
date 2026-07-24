use std::path::PathBuf;

pub mod hooks;
pub mod record;
pub mod scan;
pub mod transcript;

/// `~`, under the two names the supported platforms give it.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
