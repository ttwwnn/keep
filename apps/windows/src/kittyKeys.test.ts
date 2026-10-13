// What keys are sent as with the Kitty keyboard protocol on: the cases of
// Ghostty's own encoder tests (src/input/key_encode.zig), which is what the
// macOS app sends, and the browser's side of them.
//
// Where Ghostty writes the text a key typed, or nothing, the encoder here has
// nothing to send (undefined): the terminal types the text, and says nothing
// for what is not a key. Ghostty's cases that rest on the lock keys are left
// out: the macOS app never reports CapsLock or NumLock, and neither does this.
import { afterEach, describe, expect, it } from "vitest";
import {
  DISAMBIGUATE,
  REPORT_ALL,
  REPORT_ALTERNATES,
  REPORT_ASSOCIATED,
  REPORT_EVENTS,
  encodeKitty,
  kittyKey,
  useLayout,
  type BrowserKey,
  type KittyKey,
} from "./kittyKeys";

const key = (over: Partial<KittyKey> & { key: string }): KittyKey => ({
  action: "press",
  shift: false,
  ctrl: false,
  alt: false,
  meta: false,
  shiftSpent: false,
  composing: false,
  text: "",
  unshifted: 0,
  ...over,
});

const EVERY = DISAMBIGUATE | REPORT_EVENTS | REPORT_ALTERNATES | REPORT_ALL | REPORT_ASSOCIATED;

describe("the keys that type", () => {
  it("are typed by the terminal, with nothing held", () => {
    expect(encodeKitty(key({ key: "KeyA", text: "abcd" }), DISAMBIGUATE)).toBeUndefined();
    expect(encodeKitty(key({ key: "KeyA", action: "repeat", text: "a" }), DISAMBIGUATE)).toBeUndefined();
  });

  it("are typed when shift only made the character, and not when it was held on its own", () => {
    expect(encodeKitty(key({ key: "KeyA", shift: true, shiftSpent: true, text: "A", unshifted: 97 }), DISAMBIGUATE)).toBeUndefined();
    expect(encodeKitty(key({ key: "KeyA", shift: true, text: "A", unshifted: 97 }), DISAMBIGUATE | REPORT_ALTERNATES)).toBe("\x1b[97:65;2u");
  });

  it("are escape codes once every key is reported", () => {
    expect(encodeKitty(key({ key: "KeyA", text: "a", unshifted: 97 }), DISAMBIGUATE | REPORT_ALL)).toBe("\x1b[97u");
    expect(encodeKitty(key({ key: "KeyA", text: "a", unshifted: 97 }), DISAMBIGUATE | REPORT_ALL | REPORT_ASSOCIATED)).toBe("\x1b[97;;97u");
  });

  it("go with their modifiers: ctrl, alt and super make an escape code", () => {
    expect(encodeKitty(key({ key: "KeyC", ctrl: true, unshifted: 99 }), DISAMBIGUATE)).toBe("\x1b[99;5u");
    expect(encodeKitty(key({ key: "KeyA", alt: true, text: "a", unshifted: 97 }), DISAMBIGUATE)).toBe("\x1b[97;3u");
    expect(encodeKitty(key({ key: "KeyA", meta: true, text: "a", unshifted: 97 }), DISAMBIGUATE)).toBe("\x1b[97;9u");
    expect(encodeKitty(key({ key: "KeyA", ctrl: true, shift: true, unshifted: 97 }), DISAMBIGUATE)).toBe("\x1b[97;6u");
    expect(encodeKitty(key({ key: "Space", ctrl: true, unshifted: 32 }), DISAMBIGUATE)).toBe("\x1b[32;5u");
  });
});

