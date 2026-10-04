// The window's arithmetic: zoom steps, chords, the shape of splits, the
// state file, and the small text helpers.
import { describe, expect, it } from "vitest";
import type { TabInfo } from "./api";
import { shortcut, isShiftEnter, isPlainCopy, type KeyLike } from "./keys";
import { buildTree, isPathTitle, leaves, rootTabs } from "./layout";
import { defaultState, normalizeState } from "./state";
import { fuzzyScore, matchRanges, quotePaths } from "./text";
import { normalizeZoom, zoomPercent, zoomStep, zoomedSize } from "./zoom";

describe("zoom", () => {
  it("steps as Chrome does, and stops at the ends", () => {
    expect(zoomStep(1, 1)).toBe(1.1);
    expect(zoomStep(1, -1)).toBe(0.9);
    expect(zoomStep(3, 1)).toBeUndefined();
    expect(zoomStep(0.5, -1)).toBeUndefined();
  });
  it("a share between steps goes on in the direction asked", () => {
    expect(zoomStep(1.05, 1)).toBe(1.1);
    expect(zoomStep(1.05, -1)).toBe(1);
    expect(zoomStep(0.672, -1)).toBe(0.5);
  });
  it("says it as a browser does", () => {
    expect(zoomPercent(0.67)).toBe("67%");
    expect(zoomPercent(1.1)).toBe("110%");
    expect(zoomedSize(13, 1.1)).toBe(14.3);
  });
  it("a share read back is held to the steps", () => {
    expect(normalizeZoom(1.12)).toBe(1.1);
    expect(normalizeZoom("x")).toBe(1);
    expect(normalizeZoom(99)).toBe(3);
  });
});

