// The workspaces put away: leaving the sidebar, coming back, and not outliving
// the workspace.
import { describe, expect, it } from "vitest";
import { keptName } from "./naming";
import { listed, neighbour, putAwayWith, putAwayWithout, stillRunning } from "./putAway";

describe("the workspaces put away", () => {
  it("leave the list and come back to it", () => {
    expect(putAwayWith(["a"], "b")).toEqual(["a", "b"]);
    expect(putAwayWith(["a", "b"], "a")).toEqual(["b", "a"]);
    expect(putAwayWithout(["a", "b"], "a")).toEqual(["b"]);
    expect(putAwayWithout(["a"], "z")).toEqual(["a"]);
  });
  it("are the only ones a list leaves out", () => {
    const all = [{ name: "a" }, { name: "b" }, { name: "c" }];
    expect(listed(all, ["b"]).map((w) => w.name)).toEqual(["a", "c"]);
    expect(listed(all, []).map((w) => w.name)).toEqual(["a", "b", "c"]);
  });
  it("do not outlive the workspace: one that was ended is forgotten", () => {
    expect(stillRunning(["a", "gone", "b"], new Set(["a", "b", "c"]))).toEqual(["a", "b"]);
    expect(stillRunning([], new Set(["a"]))).toEqual([]);
  });
  it("send the window to the next workspace, else the one before", () => {
    expect(neighbour(["a", "b", "c"], "b")).toBe("c");
    expect(neighbour(["a", "b", "c"], "c")).toBe("b");
    expect(neighbour(["a"], "a")).toBeUndefined();
    expect(neighbour(["a", "b"], "z")).toBeUndefined();
  });
});

describe("the name a field comes to", () => {
  it("is what was typed, when it is something else", () => {
    expect(keptName("Primeira", "zsh")).toBe("Primeira");
    expect(keptName("", "zsh")).toBe("");
  });
  it("is nothing when the text came back as it went out: the title is not pinned", () => {
    expect(keptName("zsh", "zsh")).toBeNull();
    expect(keptName("  zsh ", "zsh")).toBeNull();
    expect(keptName("zsh", "  zsh  ")).toBeNull();
  });
  it("is nothing for Esc", () => {
    expect(keptName(null, "zsh")).toBeNull();
  });
  it("is the empty name for a field cleared, which hands a tab back to its program", () => {
    expect(keptName("   ", "zsh")).toBe("   ");
  });
});
