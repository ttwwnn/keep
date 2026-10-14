// Unicode grapheme clusters (mode 2027), as the macOS app's terminal (Ghostty)
// has them.
//
// A program that wants an emoji with a joiner, a letter with its marks or a
// flag drawn as one character switches the mode on (`CSI ? 2027 h`) and off
// again when it is done (`CSI ? 2027 l`). While it is on, the code points of a
// cluster share one cell (or two, for a wide one); while it is off, each code
// point has the width of its own, as in any terminal that knows no better. The
// daemon's grid does the same — it clusters only while the program has the
// mode on, and its repaint says the mode either way before the rows — so
// both have to agree on where the cursor is. xterm.js 6 has the pieces and not
// the mode: `@xterm/addon-unicode-graphemes` registers the clustering width
// rules and, once loaded, makes them the active ones for good. So they are
// loaded and put aside (Unicode 11, the rule without clusters, stays the
// active one), and the mode, which is the parser's to see, picks between the
// two. A program asks whether the terminal knows the mode with `CSI ? 2027 $ p`
// and is told `CSI ? 2027 ; 1 $ y` if it is on, `2` if it is off.

import type { Terminal } from "@xterm/xterm";
import { UnicodeGraphemesAddon } from "@xterm/addon-unicode-graphemes";

/** The mode's number. */
const MODE = 2027;
/** The rule without clusters, which the Unicode 11 addon registers. */
export const PLAIN = "11";
/** The rule with them, which the grapheme addon registers. */
export const CLUSTERED = "15-graphemes";

/** What a program is told when it asks about the mode: set is 1, reset is 2. */
export function modeReport(on: boolean): string {
  return `\x1b[?${MODE};${on ? 1 : 2}$y`;
}

const names = (params: (number | number[])[]) => params.some((p) => p === MODE);

export interface Graphemes {
  /** Whether the program has the mode on. */
  readonly on: boolean;
}

/**
 * Loads the clustering rules and watches the mode. The Unicode 11 addon must
 * be loaded first: its rule is the one used while the mode is off.
 */
export function installGraphemes(term: Terminal, answer: (data: string) => void): Graphemes {
  term.loadAddon(new UnicodeGraphemesAddon());
  term.unicode.activeVersion = PLAIN;
  const state = { on: false };
  const set = (on: boolean) => {
    state.on = on;
    term.unicode.activeVersion = on ? CLUSTERED : PLAIN;
  };
  const { parser } = term;
  // Every handler answers false but the question's: the parser goes on to its
  // own, which passes the other modes in the same sequence on.
  parser.registerCsiHandler({ prefix: "?", final: "h" }, (params) => {
    if (names(params)) set(true);
    return false;
  });
  parser.registerCsiHandler({ prefix: "?", final: "l" }, (params) => {
    if (names(params)) set(false);
    return false;
  });
  // The question, answered here: xterm.js would say it does not know the mode.
  parser.registerCsiHandler({ prefix: "?", intermediates: "$", final: "p" }, (params) => {
    if (params[0] !== MODE) return false;
    answer(modeReport(state.on));
    return true;
  });
  // A full reset puts the modes back as they were at the start.
  parser.registerEscHandler({ final: "c" }, () => {
    set(false);
    return false;
  });
  return {
    get on() {
      return state.on;
    },
  };
}
