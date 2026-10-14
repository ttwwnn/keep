// What several windows remember: the split of the state into the app's and
// the window's own, and the changes the windows tell one another.
import { describe, expect, it } from "vitest";
import { defaultState, normalizeState, type GuiState } from "./state";
import {
  SHARED_FIELDS,
  WINDOW_FIELDS,
  applyOps,
  blankWindow,
  diffShared,
  joinState,
  readOps,
  sharedOf,
  windowOf,
  withShared,
  type Op,
  type Shared,
} from "./windowState";

const state = (over: Partial<GuiState> = {}): GuiState => normalizeState({ ...defaultState(), ...over });
const key = (workspace: string, tab: number) => `${workspace}\u0000${tab}`;

describe("the state a window works with", () => {
  it("is the app's part and the window's own, and nothing else but its version", () => {
    const all = Object.keys(defaultState()).sort();
    expect([...SHARED_FIELDS, ...WINDOW_FIELDS, "version"].sort()).toEqual(all);
    expect(SHARED_FIELDS.filter((f) => (WINDOW_FIELDS as readonly string[]).includes(f))).toEqual([]);
  });
  it("splits into the two parts and joins back to what it was", () => {
    const s = state({
      order: ["a", "b"],
      hidden: ["b"],
      folded: ["a"],
      verticalTabs: true,
      zoom: 1.25,
      tabNames: { [key("a", 1)]: "Primeira" },
      active: { workspace: "a", tab: 1 },
      lastTab: { a: 1 },
      tabOrder: { a: [2, 1] },
      theme: "Dracula",
      daemon: "12-3456",
    });
    expect(joinState(sharedOf(s), windowOf(s))).toEqual(s);
  });
  it("keeps the order, what is folded and the tab in front to the window, and the names, zoom and what is put away to the app", () => {
    const s = state({ order: ["a"], folded: ["a"], active: { workspace: "a", tab: 1 }, hidden: ["b"], zoom: 2, tabNames: { [key("a", 1)]: "x" } });
    expect(Object.keys(windowOf(s)).sort()).toEqual([...WINDOW_FIELDS].sort());
    expect(windowOf(s).order).toEqual(["a"]);
    expect(sharedOf(s).hidden).toEqual(["b"]);
    expect(sharedOf(s).zoom).toBe(2);
  });
  it("is made sound from whatever is kept, as a state file is when read", () => {
    const joined = joinState({ zoom: "enorme", hidden: "x", tabNames: { a: "" } }, { order: "no", sidebar: { width: 9999, visible: false } });
    expect(joined.zoom).toBe(1);
    expect(joined.hidden).toEqual([]);
    expect(joined.tabNames).toEqual({});
    expect(joined.order).toEqual([]);
    expect(joined.sidebar).toEqual({ visible: false, width: 320 });
  });
});

describe("a window opened now", () => {
  it("lists nothing and shows nothing, and keeps the furniture of the window it comes from", () => {
    const first = state({ order: ["a", "b"], active: { workspace: "a", tab: 1 }, folded: ["a"], verticalTabs: true, sidebar: { visible: false, width: 300 } });
    const blank = blankWindow(windowOf(first));
    expect(blank).toEqual({ order: [], active: null, folded: ["a"], verticalTabs: true, sidebar: { visible: false, width: 300 } });
  });
  it("is the factory furniture when there is nothing to inherit", () => {
    const blank = blankWindow(undefined);
    expect(blank.order).toEqual([]);
    expect(blank.sidebar).toEqual(defaultState().sidebar);
    expect(blank.verticalTabs).toBe(false);
  });
  it("joins the app's state with a blank window", () => {
    const shared = sharedOf(state({ zoom: 1.5, hidden: ["x"] }));
    const joined = joinState(shared, null, windowOf(state({ verticalTabs: true })));
    expect(joined.zoom).toBe(1.5);
    expect(joined.hidden).toEqual(["x"]);
    expect(joined.order).toEqual([]);
    expect(joined.verticalTabs).toBe(true);
  });
});