describe("Enter, Tab and Backspace", () => {
  it("send what they always sent, so a shell can be typed in after a program that set the mode crashed", () => {
    expect(encodeKitty(key({ key: "Enter" }), DISAMBIGUATE)).toBe("\r");
    expect(encodeKitty(key({ key: "Tab" }), DISAMBIGUATE)).toBe("\t");
    expect(encodeKitty(key({ key: "Backspace" }), DISAMBIGUATE)).toBe("\x7f");
  });

  it("say what is held with them", () => {
    expect(encodeKitty(key({ key: "Enter", shift: true }), DISAMBIGUATE)).toBe("\x1b[13;2u");
    expect(encodeKitty(key({ key: "Enter", ctrl: true }), DISAMBIGUATE)).toBe("\x1b[13;5u");
    expect(encodeKitty(key({ key: "Tab", shift: true }), DISAMBIGUATE)).toBe("\x1b[9;2u");
    expect(encodeKitty(key({ key: "Backspace", shift: true }), DISAMBIGUATE)).toBe("\x1b[127;2u");
    expect(encodeKitty(key({ key: "Backspace", alt: true }), DISAMBIGUATE)).toBe("\x1b[127;3u");
  });

  it("are escape codes when every key is reported", () => {
    expect(encodeKitty(key({ key: "Enter" }), EVERY)).toBe("\x1b[13u");
    expect(encodeKitty(key({ key: "Backspace" }), EVERY)).toBe("\x1b[127u");
  });

  it("say nothing on release unless every key is reported", () => {
    const flags = DISAMBIGUATE | REPORT_EVENTS;
    for (const name of ["Enter", "Backspace", "Tab"]) {
      expect(encodeKitty(key({ key: name, action: "release" }), flags), name).toBeUndefined();
    }
    expect(encodeKitty(key({ key: "Enter", action: "release" }), flags | REPORT_ALL)).toBe("\x1b[13;1:3u");
    expect(encodeKitty(key({ key: "Backspace", action: "release" }), flags | REPORT_ALL)).toBe("\x1b[127;1:3u");
    expect(encodeKitty(key({ key: "Tab", action: "release" }), flags | REPORT_ALL)).toBe("\x1b[9;1:3u");
  });

  it("are what an IME confirmed when they carry its text, and not a dead key's leftover on Backspace", () => {
    expect(encodeKitty(key({ key: "Enter", text: "A", unshifted: 13 }), DISAMBIGUATE | REPORT_ALL)).toBe("A");
    expect(encodeKitty(key({ key: "Backspace", text: "A", unshifted: 13 }), EVERY)).toBe("");
  });
});

