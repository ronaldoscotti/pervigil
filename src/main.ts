import { invoke } from "@tauri-apps/api/core";
import "@fontsource/lora/400.css";
import "@fontsource/lora/500.css";
import "@fontsource/lora/600.css";
import "@fontsource/space-mono/400.css";
import "@fontsource/space-mono/700.css";

type SessionState = "Working" | "WaitingOnYou" | "YourTurn" | "Idle";
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
  YourTurn: "your-turn",
  Idle: "idle",
};

// Inline (Lucide-style) so there's no icon dependency.
const PIN_ICON = `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>`;
const CHECK_ICON = `<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>`;

type Lang = "en" | "pt";

const STRINGS: Record<Lang, Record<string, string>> = {
  en: {
    working: "Working",
    waiting: "Waiting on you",
    yourTurn: "Your turn",
    idle: "Idle",
    last4h: "Last 4 hours",
    today: "Today",
    thisWeek: "This week",
    span4h: "4h",
    spanToday: "Today",
    spanWeek: "Week",
    pinned: "Pinned",
    unpinned: "Unpinned",
    settings: "Settings",
    back: "‹ Back",
    waitingOnYou: "waiting on you",
    sessionsOne: "{n} session",
    sessionsMany: "{n} sessions",
    quiet: "{n} quiet",
    yourTurnCount: "{n} your turn",
    waitingShare: "{p}% waiting on you",
    now: "now",
    sectionSessions: "Sessions",
    pin: "Pin",
    pinTitle: "Pin to top",
    unpinTitle: "Unpin",
    dismiss: "Dismiss",
    dismissTitle: "Dismiss until it acts again",
    keepAbove: "Keep the panel above other windows",
    emptyTitle: "No sessions in this window.",
    emptyBody: "Start Claude Code in a project, or widen the window below.",
    notifications: "Notifications",
    projectsShown: "Projects shown",
    language: "Language",
    notDetected: "Not detected",
    installTitle: "Install hooks to track live state",
    installNote:
      "States read <em>idle</em> until these run. Names and cost are already live. Paste into {path} — pervigil never edits it for you.",
    copySnippet: "Copy snippet",
    jumpToPane: "Jump to pane",
    focusTab: "Focus tab",
    openVsCode: "Open in VS Code",
    copyResume: "Copy resume command",
    pasteToResume: "{label} — paste to resume",
    focusUnavailable: "Focus unavailable — resume with: {resume}",
    focusFailed: "Focus failed",
    snippetCopied: "Snippet copied — paste into settings.json",
    copyFailed: "Copy failed — select the snippet manually",
    hooksDetected: "Hooks detected ✓",
  },
  pt: {
    working: "Trabalhando",
    waiting: "Esperando você",
    yourTurn: "Sua vez",
    idle: "Parado",
    last4h: "Últimas 4 horas",
    today: "Hoje",
    thisWeek: "Esta semana",
    span4h: "4h",
    spanToday: "Hoje",
    spanWeek: "Semana",
    pinned: "Fixado",
    unpinned: "Solto",
    settings: "Ajustes",
    back: "‹ Voltar",
    waitingOnYou: "esperando você",
    sessionsOne: "{n} sessão",
    sessionsMany: "{n} sessões",
    quiet: "{n} quietas",
    yourTurnCount: "{n} na sua vez",
    waitingShare: "{p}% esperando você",
    now: "agora",
    sectionSessions: "Sessões",
    pin: "Fixar",
    pinTitle: "Fixar no topo",
    unpinTitle: "Soltar",
    dismiss: "Dispensar",
    dismissTitle: "Dispensar até agir de novo",
    keepAbove: "Manter o painel acima das outras janelas",
    emptyTitle: "Nenhuma sessão nesta janela.",
    emptyBody: "Inicie o Claude Code em um projeto, ou amplie a janela abaixo.",
    notifications: "Notificações",
    projectsShown: "Projetos exibidos",
    language: "Idioma",
    notDetected: "Não detectado",
    installTitle: "Instale os hooks para acompanhar o estado ao vivo",
    installNote:
      "Os estados aparecem como <em>parados</em> até os hooks rodarem. Nomes e custo já estão ao vivo. Cole em {path} — o pervigil nunca edita o arquivo por você.",
    copySnippet: "Copiar trecho",
    jumpToPane: "Ir para o painel",
    focusTab: "Focar a aba",
    openVsCode: "Abrir no VS Code",
    copyResume: "Copiar comando de retomada",
    pasteToResume: "{label} — cole para retomar",
    focusUnavailable: "Foco indisponível — retome com: {resume}",
    focusFailed: "Falha ao focar",
    snippetCopied: "Trecho copiado — cole no settings.json",
    copyFailed: "Falha ao copiar — selecione o trecho manualmente",
    hooksDetected: "Hooks detectados ✓",
  },
};

