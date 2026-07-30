// @vitest-environment jsdom

/**
 * What the panel draws, from the same files `src-tauri/tests/golden.rs` pins.
 *
 * That test says what the UI is handed; this one says what it does with it. Both read
 * `fixtures/snapshots/`, so they cannot drift into testing different days — and between
 * them they cover the seam that used to be checked only by opening the app and looking,
 * which is the check that never happens under time pressure. See
 * `docs/method/04-verification.md`.
 */

/// <reference types="vite/client" />

import { beforeEach, describe, expect, test } from "vitest";

import { render } from "./main";
import { setLang } from "./i18n";
import type { Snapshot } from "./types";

import blockedOnYou from "../fixtures/snapshots/blocked-on-you.json";
import backgroundAgent from "../fixtures/snapshots/background-agent.json";
import justOpened from "../fixtures/snapshots/just-opened.json";
import aWorkingDay from "../fixtures/snapshots/a-working-day.json";

/** The real markup, so a row that stops matching the page fails here. */
import page from "../index.html?raw";

// The goldens are written by Rust, so their JSON types are structural. The cast is the
// assertion that the two sides still agree on the shape — if they stop, this file is
// where you want to find out.
const GOLDENS = {
  "blocked-on-you": blockedOnYou,
  "background-agent": backgroundAgent,
  "just-opened": justOpened,
  "a-working-day": aWorkingDay,
} as unknown as Record<string, Snapshot>;

const golden = (name: string): Snapshot => GOLDENS[name];

function draw(name: string, span: "4h" | "today" | "week" = "4h") {
  document.documentElement.innerHTML = page;
  setLang("en");
  render(golden(name), span);
}

const rows = () => [...document.querySelectorAll("#sessions .row")];
const text = (node: Element, selector: string) =>
  node.querySelector(selector)?.textContent?.trim() ?? "";

describe("a session blocked on you", () => {
  beforeEach(() => draw("blocked-on-you"));

  test("counts as one waiting, and says so on the row", () => {
    expect(document.getElementById("waiting")?.textContent).toBe("1");
    expect(document.getElementById("waiting")?.classList.contains("quiet")).toBe(false);
    expect(rows()).toHaveLength(1);
    expect(text(rows()[0], ".state")).toBe("Waiting on you");
  });

  test("is toned as a wait, which is what makes the row amber", () => {
    expect(rows()[0].className).toBe("row waiting");
  });
});

describe("a session whose background agent is working", () => {
  beforeEach(() => draw("background-agent"));

  test("is your turn, and nothing is waiting on you", () => {
    // The reported bug read this row as "Waiting on you" while an agent churned.
    expect(text(rows()[0], ".state")).toBe("Your turn");
    expect(document.getElementById("waiting")?.textContent).toBe("0");
    expect(document.getElementById("waiting")?.classList.contains("quiet")).toBe(true);
  });

  test("tallies as your turn rather than as quiet", () => {
    expect(document.getElementById("tally")?.textContent).toBe("1 session · 1 your turn");
  });

  test("bills the agent's spend to the session that spawned it", () => {
    // 3000 of the 3350 tokens in this fixture are the agent's.
    expect(text(rows()[0], ".row-cost")).toBe("$0.08");
  });
});

describe("a project just opened", () => {
  beforeEach(() => draw("just-opened"));

  test("is idle, not working", () => {
    expect(text(rows()[0], ".state")).toBe("Idle");
  });

  test("shows no cost of its own rather than a free-looking zero", () => {
    expect(text(rows()[0], ".row-cost")).toBe("—");
  });

  test("renders an empty window's cost as a plain zero", () => {
    // The golden carries `-0.0` — Rust's `Sum` for floats uses negative zero as its
    // identity. `toFixed` drops the sign rather than any guard in `money`, which is why
    // `format.test.ts` pins it: it is inherited behaviour, not a decision.
    expect(document.getElementById("cost")?.textContent).toBe("$0.00");
  });
});

describe("an ordinary day", () => {
  // "4h", matching the span the golden was captured under. Rendering it as "today"
  // would label a 24-hour window over four hours of data — the exact drift the shared
  // fixture exists to prevent.
  beforeEach(() => draw("a-working-day"));

  test("puts the blocked session first, whatever its recency", () => {
    expect(rows().map((row) => text(row, ".state"))).toEqual([
      "Waiting on you",
      "Your turn",
      "Your turn",
      "Working",
    ]);
  });

  test("names each row from its own transcript", () => {
    expect(rows().map((row) => text(row, ".name"))).toEqual([
      "fix the failing migration",
      "rewrite the readme intro",
      "bump the toolchain",
      "add the dark theme toggle",
    ]);
  });

  test("marks the two sessions sharing a project", () => {
    const siblings = rows().filter((row) => row.querySelector(".siblings"));
    expect(siblings).toHaveLength(2);
  });
});
