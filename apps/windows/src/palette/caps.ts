// Keys drawn as keys: the caps a chord is written with.

const MAC: Readonly<Record<string, string>> = { Ctrl: "⌘", Shift: "⇧", Alt: "⌥" };

/**
 * The caps for a chord written as Windows writes it ("Ctrl", "Shift", "K").
 * On a Mac, where the app is only developed, ⌘ stands in for Ctrl as it does
 * in the shortcuts themselves (keys.ts), and Shift and Alt are drawn as the
 * Mac draws them.
 */
export function capsFor(keys: readonly string[], mac: boolean): string[] {
  return keys.map((key) => (mac ? MAC[key] ?? key : key));
}