describe("saying what changed", () => {
  const before = sharedOf(
    state({
      tabNames: { [key("a", 1)]: "um", [key("a", 2)]: "dois" },
      workspaceNames: { a: "Alfa" },
      tabOrder: { a: [1, 2] },
      lastTab: { a: 1 },
      hidden: ["x", "y"],
      zoom: 1,
    }),
  );
  it("is nothing when nothing changed", () => {
    expect(diffShared(before, before)).toEqual([]);
    expect(diffShared(before, structuredClone(before))).toEqual([]);
  });
  it("names the entries of a map that were set or taken away", () => {
    const after: Shared = { ...before, tabNames: { [key("a", 1)]: "UM", [key("b", 1)]: "novo" }, lastTab: {} };
    expect(diffShared(before, after)).toEqual([
      { op: "set", field: "tabNames", key: key("a", 1), value: "UM" },
      { op: "set", field: "tabNames", key: key("b", 1), value: "novo" },
      { op: "del", field: "tabNames", key: key("a", 2) },
      { op: "del", field: "lastTab", key: "a" },
    ]);
  });
  it("names the members of a set that came or went, and not the order they sit in", () => {
    expect(diffShared(before, { ...before, hidden: ["y", "z"] })).toEqual([
      { op: "add", field: "hidden", item: "z" },
      { op: "remove", field: "hidden", item: "x" },
    ]);
    expect(diffShared(before, { ...before, hidden: ["y", "x"] })).toEqual([]);
  });
  it("gives a value whole", () => {
    const ops = diffShared(before, { ...before, zoom: 1.25, font: { family: "Fira", size: 14 } });
    expect(ops).toHaveLength(2);
    expect(ops).toEqual(
      expect.arrayContaining([
        { op: "put", field: "font", value: { family: "Fira", size: 14 } },
        { op: "put", field: "zoom", value: 1.25 },
      ]),
    );
  });
  it("replaces the order of the tabs of one workspace without touching another's", () => {
    const after: Shared = { ...before, tabOrder: { a: [2, 1], b: [3] } };
    expect(diffShared(before, after)).toEqual([
      { op: "set", field: "tabOrder", key: "a", value: [2, 1] },
      { op: "set", field: "tabOrder", key: "b", value: [3] },
    ]);
  });
  it("makes the one state into the other when applied", () => {
    const after: Shared = {
      ...before,
      tabNames: { [key("a", 2)]: "dois", [key("a", 3)]: "três" },
      workspaceNames: {},
      tabOrder: { a: [3, 2], c: [1] },
      lastTab: { a: 3, c: 1 },
      hidden: ["y", "w"],
      zoom: 0.75,
      theme: "Nord",
      terminal: { cursor: "block", blink: false, scrollback: 500 },
    };
    expect(applyOps(before, diffShared(before, after))).toEqual(after);
  });
  it("round-trips through changes of every kind, in any order", () => {
    // A few thousand steps of invented changes: whatever one window did, the ops make the other the same.
    let seed = 7;
    const next = (n: number) => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed % n;
    };
    let current = before;
    for (let step = 0; step < 400; step++) {
      const target = structuredClone(current);
      for (let edits = 1 + next(3); edits > 0; edits--) {
        const workspace = ["a", "b", "c"][next(3)];
        switch (next(7)) {
          case 0:
            target.tabNames[key(workspace, 1 + next(4))] = `n${next(9)}`;
            break;
          case 1:
            delete target.tabNames[key(workspace, 1 + next(4))];
            break;
          case 2:
            target.tabOrder[workspace] = [1, 2, 3, 4].sort(() => next(3) - 1);
            break;
          case 3:
            delete target.lastTab[workspace];
            target.lastTab[workspace] = 1 + next(4);
            break;
          case 4:
            target.hidden = target.hidden.includes(workspace) ? target.hidden.filter((n) => n !== workspace) : [...target.hidden, workspace];
            break;
          case 5:
            target.zoom = 1 + next(5) / 4;
            break;
          case 6:
            target.workspaceNames = next(2) ? { ...target.workspaceNames, [workspace]: `W${next(5)}` } : {};
            break;
        }
      }
      const ops = diffShared(current, target);
      const applied = applyOps(current, ops);
      expect({ ...applied, hidden: [...applied.hidden].sort() }).toEqual({ ...target, hidden: [...target.hidden].sort() });
      current = applied;
    }
  });
});

