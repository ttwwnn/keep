// The Kitty keyboard protocol, from the program's side: the flags it turns on,
// and the answer to the question it asks first.
//
// A program that wants more of the keyboard than a terminal gives by default
// (Shift+Enter apart from Enter, Ctrl+Shift+letters, Escape apart from the
// start of a sequence) asks `CSI ? u` whether the terminal speaks the protocol
// and, hearing back, pushes the flags it wants with `CSI > flags u` and pops
// them on its way out with `CSI < u`. The macOS app's terminal (Ghostty)
// speaks it; so does this one, from here. What the keys are sent as, once the
// flags are on, is kittyKeys.ts. The daemon's repaint declares the flags a
// program has on with `CSI = flags ; 1 u`, so a terminal that attaches late
// has them too.
//
// https://sw.kovidgoyal.net/kitty/keyboard-protocol/

import type { IDisposable, Terminal } from "@xterm/xterm";

/** The five flags there are. */
const FLAGS = 0b11111;

/** How many pushes a screen remembers; one more evicts the oldest. */
const DEPTH = 8;

/** How `CSI = flags ; mode u` combines the flags with the ones on: set them, add to them, take from them. */
type SetMode = 1 | 2 | 3;

/** The flags one screen has on, and those under them that a pop brings back (Ghostty's ring of eight). */
export class FlagStack {
  private readonly flags = new Array<number>(DEPTH).fill(0);
  private index = 0;

  get current(): number {
    return this.flags[this.index];
  }

  set(mode: SetMode, flags: number): void {
    if (mode === 1) this.flags[this.index] = flags;
    else if (mode === 2) this.flags[this.index] |= flags;
    else this.flags[this.index] &= ~flags;
  }

  push(flags: number): void {
    this.index = (this.index + 1) % DEPTH;
    this.flags[this.index] = flags;
  }

  /** Pop `count` entries; as many as the ring holds, or more, leave it empty. */
  pop(count: number): void {
    if (count >= DEPTH) {
      this.reset();
      return;
    }
    for (let i = 0; i < count; i++) {
      this.flags[this.index] = 0;
      this.index = (this.index + DEPTH - 1) % DEPTH;
    }
  }

  reset(): void {
    this.flags.fill(0);
    this.index = 0;
  }
}

export interface Kitty extends IDisposable {
  /** The flags the program has on for the screen it is drawing on; 0 is the protocol off. */
  flags(): number;
}

/** A parameter of a sequence as a number: a sub-parameter list stands for its first. */
const numberOf = (param: number | number[] | undefined): number => (Array.isArray(param) ? (param[0] ?? 0) : (param ?? 0));

/**
 * Follow the flags a program turns on, one stack for the main screen and one
 * for the alternate (a program on the alternate screen leaves the main one's
 * as they were), and answer when it asks. `answer` sends the program a reply
 * as if typed.
 */
export function installKitty(term: Terminal, answer: (data: string) => void): Kitty {
  const main = new FlagStack();
  const alternate = new FlagStack();
  const active = () => (term.buffer.active.type === "alternate" ? alternate : main);

  const handlers = [
    // CSI ? u — the question: what flags are on?
    term.parser.registerCsiHandler({ prefix: "?", final: "u" }, () => {
      answer(`\x1b[?${active().current}u`);
      return true;
    }),
    // CSI > flags u — push: these are the flags now, and the ones before come back on a pop.
    term.parser.registerCsiHandler({ prefix: ">", final: "u" }, (params) => {
      const flags = numberOf(params[0]);
      if (flags <= FLAGS) active().push(flags);
      return true;
    }),
    // CSI < n u — pop n (one, where none is said).
    term.parser.registerCsiHandler({ prefix: "<", final: "u" }, (params) => {
      active().pop(numberOf(params[0]) || 1);
      return true;
    }),
    // CSI = flags ; mode u — set, or add to, or take from, the flags on.
    term.parser.registerCsiHandler({ prefix: "=", final: "u" }, (params) => {
      const flags = numberOf(params[0]);
      const mode = params.length > 1 ? numberOf(params[1]) : 1;
      if (flags <= FLAGS && (mode === 1 || mode === 2 || mode === 3)) active().set(mode, flags);
      return true;
    }),
    // A full reset (ESC c) turns the protocol off on both screens; the terminal does its own reset as well.
    term.parser.registerEscHandler({ final: "c" }, () => {
      main.reset();
      alternate.reset();
      return false;
    }),
    // The alternate screen is made new each time it is entered and gone when it is left, its flags with it.
    term.buffer.onBufferChange(() => alternate.reset()),
  ];

  return {
    flags: () => active().current,
    dispose: () => handlers.forEach((handler) => handler.dispose()),
  };
}
