import { invoke } from "@tauri-apps/api/core";
import "@fontsource-variable/space-grotesk";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";

type SessionState = "Working" | "WaitingOnYou" | "Idle";
type Span = "4h" | "today" | "week";

interface SessionView {
  id: string;
  project: string;
  name: string;
  branch: string | null;
  state: SessionState;
  since: number;
  siblings: number;
  cost: number | null;
  focus: string;
  pinned: boolean;
}

interface FocusOutcome {
  raised: boolean;
  label: string;
  resume: string | null;
  error: string | null;
}

interface Segment {
  state: SessionState;
  from: number;
  to: number;
}

interface Snapshot {
  now: number;
  from: number;
  waiting: number;
  sessions: SessionView[];
  segments: Segment[];
  waitingShare: number;
  cost: number;
  notifications: boolean;
  hidden: string[];
  hooksInstalled: boolean;
  hookSnippet: string;
}

const TONE: Record<SessionState, string> = {
  Working: "working",
  WaitingOnYou: "waiting",
  Idle: "idle",
};

const STATE_LABEL: Record<SessionState, string> = {
  Working: "Working",
  WaitingOnYou: "Waiting on you",
  Idle: "Idle",
};

const SPAN_LABEL: Record<Span, string> = {
  "4h": "Last 4 hours",
  today: "Today",
  week: "This week",
};

const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** One unit, the largest that fits — a watch face, not a stopwatch. */
function elapsed(seconds: number): string {
  const since = Math.max(seconds, 0);
  if (since < 60) return `${since}s`;
  if (since < 3600) return `${Math.floor(since / 60)}m`;
  if (since < 86400) return `${Math.floor(since / 3600)}h`;
  return `${Math.floor(since / 86400)}d`;
}

const money = (amount: number | null) => (amount === null ? "—" : `$${amount.toFixed(2)}`);

function axisLabel(at: number, span: Span): string {
  const moment = new Date(at * 1000);
  return span === "week"
    ? moment.toLocaleDateString(undefined, { weekday: "short" })
    : moment.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
}

function row(session: SessionView, now: number): HTMLElement {
  const node = document.createElement("div");
  node.className = `row ${TONE[session.state]}`;
  node.tabIndex = 0;
  node.dataset.id = session.id;
  node.setAttribute("role", "button");
  node.title = session.focus;

  // A branch and a count only earn their space where a project runs more than one
  // session — otherwise they label everything and say nothing.
  const parallel = session.siblings > 1;
  node.innerHTML = `
    <span class="dot"></span>
    <div class="row-main">
      <div class="row-top">
        <span class="project"></span>
        ${parallel ? '<span class="siblings"></span>' : ""}
        ${parallel && session.branch ? '<span class="branch"></span>' : ""}
      </div>
      <div class="row-sub">
        <span class="state"></span>
        <span class="name"></span>
      </div>
    </div>
    <div class="row-actions">
      <button type="button" class="row-action pin" data-act="pin" aria-pressed="${session.pinned}"
        title="${session.pinned ? "Unpin" : "Pin to top"}">${session.pinned ? "Pinned" : "Pin"}</button>
      <button type="button" class="row-action" data-act="dismiss" title="Dismiss until it acts again">Dismiss</button>
    </div>
    <div class="row-right">
      <div class="elapsed"></div>
      <div class="row-cost"></div>
    </div>`;
  node.classList.toggle("is-pinned", session.pinned);

  const text = (selector: string, value: string) => {
    const target = node.querySelector(selector);
    if (target) target.textContent = value;
  };
  text(".project", session.project);
  text(".siblings", `×${session.siblings}`);
  text(".branch", session.branch ?? "");
  text(".state", STATE_LABEL[session.state]);
  text(".name", session.name);
  text(".elapsed", elapsed(now - session.since));
  text(".row-cost", money(session.cost));

  return node;
}

function renderSessions(snapshot: Snapshot) {
  const list = el("sessions");
  const focused = (document.activeElement as HTMLElement | null)?.dataset?.id;
  list.replaceChildren();

  if (snapshot.sessions.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.innerHTML =
      "<strong>No sessions in this window.</strong>Start Claude Code in a project, or widen the window below.";
    list.append(empty);
    return;
  }

  for (const session of snapshot.sessions) {
    list.append(row(session, snapshot.now));
  }
  if (focused) list.querySelector<HTMLElement>(`[data-id="${CSS.escape(focused)}"]`)?.focus();
}

function renderLane(snapshot: Snapshot, span: Span) {
  const lane = el("segments");
  lane.replaceChildren();
  for (const segment of snapshot.segments) {
    const band = document.createElement("span");
    band.className = TONE[segment.state];
    band.style.flex = `${segment.to - segment.from} 0 0`;
    lane.append(band);
  }

  const width = snapshot.now - snapshot.from;
  const axis = el("axis");
  axis.replaceChildren();
  for (const fraction of [0, 1 / 3, 2 / 3]) {
    const tick = document.createElement("span");
    tick.style.left = `${fraction * 100}%`;
    tick.textContent = axisLabel(snapshot.from + width * fraction, span);
    axis.append(tick);
  }
  const now = document.createElement("span");
  now.textContent = "now";
  axis.append(now);

  el("share").textContent = `${Math.round(snapshot.waitingShare * 100)}% waiting on you`;
}

let hooksWere: boolean | undefined;

/**
 * The install card: shown only while the hooks are missing, so its disappearance is
 * the live "detected" signal. Pervigil never writes settings.json — the user pastes.
 */