describe("hearing what changed", () => {
  const base = sharedOf(state({ tabNames: { [key("a", 1)]: "um" }, hidden: ["x"] }));
  it("changes nothing, and gives back the same object, when the changes are already so", () => {
    const ops: Op[] = [
      { op: "set", field: "tabNames", key: key("a", 1), value: "um" },
      { op: "del", field: "tabNames", key: "ausente" },
      { op: "add", field: "hidden", item: "x" },
      { op: "remove", field: "hidden", item: "nunca" },
      { op: "put", field: "zoom", value: 1 },
    ];
    expect(applyOps(base, ops)).toBe(base);
  });
  it("does not change the state it was given", () => {
    const frozen = structuredClone(base);
    applyOps(base, [
      { op: "set", field: "tabNames", key: "k", value: "v" },
      { op: "add", field: "hidden", item: "y" },
      { op: "put", field: "zoom", value: 2 },
    ]);
    expect(base).toEqual(frozen);
  });
  it("ignores a field it does not share and a change that does not suit the field", () => {
    const ops: Op[] = [
      { op: "put", field: "order", value: ["a"] },
      { op: "set", field: "zoom", key: "k", value: 3 },
      { op: "add", field: "tabNames", item: "x" },
      { op: "put", field: "hidden", value: ["z"] },
      { op: "del", field: "daemon", key: "k" },
    ];
    expect(applyOps(base, ops)).toBe(base);
  });
  it("reads only the changes that make sense", () => {
    const raw = [
      { op: "put", field: "zoom", value: 2 },
      { op: "put", field: "tabNames", value: {} },
      { op: "set", field: "tabNames", key: "k", value: "v" },
      { op: "set", field: "tabNames", value: "sem chave" },
      { op: "add", field: "hidden", item: "x" },
      { op: "add", field: "hidden", item: 3 },
      { op: "remove", field: "hidden", item: "x" },
      { op: "del", field: "lastTab", key: "a" },
      { op: "boom", field: "lastTab", key: "a" },
      { op: "put", field: "version", value: 9 },
      "x",
      null,
    ];
    expect(readOps(raw).map((o) => `${o.op}:${o.field}`)).toEqual(["put:zoom", "set:tabNames", "add:hidden", "remove:hidden", "del:lastTab"]);
    expect(readOps("nada")).toEqual([]);
  });
  it("puts the app's part into a window's state, made sound", () => {
    const own = state({ order: ["a"], active: { workspace: "a", tab: 2 }, zoom: 1 });
    const next = withShared(own, { ...sharedOf(own), zoom: 1.5, hidden: ["b"] });
    expect(next.zoom).toBe(1.5);
    expect(next.hidden).toEqual(["b"]);
    expect(next.order).toEqual(["a"]);
    expect(next.active).toEqual({ workspace: "a", tab: 2 });
  });
});

describe("two windows changing the same state at once", () => {
  // The app orders the changes it hears and tells every window, the one that made them too. A window makes its
  // own changes on the spot and takes every change told, in the order told: they end the same.
  interface Window {
    state: Shared;
  }
  function converge(start: Shared, a: (s: Shared) => Shared, b: (s: Shared) => Shared) {
    const windows: Record<string, Window> = { A: { state: start }, B: { state: start } };
    const made: { from: string; ops: Op[] }[] = [];
    let server = start;
    const change = (from: string, make: (s: Shared) => Shared) => {
      const next = make(windows[from].state);
      made.push({ from, ops: diffShared(windows[from].state, next) });
      windows[from].state = next;
    };
    change("A", a);
    change("B", b);
    // The app hears them in this order and tells everybody.
    for (const { ops } of made) {
      server = applyOps(server, ops);
      for (const w of Object.values(windows)) w.state = applyOps(w.state, ops);
    }
    return { A: windows.A.state, B: windows.B.state, server };
  }
  const start = sharedOf(state({ tabNames: { [key("a", 1)]: "um" }, hidden: ["x"], zoom: 1 }));
  it("keep what each did when they touched different things", () => {
    const { A, B, server } = converge(
      start,
      (s) => ({ ...s, tabNames: { ...s.tabNames, [key("a", 2)]: "dois" } }),
      (s) => ({ ...s, hidden: [...s.hidden, "y"], zoom: 1.5 }),
    );
    expect(A).toEqual(server);
    expect(B).toEqual(server);
    expect(server.tabNames).toEqual({ [key("a", 1)]: "um", [key("a", 2)]: "dois" });
    expect(server.hidden).toEqual(["x", "y"]);
    expect(server.zoom).toBe(1.5);
  });
  it("end with the last one heard when they touched the same thing", () => {
    const { A, B, server } = converge(
      start,
      (s) => ({ ...s, zoom: 2, tabNames: { ...s.tabNames, [key("a", 1)]: "A" } }),
      (s) => ({ ...s, zoom: 1.25, tabNames: { ...s.tabNames, [key("a", 1)]: "B" } }),
    );
    expect(server.zoom).toBe(1.25);
    expect(server.tabNames[key("a", 1)]).toBe("B");
    expect(A).toEqual(server);
    expect(B).toEqual(server);
  });
  it("lose nothing when one puts away what the other has just put away", () => {
    const { A, B, server } = converge(
      start,
      (s) => ({ ...s, hidden: [...s.hidden, "p"] }),
      (s) => ({ ...s, hidden: [...s.hidden, "q"] }),
    );
    expect([...server.hidden].sort()).toEqual(["p", "q", "x"]);
    expect([...A.hidden].sort()).toEqual(["p", "q", "x"]);
    expect([...B.hidden].sort()).toEqual(["p", "q", "x"]);
  });
});
