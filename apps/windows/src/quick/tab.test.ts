import { describe, expect, it } from "vitest";
import type { TabInfo, WorkspaceInfo } from "../api";
import { guessGrid, quickTabOf } from "./tab";

const tab = (id: number, finished = false): TabInfo => ({
  id,
  cols: 80,
  rows: 24,
  clients: 0,
  finished,
  title: "",
  busy: false,
  splitOf: 0,
  splitDir: 0,
  cwd: "",
  lastActive: 0,
  command: "",
  shellPid: 0,
});
const space = (name: string, ...tabs: TabInfo[]): WorkspaceInfo => ({ name, tabs });

describe("the tab the panel shows", () => {
  it("is the first live tab of the quick workspace", () => {
    expect(quickTabOf([space("alfa", tab(1)), space("quick", tab(3), tab(4))])).toBe(3);
  });

  it("skips tabs whose shell ended: the panel opens another rather than a dead one", () => {
    expect(quickTabOf([space("quick", tab(3, true), tab(4))])).toBe(4);
    expect(quickTabOf([space("quick", tab(3, true))])).toBeNull();
  });

  it("is none when there is no quick workspace, or it is empty", () => {
    expect(quickTabOf([])).toBeNull();
    expect(quickTabOf([space("alfa", tab(1))])).toBeNull();
    expect(quickTabOf([space("quick")])).toBeNull();
  });

  it("never takes a tab of a workspace that is only named something like it", () => {
    expect(quickTabOf([space("Quick", tab(1)), space("quick2", tab(2))])).toBeNull();
  });
});

describe("the grid a new tab starts at", () => {
  it("follows the box and the text", () => {
    expect(guessGrid(1600, 400, 16)).toEqual({ cols: 166, rows: 20 });
  });

  it("has room for a shell in a box not measured yet", () => {
    expect(guessGrid(0, 0, 16)).toEqual({ cols: 20, rows: 5 });
  });
});