const key = (code: string, mods: Partial<KeyLike> = {}): KeyLike => ({
  key: "",
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe("chords", () => {
  it("Windows Terminal's and Chrome's", () => {
    expect(shortcut(key("KeyT", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "newTab" });
    expect(shortcut(key("KeyN", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "newWorkspace" });
    expect(shortcut(key("Tab", { ctrlKey: true }))).toEqual({ kind: "nextTab" });
    expect(shortcut(key("Tab", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "previousTab" });
    expect(shortcut(key("Digit3", { ctrlKey: true }))).toEqual({ kind: "goToTab", index: 2 });
    expect(shortcut(key("Digit9", { ctrlKey: true }))).toEqual({ kind: "lastTab" });
    expect(shortcut(key("KeyP", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "picker" });
    expect(shortcut(key("KeyF", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "searchAll" });
    expect(shortcut(key("KeyF", { ctrlKey: true }))).toEqual({ kind: "findInTab" });
    expect(shortcut(key("Equal", { ctrlKey: true }))).toEqual({ kind: "zoomIn" });
    expect(shortcut(key("Equal", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "zoomIn" });
    expect(shortcut(key("Minus", { ctrlKey: true }))).toEqual({ kind: "zoomOut" });
    expect(shortcut(key("Digit0", { ctrlKey: true }))).toEqual({ kind: "zoomReset" });
    expect(shortcut(key("KeyD", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "splitRight" });
    expect(shortcut(key("KeyE", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "splitDown" });
    expect(shortcut(key("F2"))).toEqual({ kind: "rename" });
    expect(shortcut(key("KeyV", { ctrlKey: true }))).toEqual({ kind: "paste" });
    expect(shortcut(key("KeyC", { ctrlKey: true, shiftKey: true }))).toEqual({ kind: "copy" });
  });
  it("leaves the program its keys", () => {
    expect(shortcut(key("KeyC", { ctrlKey: true }))).toBeNull();
    expect(shortcut(key("KeyR", { ctrlKey: true }))).toBeNull();
    expect(shortcut(key("KeyW", { ctrlKey: true }))).toBeNull();
    expect(shortcut(key("KeyT", { ctrlKey: true }))).toBeNull();
    expect(shortcut(key("KeyT", { ctrlKey: true, shiftKey: true, altKey: true }))).toBeNull();
    expect(shortcut(key("KeyA"))).toBeNull();
  });
  it("nothing closes a tab from the keyboard", () => {
    for (const code of ["KeyW", "F4"]) {
      for (const mods of [{ ctrlKey: true }, { ctrlKey: true, shiftKey: true }]) {
        expect(shortcut(key(code, mods))).toBeNull();
      }
    }
  });
  it("on a Mac, ⌘ is the window's and Ctrl the program's", () => {
    expect(shortcut(key("KeyT", { metaKey: true, shiftKey: true }), true)).toEqual({ kind: "newTab" });
    expect(shortcut(key("KeyT", { ctrlKey: true, shiftKey: true }), true)).toBeNull();
  });
  it("Shift+Enter and Ctrl+C are told apart", () => {
    expect(isShiftEnter(key("Enter", { shiftKey: true }))).toBe(true);
    expect(isShiftEnter(key("Enter"))).toBe(false);
    expect(isShiftEnter(key("Enter", { shiftKey: true, ctrlKey: true }))).toBe(false);
    expect(isPlainCopy(key("KeyC", { ctrlKey: true }))).toBe(true);
    expect(isPlainCopy(key("KeyC", { ctrlKey: true, shiftKey: true }))).toBe(false);
  });
});

const tab = (id: number, splitOf = 0, splitDir = 0): TabInfo => ({
  id,
  cols: 80,
  rows: 24,
  clients: 1,
  finished: false,
  title: "",
  busy: false,
  splitOf,
  splitDir,
  cwd: "",
  lastActive: 0,
  command: "",
});

describe("splits", () => {
  it("replaying the records rebuilds the shape", () => {
    const tree = buildTree(1, [
      { tab: 2, splitOf: 1, splitDir: 1 },
      { tab: 3, splitOf: 2, splitDir: 2 },
    ]);
    expect(tree).toEqual({
      kind: "split",
      vertical: true,
      first: { kind: "leaf", id: 1 },
      second: {
        kind: "split",
        vertical: false,
        first: { kind: "leaf", id: 2 },
        second: { kind: "leaf", id: 3 },
      },
    });
    expect(leaves(tree)).toEqual([1, 2, 3]);
  });
  it("panes belong to the tab at the top of their chain", () => {
    const tabs = rootTabs([tab(1), tab(2, 1, 1), tab(3), tab(4, 2, 2)]);
    expect(tabs.map((t) => t.root.id)).toEqual([1, 3]);
    expect(tabs[0].panes.map((p) => p.id)).toEqual([1, 2, 4]);
    expect(tabs[1].panes.map((p) => p.id)).toEqual([3]);
  });
  it("a pane whose parent is gone is a tab of its own", () => {
    expect(rootTabs([tab(5, 9, 1)]).map((t) => t.root.id)).toEqual([5]);
  });
  it("ConPTY's path titles are not names", () => {
    expect(isPathTitle("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")).toBe(true);
    expect(isPathTitle("C:\\Program Files\\PowerShell\\7\\pwsh.exe")).toBe(true);
    expect(isPathTitle("vim README.md")).toBe(false);
    expect(isPathTitle("✳ Claude Code")).toBe(false);
  });
});

describe("the state file", () => {
  it("anything unreadable takes its default", () => {
    expect(normalizeState(null)).toEqual(defaultState());
    expect(normalizeState("x")).toEqual(defaultState());
    const odd = normalizeState({ zoom: 7, appearance: "purple", sidebar: { width: 5 }, order: [1, "a"], font: { size: 400 } });
    expect(odd.zoom).toBe(3);
    expect(odd.appearance).toBe("auto");
    expect(odd.sidebar.width).toBe(170);
    expect(odd.order).toEqual(["a"]);
    expect(odd.font.size).toBe(0);
  });
  it("what it says survives", () => {
    const state = {
      ...defaultState(),
      order: ["b", "a"],
      folded: ["a"],
      tabNames: { "a\u00001": "Primeira" },
      zoom: 1.25,
      appearance: "dark" as const,
      active: { workspace: "a", tab: 1 },
      restore: [{ name: "a", tabs: [{ id: 1, cwd: "C:\\x", name: "Primeira", splitOf: 0, splitDir: 0 }] }],
    };
    expect(normalizeState(JSON.parse(JSON.stringify(state)))).toEqual(state);
  });
});

describe("text", () => {
  it("dropped paths are quoted when a shell would split them", () => {
    expect(quotePaths(["C:\\a\\b.txt"])).toBe("C:\\a\\b.txt");
    expect(quotePaths(["C:\\Meus Documentos\\x.png", "C:\\y"])).toBe('"C:\\Meus Documentos\\x.png" C:\\y');
    expect(quotePaths(["/tmp/a&b"])).toBe('"/tmp/a&b"');
  });
  it("the picker matches in order, and a plain substring first", () => {
    expect(fuzzyScore("clin", "Clinica")).toBeLessThan(fuzzyScore("cla", "Clinica a")!);
    expect(fuzzyScore("xyz", "Clinica")).toBeUndefined();
    expect(fuzzyScore("", "qualquer")).toBe(0);
    expect(fuzzyScore("brk", "Brokerfy")).toBeDefined();
  });
  it("what matched is marked", () => {
    expect(matchRanges("ker", "Brokerfy")).toEqual([[3, 6]]);
    // B-r-o-k-e-r-f-y: the f and the y sit together and are one mark.
    expect(matchRanges("bfy", "Brokerfy")).toEqual([
      [0, 1],
      [6, 8],
    ]);
  });
});
