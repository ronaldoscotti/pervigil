# Tray Status and Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the tray answer *is something blocked on me* and let you act on it with the panel closed, with the same count on macOS, Windows and Linux.

**Architecture:** A pure `core::tray::tray_view` turns sessions plus today's cost into everything the tray shows — icon key, tooltip, summary, menu items, and a rebuild signature. `lib.rs` applies that result and owns the lifecycle: the app survives a closed window, and exactly one clock runs at a time (the webview's poll while the panel is visible, a Rust ticker while it is hidden).

**Tech Stack:** Rust, Tauri v2 (`tray-icon`, `menu`), TypeScript frontend, Python + `cairosvg` for the icon generator (developer-only, outputs committed).

**Spec:** [`../specs/2026-07-27-tray-status-and-actions.md`](../specs/2026-07-27-tray-status-and-actions.md)

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/tray.rs` (new) | Pure decision: `TrayView`, `IconKey`, `tray_view`. No Tauri types, no clock, no I/O. Inline `#[cfg(test)]` tests, matching `core/prune.rs` and `core/store.rs`. |
| `src-tauri/src/core/session.rs` | Gains `project(cwd)`, moved from `app.rs:407` so `core::tray` can label rows without depending on `app`. |
| `src-tauri/src/io/scan.rs` | `Scanner::scan` takes two floors — one for sessions, one for usage. |
| `src-tauri/src/app.rs` | `Snapshot` gains `today_cost`; `badge()` is deleted; `snapshot` keeps its command shape. |
| `src-tauri/src/lib.rs` | Lifecycle handlers, `show_panel`/`hide_panel`, the ticker, tray icon and menu application. Currently 69 lines; this is where the growth lands. |
| `src-tauri/icons/tray/*.png` (new) | Generated, committed, embedded with `include_bytes!`. |
| `assets/tray.svg` (new) | Source glyph. |
| `scripts/gen-tray-icons.py` (new) | SVG → PNG. Developer-only. |

`app.rs` is already 857 lines and mixes Tauri commands with pure helpers. The tray decision goes to `core/` rather than growing it further — that is also what makes it testable without a Tauri runtime.

---

## Task 1: Scanner takes two floors

Today `Scanner::scan(root, since)` skips any transcript whose mtime predates `since` (`io/scan.rs:100`). Under the default `4h` span that hides this morning's work entirely, so a today-scoped cost cannot be recovered by filtering afterwards.

**Files:**
- Modify: `src-tauri/src/io/scan.rs:66-100`
- Modify: `src-tauri/src/app.rs:256-260` (the one call site)
- Test: `src-tauri/src/io/scan.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_transcript_older_than_the_session_floor_still_contributes_usage() {
    let root = tempdir().expect("tempdir");
    let project = root.path().join("proj");
    std::fs::create_dir_all(&project).expect("mkdir");
    let path = project.join("old.jsonl");
    write_transcript(&path, 1_000);
    set_mtime(&path, 1_000);

    let scan = Scanner::default().scan(root.path(), 5_000, 500);

    assert!(scan.sessions.is_empty(), "too old to be a session");
    assert!(!scan.usage.is_empty(), "but its usage is still priced");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd src-tauri && cargo test a_transcript_older_than_the_session_floor -- --nocapture`
Expected: FAIL — `scan` takes two arguments, not three.

- [ ] **Step 3: Widen the signature**

`transcripts()` walks with the *lower* floor; the session floor is applied per file afterwards.

```rust
pub fn scan(&mut self, root: &Path, sessions_since: Timestamp, usage_since: Timestamp) -> Scan {
    let mut sessions = Vec::new();
    let mut usage: HashMap<SessionId, Vec<UsageEntry>> = HashMap::new();

    for (path, modified) in transcripts(root, usage_since.min(sessions_since)) {
        let cached = self.files.entry(path.clone()).or_default();
        cached.absorb_appended(&path);

        let Some(session) = cached.transcript.session() else {
            continue;
        };
        usage
            .entry(session.id.clone())
            .or_default()
            .extend(cached.transcript.usage.iter().cloned());
        if modified >= sessions_since {
            sessions.push(session);
        }
    }

    Scan { sessions, usage }
}
```

`transcripts()` returns `Vec<(PathBuf, Timestamp)>` so the mtime is read once.

- [ ] **Step 4: Update the call site**

`src-tauri/src/app.rs`, inside `App::snapshot`:

