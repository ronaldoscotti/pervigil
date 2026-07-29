/** The state-to-render path, with no DOM in it. */

import type { Snapshot, Span } from "./types";

/** One unit, the largest that fits — a watch face, not a stopwatch. */
export function elapsed(seconds: number): string {
  const since = Math.max(seconds, 0);
  if (since < 60) return `${since}s`;
  if (since < 3600) return `${Math.floor(since / 60)}m`;
  if (since < 86400) return `${Math.floor(since / 3600)}h`;
  return `${Math.floor(since / 86400)}d`;
}

export const money = (amount: number | null) => (amount === null ? "—" : `$${amount.toFixed(2)}`);

export function axisLabel(at: number, span: Span): string {
  const moment = new Date(at * 1000);
  return span === "week"
    ? moment.toLocaleDateString(undefined, { weekday: "short" })
    : moment.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
}


/** Everything that changes a row's identity, order, or shape — but NOT the ticking
 *  elapsed timer or cost, which update in place so an active session never forces a
 *  rebuild (a rebuild would reset the scroll position). */
export function structuralSignature(snapshot: Snapshot): string {
  return snapshot.sessions
    .map((s) => [s.id, s.state, s.pinned, s.siblings, s.branch ?? "", s.name].join(""))
    .join("");
}


export function formatTokens(n: number): string {
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 0 : 1).replace(/\.0$/, "") + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(n >= 1e5 ? 0 : 1).replace(/\.0$/, "") + "K";
  return String(n);
}
