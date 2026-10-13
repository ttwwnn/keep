// Which tabs keep their terminals: the policy of the macOS app, and who is awake.
import { beforeEach, describe, expect, it } from "vitest";
import { awake, forget, lastShown, refile, sleep, sleepers, wake, type Candidate } from "./hibernation";

const tab = (lastShown: number, extra: Partial<Candidate> = {}): Candidate => ({
  visible: false,
  lastShown,
  blocked: false,
  ...extra,
});

describe("the tabs to put to sleep", () => {
  it("lets go of none while they fit in the limit", () => {
    expect(sleepers([tab(1), tab(2), tab(3, { visible: true })], 6)).toEqual([]);
  });
  it("keeps the most recently shown and lets go of the rest", () => {
    // Six places: the visible one and the five most recent of the others.
    const tabs = [tab(1), tab(2), tab(3), tab(4), tab(5), tab(6), tab(7), tab(8, { visible: true })];
    expect(sleepers(tabs, 6).sort()).toEqual([0, 1]);
  });
  it("never lets go of the tab on screen, however old", () => {
    const tabs = [tab(0, { visible: true }), tab(5), tab(6)];
    expect(sleepers(tabs, 1).sort()).toEqual([1, 2]);
    expect(sleepers(tabs, 0)).not.toContain(0);
  });
  it("counts every tab on screen against the limit", () => {
    const tabs = [tab(1, { visible: true }), tab(2, { visible: true }), tab(3), tab(4)];
    // Two on screen leave one place of the three: the newer of the others keeps it.
    expect(sleepers(tabs, 3)).toEqual([2]);
  });
  it("leaves a blocked tab awake, over the limit", () => {
    const tabs = [tab(1, { blocked: true }), tab(2), tab(3, { visible: true })];
    // Room for the visible one and the newer: the blocked, oldest one would go, and does not.
    expect(sleepers(tabs, 2)).toEqual([]);
    // No room but the visible one's: the newer goes, the blocked one stays.
    expect(sleepers(tabs, 1)).toEqual([1]);
  });
  it("is nothing for nothing", () => {
    expect(sleepers([], 6)).toEqual([]);
  });
});

describe("who is awake", () => {
  beforeEach(() => {
    awake.value = new Set();
  });
  it("is the tabs that were shown", () => {
    wake("a");
    wake("b");
    expect([...awake.value]).toEqual(["a", "b"]);
    expect(lastShown("b")).toBeGreaterThan(lastShown("a"));
  });
  it("puts a tab to sleep, and wakes it when it is shown again", () => {
    wake("a");
    wake("b");
    sleep(["a"]);
    expect([...awake.value]).toEqual(["b"]);
    wake("a");
    expect(awake.value.has("a")).toBe(true);
    expect(lastShown("a")).toBeGreaterThan(lastShown("b"));
  });
  it("keeps a tab that was re-filed as awake as it was, in the same place in the order", () => {
    wake("a");
    wake("b");
    refile("a", "c");
    expect(awake.value.has("a")).toBe(false);
    expect(awake.value.has("c")).toBe(true);
    expect(lastShown("c")).toBeLessThan(lastShown("b"));
    expect(lastShown("a")).toBe(0);
    // One that was asleep stays so.
    refile("x", "y");
    expect(awake.value.has("y")).toBe(false);
  });
  it("forgets the tabs that are gone", () => {
    wake("a");
    wake("b");
    forget(new Set(["b"]));
    expect([...awake.value]).toEqual(["b"]);
    expect(lastShown("a")).toBe(0);
  });
});
