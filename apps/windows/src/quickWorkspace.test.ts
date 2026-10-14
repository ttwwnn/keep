import { describe, expect, it } from "vitest";
import { QUICK_WORKSPACE, isQuickWorkspace, withoutQuick, withoutQuickHits } from "./quickWorkspace";

describe("the drop-down terminal's workspace", () => {
  it("is the one the Mac calls quick, and only that exact name", () => {
    expect(QUICK_WORKSPACE).toBe("quick");
    expect(isQuickWorkspace("quick")).toBe(true);
    // A workspace a person made with a name that merely looks like it is theirs.
    for (const name of ["Quick", "quick ", "quick2", "quickly", "", "q"]) expect(isQuickWorkspace(name)).toBe(false);
  });

  it("is left out of the workspaces the window shows, the rest in their order", () => {
    const list = [{ name: "alfa", tabs: [1] }, { name: "quick", tabs: [2] }, { name: "beta", tabs: [] }];
    expect(withoutQuick(list).map((w) => w.name)).toEqual(["alfa", "beta"]);
    // The same objects, not copies: what the window keeps by identity still works.
    expect(withoutQuick(list)[0]).toBe(list[0]);
    expect(withoutQuick([])).toEqual([]);
  });

  it("leaves a list that never had it as it was", () => {
    const list = [{ name: "alfa" }, { name: "beta" }];
    expect(withoutQuick(list)).toEqual(list);
  });

  it("is left out of what a search found", () => {
    const hits = [{ workspace: "alfa", line: 1 }, { workspace: "quick", line: 2 }, { workspace: "alfa", line: 3 }];
    expect(withoutQuickHits(hits).map((h) => h.line)).toEqual([1, 3]);
  });
});