```rust
let scan = self
    .scanner
    .lock()
    .expect("scanner lock")
    .scan(&self.home.join(PROJECTS), from, start_of_day(now).timestamp().max(0) as Timestamp);
```

- [ ] **Step 5: Run the whole suite**

Run: `cd src-tauri && cargo test`
Expected: PASS, 128 tests. Nothing else calls `scan`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/io/scan.rs src-tauri/src/app.rs
git commit -m "feat(scan): read usage from a wider floor than sessions"
```

---

## Task 2: `today_cost` on the snapshot

**Files:**
- Modify: `src-tauri/src/app.rs:88-109` (struct), `:333-352` (construction)
- Test: `src-tauri/src/app.rs` inline tests

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn today_cost_ignores_the_span_and_counts_from_local_midnight() {
    let now = Local.with_ymd_and_hms(2026, 7, 27, 15, 0, 0).unwrap();
    let midnight = start_of_day(now).timestamp() as Timestamp;

    let entries = vec![
        usage_entry(midnight - 60),      // yesterday
        usage_entry(midnight + 60),      // this morning, outside a 4h span
        usage_entry(now.timestamp() as Timestamp - 60),
    ];

    let today = today_cost(&PriceTable::default(), &entries, midnight, now.timestamp() as Timestamp);

    assert_eq!(counted(&today), 2, "yesterday is out, this morning is in");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd src-tauri && cargo test today_cost_ignores_the_span`
Expected: FAIL — `today_cost` not found.

- [ ] **Step 3: Add the field and the helper**

`Snapshot` gains, next to `cost`:

```rust
/// Cost incurred since local midnight, whatever span the panel is showing. The
/// tray has no filter UI, so its summary must mean the same thing under either
/// clock — see the spec.
pub today_cost: f64,
```

Construction reuses the `spent` vec already collected:

```rust
today_cost: spent
    .iter()
    .filter(|entry| entry.at >= midnight && entry.at <= to)
    .filter_map(|entry| pricing::cost(&self.prices, &entry.model, &entry.usage))
    .sum(),
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/app.rs
git commit -m "feat: expose today's cost independently of the panel's span"
```

---

## Task 3: Move `project()` into `core`

`core::tray` needs to label a row with its project, and `project()` currently lives in `app.rs:407`. Moving it is what keeps the tray decision free of `app`.

**Files:**
- Modify: `src-tauri/src/core/session.rs`, `src-tauri/src/app.rs:407` and its call sites

- [ ] **Step 1: Move the function verbatim** into `core/session.rs`, making it `pub`. Take its tests with it if it has any.
- [ ] **Step 2: Replace the definition in `app.rs` with an import** — `use crate::core::session::project;`
- [ ] **Step 3: Run tests** — `cd src-tauri && cargo test`. Expected: PASS, no behaviour change.
- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/core/session.rs src-tauri/src/app.rs
git commit -m "refactor: move project() into core so the tray can label rows"
```

---

## Task 4: The pure tray decision

This is the heart, and the only part that is testable everywhere. Everything downstream just applies it.

**Files:**
- Create: `src-tauri/src/core/tray.rs`
- Modify: `src-tauri/src/core/mod.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn no_waiting_session_shows_the_bare_icon() {
    let view = tray_view(&[idle("a")], 0.0);

    assert_eq!(view.icon, IconKey::Bare);
    assert_eq!(view.tooltip, "Pervigil — nothing waiting");
}

#[test]
fn the_count_is_the_number_of_waiting_sessions() {
    let view = tray_view(&[waiting("a"), waiting("b"), idle("c")], 4.2);

    assert_eq!(view.icon, IconKey::Count(2));
    assert_eq!(view.summary, "2 waiting · $4.20 today");
    assert_eq!(view.items.len(), 2, "idle sessions are not menu items");
}

#[test]
fn above_nine_the_icon_overflows_but_the_summary_tells_the_truth() {
    let sessions: Vec<Session> = (0..12).map(|i| waiting(&i.to_string())).collect();

    let view = tray_view(&sessions, 0.0);

    assert_eq!(view.icon, IconKey::Overflow);
    assert!(view.summary.starts_with("12 waiting"), "the real count is never hidden");
    assert_eq!(view.items.len(), 9, "the menu caps, the summary does not");
}

