#!/usr/bin/env bash
# Regenerate the README hero from the *real* UI with fixed mock data. Renders the
# actual frontend (only Tauri's `invoke` is stubbed), captures the panel at retina,
# and frames it as a macOS window. Re-run any time the UI changes:
#
#   bash scripts/screenshot.sh
#
# Requires agent-browser and Python (PIL). Deterministic mock — no real data.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="assets/specola-screenshot.png"
TMP="$(mktemp -d)"
PORT=1420

npm run dev >"$TMP/vite.log" 2>&1 &
VITE=$!
cleanup() {
  kill "$VITE" 2>/dev/null || true
  agent-browser --session shot close >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT
sleep 4

agent-browser --session shot set viewport 380 760 2 >/dev/null
agent-browser --session shot open "http://localhost:$PORT" >/dev/null
# Pin the hero to English, then reload so language detection reads it before render.
agent-browser --session shot eval "localStorage.setItem('lang','en')" >/dev/null
agent-browser --session shot open "http://localhost:$PORT" >/dev/null
agent-browser --session shot wait --load networkidle >/dev/null

agent-browser --session shot eval --stdin >/dev/null <<'EVALEOF'
(() => {
  const now = Math.floor(Date.now() / 1000);
  const from = now - 14400;
  const S = (id, project, name, branch, state, ago, siblings, cost, focus, pinned) =>
    ({ id, project, name, branch, state, since: now - ago, siblings, cost, focus, pinned });
  const snap = {
    now, from, waiting: 1, waitingShare: 0.22, cost: 30.22, tokens: 4120000,
    notifications: true, dismissRead: false, hidden: [], hooksInstalled: true, hookSnippet: "",
    sessions: [
      S("1", "ronaldoscotti.com", "Implement the Editorial Noir hero variant", "main", "WaitingOnYou", 95, 1, 12.4, "Open in VS Code", false),
      S("2", "specola", "Read the plan and execute milestone M10", null, "YourTurn", 1160, 1, 8.72, "Jump to pane", true),
      S("3", "meu-feed-catolico-api", "Add a retry worker to the webhook", "feat/retries", "Working", 140, 2, 3.1, "Focus tab", false),
      S("4", "meu-feed-catolico-api", "Investigate the flaky calendar test", "fix/flaky", "Idle", 3600, 2, 0.8, "Copy resume command", false),
      S("5", "meu-feed-catolico-app", "Optimize the App Store submission", null, "Idle", 7200, 1, 5.2, "Copy resume command", false),
    ],
    segments: [
      { state: "Idle", from: from, to: from + 5200 },
      { state: "Working", from: from + 5200, to: from + 8600 },
      { state: "Idle", from: from + 8600, to: from + 10200 },
      { state: "Working", from: from + 10200, to: from + 12400 },
      { state: "WaitingOnYou", from: from + 12400, to: now },
    ],
  };
  window.__TAURI_INTERNALS__ = { invoke: async (cmd) => (cmd === "snapshot" ? snap : null) };

  // The macOS window chrome the native app draws — so the shot matches the real thing.
  document.body.classList.add("mac");
  const lights = document.createElement("div");
  lights.style.cssText =
    "position:fixed;top:14px;left:14px;display:flex;gap:8px;z-index:200;pointer-events:none";
  for (const c of ["#ff5f57", "#febc2e", "#28c840"]) {
    const dot = document.createElement("span");
    dot.style.cssText = `width:12px;height:12px;border-radius:50%;background:${c}`;
    lights.appendChild(dot);
  }
  document.body.appendChild(lights);
})();
EVALEOF

sleep 1.6
agent-browser --session shot screenshot "$TMP/panel.png" >/dev/null
python3 scripts/screenshot-frame.py "$TMP/panel.png" "$OUT"