let lang: Lang =
  (localStorage.getItem("lang") as Lang | null) ??
  (navigator.language.toLowerCase().startsWith("pt") ? "pt" : "en");

function t(key: string, params?: Record<string, string | number>): string {
  let value = STRINGS[lang][key] ?? STRINGS.en[key] ?? key;
  if (params) {
    for (const [name, sub] of Object.entries(params)) {
      value = value.replace(`{${name}}`, String(sub));
    }
  }
  return value;
}

const STATE_KEY: Record<SessionState, string> = {
  Working: "working",
  WaitingOnYou: "waiting",
  YourTurn: "yourTurn",
  Idle: "idle",
};

const SPAN_KEY: Record<Span, string> = { "4h": "last4h", today: "today", week: "thisWeek" };

/** Backend focus labels arrive in English; map them to a translatable key. */
const FOCUS_KEY: Record<string, string> = {
  "Jump to pane": "jumpToPane",
  "Focus tab": "focusTab",
  "Open in VS Code": "openVsCode",
  "Copy resume command": "copyResume",
};
const focusLabel = (english: string) => t(FOCUS_KEY[english] ?? "copyResume");

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
  node.title = focusLabel(session.focus);

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
        aria-label="${session.pinned ? t("unpinTitle") : t("pinTitle")}"
        title="${session.pinned ? t("unpinTitle") : t("pinTitle")}">${PIN_ICON}</button>
      <button type="button" class="row-action" data-act="dismiss"
        aria-label="${t("dismissTitle")}" title="${t("dismissTitle")}">${CHECK_ICON}</button>
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
  text(".state", t(STATE_KEY[session.state]));
  text(".name", session.name);
  text(".elapsed", elapsed(now - session.since));
  text(".row-cost", money(session.cost));

  return node;
}

let sessionSig = "";

/** Everything that changes a row's identity, order, or shape — but NOT the ticking
 *  elapsed timer or cost, which update in place so an active session never forces a
 *  rebuild (a rebuild would reset the scroll position). */
function structuralSignature(snapshot: Snapshot): string {
  return snapshot.sessions
    .map((s) => [s.id, s.state, s.pinned, s.siblings, s.branch ?? "", s.name].join(""))
    .join("");
}

function refreshRow(node: HTMLElement, session: SessionView, now: number) {
  const set = (selector: string, value: string) => {
    const target = node.querySelector(selector);
    if (target) target.textContent = value;
  };
  set(".elapsed", elapsed(now - session.since));
  set(".row-cost", money(session.cost));
}

function renderSessions(snapshot: Snapshot) {
  const list = el("sessions");

  if (snapshot.sessions.length === 0) {
    if (!list.querySelector(".empty")) {
      const empty = document.createElement("div");
      empty.className = "empty";
      empty.innerHTML = `<strong></strong><span></span>`;
      (empty.querySelector("strong") as HTMLElement).textContent = t("emptyTitle");
      (empty.querySelector("span") as HTMLElement).textContent = t("emptyBody");
      list.replaceChildren(empty);
    }
    sessionSig = "";
    return;
  }

  const sig = structuralSignature(snapshot);
  if (sig === sessionSig && list.querySelector(".row")) {
    // Structure unchanged — only the ticking fields move. Update them in place and
    // leave the DOM (and the scroll position, and any focus) alone.
    for (const session of snapshot.sessions) {
      const node = list.querySelector<HTMLElement>(`[data-id="${CSS.escape(session.id)}"]`);
      if (node) refreshRow(node, session, snapshot.now);
    }
    return;
  }

  sessionSig = sig;
  const focused = (document.activeElement as HTMLElement | null)?.dataset?.id;
  const scroll = list.scrollTop;
  list.replaceChildren(...snapshot.sessions.map((session) => row(session, snapshot.now)));
  list.scrollTop = scroll;
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
  now.textContent = t("now");
  axis.append(now);

  el("share").textContent = t("waitingShare", { p: Math.round(snapshot.waitingShare * 100) });
}

let hooksWere: boolean | undefined;

/**
 * The install card: shown only while the hooks are missing, so its disappearance is
 * the live "detected" signal. Pervigil never writes settings.json — the user pastes.
 */
function renderHooks(snapshot: Snapshot) {
  if (hooksWere === false && snapshot.hooksInstalled) toast(t("hooksDetected"));
  hooksWere = snapshot.hooksInstalled;

  const existing = document.querySelector<HTMLElement>(".hook-card");
  if (snapshot.hooksInstalled) {
    existing?.remove();
    return;
  }
  if (existing) return; // already showing; don't clobber a scroll position mid-poll

  const path = `<button type="button" class="hook-path">~/.claude/settings.json</button>`;
  const card = document.createElement("section");
  card.className = "hook-card";
  card.innerHTML = `
    <div class="hook-head">
      <span class="hook-mark">${t("notDetected")}</span>
      <span class="hook-title">${t("installTitle")}</span>
    </div>
    <p class="hook-note">${t("installNote", { path })}</p>
    <pre class="hook-snippet"></pre>
    <button type="button" class="hook-copy">${t("copySnippet")}</button>`;
  (card.querySelector(".hook-snippet") as HTMLElement).textContent = snapshot.hookSnippet;
  card.querySelector(".hook-path")?.addEventListener("click", () => {
    invoke("open_settings").catch(console.error);
  });
  card.querySelector(".hook-copy")?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(snapshot.hookSnippet);
      toast(t("snippetCopied"));
    } catch {
      toast(t("copyFailed"));
    }
  });
  el("sessions").after(card);
}

