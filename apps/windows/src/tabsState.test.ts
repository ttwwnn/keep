// What the state file keeps for the tabs: their order, the workspaces put away
// and the vertical tabs, read back whatever the file holds.
import { describe, expect, it } from "vitest";
import { defaultState, normalizeState } from "./state";

describe("the order, the workspaces put away and the vertical tabs in the state file", () => {
  it("start empty, off", () => {
    const state = defaultState();
    expect(state.tabOrder).toEqual({});
    expect(state.hidden).toEqual([]);
    expect(state.verticalTabs).toBe(false);
  });
  it("survive being written and read back", () => {
    const state = { ...defaultState(), tabOrder: { a: [3, 1, 2] }, hidden: ["b"], verticalTabs: true };
    expect(normalizeState(JSON.parse(JSON.stringify(state)))).toEqual(state);
  });
  it("read back from a file that predates them, take their defaults", () => {
    const old = normalizeState({ order: ["a"], zoom: 1.1 });
    expect(old.tabOrder).toEqual({});
    expect(old.hidden).toEqual([]);
    expect(old.verticalTabs).toBe(false);
  });
  it("read back from a damaged one, keep what can be trusted", () => {
    const odd = normalizeState({
      tabOrder: { a: [2, "x", 2, -1, 1.5, 7], b: "nope", c: [] },
      hidden: ["a", 5, "a", "b"],
      verticalTabs: "yes",
    });
    expect(odd.tabOrder).toEqual({ a: [2, 7] });
    expect(odd.hidden).toEqual(["a", "b"]);
    expect(odd.verticalTabs).toBe(false);
    expect(normalizeState({ tabOrder: [1, 2] }).tabOrder).toEqual({});
  });
});
