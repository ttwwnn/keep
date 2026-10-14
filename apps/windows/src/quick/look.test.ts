import { describe, expect, it } from "vitest";
import { defaultState, type GuiState } from "../state";
import { sameLook } from "./look";

const state = (change: (s: GuiState) => void = () => {}): GuiState => {
  const s = defaultState();
  change(s);
  return s;
};

describe("what the panel wears", () => {
  it("is the same state's look", () => {
    expect(sameLook(state(), state())).toBe(true);
  });

  it("does not change with what is not its look: the sidebar, the names, the tabs, where the window was", () => {
    const other = state((s) => {
      s.sidebar = { visible: false, width: 300 };
      s.tabNames = { "a\u00001": "um nome" };
      s.workspaceNames = { a: "outro" };
      s.order = ["b", "a"];
      s.hidden = ["a"];
      s.folded = ["b"];
      s.lastTab = { a: 4 };
      s.tabOrder = { a: [3, 2] };
      s.verticalTabs = true;
      s.active = { workspace: "a", tab: 1 };
      s.daemon = "7-1";
    });
    expect(sameLook(state(), other)).toBe(true);
  });

  it("changes with the theme, the appearance, the zoom, the face and its size, the cursor and the history", () => {
    const changes: [string, (s: GuiState) => void][] = [
      ["theme", (s) => void (s.theme = "Dracula")],
      ["appearance", (s) => void (s.appearance = "light")],
      ["zoom", (s) => void (s.zoom = 1.5)],
      ["face", (s) => void (s.font = { family: "Consolas", size: 0 })],
      ["size", (s) => void (s.font = { family: "", size: 18 })],
      ["cursor", (s) => void (s.terminal = { ...s.terminal, cursor: "block" })],
      ["blink", (s) => void (s.terminal = { ...s.terminal, blink: false })],
      ["history", (s) => void (s.terminal = { ...s.terminal, scrollback: 500 })],
    ];
    for (const [name, change] of changes) expect(sameLook(state(), state(change)), name).toBe(false);
  });
});
