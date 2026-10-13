// The arrows between panes: Windows Terminal's chords, and nothing else's.
import { describe, expect, it } from "vitest";
import { shortcut, type KeyLike } from "./keys";

const key = (code: string, mods: Partial<KeyLike> = {}): KeyLike => ({
  key: "",
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe("the chords between panes", () => {
  it("Alt and an arrow go to the pane beside", () => {
    expect(shortcut(key("ArrowLeft", { altKey: true }))).toEqual({ kind: "focusPane", direction: "left" });
    expect(shortcut(key("ArrowRight", { altKey: true }))).toEqual({ kind: "focusPane", direction: "right" });
    expect(shortcut(key("ArrowUp", { altKey: true }))).toEqual({ kind: "focusPane", direction: "up" });
    expect(shortcut(key("ArrowDown", { altKey: true }))).toEqual({ kind: "focusPane", direction: "down" });
  });
  it("with Shift they move the divider", () => {
    expect(shortcut(key("ArrowLeft", { altKey: true, shiftKey: true }))).toEqual({ kind: "resizePane", direction: "left" });
    expect(shortcut(key("ArrowDown", { altKey: true, shiftKey: true }))).toEqual({ kind: "resizePane", direction: "down" });
  });
  it("are the same on a Mac, where Option is Alt", () => {
    expect(shortcut(key("ArrowLeft", { altKey: true }), true)).toEqual({ kind: "focusPane", direction: "left" });
  });
  it("leave the arrows of the program alone: bare, with Ctrl or with the system key", () => {
    expect(shortcut(key("ArrowLeft"))).toBeNull();
    expect(shortcut(key("ArrowLeft", { shiftKey: true }))).toBeNull();
    expect(shortcut(key("ArrowLeft", { ctrlKey: true }))).toBeNull();
    expect(shortcut(key("ArrowLeft", { ctrlKey: true, altKey: true }))).toBeNull();
    expect(shortcut(key("ArrowLeft", { metaKey: true, altKey: true }))).toBeNull();
  });
  it("do not take Ctrl+Alt+PageUp and PageDown, the workspaces' chords", () => {
    expect(shortcut(key("PageDown", { ctrlKey: true, altKey: true }))).toEqual({ kind: "nextWorkspace" });
    expect(shortcut(key("PageUp", { ctrlKey: true, altKey: true }))).toEqual({ kind: "previousWorkspace" });
  });
});