function render(snapshot: Snapshot, span: Span) {
  const waiting = el("waiting");
  waiting.textContent = String(snapshot.waiting);
  waiting.classList.toggle("quiet", snapshot.waiting === 0);

  const total = snapshot.sessions.length;
  const yourTurn = snapshot.sessions.filter((s) => s.state === "YourTurn").length;
  const count = t(total === 1 ? "sessionsOne" : "sessionsMany", { n: total });
  const tail =
    yourTurn > 0 ? t("yourTurnCount", { n: yourTurn }) : t("quiet", { n: total - snapshot.waiting });
  el("tally").textContent = `${count} · ${tail}`;

  el("lane-label").textContent = t(SPAN_KEY[span]);
  el("cost-label").textContent = t(SPAN_KEY[span]);
  el("cost").textContent = money(snapshot.cost);

  renderLane(snapshot, span);
  renderSessions(snapshot);
  renderHooks(snapshot);
  renderSettings(snapshot);
}

let projectSig = "";

/** The notifications switch tracks the server every poll — cheap, no flicker. */
function renderSettings(snapshot: Snapshot) {
  el("notifications-switch").setAttribute("aria-checked", String(snapshot.notifications));
  if (!el("settings").hidden) renderProjects(snapshot);
}

/** Rebuild the project list only when it actually changed, so the open sheet doesn't
 *  flicker or fight a click each second. */
function renderProjects(snapshot: Snapshot) {
  const projects = [...new Set([...snapshot.sessions.map((s) => s.project), ...snapshot.hidden])].sort();
  const sig = `${projects.join("|")}::${[...snapshot.hidden].sort().join("|")}`;
  if (sig === projectSig) return;
  projectSig = sig;

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

/** Set every static string in the chrome for the current language. */
function applyStaticStrings() {
  for (const node of document.querySelectorAll<HTMLElement>("[data-i18n]")) {
    node.textContent = t(node.dataset.i18n as string);
  }
  const pin = el("pin-toggle");
  pin.textContent = pin.getAttribute("aria-pressed") === "true" ? t("pinned") : t("unpinned");
  pin.title = t("keepAbove");
  const settings = el("settings-toggle");
  settings.textContent = settings.getAttribute("aria-expanded") === "true" ? t("back") : t("settings");
  for (const button of document.querySelectorAll<HTMLElement>("#lang-select [data-lang]")) {
    button.classList.toggle("on", button.dataset.lang === lang);
  }
  document.documentElement.lang = lang;
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
    const label = focusLabel(result.label);
    if (result.raised) toast(label);
    else if (result.error) toast(t("focusUnavailable", { resume: result.resume ?? "" }));
    else toast(t("pasteToResume", { label }));
  } catch (error) {
    console.error(error);
    toast(t("focusFailed"));
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
let lastSnapshot: Snapshot | undefined;

async function poll() {
  if (inflight) return;
  inflight = true;
  try {
    lastSnapshot = await invoke<Snapshot>("snapshot", { span });
    render(lastSnapshot, span);
  } catch (error) {
    console.error(error);
  } finally {
    inflight = false;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  if (navigator.userAgent.includes("Mac")) document.body.classList.add("mac");
  applyStaticStrings();

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
    const opening = sheet.hidden;
    sheet.hidden = !opening;
    const button = el("settings-toggle");
    button.setAttribute("aria-expanded", String(opening));
    button.textContent = opening ? t("back") : t("settings");
    if (opening && lastSnapshot) renderProjects(lastSnapshot);
  });

  el("pin-toggle").addEventListener("click", (event) => {
    const button = event.currentTarget as HTMLElement;
    const pinned = button.getAttribute("aria-pressed") !== "true";
    button.setAttribute("aria-pressed", String(pinned));
    button.textContent = pinned ? t("pinned") : t("unpinned");
    invoke("set_window_pinned", { pinned }).catch(console.error);
  });

  el("lang-select").addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLElement>("[data-lang]");
    if (!button?.dataset.lang || button.dataset.lang === lang) return;
    lang = button.dataset.lang as Lang;
    localStorage.setItem("lang", lang);
    projectSig = "";
    sessionSig = "";
    applyStaticStrings();
    if (lastSnapshot) render(lastSnapshot, span);
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