function renderHooks(snapshot: Snapshot) {
  if (hooksWere === false && snapshot.hooksInstalled) toast("Hooks detected ✓");
  hooksWere = snapshot.hooksInstalled;

  const existing = document.querySelector<HTMLElement>(".hook-card");
  if (snapshot.hooksInstalled) {
    existing?.remove();
    return;
  }
  if (existing) return; // already showing; don't clobber a scroll position mid-poll

  const card = document.createElement("section");
  card.className = "hook-card";
  card.innerHTML = `
    <div class="hook-head">
      <span class="hook-mark">Not detected</span>
      <span class="hook-title">Install hooks to track live state</span>
    </div>
    <p class="hook-note">States read <em>idle</em> until these run. Names and cost are already live. Paste into <code>~/.claude/settings.json</code> — pervigil never edits it for you.</p>
    <pre class="hook-snippet"></pre>
    <button type="button" class="hook-copy">Copy snippet</button>`;
  (card.querySelector(".hook-snippet") as HTMLElement).textContent = snapshot.hookSnippet;
  card.querySelector(".hook-copy")?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(snapshot.hookSnippet);
      toast("Snippet copied — paste into settings.json");
    } catch {
      toast("Copy failed — select the snippet manually");
    }
  });
  el("sessions").after(card);
}

function render(snapshot: Snapshot, span: Span) {
  const waiting = el("waiting");
  waiting.textContent = String(snapshot.waiting);
  waiting.classList.toggle("quiet", snapshot.waiting === 0);

  const total = snapshot.sessions.length;
  el("tally").textContent = `${total} session${total === 1 ? "" : "s"} · ${
    total - snapshot.waiting
  } quiet`;

  el("lane-label").textContent = SPAN_LABEL[span];
  el("cost-label").textContent = SPAN_LABEL[span];
  el("cost").textContent = money(snapshot.cost);

  renderLane(snapshot, span);
  renderSessions(snapshot);
  renderHooks(snapshot);
  renderSettings(snapshot);
}

/** Reflect current settings; skipped while the sheet is open so a click isn't fought. */
function renderSettings(snapshot: Snapshot) {
  const sw = el("notifications-switch");
  sw.setAttribute("aria-checked", String(snapshot.notifications));

  if (el("settings").hidden) return;

  const projects = [...new Set([...snapshot.sessions.map((s) => s.project), ...snapshot.hidden])].sort();
  const list = el("project-list");
  list.replaceChildren();
  for (const project of projects) {
    const shown = !snapshot.hidden.includes(project);
    const item = document.createElement("button");
    item.type = "button";
    item.className = "project-item";
    item.dataset.project = project;
    item.setAttribute("aria-pressed", String(shown));
    item.innerHTML = `<span class="project-dot"></span><span class="project-name"></span>`;
    (item.querySelector(".project-name") as HTMLElement).textContent = project;
    list.append(item);
  }
}

let span: Span = "4h";
let toastTimer: number | undefined;

/** A brief line at the foot of the panel — what a click just did. */
function toast(message: string) {
  const node = el("toast");
  node.textContent = message;
  node.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => node.classList.remove("show"), 3200);
}

async function jump(id: string) {
  try {
    const result = await invoke<FocusOutcome>("focus", { id });
    if (result.raised) toast(result.label);
    else if (result.error) toast(`Focus unavailable — resume with: ${result.resume}`);
    else toast(`${result.label} — paste to resume`);
  } catch (error) {
    console.error(error);
    toast("Focus failed");
  }
}

/** A settings mutation: fire-and-forget, then re-poll so the change shows at once. */
async function set(command: string, args: Record<string, unknown>) {
  try {
    await invoke(command, args);
  } catch (error) {
    console.error(error);
  }
  poll();
}

let inflight = false;

async function poll() {
  if (inflight) return;
  inflight = true;
  try {
    render(await invoke<Snapshot>("snapshot", { span }), span);
  } catch (error) {
    console.error(error);
  } finally {
    inflight = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  if (navigator.userAgent.includes("Mac")) document.body.classList.add("mac");

  el("spans").addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-span]");
    if (!button) return;
    span = button.dataset.span as Span;
    for (const other of el("spans").querySelectorAll("button")) {
      other.classList.toggle("on", other === button);
    }
    poll();
  });

  const activate = (event: Event) => {
    const target = event.target as HTMLElement;
    const action = target.closest<HTMLElement>("[data-act]");
    const row = target.closest<HTMLElement>(".row[data-id]");
    const id = row?.dataset.id;
    if (!id) return;

    // A pin/dismiss control acts on the session without jumping to it.
    if (action?.dataset.act === "pin") {
      set("set_pinned", { id, pinned: action.getAttribute("aria-pressed") !== "true" });
    } else if (action?.dataset.act === "dismiss") {
      set("dismiss", { id });
    } else {
      jump(id);
    }
  };
  el("sessions").addEventListener("click", activate);
  el("sessions").addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate(event);
    }
  });

  el("settings-toggle").addEventListener("click", () => {
    const sheet = el("settings");
    sheet.hidden = !sheet.hidden;
    el("settings-toggle").setAttribute("aria-expanded", String(!sheet.hidden));
    if (!sheet.hidden) poll();
  });

  el("notifications-switch").addEventListener("click", (event) => {
    const on = (event.currentTarget as HTMLElement).getAttribute("aria-checked") !== "true";
    set("set_notifications", { on });
  });

  el("project-list").addEventListener("click", (event) => {
    const item = (event.target as HTMLElement).closest<HTMLElement>("[data-project]");
    if (!item?.dataset.project) return;
    set("set_project_hidden", {
      project: item.dataset.project,
      hidden: item.getAttribute("aria-pressed") === "true",
    });
  });

  // ponytail: polling, not a filesystem watcher — the panel is ~10 rows and the
  // scanner only reads the bytes appended since the last tick. Revisit if that changes.
  setInterval(poll, 1000);
  poll();
});
