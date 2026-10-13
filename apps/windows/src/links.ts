// What can be clicked in a terminal's text: web addresses, and the files and
// folders it names.
//
// The macOS app's terminal does the same (Model/LinkOpener.swift): ⌘-click on
// an address opens it, a plain click on a path that exists opens that, and a
// right-click on one offers "Abrir", "Mostrar no Finder" and "Copiar caminho".
// Here Ctrl takes the place of ⌘. A path is only a link when the file system
// says it is there, so the words around it are not underlined for nothing.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { IBuffer, IBufferCell, IDisposable, ILink, Terminal } from "@xterm/xterm";
import * as api from "./api";
import { flatten, pathTokens, runsWhenOpened, tokenAt, type Candidate, type Flavor, type PathToken, type Piece, type Where } from "./paths";
import type { MenuItem } from "./ui/common";
import { filePath, findUrls } from "./urls";

export interface LinkEnv {
  flavor: Flavor;
  /** ⌘ takes the place of Ctrl: the app developed on a Mac. */
  mac: boolean;
  /** Where the pane's program works; empty when unknown. */
  cwd(): string;
  home(): string;
  /** A quiet line for the person. */
  say(message: string): void;
}

/** How long a second click has to follow the first to make a double one (Windows' default). */
const DOUBLE_CLICK_MS = 500;
/** How far a press may move and still be a click. */
const CLICK_SLOP = 4;
/** How many rows of the screen a window of lines is made of, each side of the one asked about. */
const NEIGHBOURS = 3;
/** The longest a line is followed through the rows the terminal wrapped. */
const MAX_ROWS = 60;

// ---------------------------------------------------------------- the screen as lines

/** Where a character of a line sits on the screen. */
interface Cells {
  /** The buffer line, from 1. */
  y: number;
  /** The first cell it takes, from 0. */
  x: number;
  /** One past the last, which is the last from 1. */
  end: number;
}

/** A line as a program wrote it: the rows the terminal wrapped, joined. */
interface Logical {
  text: string;
  cells: Cells[];
}

const groupStart = (buffer: IBuffer, y: number) => {
  while (y > 0 && buffer.getLine(y)?.isWrapped) y--;
  return y;
};

const groupEnd = (buffer: IBuffer, y: number) => {
  while (buffer.getLine(y + 1)?.isWrapped) y++;
  return y;
};

function logicalFrom(buffer: IBuffer, first: number, last: number, scratch: IBufferCell): Logical {
  let text = "";
  const cells: Cells[] = [];
  for (let y = first; y <= Math.min(last, first + MAX_ROWS); y++) {
    const line = buffer.getLine(y);
    if (!line) break;
    for (let x = 0; x < line.length; x++) {
      const cell = line.getCell(x, scratch);
      if (!cell) break;
      const width = cell.getWidth();
      // The second half of a wide character has no character of its own.
      if (width === 0) continue;
      const chars = cell.getChars() || " ";
      for (let i = 0; i < chars.length; i++) cells.push({ y: y + 1, x, end: x + width });
      text += chars;
    }
  }
  let n = text.length;
  while (n > 0 && text[n - 1] === " ") n--;
  return { text: text.slice(0, n), cells: cells.slice(0, n) };
}

/** The line holding buffer line `y`, with the lines above and below it. */
function around(buffer: IBuffer, y: number): { lines: Logical[]; row: number } {
  const scratch = buffer.getNullCell();
  const first = groupStart(buffer, y);
  const last = groupEnd(buffer, y);
  const before: Logical[] = [];
  for (let top = first; before.length < NEIGHBOURS && top > 0; ) {
    const start = groupStart(buffer, top - 1);
    before.unshift(logicalFrom(buffer, start, top - 1, scratch));
    top = start;
  }
  const after: Logical[] = [];
  for (let bottom = last; after.length < NEIGHBOURS && buffer.getLine(bottom + 1); ) {
    const end = groupEnd(buffer, bottom + 1);
    after.push(logicalFrom(buffer, bottom + 1, end, scratch));
    bottom = end;
  }
  return { lines: [...before, logicalFrom(buffer, first, last, scratch), ...after], row: before.length };
}

/** The range on the screen that runs from the start of the first piece to the end of the last, from 1. */
function rangeOf(lines: Logical[], pieces: Piece[]): ILink["range"] {
  const first = pieces[0];
  const last = pieces[pieces.length - 1];
  const from = lines[first.line].cells[first.start];
  const to = lines[last.line].cells[last.end - 1];
  return { start: { x: from.x + 1, y: from.y }, end: { x: to.end, y: to.y } };
}

/** The index in a line of the character at a cell, or -1 past its text. */
function indexAt(line: Logical, y: number, column: number): number {
  return line.cells.findIndex((c) => c.y === y && c.x <= column && column < c.end);
}

// ---------------------------------------------------------------- opening

const fileManager = (mac: boolean) => (mac ? "Finder" : "Explorer");

const nameOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

/**
 * Open what a click found. A folder opens in the file manager. A file opens
 * with what the system opens it with — unless that would run it, which a click
 * on text that a program printed is not the way to ask for: those are shown
 * in the file manager instead.
 */