describe("the functional keys", () => {
  it("are the protocol's", () => {
    expect(encodeKitty(key({ key: "Escape" }), DISAMBIGUATE)).toBe("\x1b[27u");
    expect(encodeKitty(key({ key: "ArrowUp" }), DISAMBIGUATE)).toBe("\x1b[A");
    expect(encodeKitty(key({ key: "ArrowLeft", ctrl: true }), DISAMBIGUATE)).toBe("\x1b[1;5D");
    expect(encodeKitty(key({ key: "Home" }), DISAMBIGUATE)).toBe("\x1b[H");
    expect(encodeKitty(key({ key: "End", shift: true }), DISAMBIGUATE)).toBe("\x1b[1;2F");
    expect(encodeKitty(key({ key: "Delete" }), DISAMBIGUATE)).toBe("\x1b[3~");
    expect(encodeKitty(key({ key: "Delete", ctrl: true, alt: true }), DISAMBIGUATE)).toBe("\x1b[3;7~");
    expect(encodeKitty(key({ key: "PageUp" }), DISAMBIGUATE)).toBe("\x1b[5~");
    expect(encodeKitty(key({ key: "Insert", shift: true }), DISAMBIGUATE)).toBe("\x1b[2;2~");
    expect(encodeKitty(key({ key: "F1" }), DISAMBIGUATE)).toBe("\x1b[P");
    expect(encodeKitty(key({ key: "F3" }), DISAMBIGUATE)).toBe("\x1b[13~");
    expect(encodeKitty(key({ key: "F5", shift: true }), DISAMBIGUATE)).toBe("\x1b[15;2~");
    expect(encodeKitty(key({ key: "F12" }), DISAMBIGUATE)).toBe("\x1b[24~");
    expect(encodeKitty(key({ key: "F13" }), DISAMBIGUATE)).toBe("\x1b[57376u");
    expect(encodeKitty(key({ key: "F24" }), DISAMBIGUATE)).toBe("\x1b[57387u");
  });

  it("drop what the platform typed for them: a control character, and a private-use marker", () => {
    expect(encodeKitty(key({ key: "Delete", text: "\x7f" }), DISAMBIGUATE)).toBe("\x1b[3~");
    expect(encodeKitty(key({ key: "ArrowUp", text: "\x1e" }), DISAMBIGUATE)).toBe("\x1b[A");
    expect(encodeKitty(key({ key: "Delete", text: "\x7f" }), DISAMBIGUATE | REPORT_ALTERNATES | REPORT_ALL)).toBe("\x1b[3~");
  });

  it("are the keypad's own, with the other legends of it when NumLock is off", () => {
    expect(encodeKitty(key({ key: "NumpadEnter" }), DISAMBIGUATE)).toBe("\x1b[57414u");
    expect(encodeKitty(key({ key: "NumpadLeft" }), DISAMBIGUATE)).toBe("\x1b[57417u");
    expect(encodeKitty(key({ key: "NumpadAdd", ctrl: true }), DISAMBIGUATE)).toBe("\x1b[57413;5u");
    expect(encodeKitty(key({ key: "Numpad1", text: "1" }), DISAMBIGUATE)).toBeUndefined();
    expect(encodeKitty(key({ key: "Numpad1", text: "1" }), EVERY)).toBe("\x1b[57400;;49u");
  });
});

describe("the modifiers alone", () => {
  it("say nothing unless every key is reported", () => {
    expect(encodeKitty(key({ key: "ShiftLeft" }), DISAMBIGUATE | REPORT_ALTERNATES)).toBeUndefined();
    expect(encodeKitty(key({ key: "ShiftLeft" }), DISAMBIGUATE | REPORT_ALL)).toBe("\x1b[57441u");
  });

  it("say what is held, and the event", () => {
    expect(encodeKitty(key({ key: "ControlLeft", ctrl: true }), EVERY)).toBe("\x1b[57442;5u");
    expect(encodeKitty(key({ key: "ControlLeft", ctrl: true, action: "release" }), EVERY)).toBe("\x1b[57442;5:3u");
  });

  it("are all there is of a composition", () => {
    expect(encodeKitty(key({ key: "KeyA", shift: true, composing: true }), DISAMBIGUATE)).toBeUndefined();
    expect(encodeKitty(key({ key: "ShiftLeft", shift: true, composing: true }), DISAMBIGUATE | REPORT_ALL)).toBe("\x1b[57441;2u");
  });
});

describe("text the key has no code for", () => {
  it("is typed by the terminal, and not again on release", () => {
    expect(encodeKitty(key({ key: "Unidentified", text: "û" }), EVERY)).toBeUndefined();
    const flags = DISAMBIGUATE | REPORT_EVENTS | REPORT_ALTERNATES | REPORT_ASSOCIATED;
    expect(encodeKitty(key({ key: "Unidentified", shift: true, text: "!", action: "repeat" }), flags)).toBeUndefined();
    expect(encodeKitty(key({ key: "Unidentified", shift: true, text: "!", action: "release" }), flags | REPORT_ALL)).toBeUndefined();
    expect(encodeKitty(key({ key: "Unidentified", shift: true, text: "!", action: "release" }), flags)).toBeUndefined();
  });
});

