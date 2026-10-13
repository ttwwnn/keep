// Web addresses in the text of a terminal, and what a `file:` one stands for.
//
// A port of the macOS app's own reading of them (Model/LinkOpener.swift,
// `webAddress`): the terminal's runtime finds what looks like a link, but only
// the piece on the clicked row, and Claude Code breaks an address too long for
// a row — it fills the row to two columns short of the edge and goes on,
// indented, on the next — so the address is pieced back from the screen.
// Only the schemes a person means to open by clicking: pages, mail, FTP, and
// `file:` (which is a path in the end).

import type { Flavor, Piece } from "./paths";

/** Where a web address starts inside a word. Only these: a word read off the screen as `Nota:` would otherwise pass for one. */
const STARTS = ["https://", "http://", "ftp://", "mailto:", "www."];

/** What closes a sentence or a quotation around an address, and not the address. */
const TRAILING = new Set([..."`'\".,;:!?>]}*"]);

/** A `file:` address, where one starts a word: `profile:` is not one. */
const FILE_START = /(?:^|[^A-Za-z0-9])(file:\/)/i;

export interface Address {
  /** Where the address sits in the word it was read from: `[start, end)`. */
  start: number;
  end: number;
  /** What to open: a `www.` address is a page on https. */
  url: string;
}

const count = (text: string, char: string) => text.split(char).length - 1;

/** The word from `start`, without what closes a sentence or a quotation: a full stop, a comma, a bracket, a parenthesis it did not open. */
function trimmed(word: string, start: number): string {
  let text = word.slice(start);
  while (text !== "") {
    const last = text[text.length - 1];
    if (TRAILING.has(last)) text = text.slice(0, -1);
    else if (last === ")" && count(text, ")") > count(text, "(")) text = text.slice(0, -1);
    else break;
  }
  return text;
}

/** Whether text parses as an address worth opening: it has a host with a dot in it (or `localhost`), or is mail with something after it. */
function worthOpening(text: string): boolean {
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return false;
  }
  if (url.protocol === "mailto:") return text.length > "mailto:".length;
  return url.hostname.includes(".") || url.hostname === "localhost";
}

/**
 * The address a word holds, from where it starts to where the sentence takes
 * over again, or undefined when it holds none.
 */
export function addressIn(word: string): Address | undefined {
  const lowered = word.toLowerCase();
  let start = -1;
  for (const prefix of STARTS) {
    const at = lowered.indexOf(prefix);
    if (at >= 0 && (start < 0 || at < start)) start = at;
  }
  const file = FILE_START.exec(word);
  const fileAt = file ? file.index + file[0].length - file[1].length : -1;
  if (fileAt >= 0 && (start < 0 || fileAt < start)) {
    const text = trimmed(word, fileAt);
    const real = filePath(text, "posix") !== undefined || filePath(text, "windows") !== undefined;
    return real ? { start: fileAt, end: fileAt + text.length, url: text } : undefined;
  }
  if (start < 0) return undefined;
  const text = trimmed(word, start);
  const url = lowered.startsWith("www.", start) ? "https://" + text : text;
  return worthOpening(url) ? { start, end: start + text.length, url } : undefined;
}

/**
 * Whether a click may open an address without the person asking for that
 * particular one: a program can put any address behind its text, and a click
 * meant to put the cursor somewhere should not start an app.
 */
export function isOpenable(url: string): boolean {
  const scheme = /^([A-Za-z][A-Za-z0-9+.-]+):/.exec(url)?.[1].toLowerCase();
  return scheme !== undefined && ["http", "https", "mailto", "ftp", "file"].includes(scheme);
}

/** A scheme of more than one letter makes an address; `C:\` is a drive. */
export function hasScheme(text: string): boolean {
  return /^[A-Za-z][A-Za-z0-9+.-]+:/.test(text);
}

export interface AddressHit {
  url: string;
  /** Where it is on screen: one piece, or one on each row the program broke it across. */
  pieces: Piece[];
}

/** How many rows an address may be pieced from, each side of the word clicked. */
const MORE_ROWS = 3;

interface Part {
  piece: Piece;
  text: string;
}

/**
 * The web addresses of `lines[row]`, each pieced back together across rows
 * when the program broke it: a row continues onto the next only when it is
 * full (`full(line)`: it reaches within a few columns of the edge), so an
 * address that merely ends a row keeps clear of the sentence below it.
 */
export function addressesIn(lines: string[], row: number, full: (line: number) => boolean): AddressHit[] {
  const text = lines[row] ?? "";
  const hits: AddressHit[] = [];
  for (const word of text.matchAll(/\S+/g)) {
    const low = word.index;
    const high = low + word[0].length;
    const parts: Part[] = [{ piece: { line: row, start: low, end: high }, text: word[0] }];
    if (text.slice(0, low).trim() === "") {
      // Up the screen while the word starts its row and the row above is full.
      let index = row;
      while (index > 0 && row - index < MORE_ROWS) {
        const above = lines[index - 1].replace(/\s+$/, "");
        if (above === "" || !full(index - 1)) break;
        const at = above.search(/\S*$/);
        parts.unshift({ piece: { line: index - 1, start: at, end: above.length }, text: above.slice(at) });
        index--;
        if (above.slice(0, at).trim() !== "") break;
      }
    }
    const rest = text.replace(/\s+$/, "");
    if (high >= rest.length) {
      // And down it while the word ends its row and that row is full.
      let index = row;
      let ending = rest;
      while (index < lines.length - 1 && index - row < MORE_ROWS && ending !== "" && full(index)) {
        const below = lines[index + 1];
        const first = below.search(/\S/);
        if (first < 0) break;
        const end = first + below.slice(first).search(/\s|$/);
        parts.push({ piece: { line: index + 1, start: first, end }, text: below.slice(first, end) });
        index++;
        ending = below.replace(/\s+$/, "");
        if (end < ending.length) break;
      }
    }
    const joined = parts.map((p) => p.text).join("");
    const found = addressIn(joined);
    if (found) hits.push({ url: found.url, pieces: piecesOf(parts, found.start, found.end) });
  }
  return hits;
}

/** The runs of the parts that the stretch `[from, to)` of their joined text covers. */
function piecesOf(parts: Part[], from: number, to: number): Piece[] {
  const out: Piece[] = [];
  let at = 0;
  for (const part of parts) {
    const end = at + part.text.length;
    const lo = Math.max(from, at);
    const hi = Math.min(to, end);
    if (hi > lo) out.push({ line: part.piece.line, start: part.piece.start + (lo - at), end: part.piece.start + (hi - at) });
    at = end;
  }
  return out;
}

/** The file a `file:` address names, as a path of the platform, or undefined when it is not one. */
export function filePath(url: string, flavor: Flavor): string | undefined {
  const match = /^file:(?:\/\/([^/]*))?(\/.*)?$/i.exec(url);
  if (!match) return undefined;
  const host = match[1] ?? "";
  let path: string;
  try {
    path = decodeURIComponent(match[2] ?? "");
  } catch {
    return undefined;
  }
  if (flavor === "windows") {
    // file:///C:/Users/t/a.txt, and file://server/share/a.txt for a share.
    if (/^\/[A-Za-z]:/.test(path)) return path.slice(1).replace(/\//g, "\\");
    if (host !== "" && host !== "localhost") return `\\\\${host}${path.replace(/\//g, "\\")}`;
    return undefined;
  }
  if (host !== "" && host !== "localhost") return undefined;
  return path === "" ? undefined : path;
}
