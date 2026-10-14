// Several windows: which one lists which workspace, what taking one off a
// list or going to one does, and the tab shown in two windows.
import { describe, expect, it } from "vitest";
import {
  adopt,
  carriedByAll,
  carriedElsewhere,
  carriedOf,
  cascaded,
  forgetTaken,
  frameOfTornTab,
  leaderOf,
  orphansOf,
  pulledOut,
  readPeers,
  readPlacement,
  remove,
  shownElsewhere,
  viewerSize,
  type Peer,
} from "./windowModel";

const peer = (label: string, carries: string[], showing: Peer["placement"]["showing"] = null): Peer => ({
  label,
  placement: { carries, showing },
});

describe("what a window says of itself", () => {
  it("is read back whatever arrives", () => {
    expect(readPlacement(null)).toEqual({ carries: [], showing: null });
    expect(readPlacement({ carries: ["a", 3, "b"], showing: { workspace: "a", tab: 2 } })).toEqual({
      carries: ["a", "b"],
      showing: { workspace: "a", tab: 2 },
    });
    expect(readPlacement({ carries: [], showing: { workspace: "a", tab: "2" } }).showing).toBeNull();
    expect(readPlacement({ carries: [], showing: { workspace: "a", tab: 1.5 } }).showing).toBeNull();
  });
  it("makes the list of windows from what the app sends, oldest first", () => {
    const peers = readPeers([{ label: "main", placement: { carries: ["a"] } }, { nolabel: 1 }, { label: "w1" }, "x"]);
    expect(peers.map((p) => p.label)).toEqual(["main", "w1"]);
    expect(peers[0].placement.carries).toEqual(["a"]);
    expect(peers[1].placement).toEqual({ carries: [], showing: null });
    expect(readPeers(undefined)).toEqual([]);
    expect(leaderOf(peers)).toBe("main");
    expect(leaderOf([])).toBeNull();
  });
  it("lists the workspaces of the order that are not put away", () => {
    expect(carriedOf(["a", "b", "c"], ["b"])).toEqual(["a", "c"]);
    expect(carriedOf([], ["b"])).toEqual([]);
  });
  it("knows when another window lists a workspace", () => {
    const peers = [peer("main", ["a", "b"]), peer("w1", ["b", "c"])];
    expect(carriedElsewhere(peers, "main", "b")).toBe(true);
    expect(carriedElsewhere(peers, "main", "a")).toBe(false);
    expect(carriedElsewhere(peers, "w1", "a")).toBe(true);
    expect(carriedElsewhere(peers, "w1", "z")).toBe(false);
  });
  it("takes this window's own list as it stands, not as it last said it", () => {
    const peers = [peer("main", ["stale"]), peer("w1", ["c"])];
    expect([...carriedByAll(peers, "main", ["fresh"])].sort()).toEqual(["c", "fresh"]);
  });
  it("knows when another window has a tab in front", () => {
    const peers = [peer("main", ["a"], { workspace: "a", tab: 1 }), peer("w1", ["a"], { workspace: "a", tab: 2 }), peer("w2", [])];
    expect(shownElsewhere(peers, "main", "a", 2)).toBe(true);
    expect(shownElsewhere(peers, "main", "a", 1)).toBe(false);
    expect(shownElsewhere(peers, "w1", "a", 1)).toBe(true);
    expect(shownElsewhere(peers, "w2", "a", 1)).toBe(true);
    expect(shownElsewhere(peers, "main", "b", 2)).toBe(false);
  });
});

describe("the workspaces nobody lists", () => {
  it("are the running ones no window lists and nobody put away, in the daemon's order", () => {
    const listed = new Set(["a", "c"]);
    expect(orphansOf(["a", "b", "c", "d", "e"], listed, ["d"])).toEqual(["b", "e"]);
  });
  it("are none when every window lists what runs", () => {
    expect(orphansOf(["a", "b"], new Set(["a", "b"]), [])).toEqual([]);
  });
});

describe("going to a workspace", () => {
  it("lists it at the end of this window's order when it was not there", () => {
    expect(adopt({ order: ["a"], hidden: [] }, "b")).toEqual({ order: ["a", "b"], hidden: [] });
  });
  it("puts it back where this window remembers it when it had been put away", () => {
    expect(adopt({ order: ["a", "b", "c"], hidden: ["b"] }, "b")).toEqual({ order: ["a", "b", "c"], hidden: [] });
  });
  it("leaves a workspace already listed where it is", () => {
    expect(adopt({ order: ["a", "b"], hidden: [] }, "a")).toEqual({ order: ["a", "b"], hidden: [] });
  });
  it("lists a workspace put away by the last window that listed it in a window that never did", () => {
    expect(adopt({ order: [], hidden: ["x", "b"] }, "b")).toEqual({ order: ["b"], hidden: ["x"] });
  });
});

describe("taking a workspace off a window's list", () => {
  const listing = { order: ["a", "b", "c"], hidden: [] as string[] };
  it("only leaves this window when another one lists it", () => {
    const peers = [peer("main", ["a", "b", "c"]), peer("w1", ["b"])];
    expect(remove(listing, peers, "main", "b")).toEqual({ order: ["a", "c"], hidden: [] });
  });
  it("puts it away when this was the last window listing it, keeping its place", () => {
    const peers = [peer("main", ["a", "b", "c"]), peer("w1", ["a"])];
    expect(remove(listing, peers, "main", "b")).toEqual({ order: ["a", "b", "c"], hidden: ["b"] });
  });
  it("puts it away in a window that is alone", () => {
    expect(remove(listing, [peer("main", ["a", "b", "c"])], "main", "c")).toEqual({ order: ["a", "b", "c"], hidden: ["c"] });
  });
  it("is undone by going to it", () => {
    const away = remove(listing, [peer("main", ["a", "b", "c"])], "main", "b");
    expect(adopt(away, "b")).toEqual(listing);
  });
});