describe("the alternates", () => {
  const flags = DISAMBIGUATE | REPORT_ALL | REPORT_ALTERNATES | REPORT_ASSOCIATED;

  it("give the shifted key, and the key on a US keyboard", () => {
    expect(encodeKitty(key({ key: "KeyJ", shift: true, text: "J", unshifted: 106 }), flags)).toBe("\x1b[106:74;2;74u");
    expect(encodeKitty(key({ key: "Semicolon", shift: true, text: ":", unshifted: 59 }), flags)).toBe("\x1b[59:58;2;58u");
    // Hypothetical: an unshifted code that does not match the key's own gives the key as the alternate.
    expect(encodeKitty(key({ key: "KeyA", shift: true, text: "A", unshifted: 65 }), DISAMBIGUATE | REPORT_ALTERNATES)).toBe("\x1b[65::97;2u");
  });

  it("give a Russian layout's key and the US one under it", () => {
    expect(encodeKitty(key({ key: "Semicolon", text: "ч", unshifted: 1095 }), flags)).toBe("\x1b[1095::59;;1095u");
    expect(encodeKitty(key({ key: "Semicolon", shift: true, text: "Ч", unshifted: 1095 }), flags)).toBe("\x1b[1095:1063:59;2;1063u");
  });

  it("are given for a key with no text too, on release", () => {
    expect(
      encodeKitty(key({ key: "BracketLeft", ctrl: true, action: "release", unshifted: 337 }), flags | REPORT_EVENTS),
    ).toBe("\x1b[337::91;5:3u");
    expect(
      encodeKitty(key({ key: "KeyJ", shift: true, text: "J", unshifted: 106, action: "release" }), flags | REPORT_EVENTS),
    ).toBe("\x1b[106:74;2:3u");
  });

  it("are left out for the keys that are controls", () => {
    expect(encodeKitty(key({ key: "Delete", text: "\x7f" }), flags)).toBe("\x1b[3~");
    expect(encodeKitty(key({ key: "Tab", shift: true }), DISAMBIGUATE | REPORT_ALTERNATES)).toBe("\x1b[9;2u");
  });

  it("do not carry the text of a key held with ctrl", () => {
    expect(encodeKitty(key({ key: "KeyJ", ctrl: true, text: "j", unshifted: 106 }), flags)).toBe("\x1b[106;5u");
  });
});

describe("the events", () => {
  const flags = DISAMBIGUATE | REPORT_EVENTS;

  it("are said for a key that has an escape code, and a repeat is a 2", () => {
    expect(encodeKitty(key({ key: "ArrowUp", action: "repeat" }), flags)).toBe("\x1b[1;1:2A");
    expect(encodeKitty(key({ key: "ArrowUp", action: "release", shift: true }), flags)).toBe("\x1b[1;2:3A");
    expect(encodeKitty(key({ key: "Escape", action: "release" }), flags)).toBe("\x1b[27;1:3u");
    expect(encodeKitty(key({ key: "Escape", action: "repeat" }), flags)).toBe("\x1b[27;1:2u");
  });

  it("are not said when no flag asks for them", () => {
    expect(encodeKitty(key({ key: "Escape", action: "release" }), DISAMBIGUATE)).toBeUndefined();
  });

  it("report the release of a key that typed, as an escape code", () => {
    expect(encodeKitty(key({ key: "KeyA", text: "a", unshifted: 97, action: "release" }), flags)).toBe("\x1b[97;1:3u");
  });
});

describe("with the protocol off", () => {
  it("is never the encoder's", () => {
    expect(encodeKitty(key({ key: "Escape" }), 0)).toBeUndefined();
    expect(encodeKitty(key({ key: "Enter", shift: true }), 0)).toBeUndefined();
  });
});

/** A KeyboardEvent as the encoder reads it. */
const event = (over: Partial<BrowserKey> & { code: string; key: string }): BrowserKey => ({
  type: "keydown",
  keyCode: 0,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  repeat: false,
  isComposing: false,
  getModifierState: () => false,
  ...over,
});

const win = { mac: false };

