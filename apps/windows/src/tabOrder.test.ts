// The order of a workspace's tabs: applying what was remembered, moving a tab
// along, keeping the memory to what is alive, and handing a closed root's place
// to the pane that stood in for it.
import { describe, expect, it } from "vitest";
import type { TabInfo } from "./api";
import { arrangeTabs, heirsOf, insertBefore, memberIds, moveId, pruneOrders, substitute } from "./tabOrder";

const tab = (id: number, splitOf = 0): TabInfo => ({
  id,
  cols: 80,
  rows: 24,
  clients: 1,
  finished: false,
  title: "",
  busy: false,
  splitOf,
  splitDir: splitOf === 0 ? 0 : 1,
  cwd: "",
  lastActive: 0,
  command: "",
  shellPid: 0,
});

const ids = (items: { id: number }[]) => items.map((i) => i.id);

describe("arranging tabs", () => {
  const items = [1, 2, 3, 4].map((id) => ({ id }));
  it("is the daemon's order while nobody arranged one", () => {
    expect(ids(arrangeTabs(items, undefined, (t) => t.id))).toEqual([1, 2, 3, 4]);
    expect(ids(arrangeTabs(items, [], (t) => t.id))).toEqual([1, 2, 3, 4]);
  });
  it("follows the remembered order", () => {
    expect(ids(arrangeTabs(items, [3, 1, 4, 2], (t) => t.id))).toEqual([3, 1, 4, 2]);
  });
  it("puts a tab it never heard of after the others, where the daemon had it", () => {
    expect(ids(arrangeTabs(items, [4, 2], (t) => t.id))).toEqual([4, 2, 1, 3]);
  });
  it("ignores an id the daemon no longer lists", () => {
    expect(ids(arrangeTabs(items, [9, 3, 8, 1], (t) => t.id))).toEqual([3, 1, 2, 4]);
  });
  it("leaves the list it was given alone", () => {
    const before = [...items];
    arrangeTabs(items, [4, 3, 2, 1], (t) => t.id);
    expect(items).toEqual(before);
  });
});

describe("moving a tab along", () => {
  it("takes it out and puts it where the result says", () => {
    expect(moveId([1, 2, 3, 4], 1, 2)).toEqual([2, 3, 1, 4]);
    expect(moveId([1, 2, 3, 4], 4, 0)).toEqual([4, 1, 2, 3]);
    expect(moveId([1, 2, 3, 4], 2, 2)).toEqual([1, 3, 2, 4]);
  });
  it("holds the place to the ends of the list", () => {
    expect(moveId([1, 2, 3], 1, 99)).toEqual([2, 3, 1]);
    expect(moveId([1, 2, 3], 3, -4)).toEqual([3, 1, 2]);
  });
  it("is no change for a tab that is not there", () => {
    expect(moveId([1, 2, 3], 7, 0)).toEqual([1, 2, 3]);
  });
  it("lands a tab from another workspace before the one it was dropped on", () => {
    expect(insertBefore([1, 2, 3], 9, 2)).toEqual([1, 9, 2, 3]);
    expect(insertBefore([1, 2, 3], 9, null)).toEqual([1, 2, 3, 9]);
    expect(insertBefore([1, 2, 3], 9, 77)).toEqual([1, 2, 3, 9]);
  });
});

describe("the memory of the order", () => {
  it("keeps only what is alive", () => {
    const alive = new Map([
      ["a", new Set([1, 3])],
      ["b", new Set([5])],
    ]);
    expect(pruneOrders({ a: [3, 2, 1], b: [4], gone: [1] }, alive)).toEqual({ a: [3, 1] });
  });
  it("stands an heir in the place of a root", () => {
    expect(substitute([4, 2, 7], 2, 9)).toEqual([4, 9, 7]);
    expect(substitute([4, 2, 7], 5, 9)).toEqual([4, 2, 7]);
    expect(substitute([4, 2, 9], 2, 9)).toEqual([4, 9]);
  });
});

describe("the pane that stands in for a root", () => {
  it("is the oldest pane of a root that closed", () => {
    const before = [tab(1), tab(2, 1), tab(3, 1), tab(4)];
    // The daemon promotes 2 and hangs 3 off it.
    const after = [tab(2), tab(3, 2), tab(4)];
    expect([...heirsOf(before, after)]).toEqual([[1, 2]]);
  });
  it("sees through a pane that went with it", () => {
    const before = [tab(1), tab(2, 1), tab(3, 2)];
    const after = [tab(3)];
    expect([...heirsOf(before, after)]).toEqual([[1, 3]]);
  });
  it("is the pane left when a root is dragged into another tab", () => {
    const before = [tab(1), tab(2, 1), tab(5)];
    const after = [tab(1, 5), tab(2), tab(5)];
    expect([...heirsOf(before, after)]).toEqual([[1, 2]]);
  });
  it("has none for a tab that closed alone, or that is still a root", () => {
    expect(heirsOf([tab(1), tab(2)], [tab(2)]).size).toBe(0);
    expect(heirsOf([tab(1), tab(2, 1)], [tab(1), tab(2, 1)]).size).toBe(0);
  });
  it("does not take another root's pane for an heir", () => {
    const before = [tab(1), tab(2), tab(3, 2)];
    const after = [tab(2), tab(3, 2)];
    expect(heirsOf(before, after).size).toBe(0);
  });
});

describe("the members of a tab", () => {
  it("are the root and every pane under it, in the daemon's order", () => {
    const tabs = [tab(1), tab(2), tab(3, 1), tab(4, 2), tab(5, 3)];
    expect(memberIds(tabs, 1)).toEqual([1, 3, 5]);
    expect(memberIds(tabs, 2)).toEqual([2, 4]);
  });
  it("count the ones that finished, as the daemon moves them too", () => {
    const tabs = [tab(1), { ...tab(2, 1), finished: true }];
    expect(memberIds(tabs, 1)).toEqual([1, 2]);
  });
});
