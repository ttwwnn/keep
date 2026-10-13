import { describe, expect, it } from "vitest";
import { DEFAULT_SHORTCUT, keyName, refusalNotice, shortcutFromChord, shortcutKeys, shortcutText, whyUnusable } from "./shortcut";

const chord = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("the shortcut a chord spells", () => {
  it("names the modifiers in one order, then the key by its place on the keyboard", () => {
    expect(shortcutFromChord(chord("Backquote", { ctrlKey: true }))).toBe("Control+Backquote");
    // Whatever order they were pressed in, and with the layout's own character out of it.
    expect(shortcutFromChord(chord("KeyK", { shiftKey: true, altKey: true, ctrlKey: true }))).toBe("Control+Alt+Shift+KeyK");
    expect(shortcutFromChord(chord("Digit1", { metaKey: true }))).toBe("Super+Digit1");
    expect(shortcutFromChord(chord("F12"))).toBe("F12");
  });

  it("is nothing while only modifiers are down", () => {
    for (const code of ["ControlLeft", "ControlRight", "ShiftLeft", "AltRight", "MetaLeft", "OSRight"]) {
      expect(shortcutFromChord(chord(code, { ctrlKey: true, shiftKey: true }))).toBeNull();
    }
    expect(shortcutFromChord(chord("", { ctrlKey: true }))).toBeNull();
    expect(shortcutFromChord(chord("Unidentified", { ctrlKey: true }))).toBeNull();
  });

  it("makes the default from the chord a person would press", () => {
    expect(shortcutFromChord(chord("Backquote", { ctrlKey: true }))).toBe(DEFAULT_SHORTCUT);
  });
});

describe("whether a shortcut will do", () => {
  it("needs Ctrl, Alt or Win, unless it is a function key", () => {
    expect(whyUnusable("Control+Backquote")).toBeNull();
    expect(whyUnusable("Alt+Space")).toBeNull();
    expect(whyUnusable("Super+KeyK")).toBeNull();
    expect(whyUnusable("Control+Shift+KeyK")).toBeNull();
    expect(whyUnusable("F1")).toBeNull();
    expect(whyUnusable("Shift+F24")).toBeNull();
    expect(whyUnusable("KeyK")).toMatch(/precisa de Ctrl, Alt ou Win/);
    expect(whyUnusable("Shift+KeyK")).toMatch(/precisa de Ctrl, Alt ou Win/);
    expect(whyUnusable("Backquote")).toMatch(/precisa de Ctrl, Alt ou Win/);
    // F25 is no key.
    expect(whyUnusable("F25")).not.toBeNull();
  });

  it("is never Esc, which a program needs", () => {
    expect(whyUnusable("Control+Escape")).toMatch(/Esc/);
    expect(whyUnusable("Escape")).toMatch(/Esc/);
  });

  it("is not a shortcut when it says no key", () => {
    expect(whyUnusable("")).not.toBeNull();
    expect(whyUnusable("Control+")).not.toBeNull();
  });
});

describe("a shortcut drawn as keys", () => {
  it("is one cap for each modifier and one for the key", () => {
    expect(shortcutKeys("Control+Backquote")).toEqual(["Ctrl", "`"]);
    expect(shortcutKeys("Control+Alt+Shift+KeyK")).toEqual(["Ctrl", "Alt", "Shift", "K"]);
    expect(shortcutKeys("Super+Digit1")).toEqual(["Win", "1"]);
    expect(shortcutKeys("F12")).toEqual(["F12"]);
    expect(shortcutKeys("Control+Alt+Space")).toEqual(["Ctrl", "Alt", "Espaço"]);
  });

  it("is drawn as the Mac draws it where the app is only developed", () => {
    expect(shortcutKeys("Control+Alt+Shift+Super+KeyK", true)).toEqual(["⌃", "⌥", "⇧", "⌘", "K"]);
    expect(shortcutText("Control+Backquote", true)).toBe("⌃`");
  });

  it("is a line for a sentence", () => {
    expect(shortcutText("Control+Shift+Backquote")).toBe("Ctrl+Shift+`");
    expect(shortcutText("Control+KeyK")).toBe("Ctrl+K");
  });

  it("names the keys as their caps do", () => {
    expect(keyName("KeyQ")).toBe("Q");
    expect(keyName("Digit0")).toBe("0");
    expect(keyName("Backslash")).toBe("\\");
    expect(keyName("ArrowUp")).toBe("↑");
    expect(keyName("Numpad5")).toBe("Num 5");
    // A key with no cap of its own keeps its code rather than nothing.
    expect(keyName("IntlBackslash")).toBe("IntlBackslash");
  });
});

describe("what a refused shortcut is told as", () => {
  it("says which, why, and where to choose another", () => {
    const text = refusalNotice({ shortcut: "Control+Backquote", problem: "O sistema não liberou esse atalho.", suggestion: "Control+Shift+Backquote" });
    expect(text).toBe(
      "A janela rápida não pôde tomar Ctrl+`. O sistema não liberou esse atalho. Em Configurações dá para escolher outro atalho, por exemplo Ctrl+Shift+`.",
    );
  });

  it("offers nothing it has not got", () => {
    const text = refusalNotice({ shortcut: "Control+KeyK", problem: null, suggestion: null });
    expect(text).toBe("A janela rápida não pôde tomar Ctrl+K. Em Configurações dá para escolher outro atalho.");
  });
});