export async function openFound(path: string, dir: boolean, env: LinkEnv): Promise<void> {
  try {
    if (!dir && runsWhenOpened(path)) {
      await api.revealPath(path);
      env.say(`${nameOf(path)} não é aberto por um clique: foi mostrado no ${fileManager(env.mac)}.`);
    } else {
      await api.openPath(path);
    }
  } catch (error) {
    env.say(`Não foi possível abrir ${nameOf(path)}: ${String(error)}`);
  }
}

async function openAddress(url: string, env: LinkEnv): Promise<void> {
  const local = filePath(url, env.flavor);
  if (/^file:/i.test(url)) {
    if (local === undefined) {
      env.say("Esse endereço de arquivo não é daqui.");
      return;
    }
    const [hit] = await api.findPaths([[local]]).catch(() => [null]);
    if (!hit) env.say(`${nameOf(local)} não foi encontrado.`);
    else await openFound(hit.path, hit.dir, env);
    return;
  }
  try {
    await api.openUrl(url);
  } catch (error) {
    env.say(`Não foi possível abrir ${url}: ${String(error)}`);
  }
}

/** An address clicked in the terminal's text, or in a link the program drew (OSC 8): opened on Ctrl-click only. */
export function activateAddress(event: MouseEvent, url: string, env: LinkEnv): void {
  if (event.button === 0 && (env.mac ? event.metaKey : event.ctrlKey) && !event.shiftKey && !event.altKey) void openAddress(url, env);
}

// ---------------------------------------------------------------- asking the file system

interface Cached {
  at: number;
  hits: (api.PathHit | null)[];
}

const TTL_MS = 2500;
const cache = new Map<string, Cached>();

/** Ask which of each group of spellings exists, remembering the answer for a moment: hovering a line again costs nothing. */
async function exists(groups: string[][]): Promise<(api.PathHit | null)[]> {
  if (groups.length === 0) return [];
  const key = JSON.stringify(groups);
  const known = cache.get(key);
  if (known && Date.now() - known.at < TTL_MS) return known.hits;
  const hits = await api.findPaths(groups).catch(() => groups.map(() => null));
  if (cache.size > 200) cache.clear();
  cache.set(key, { at: Date.now(), hits });
  return hits;
}

interface Found {
  token: PathToken;
  candidate: Candidate;
  path: string;
  dir: boolean;
}

async function resolveTokens(tokens: PathToken[]): Promise<Found[]> {
  const flat = tokens.map(flatten);
  const hits = await exists(flat.map((list) => list.map((f) => f.path)));
  const found: Found[] = [];
  tokens.forEach((token, i) => {
    const hit = hits[i];
    if (hit) found.push({ token, candidate: flat[i][hit.index].candidate, path: hit.path, dir: hit.dir });
  });
  return found;
}

// ---------------------------------------------------------------- the links

/** The window's focus, to tell a click that only brought the window forward. */
let windowFocusedAt = 0;

/** The modifiers last seen held, by the keyboard or by the pointer: what an address under the pointer is underlined by. */
const mods = { ctrlKey: false, metaKey: false, shiftKey: false, altKey: false };
const seeMods = (e: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }) => {
  mods.ctrlKey = e.ctrlKey;
  mods.metaKey = e.metaKey;
  mods.shiftKey = e.shiftKey;
  mods.altKey = e.altKey;
};

if (typeof window !== "undefined") {
  window.addEventListener("focus", () => (windowFocusedAt = Date.now()));
  window.addEventListener("keydown", seeMods, true);
  window.addEventListener("keyup", seeMods, true);
  window.addEventListener("blur", () => seeMods({ ctrlKey: false, metaKey: false, shiftKey: false, altKey: false }));
}

export interface Links extends IDisposable {
  /** Items for the menu of a right-click on a path; none where there is none. */
  menu(event: MouseEvent): Promise<(MenuItem | "separator")[]>;
}

/**
 * Link the terminal's addresses and paths. An address opens on Ctrl-click and
 * is underlined while Ctrl is held; a path opens on a plain click that is
 * nothing else — one press, not dragged, not the first of two, in a pane that
 * already had the keyboard — and on Ctrl-click at once.
 */