describe("the keys of a browser", () => {
  afterEach(() => useLayout(undefined));

  it("send Shift+Enter, Ctrl+Enter and Escape as the program asked", () => {
    expect(kittyKey(event({ code: "Enter", key: "Enter", shiftKey: true }), DISAMBIGUATE, win)).toBe("\x1b[13;2u");
    expect(kittyKey(event({ code: "Enter", key: "Enter", ctrlKey: true }), DISAMBIGUATE, win)).toBe("\x1b[13;5u");
    expect(kittyKey(event({ code: "NumpadEnter", key: "Enter" }), DISAMBIGUATE, win)).toBe("\x1b[57414u");
    expect(kittyKey(event({ code: "Escape", key: "Escape" }), DISAMBIGUATE, win)).toBe("\x1b[27u");
    expect(kittyKey(event({ code: "Enter", key: "Enter" }), DISAMBIGUATE, win)).toBe("\r");
  });

  it("send Ctrl and Alt with a letter, and Ctrl+Shift with the letter unshifted", () => {
    expect(kittyKey(event({ code: "KeyC", key: "c", ctrlKey: true }), DISAMBIGUATE, win)).toBe("\x1b[99;5u");
    expect(kittyKey(event({ code: "KeyB", key: "b", altKey: true }), DISAMBIGUATE, win)).toBe("\x1b[98;3u");
    expect(kittyKey(event({ code: "KeyA", key: "A", ctrlKey: true, shiftKey: true }), DISAMBIGUATE, win)).toBe("\x1b[97;6u");
    // Ctrl makes a control code, so the key typed no text to be the shifted alternate (the macOS app is the same).
    expect(kittyKey(event({ code: "KeyA", key: "A", ctrlKey: true, shiftKey: true }), DISAMBIGUATE | REPORT_ALTERNATES, win)).toBe("\x1b[97;6u");
    expect(kittyKey(event({ code: "Digit2", key: "@", ctrlKey: true, shiftKey: true }), DISAMBIGUATE, win)).toBe("\x1b[50;6u");
  });

  it("leave a key that types to the terminal", () => {
    expect(kittyKey(event({ code: "KeyA", key: "a" }), DISAMBIGUATE, win)).toBeUndefined();
    expect(kittyKey(event({ code: "KeyA", key: "A", shiftKey: true }), DISAMBIGUATE, win)).toBeUndefined();
    expect(kittyKey(event({ code: "Digit1", key: "!", shiftKey: true }), DISAMBIGUATE, win)).toBeUndefined();
    expect(kittyKey(event({ code: "Space", key: " " }), DISAMBIGUATE, win)).toBeUndefined();
    expect(kittyKey(event({ code: "KeyE", key: "é" }), DISAMBIGUATE, win)).toBeUndefined();
  });

  it("leave AltGr to the terminal: it types, with Ctrl and Alt for the browser", () => {
    const altGr = (name: string) => name === "AltGraph";
    expect(
      kittyKey(event({ code: "Digit7", key: "{", ctrlKey: true, altKey: true, getModifierState: altGr }), DISAMBIGUATE, win),
    ).toBeUndefined();
    expect(
      kittyKey(event({ code: "KeyQ", key: "/", ctrlKey: true, altKey: true, getModifierState: altGr }), EVERY, win),
    ).toBeUndefined();
  });

  it("leave a composition and a dead key to the terminal", () => {
    expect(kittyKey(event({ code: "Quote", key: "Dead" }), EVERY, win)).toBeUndefined();
    expect(kittyKey(event({ code: "KeyA", key: "a", isComposing: true }), EVERY, win)).toBeUndefined();
    expect(kittyKey(event({ code: "KeyA", key: "Process", keyCode: 229 }), EVERY, win)).toBeUndefined();
  });

  it("keep the two chords the macOS app spells out: Shift+Tab is a backtab, Alt+Backspace deletes a word", () => {
    expect(kittyKey(event({ code: "Tab", key: "Tab", shiftKey: true }), DISAMBIGUATE, win)).toBe("\x1b[Z");
    expect(kittyKey(event({ code: "Tab", key: "Tab", shiftKey: true, ctrlKey: true }), DISAMBIGUATE, win)).toBe("\x1b[9;6u");
    expect(kittyKey(event({ code: "Backspace", key: "Backspace", altKey: true }), DISAMBIGUATE, win)).toBe("\x1b\x7f");
    expect(kittyKey(event({ code: "Backspace", key: "Backspace", altKey: true, shiftKey: true }), DISAMBIGUATE, win)).toBe("\x1b[127;4u");
  });

  it("tell a press from a repeat from a release", () => {
    const flags = DISAMBIGUATE | REPORT_EVENTS;
    expect(kittyKey(event({ code: "ArrowDown", key: "ArrowDown" }), flags, win)).toBe("\x1b[1;1:1B");
    expect(kittyKey(event({ code: "ArrowDown", key: "ArrowDown", repeat: true }), flags, win)).toBe("\x1b[1;1:2B");
    expect(kittyKey(event({ type: "keyup", code: "ArrowDown", key: "ArrowDown" }), flags, win)).toBe("\x1b[1;1:3B");
    expect(kittyKey(event({ type: "keyup", code: "ArrowDown", key: "ArrowDown" }), DISAMBIGUATE, win)).toBeUndefined();
    expect(kittyKey(event({ type: "keypress", code: "KeyA", key: "a" }), flags, win)).toBeUndefined();
  });

  it("read the keypad's other legends when NumLock is off", () => {
    expect(kittyKey(event({ code: "Numpad4", key: "ArrowLeft" }), DISAMBIGUATE, win)).toBe("\x1b[57417u");
    expect(kittyKey(event({ code: "Numpad7", key: "Home", ctrlKey: true }), DISAMBIGUATE, win)).toBe("\x1b[57423;5u");
    expect(kittyKey(event({ code: "NumpadDecimal", key: "Delete" }), DISAMBIGUATE, win)).toBe("\x1b[57426u");
    expect(kittyKey(event({ code: "Numpad4", key: "4" }), DISAMBIGUATE, win)).toBeUndefined();
  });

  it("take the key from the layout the browser reports, and from the US keyboard where it does not", () => {
    // A Russian layout: the key under Q types "й", and a Ctrl chord on it is that letter's.
    useLayout(new Map([["KeyQ", "й"], ["KeyC", "с"]]));
    expect(kittyKey(event({ code: "KeyC", key: "с", ctrlKey: true }), DISAMBIGUATE | REPORT_ALTERNATES, win)).toBe("\x1b[1089::99;5u");
    expect(kittyKey(event({ code: "KeyQ", key: "Й", ctrlKey: true, shiftKey: true }), DISAMBIGUATE | REPORT_ALTERNATES, win)).toBe(
      "\x1b[1081::113;6u",
    );
    useLayout(undefined);
    expect(kittyKey(event({ code: "KeyC", key: "c", ctrlKey: true }), DISAMBIGUATE | REPORT_ALTERNATES, win)).toBe("\x1b[99;5u");
    expect(kittyKey(event({ code: "BracketLeft", key: "{", ctrlKey: true, shiftKey: true }), DISAMBIGUATE, win)).toBe("\x1b[91;6u");
  });

  it("take the Option of a Mac for Alt, and not for a compose key", () => {
    const mac = { mac: true };
    expect(kittyKey(event({ code: "KeyW", key: "∑", altKey: true }), DISAMBIGUATE, mac)).toBe("\x1b[119;3u");
    expect(kittyKey(event({ code: "KeyW", key: "w", metaKey: true }), DISAMBIGUATE, mac)).toBe("\x1b[119;9u");
  });

  it("are nothing without the protocol", () => {
    expect(kittyKey(event({ code: "Escape", key: "Escape" }), 0, win)).toBeUndefined();
  });
});
