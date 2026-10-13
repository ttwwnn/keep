// Telling a program whether the ground is dark or light (the macOS app's
// SurfaceView.tellScheme).
//
// A program that switches mode 2031 on (`CSI ? 2031 h`) is told, with
// `CSI ? 997 ; 1 n` for a dark ground and `CSI ? 997 ; 2 n` for a light one,
// at once and each time the ground turns the other way; and `CSI ? 996 n`
// asks for the report once. Claude Code does both, and answers by asking for
// the background (OSC 11, which xterm.js answers from the theme) and redrawing
// in the colours that go with it.

import type { Terminal } from "@xterm/xterm";

/** The report: `CSI ? 997 ; 1 n` for a dark ground, `2` for a light one. */
export function schemeReport(dark: boolean): string {
  return `\x1b[?997;${dark ? 1 : 2}n`;
}

const asks = (params: (number | number[])[], mode: number) => params.some((p) => p === mode);

export class SchemeReporter {
  /** The program asked to be told: mode 2031 is on. */
  private told = false;

  constructor(
    private readonly term: Pick<Terminal, "parser" | "input">,
    private dark: boolean,
  ) {
    const { parser } = term;
    // Every handler answers false: the parser goes on to its own, which
    // knows nothing of these modes and passes the rest of the sequence on.
    parser.registerCsiHandler({ prefix: "?", final: "h" }, (params) => {
      if (asks(params, 2031)) {
        this.told = true;
        this.report();
      }
      return false;
    });
    parser.registerCsiHandler({ prefix: "?", final: "l" }, (params) => {
      if (asks(params, 2031)) this.told = false;
      return false;
    });
    parser.registerCsiHandler({ prefix: "?", final: "n" }, (params) => {
      if (asks(params, 996)) this.report();
      return false;
    });
    // A reset puts the modes back as they were at the start: told no more.
    parser.registerEscHandler({ final: "c" }, () => {
      this.told = false;
      return false;
    });
    parser.registerCsiHandler({ intermediates: "!", final: "p" }, () => {
      this.told = false;
      return false;
    });
  }

  /** The ground turned dark or light; a program that asked is told. */
  setDark(dark: boolean): void {
    if (dark === this.dark) return;
    this.dark = dark;
    if (this.told) this.report();
  }

  private report(): void {
    // Not typed by the person: the screen is not scrolled to the bottom for it.
    this.term.input(schemeReport(this.dark), false);
  }
}