describe("what another window's going to a workspace does to this one's memory", () => {
  it("drops the places of the workspaces that left the put-away list", () => {
    expect(forgetTaken(["a", "b", "c"], ["b", "x"], ["x"])).toEqual(["a", "c"]);
  });
  it("changes nothing when nothing left it", () => {
    expect(forgetTaken(["a", "b"], ["b"], ["b", "c"])).toEqual(["a", "b"]);
    expect(forgetTaken(["a"], [], [])).toEqual(["a"]);
  });
});

describe("every running workspace is listed by some window or put away", () => {
  // The invariant the Mac's Session keeps (`publish`, `placeOrphans`), walked through a few gestures with two windows.
  interface World {
    windows: Record<string, string[]>; // each window's order
    hidden: string[];
  }
  const peersOf = (w: World): Peer[] => Object.entries(w.windows).map(([label, order]) => peer(label, carriedOf(order, w.hidden)));
  const take = (w: World, label: string, workspace: string): World => {
    const was = new Set(w.hidden);
    const next = adopt({ order: w.windows[label], hidden: w.hidden }, workspace);
    const windows = Object.fromEntries(
      Object.entries(w.windows).map(([other, order]) => [other, other === label ? next.order : forgetTaken(order, [...was], next.hidden)]),
    );
    return { windows, hidden: next.hidden };
  };
  const dropIn = (w: World, label: string, workspace: string): World => {
    const next = remove({ order: w.windows[label], hidden: w.hidden }, peersOf(w), label, workspace);
    return { windows: { ...w.windows, [label]: next.order }, hidden: next.hidden };
  };
  const covered = (w: World, running: string[]) =>
    running.every((name) => w.hidden.includes(name) || Object.values(w.windows).some((order) => carriedOf(order, w.hidden).includes(name)));
  const listedBy = (w: World, name: string) =>
    Object.entries(w.windows)
      .filter(([, order]) => carriedOf(order, w.hidden).includes(name))
      .map(([label]) => label);

  it("holds through putting away in two windows, and going to the workspace from either", () => {
    const running = ["a", "b", "c"];
    let w: World = { windows: { main: ["a", "b", "c"], w1: [] }, hidden: [] };
    w = take(w, "w1", "b");
    expect(listedBy(w, "b")).toEqual(["main", "w1"]);
    w = dropIn(w, "main", "b");
    expect(listedBy(w, "b")).toEqual(["w1"]);
    expect(w.hidden).toEqual([]);
    w = dropIn(w, "w1", "b");
    expect(w.hidden).toEqual(["b"]);
    expect(covered(w, running)).toBe(true);
    // Gone to from the other window: listed there alone, whatever the first remembered.
    w = take(w, "main", "b");
    expect(listedBy(w, "b")).toEqual(["main"]);
    expect(w.windows.w1).not.toContain("b");
    expect(covered(w, running)).toBe(true);
  });
  it("does not list a workspace in the window that put it away when another goes to it", () => {
    let w: World = { windows: { main: ["a", "b"], w1: ["c"] }, hidden: [] };
    w = dropIn(w, "main", "b");
    expect(w.hidden).toEqual(["b"]);
    expect(w.windows.main).toEqual(["a", "b"]);
    w = take(w, "w1", "b");
    expect(listedBy(w, "b")).toEqual(["w1"]);
    expect(w.windows.main).toEqual(["a"]);
  });
});

describe("a tab pulled out of the row", () => {
  const row = { top: 30, bottom: 66 };
  it("is out of the row past the threshold above or below its band", () => {
    expect(pulledOut(48, row)).toBe(false);
    expect(pulledOut(30 - 39, row)).toBe(false);
    expect(pulledOut(30 - 41, row)).toBe(true);
    expect(pulledOut(66 + 39, row)).toBe(false);
    expect(pulledOut(66 + 41, row)).toBe(true);
    expect(pulledOut(200, row)).toBe(true);
    expect(pulledOut(-10, row)).toBe(false);
    expect(pulledOut(-11, row)).toBe(true);
  });
  it("makes a window the size of the one it left, with the tab under the pointer", () => {
    // The pointer let go at (1500, 400) on the screen; the tab was pressed at (300, 48) of the old window, on a 150% screen.
    expect(frameOfTornTab({ x: 1500, y: 400 }, { x: 300, y: 48 }, 1.5, { width: 1920, height: 1200 })).toEqual({
      x: 1050,
      y: 328,
      width: 1920,
      height: 1200,
    });
  });
  it("steps a new window down and across from the one it was opened from", () => {
    expect(cascaded({ x: 100, y: 80, width: 1280, height: 800 }, 1)).toEqual({ x: 124, y: 104, width: 1280, height: 800 });
    expect(cascaded({ x: 100, y: 80, width: 1280, height: 800 }, 1.5)).toEqual({ x: 136, y: 116, width: 1280, height: 800 });
  });
});

describe("the size a terminal gives the daemon", () => {
  it("is its own on screen, and whatever the number of windows", () => {
    expect(viewerSize(true, false, 120, 40)).toEqual({ cols: 120, rows: 40 });
    expect(viewerSize(true, true, 120, 40)).toEqual({ cols: 120, rows: 40 });
  });
  it("is its own out of sight in a window that is alone, as it always was", () => {
    expect(viewerSize(false, false, 120, 40)).toEqual({ cols: 120, rows: 40 });
  });
  it("is not looking out of sight when there are several windows", () => {
    expect(viewerSize(false, true, 120, 40)).toEqual({ cols: 0, rows: 0 });
  });
});
