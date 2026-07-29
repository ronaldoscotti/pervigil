//! Specola's core: pure functions over values. No filesystem, no GUI, no Tauri —
//! enforced by this crate's dependency list rather than by review.

pub mod event;
pub mod notify;
pub mod pricing;
pub mod prune;
pub mod session;
pub mod span;
pub mod store;
pub mod terminal;
pub mod tray;