#[test]
fn the_signature_ignores_cost_so_a_rebuild_never_closes_an_open_menu() {
    let sessions = [waiting("a")];

    let cheap = tray_view(&sessions, 1.00);
    let dear = tray_view(&sessions, 99.00);

    assert_eq!(cheap.signature, dear.signature);
}

#[test]
fn the_signature_changes_when_the_waiting_set_does() {
    let before = tray_view(&[waiting("a")], 0.0);
    let after = tray_view(&[waiting("b")], 0.0);

    assert_ne!(before.signature, after.signature);
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd src-tauri && cargo test core::tray`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Write the module**

```rust
//! What the tray shows, decided without a Tauri runtime, a clock, or the disk.

use super::session::{project, Session};
use super::session::SessionState;

/// Which generated asset to display. `Overflow` is the `9+` artwork.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKey {
    Bare,
    Count(u8),
    Overflow,
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
    /// Changes only when the menu's *structure* does. Cost is deliberately
    /// excluded: it moves almost every tick, and rebuilding a macOS menu closes
    /// it under the user's cursor.
    pub signature: String,
}

/// The menu lists at most this many sessions. The summary always states the true
/// count, so a cap never hides the number.
const MENU_CAP: usize = 9;

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
            label: match &session.title {
                Some(title) => format!("{} — {}", project(&session.cwd), title),
                None => project(&session.cwd),
            },
        })
        .collect();

    TrayView {
        icon: match count {
            0 => IconKey::Bare,
            n if n <= 9 => IconKey::Count(n as u8),
            _ => IconKey::Overflow,
        },
        tooltip: match count {
            0 => "Pervigil — nothing waiting".into(),
            n => format!("Pervigil — {n} waiting"),
        },
        summary: format!("{count} waiting · ${today_cost:.2} today"),
        items: items.clone(),
        signature: items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>()
            .join("\u{1f}"),
    }
}
```

- [ ] **Step 4: Register the module** — add `pub mod tray;` to `src-tauri/src/core/mod.rs`.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test core::tray`
Expected: PASS, 5 new tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/tray.rs src-tauri/src/core/mod.rs
git commit -m "feat(tray): decide the tray's contents as a pure function"
```

---

## Task 5: Icon assets and their generator

**Files:**
- Create: `assets/tray.svg`, `scripts/gen-tray-icons.py`, `src-tauri/icons/tray/*.png`

- [ ] **Step 1: Draw `assets/tray.svg`** — a silhouette, not the colour logo. Two artboards: bare (square) and badged (wider, per the spec — `9+` crammed into a square is a smudge). Solid shapes only; a template image is an alpha mask and gradients turn to mud.
- [ ] **Step 2: Write `scripts/gen-tray-icons.py`** — rasterise with `cairosvg` at one high resolution per state. Emit `bare`, `1`…`9`, `overflow`, each in `-light` and `-dark` variants. No `@2x`: `set_icon` takes a single `Image` and the backends rescale, so a second density is an asset no code could select.
- [ ] **Step 3: Generate and eyeball every file.** This is the verification — the assets are checked at build time precisely because nobody can check them on Windows or Linux at runtime.
- [ ] **Step 4: Commit** the SVG, the script, and the PNGs.

```bash
git add assets/tray.svg scripts/gen-tray-icons.py src-tauri/icons/tray
git commit -m "feat(tray): generate the icon set from one svg source"
```

---

## Task 6: Survive a closed window

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the panel helpers.** Every show and hide in the codebase routes through these — Tauri has no window-visibility event, so this pair *is* the one-clock invariant.

```rust
/// Show the panel and stand the ticker down. Every show goes through here:
/// Tauri has no visibility event, so the invariant is owned, not observed.
fn show_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    ticker::stop(app);
}

fn hide_panel(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("main") {
        let _ = panel.hide();
    }
    ticker::start(app);
}
```

- [ ] **Step 2: Route the two existing call sites through them** — the single-instance handler (`lib.rs:20-25`) and the tray click handler (`lib.rs:55-58`). The single-instance one reads as unrelated plumbing; it is not.

- [ ] **Step 3: Hide instead of close**

```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        hide_panel(window.app_handle());
    }
})
```

- [ ] **Step 4: Keep the process alive, and let `Quit` through**

```rust
.build(tauri::generate_context!())?
.run(|app, event| match event {
    // `None` is a user-interaction exit; `Some` came from AppHandle::exit,
    // which is our own Quit. Guarding on it is what keeps Quit working.
    tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
    #[cfg(target_os = "macos")]
    tauri::RunEvent::Reopen { .. } => show_panel(app),
    _ => {}
});
```

- [ ] **Step 5: Verify by hand.** `npm run tauri dev`; close the panel — the tray survives and the app is still running. `Cmd+Q` still quits (it is not a window close, so `CloseRequested` never sees it). Click the Dock icon — the panel returns.
- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: keep running when the panel is closed"
```

---

## Task 7: The ticker

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write it** — a thread parked on a channel, started by `hide_panel` and stopped by `show_panel`, calling `App::snapshot(Span::Today, Local::now())` once a second and handing the result to the tray. It also drains pending notices, which the `snapshot` command does today; while the panel is hidden nothing else would.
- [ ] **Step 2: Verify the invariant by hand.** Close the panel, watch the tray update. Open it, confirm the ticker stops (log a line at start/stop during development). Two clocks corrupt `App::targets`; none freezes the tray.
- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tray): drive the tray from rust while the panel is hidden"
```

---

## Task 8: Apply the view — icon and menu

**Files:**
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/app.rs` (delete `badge`)

- [ ] **Step 1: Build the tray with the new icon** — `icon_as_template(true)` on macOS, `show_menu_on_left_click(false)` on macOS and Windows (it defaults to `true`, so a menu would otherwise steal the left click). Pick the light or dark variant from `WebviewWindow::theme()`, defaulting to the dark-background art when unknown, and re-pick on `WindowEvent::ThemeChanged`.
- [ ] **Step 2: Swap icons with `set_icon_with_as_template`** — atomic, so macOS does not flicker between image and template flag; it degrades to a plain `set_icon` elsewhere.
- [ ] **Step 3: Build the menu** from `TrayView`: disabled summary, separator, session items, separator, `Open Pervigil`, `Quit`. Rebuild only when `view.signature` differs from the last applied one.
- [ ] **Step 4: Wire the menu events** — a session id calls the existing `App::focus`; `Open Pervigil` calls `show_panel`; `Quit` calls `app.exit(0)`.
- [ ] **Step 5: Set the tooltip**, skipping it on Linux where it is unsupported.
- [ ] **Step 6: Delete `badge()`** from `app.rs` and its call in the `snapshot` command.
- [ ] **Step 7: Verify by hand** — count appears and clears, menu jumps to the right session, menu does not rebuild while merely the cost moves.
- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/app.rs
git commit -m "feat(tray): show the count in the icon and act from the menu"
```

---

## Task 9: A tray click never fails silently

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Handle the clipboard floor.** `App::focus` degrades to copying the resume command. The panel reports that with a toast; a hidden panel has none, so a silent copy is indistinguishable from a dead click. Fire a native notification instead — and when the user has notifications switched off, **show the panel** rather than staying quiet. The toggle silences ambient alerts, not the feedback for something just clicked.
- [ ] **Step 2: Verify by hand** — click a session whose terminal is gone; confirm the notification, then turn notifications off and confirm the panel opens instead.
- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tray): say so when a click falls back to the clipboard"
```

---

## Task 10: Tray strings in the user's language — **decision needed**

Not covered by the spec, found while planning. The panel ships ten languages with RTL; the tray menu is built in Rust, where no translation table exists. Shipping it English-only is a visible regression in polish for nine of those ten.

Proposed resolution, pending the maintainer's call: the frontend already owns the strings, so it pushes the seven the tray needs — `Open Pervigil`, `Quit`, `{n} waiting`, `nothing waiting`, `today`, and the two tooltip forms — through a `set_tray_strings` command at startup and on language change. Rust stores them beside the config and `tray_view` formats with them. Cost is one command, one struct, and seven keys in ten locale blocks.

**Do not implement this task until the decision is made.** The alternative is an explicit English-only tray recorded in the spec's honesty section, which is defensible but should be chosen, not defaulted into.

---

## Verification before the PR

- [ ] `cd src-tauri && cargo test` — green, no warnings.
- [ ] `npx tsc --noEmit` — clean.
- [ ] Built and exercised on macOS: count appears and clears, menu jumps, close hides, `Cmd+Q` quits, Dock icon reopens.
- [ ] The PR states plainly that the Windows and Linux appearance is unverified on this machine, per the spec's honesty section. Do not claim parity that was not observed.
