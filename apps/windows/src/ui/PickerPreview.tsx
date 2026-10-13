// The picture of a screen beside the list: a tab's own, with the colours it had
// — the daemon hands it over as the sequences it would repaint a terminal with
// — drawn by a terminal that is only looked at. What the person sees is what
// they would see if they went there.
//
// A terminal rather than a reading of the colours, because a screen holds more
// than colours: programs that draw their own put things where they want them,
// and the snapshot says where in sequences that only a terminal reads right.

import { Unicode11Addon } from "@xterm/addon-unicode11";
import { Terminal, type ITheme } from "@xterm/xterm";
import { useEffect, useRef } from "preact/hooks";
import * as api from "../api";
import { locateLine } from "../palette/hits";
import { historyFor, plainAsScreen, rowsToSkip, topFor, type PreviewTarget } from "../palette/preview";
import { markLine, type Look } from "../palette/reveal";
import { fontFamily, palette } from "../store";

/** The text of a preview: smaller than the terminal's own, as it is a picture of one. */
const FONT_SIZE = 11;

/** The room the picture leaves round itself in its frame. */
const INSET = { x: 10, y: 8 };

/** How long a request waits, so that holding ↓ asks only for the row it stops on. */
const SETTLE_MS = 60;

/** The app's terminal look on a ground that is the frame's own. */
function look(theme: ITheme): ITheme {
  return { ...theme, background: "#00000000", cursor: "#00000000" };
}

/**
 * The screen with its colours, on the grid the daemon holds it on right now;
 * an older daemon refuses the question, and the plain screen — on the grid the
 * list last said — is better than none.
 */
async function screenOf(target: PreviewTarget): Promise<api.Screen> {
  try {
    return await api.previewVt(target.workspace, target.pane);
  } catch {
    const text = plainAsScreen(await api.preview(target.workspace, target.pane));
    return { cols: target.cols, rows: target.rows, text };
  }
}

/** Everything written so far has been read. */
const read = (term: Terminal) => new Promise<void>((done) => term.write("", done));

/** The rows of the terminal's buffer, for finding a line in it. */
const rowsOf = (term: Terminal) => {
  const buffer = term.buffer.active;
  return { length: buffer.length, text: (i: number) => buffer.getLine(i)?.translateToString(true) ?? "" };
};

/**
 * What a picture shows once it is drawn. A screen: at its bottom, with the rows
 * that do not fit the frame taken off its top so its last row is in view — but
 * a program on the alternate screen is a whole drawn page, whose top is what
 * tells what it is. A line asked for: in the middle, and marked. Returns the
 * mark's removal.
 */
function settle(term: Terminal, picture: HTMLElement, frame: HTMLElement, target: PreviewTarget, colours: Look): (() => void) | null {
  const buffer = term.buffer.active;
  const rows = picture.querySelector<HTMLElement>(".xterm-rows");
  const cell = rows?.children[0]?.getBoundingClientRect().height ?? 0;
  const room = frame.clientHeight - 2 * INSET.y;
  if (!target.hit) {
    term.scrollToBottom();
    let last = -1;
    for (let y = term.rows - 1; y >= 0 && last < 0; y--) {
      if ((buffer.getLine(buffer.viewportY + y)?.translateToString(true) ?? "").trim() !== "") last = y;
    }
    // Moved by where the row really is: a row's height is not a whole number of pixels, and rows counted off at the width of one drift.
    const skip = buffer.type === "alternate" ? 0 : rowsToSkip(last, cell, room);
    const top = (rows?.children[skip] as HTMLElement | undefined)?.offsetTop ?? 0;
    picture.style.transform = `translateY(${-top}px)`;
    return null;
  }
  const located = locateLine(rowsOf(term), target.hit);
  if (!located) return null;
  term.scrollToLine(topFor(located.index, cell > 0 ? Math.floor(room / cell) : term.rows, buffer.baseY));
  return located.exact ? markLine(term, located.index, target.hit.span, colours) : null;
}

export function Preview(props: { target: PreviewTarget | null; title: string; shown: boolean }) {
  const { target } = props;
  const frame = useRef<HTMLDivElement>(null);
  const picture = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const mark = useRef<(() => void) | null>(null);
  const family = fontFamily.value;
  const theme = palette.value.terminal;
  const colours = palette.value;

  // One terminal for as long as the card is up: a new one per row would be a
  // new one per arrow key.
  useEffect(() => {
    const created = new Terminal({
      allowProposedApi: true,
      allowTransparency: true,
      cursorBlink: false,
      disableStdin: true,
      fontFamily: family,
      fontSize: FONT_SIZE,
      scrollback: 0,
      theme: look(theme),
    });
    created.loadAddon(new Unicode11Addon());
    created.unicode.activeVersion = "11";
    created.open(picture.current!);
    term.current = created;
    return () => {
      created.dispose();
      term.current = null;
    };
  }, []);

  useEffect(() => {
    const current = term.current;
    if (!current) return;
    current.options.fontFamily = family;
    current.options.theme = look(theme);
  }, [family, theme]);

  const key = target ? `${target.workspace}\u0000${target.pane}\u0000${target.hit?.line ?? ""}` : "";
  useEffect(() => {
    const current = term.current;
    const shown = picture.current;
    if (!current || !shown) return;
    mark.current?.();
    mark.current = null;
    // The last picture goes at once: a screen shown under another tab's name,
    // for the moment the next takes to come, would be worse than none.
    shown.style.visibility = "hidden";
    shown.style.transform = "";
    if (!target) return;
    let cancelled = false;
    const wait = setTimeout(async () => {
      let screen: api.Screen;
      try {
        screen = await screenOf(target);
      } catch {
        return;
      }
      if (cancelled) return;
      // Reset only once what was written before is read: a write still being
      // read would land on the new screen.
      await read(current);
      if (cancelled) return;
      current.reset();
      current.resize(Math.max(2, screen.cols), Math.max(1, screen.rows));
      current.options.scrollback = historyFor(target, screen.rows);
      // The cursor hidden: a picture of a screen has no one typing in it.
      current.write(`${screen.text}\x1b[?25l`, () => {
        if (cancelled || !frame.current) return;
        mark.current = settle(current, shown, frame.current, target, colours);
        shown.style.visibility = "";
      });
    }, SETTLE_MS);
    return () => {
      cancelled = true;
      clearTimeout(wait);
    };
  }, [key]);

  return (
    <>
      <div class={`pk-preview-title ${props.shown ? "" : "off"}`}>{props.title}</div>
      <div class={`pk-preview ${props.shown ? "" : "off"}`} ref={frame}>
        <div class="pk-screen" ref={picture} />
      </div>
    </>
  );
}
