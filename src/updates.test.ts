import { describe, expect, it } from "vitest";

import { UPDATE_FLOOR_MS, dueForCheck } from "./updates";

describe("dueForCheck", () => {
  it("checks on launch, when nothing has been checked yet", () => {
    expect(dueForCheck(1_000, 0, false)).toBe(true);
  });

  it("stops checking once an update is waiting to be installed", () => {
    expect(dueForCheck(Date.now(), 0, true)).toBe(false);
  });

  it("holds off until the floor has passed", () => {
    const last = 10_000_000;

    expect(dueForCheck(last + UPDATE_FLOOR_MS - 1, last, false)).toBe(false);
    expect(dueForCheck(last + UPDATE_FLOOR_MS, last, false)).toBe(true);
  });

  it("re-opening the panel in a loop cannot hammer the endpoint", () => {
    const last = 10_000_000;
    const opens = [0, 1_000, 60_000, 600_000].map((d) => dueForCheck(last + d, last, false));

    expect(opens).toEqual([false, false, false, false]);
  });
});
