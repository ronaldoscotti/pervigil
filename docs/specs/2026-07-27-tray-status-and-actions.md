# Spec — Tray status and actions

Status: **draft** (review gate: this PR). Follows the pipeline in
[`../method/README.md`](../method/README.md).

## Problem

The tray is the only part of Pervigil that is always on screen, and it is the
least finished. Four things are wrong, and the fourth invalidates the other three.

1. **The icon is the app icon.** `TrayIconBuilder` takes
   `app.default_window_icon()` — full-colour artwork, no template treatment. It
   breaks from every other icon in the macOS menu bar.
2. **There is no menu.** Left click shows and focuses the panel; right click does
   nothing. Every action costs a window.
3. **The badge does not exist on Windows.** `badge()` calls `tray.set_title(count)`.
   Per the Tauri docs, `set_title` is *unsupported on Windows*. The count is not
   unverified there — it is absent by API, while the code reads as cross-platform.
4. **There is no "panel closed" state.** No `ExitRequested` handler, no
   `CloseRequested` handler, no hide affordance anywhere. Closing the last window
   ends the process and takes the tray with it. Pervigil is a background monitor
   that cannot run in the background.

## Goal

With the panel closed, the tray answers *is something blocked on me* and lets you
act on it — the same count on all three platforms, and each blocked session
reachable in one click.

## Approach

Four layers, from the foundation up. The first is a prerequisite, not a feature.

```
RunEvent::ExitRequested  ──►  prevent_exit()        app survives a closed window
WindowEvent::CloseRequested ─► prevent_close + hide  closing the panel hides it
RunEvent::Reopen         ──►  show panel            the Dock icon re-opens it
        │
        ▼
Snapshot ──► tray_view() ──► TrayView { icon, tooltip, summary, items }
   (pure)                            │
                                     ▼
                          icon asset + menu (rebuilt only on change)
```

**Lifecycle.** `prevent_exit` keeps the core thread alive with no windows — the
Tauri docs name this as its purpose. `CloseRequested` hides instead of closing.
`Quit` in the tray menu is then the only real exit, so it is mandatory, not
optional.

**Decision, pure.** `tray_view(&Snapshot) -> TrayView` takes no Tauri types, no
clock, no I/O — the same shape as `store::timeline`. It picks the icon key, the
tooltip, the summary line, and the session items. The Tauri side applies the
result; it does not decide anything.

**Icon.** Count 0 is the bare glyph; count *n* carries a badge; above 9 the badge
reads `9+`. Assets are pre-rendered from one SVG source by
`scripts/gen-tray-icons.py`, so the asset count is a number rather than a cost.
The badged icon is *wider* than the bare one — `9+` crammed into a 16pt square is
a smudge.

**Menu.**

```
3 waiting · $4.20 today      (disabled)
──────────────
pervigil — fix the axis ticks
comercial-api — migrate to valkey
──────────────
Open Pervigil
Quit
```

Session items call the existing `app::focus` path — the same code the panel's
rows already use. The menu is rebuilt only when its content changes, keyed by a
structural signature over the waiting set and the summary line; `src/main.ts:852`
already applies that idea to the panel's rows.

## Decisions

- **Count rendered into the icon, not beside it.** `set_title` is unsupported on
  Windows, so a text badge is macOS/Linux-only by construction. Drawing the count
  into the image is the only form that reaches all three platforms.
  (Alternative: keep `set_title` and let Windows go without. Rejected — it makes
  the product's central signal a macOS privilege and hides that fact in a no-op.)
- **Pre-rendered assets, not runtime rasterisation.** No new dependency, no image
  code on a surface nobody can inspect. Runtime compositing would put pixel logic
  precisely where this project cannot look — see the honesty section. A generator
  script keeps the assets cheap to regenerate when the artwork changes.
  (Alternative: compose digits at runtime with the `image` crate. Rejected — buys
  unlimited counts, which nobody needs, and pays in unverifiable code.)
- **macOS uses a template image; Windows and Linux use light/dark pairs.**
  `icon_as_template` is macOS-only; there the system tints the alpha mask and the
  icon is correct in both themes for free. Elsewhere a white silhouette vanishes
  on a light Windows 11 taskbar, so the theme is detected and the matching
  variant selected.
- **Two states, not three.** Waiting and not-waiting. A `working` state would
  change constantly while asking nothing of the user, competing for attention
  with the one state that does.
- **The Dock icon stays.** `RunEvent::Reopen` fires when it is clicked; the
  handler shows the panel, so the Dock becomes a second door rather than a dead
  click. (Alternative: `ActivationPolicy::Accessory`. Rejected — it removes the
  menu bar along with the Dock icon, costing `Cmd+Q` to solve a problem four
  lines already solve. `set_dock_visibility(false)` remains available later as a
  one-line preference; it is deliberately not taken now.)
- **`Open Pervigil` is in the menu because of Linux.**
  `set_show_menu_on_left_click` is unsupported there, so left-click behaviour is
  not ours to choose. Without the menu item, a Linux user has no way to open the
  panel.

## What this is and isn't (honesty)

- **Verified on macOS:** the icon treatment, the menu, the lifecycle, and the
  count, by using the build.
- **Verified everywhere:** `tray_view` is pure and fixture-tested — count 0 maps
  to the bare icon, 12 maps to `9+`, and the menu signature changes only when the
  waiting set or the summary does. What is tested is the decision, not the
  drawing.
- **Not verified on Windows or Linux:** how the icon actually looks in those
  shells, and whether the light/dark variant selection matches the system theme
  in practice. `CLAUDE.md` already records the tray badge as visually unverified
  on this machine; this feature widens that area. It is stated here and in the
  PR, and the per-platform behaviour table is written from the Tauri
  documentation rather than from observation.
- **Degradation is explicit:** the tooltip is skipped on Linux, where it is
  unsupported. Nothing pretends to work where it does not.

| | macOS | Windows | Linux |
|---|---|---|---|
| Left click | opens panel | opens panel | opens menu (not our choice) |
| Right click | menu | menu | menu |
| Tooltip | yes | yes | skipped — unsupported |
| Icon | template | light/dark pair | light/dark pair |

## Non-goals

A third `working` icon state; a notifications toggle, a settings shortcut, or a
span switch in the menu; hiding the Dock icon; any change to the panel itself.

## Files

- `src-tauri/src/lib.rs` — lifecycle handlers, tray menu construction, icon
  application.
- `src-tauri/src/app.rs` — `tray_view` and its `TrayView` type; `badge()` is
  replaced by it.
- `src-tauri/icons/tray/` — generated icon assets (new).
- `scripts/gen-tray-icons.py` — SVG → PNG generator (new).
- `docs/specs/2026-07-27-tray-status-and-actions.md` — this file.
