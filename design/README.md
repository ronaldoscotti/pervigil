# M2 — Design-direction mock

The visual direction for the Pervigil panel, locked before any implementation
(stage M2 of [`../../docs/plans/2026-07-23-pervigil-plan.md`](../../docs/plans/2026-07-23-pervigil-plan.md)).

Open [`index.html`](index.html) in a browser. It is standalone — no build step.

## Source of truth

Editable source lives in Claude Design:
**`Pervigil macOS panel design`** — project `bac27250-4751-4d8f-9b1a-2f74e82b3cdb`, file `Pervigil.dc.html`.

`index.html` here is that file with the `<x-dc>` / `support.js` harness stripped so it
renders in a plain browser, plus a real `:focus-visible` rule replacing the harness's
`style-focus` attribute. Content is otherwise unchanged.

## The direction

Concept: **a lamp kept lit through the night** (*pervigil* — ever-watchful). Session
states are temperatures of light:

| State | Color | Reads as |
|---|---|---|
| Waiting on you | amber `#F4B860`, with a soft radial bloom | the lit lamp — needs you |
| Working | teal `#6FB2C4` | alive, but quiet |
| Idle | ash `#565C6A`, dimmed row | asleep |

Ground `#13151B`–`#181A22`, ink `#EAE7DE`. Display and labels in **Fraunces** (a serif
that echoes the wordmark in the owl logo); **all data — timers, costs, axis — in IBM Plex
Mono with tabular figures**, because this is a watch instrument and the numerals are its
readout. *(The M2 mock used Space Grotesk; the type moved to Fraunces once the serif logo
landed, so the app and the mark read as one voice.)*

**Signature element:** the combined `LAST 6 HOURS` activity lane, with the `35% waiting on
you` stat. It answers *"what did my day look like, and how much of it was I the
bottleneck?"* — the product thesis in one band.

## Design decisions this mock settled

- **Per-row timelines were cut.** Three iterations proved a ~380px panel is too narrow to
  legibly encode 6h of multi-state history on every row — the strips kept reading as
  progress bars, and the geometry didn't even match the data (a 47-second session rendered
  a third of the lane). Replaced by one combined lane. Recorded here so the decision isn't
  re-litigated later.
- **Branch chips only when they disambiguate.** A label shown on every row (`main`
  everywhere) carries no information. Branch + `×2` appear only when a project has multiple
  live sessions — which is how the parallel-agent case finally became visible.

## Where the shipped panel differs

This file is the **frozen M2 direction**, not a mirror of the app. The live UI
(`index.html` + `src/`) moved on in three places during M6, all deliberate:

- **Session name** (spec item 13) — the mock predates it. Rows in the app carry a second
  line: state label · session name (`aiTitle` → `lastPrompt` → branch → short id, muted and
  truncated). The mock still shows project · branch · state · elapsed · cost.
- **Footer** — the mock's `Today | This week` pair became one window-scoped cost plus the
  `4h · Today · Week` control. The filter already scopes the figure; showing two totals next
  to a filter that governs one of them reads as a bug.
- **Lane header** — labelled by the selected span (`Last 4 hours` / `Today` / `This week`)
  rather than a fixed `LAST 6 HOURS`.

Everything else — the palette, the lamp metaphor, the type pairing, the combined lane, and
the branch-chip-only-when-it-disambiguates rule — shipped as drawn.
