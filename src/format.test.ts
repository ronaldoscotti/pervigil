import { describe, expect, it } from "vitest";

import { elapsed, formatTokens, money, structuralSignature } from "./format";
import type { SessionView, Snapshot } from "./types";

const session = (over: Partial<SessionView> = {}): SessionView => ({
  id: "s1",
  project: "specola",
  name: "do the thing",
  branch: null,
  state: "Working",
  since: 0,
  siblings: 1,
  cost: null,
  focus: "Copy resume command",
  pinned: false,
  ...over,
});

const snapshot = (sessions: SessionView[]): Snapshot => ({
  now: 1_000,
  from: 0,
  waiting: 0,
  waitingOutsideWindow: 0,
  sessions,
  segments: [],
  waitingShare: 0,
  cost: 0,
  tokens: 0,
  notifications: true,
  dismissRead: false,
  hidden: [],
  hooksInstalled: true,
  hookSnippet: "",
});

describe("elapsed", () => {
  it("shows the largest unit that fits, and only that one", () => {
    expect(elapsed(0)).toBe("0s");
    expect(elapsed(59)).toBe("59s");
    expect(elapsed(60)).toBe("1m");
    expect(elapsed(3599)).toBe("59m");
    expect(elapsed(3600)).toBe("1h");
    expect(elapsed(86_400)).toBe("1d");
  });

  it("clamps a clock that ran backwards rather than printing a negative age", () => {
    expect(elapsed(-30)).toBe("0s");
  });
});

describe("money", () => {
  it("renders an em dash when nothing in the session can be priced", () => {
    expect(money(null)).toBe("—");
  });

  it("always shows two decimals, so the column stays aligned", () => {
    expect(money(0)).toBe("$0.00");
    expect(money(1.5)).toBe("$1.50");
    expect(money(12.345)).toBe("$12.35");
  });

  it("drops the sign on negative zero", () => {
    // A window with nothing priced arrives as -0.0: Rust's `Sum` for floats uses
    // negative zero as its identity. "-$0.00" in the footer would read as a refund.
    expect(money(-0)).toBe("$0.00");
  });
});

describe("formatTokens", () => {
  it("keeps small counts exact and abbreviates the rest", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1_500)).toBe("1.5K");
    expect(formatTokens(150_000)).toBe("150K");
    expect(formatTokens(1_500_000)).toBe("1.5M");
    expect(formatTokens(15_000_000)).toBe("15M");
  });

  it("drops a trailing .0 rather than showing 2.0K", () => {
    expect(formatTokens(2_000)).toBe("2K");
    expect(formatTokens(2_000_000)).toBe("2M");
  });
});

describe("structuralSignature", () => {
  it("ignores the ticking fields, so a live session never forces a rebuild", () => {
    const before = snapshot([session({ since: 0, cost: null })]);
    const after = snapshot([session({ since: 500, cost: 3.2 })]);

    expect(structuralSignature(after)).toBe(structuralSignature(before));
  });

  it("changes when a row's state, pin, or order changes", () => {
    const base = snapshot([session()]);

    expect(structuralSignature(snapshot([session({ state: "WaitingOnYou" })]))).not.toBe(
      structuralSignature(base),
    );
    expect(structuralSignature(snapshot([session({ pinned: true })]))).not.toBe(
      structuralSignature(base),
    );
    expect(structuralSignature(snapshot([session({ id: "s2" }), session()]))).not.toBe(
      structuralSignature(base),
    );
  });
});
