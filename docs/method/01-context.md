# Stage 1 — Gather context

*What I learned before committing to a design. Everything here is real research
done during the brainstorm: prior-art search, a framework check via `context7`,
and name due-diligence via web search.*

## Lifecycle signals exist (sensing is cheap)

Claude Code emits hooks per session with session id and cwd:

- `Notification` → the session is **waiting on you** (permission prompt / idle).
- `Stop` → the session finished its turn.
- `SessionStart` → a new session (gives cwd, pid).

So the sensing layer is essentially free. The value to build is **aggregation**,
not detection.

## Prior art — the category is crowded but unowned

A search surfaced a dozen tools in this space:

| Tool | What it is |
|------|-----------|
| `gmr/claude-status` | Native Swift menu-bar + widgets; states, per-state time, **click-to-focus** window/tab/pane. ~50★ |
| `so-agentbar` | Claude + Codex; approval detection; floating window w/ pixel-art sessions |
| `m1ckc3s/claude-status-bar` | Menu-bar status indicator |
| `tddworks/ClaudeBar` | Usage/quota tracker |
| `KyleAMathews/claude-code-ui` | Web dashboard, real-time |
| *(several usage/quota trackers)* | — |

**Findings that shaped the wedge:**

1. Building the obvious version is a **clone** — the worst signal for a portfolio
   piece. `gmr/claude-status` is already almost exactly the naive design.
2. But ~50★ on the leader means nobody has *won* the category. It's validated and
   unowned.
3. **Every incumbent is macOS-only and read-only.** Two gaps nobody fills:
   - `claude code cross platform monitor`
   - `claude code waiting for input` (organizing the whole UI around the urgent state)

These two become the differentiators. Cross-platform is an *architecture /
credibility* differentiator (it doesn't show in a screenshot); "waiting on you"
is the *product* differentiator.

## Stack decision — Tauri v2 (`context7`-checked)

Requirements: real system tray on all three OSes, small binary, full UI freedom
for a polished look, one codebase.

Checked Tauri v2 tray/window docs via `context7`. Confirmed:

- System tray on macOS / Windows / Linux, with documented Linux caveats (tray icon
  may need a menu set; left-click-to-open-menu unsupported on Linux).
- Rust core + web frontend fits a **pure `store` core with per-OS adapters**.

Consequence: the "sensing + state + timeline + cost" core is fully portable (pure
data from `~/.claude/projects/**` + hooks). Only **click-to-focus** and
**always-on-top** are platform-specific, and they **degrade honestly** — notably,
Wayland forbids programmatic window activation by design (no workaround; fall back
to copy-command).

## Testability constraint

Cross-platform testing is real, not aspirational: I have a Windows machine and can
line up a Linux tester. So v1 ships **macOS tested**, with Windows/Linux marked
*architecturally supported, untested — help wanted*. Claiming tested support I
haven't run would backfire harder than shipping macOS-only.

## Name due-diligence

Requirement: distinct from the `claude-*-bar` clones, no trademark wall,
discoverable. Ran availability searches:

| Candidate | Result |
|-----------|--------|
| Vigil / Lookout | Vigil = crowded monitoring category; Lookout = **registered TM**, security co. Both out. |
| Lucerna | Live adjacent SaaS (`lucerna.team`, eng-visibility) + `LUCEN SOFTWARE` mark. Out. |
| Lampas | Existing dev tool `ziozzang/lampas` + LAMPA software co. Out. |
| Specto | Muddy — existing "Specto" app + Spec-Kit search noise. Pass. |
| **Pervigil** | **Clean.** No GitHub/npm collision; only hit is a defunct 2012 IT co. Chosen. |

**Pervigil** — Latin, *"ever-watchful; keeping watch through the whole night."*
Semantic fit (watch that lasts through the night while you wait), distinctive,
clean namespace. Rationale in the spec, §8.

## Where this leads

Context confirms: build the portable pure core + honest per-OS focus seam, ship
macOS, organize the UI around "waiting on you," and treat the repo itself as proof
of workflow. → proceed to brainstorm approaches and write the spec.