export function installLinks(term: Terminal, host: HTMLElement, env: LinkEnv): Links {
  const where = (): Where => ({ flavor: env.flavor, cwd: env.cwd(), home: env.home() });
  const held = (e: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }) =>
    (env.mac ? e.metaKey : e.ctrlKey) && !e.shiftKey && !e.altKey;

  let down: { x: number; y: number; at: number; focused: boolean } | undefined;
  let pending: ReturnType<typeof setTimeout> | undefined;
  // Seen before the terminal's own handler takes the focus.
  const onDown = (e: MouseEvent) => {
    clearTimeout(pending);
    down = {
      x: e.clientX,
      y: e.clientY,
      at: Date.now(),
      focused: document.hasFocus() && document.activeElement === term.textarea && Date.now() - windowFocusedAt > 500,
    };
  };
  host.addEventListener("mousedown", onDown, true);
  // Before the terminal's own handler asks for the links under the pointer.
  host.addEventListener("mousemove", seeMods, true);

  const activatePath = (e: MouseEvent, path: string, dir: boolean) => {
    if (held(e)) {
      void openFound(path, dir, env);
      return;
    }
    if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey || e.altKey || e.detail !== 1) return;
    if (!down?.focused || term.hasSelection() || Math.hypot(e.clientX - down.x, e.clientY - down.y) >= CLICK_SLOP) return;
    const pressed = down.at;
    clearTimeout(pending);
    // A double click selects the word: wait the interval out, and a press in it cancels.
    pending = setTimeout(() => {
      if (down?.at === pressed) void openFound(path, dir, env);
    }, DOUBLE_CLICK_MS);
  };

  /** An address is underlined, and the pointer a hand, only while Ctrl is held — also when it is pressed with the pointer at rest. */
  const underlineWithModifier = (link: ILink) => {
    let off: (() => void) | undefined;
    const set = (on: boolean) => {
      if (link.decorations) {
        link.decorations.underline = on;
        link.decorations.pointerCursor = on;
      }
    };
    link.hover = () => {
      // The terminal swaps the decorations for ones it tracks once this returns.
      queueMicrotask(() => set(held(mods)));
      const follow = (k: KeyboardEvent) => set(held(k));
      window.addEventListener("keydown", follow);
      window.addEventListener("keyup", follow);
      off = () => {
        window.removeEventListener("keydown", follow);
        window.removeEventListener("keyup", follow);
      };
    };
    link.leave = () => off?.();
    link.dispose = () => off?.();
  };

  const provider = term.registerLinkProvider({
    provideLinks(y, callback) {
      const buffer = term.buffer.active;
      const { lines, row } = around(buffer, y - 1);
      const text = lines[row].text;
      if (text === "") return callback(undefined);

      const links: ILink[] = [];
      for (const match of findUrls(text)) {
        const link: ILink = {
          range: rangeOf(lines, [{ line: row, start: match.start, end: match.end }]),
          text: match.url,
          decorations: { underline: held(mods), pointerCursor: held(mods) },
          activate: (e, url) => activateAddress(e, url, env),
        };
        underlineWithModifier(link);
        links.push(link);
      }

      const tokens = pathTokens(
        lines.map((l) => l.text),
        row,
        where(),
      );
      if (tokens.length === 0) return callback(links.length > 0 ? links : undefined);
      void resolveTokens(tokens).then((found) => {
        for (const f of found) {
          const range = rangeOf(lines, f.candidate.pieces);
          links.push({
            range,
            text: f.path,
            decorations: { underline: true, pointerCursor: true },
            activate: (e) => activatePath(e, f.path, f.dir),
          });
        }
        // Where two cover the same cells the first stays: an address wins over a path in it.
        links.sort((a, b) => a.range.start.y - b.range.start.y || a.range.start.x - b.range.start.x);
        const kept: ILink[] = [];
        for (const link of links) {
          const last = kept[kept.length - 1];
          const clash = last && (link.range.start.y < last.range.end.y || (link.range.start.y === last.range.end.y && link.range.start.x <= last.range.end.x));
          if (!clash) kept.push(link);
        }
        callback(kept.length > 0 ? kept : undefined);
      });
    },
  });

  /** The cell under a mouse event, as a buffer line from 0 and a column from 0. */
  const cellAt = (e: MouseEvent): { y: number; column: number } | undefined => {
    const screen = term.element?.querySelector<HTMLElement>(".xterm-screen");
    if (!screen || term.cols === 0 || term.rows === 0) return undefined;
    const box = screen.getBoundingClientRect();
    const column = Math.floor(((e.clientX - box.left) / box.width) * term.cols);
    const row = Math.floor(((e.clientY - box.top) / box.height) * term.rows);
    if (column < 0 || column >= term.cols || row < 0 || row >= term.rows) return undefined;
    return { y: term.buffer.active.viewportY + row, column };
  };

  return {
    async menu(event) {
      const at = cellAt(event);
      if (!at) return [];
      const { lines, row } = around(term.buffer.active, at.y);
      const index = indexAt(lines[row], at.y + 1, at.column);
      if (index < 0) return [];
      const token = tokenAt(
        lines.map((l) => l.text),
        row,
        index,
        where(),
      );
      if (!token) return [];
      const [found] = await resolveTokens([token]);
      if (!found) return [];
      const { path, dir } = found;
      return [
        { label: "Abrir", action: () => void openFound(path, dir, env) },
        { label: `Mostrar no ${fileManager(env.mac)}`, action: () => void api.revealPath(path).catch((e) => env.say(`Não foi possível mostrar: ${String(e)}`)) },
        { label: "Copiar o caminho", action: () => void writeText(path).catch(() => navigator.clipboard?.writeText(path)) },
      ];
    },
    dispose() {
      clearTimeout(pending);
      host.removeEventListener("mousedown", onDown, true);
      provider.dispose();
    },
  };
}
