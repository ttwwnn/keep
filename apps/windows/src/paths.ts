// Paths in the text of a terminal: which words may name a file or a folder,
// and every spelling of the path they stand for, to put to the file system.
//
// A port of the macOS app's LinkOpener (Model/LinkOpener.swift), widened for
// Windows: drive letters, UNC shares, backslashes, `/mnt/c/…` from WSL, quoted
// paths with spaces in them. Nothing here touches the file system or the page;
// the caller asks the file system which spelling exists (see links.ts).
//
// Paths arrive as they were printed: `~/x`, relative to the shell's directory,
// with the `:line` or `:line:col` that tools append, broken across two rows by
// a program that wraps its own text (Claude Code does), or with the spaces
// escaped by a backslash the way a dropped file arrives on a Mac.

/** How paths are written where the shell runs: Windows' own, or POSIX's. */
export type Flavor = "windows" | "posix";

export interface Where {
  flavor: Flavor;
  /** The pane's directory; empty when unknown. */
  cwd: string;
  /** The person's home folder; empty when unknown. */
  home: string;
}

/** A run of one line of text, in UTF-16 units: `[start, end)`. */
export interface Piece {
  line: number;
  start: number;
  end: number;
}

export interface Candidate {
  /** What is on screen for it, in order: one piece, or the rows a path was broken across. */
  pieces: Piece[];
  /** The spellings of the path to ask the file system about, the likeliest first. */
  paths: string[];
}

export interface PathToken {
  /** The word's own run, to tell whether a point is on it. */
  at: Piece;
  /** The longest piece of text first: the first with a path that exists is the link. */
  candidates: Candidate[];
}

const isSpace = (c: string | undefined) => c !== undefined && /\s/.test(c);

/** Quotes, brackets and the sentence's own punctuation around a path. */
const WRAPPING = new Set([..."`'\"()<>[]{}.,;:!?"]);

// ---------------------------------------------------------------- spelling

const DRIVE_ROOT = /^[A-Za-z]:[\\/]/;
const UNC_ROOT = /^[\\/]{2}[^\\/\s]/;

export function isAbsolute(path: string, flavor: Flavor): boolean {
  if (flavor === "posix") return path.startsWith("/");
  return DRIVE_ROOT.test(path) || UNC_ROOT.test(path);
}

/** Fold `.` and `..` out of the parts of a path; a `..` at the root stays at the root. */
function fold(rest: string, separator: RegExp): string[] {
  const parts: string[] = [];
  for (const part of rest.split(separator)) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return parts;
}

/**
 * The path with `.` and `..` folded, one kind of separator and no doubled or
 * trailing ones (a root keeps its own).
 */
export function normalize(path: string, flavor: Flavor): string {
  if (flavor === "posix") return "/" + fold(path, /\//).join("/");
  const text = path.replace(/\//g, "\\");
  const drive = /^[A-Za-z]:\\?/.exec(text);
  if (drive) {
    const rest = fold(text.slice(drive[0].length), /\\/).join("\\");
    return `${drive[0].slice(0, 2)}\\${rest}`;
  }
  if (text.startsWith("\\\\")) {
    // \\server\share is the root; what follows is inside it.
    const parts = text.slice(2).split("\\").filter((part) => part !== "");
    const root = "\\\\" + parts.slice(0, 2).join("\\");
    const rest = fold(parts.slice(2).join("\\"), /\\/).join("\\");
    return rest === "" ? root : `${root}\\${rest}`;
  }
  return fold(text, /\\/).join("\\");
}

function join(directory: string, relative: string, flavor: Flavor): string {
  return directory.replace(/[\\/]+$/, "") + (flavor === "windows" ? "\\" : "/") + relative;
}

/** `/mnt/c/Users/x` and `/c/Users/x` (WSL's and Git Bash's) as the Windows path they stand for. */
function drivePath(text: string): string | undefined {
  const match = /^\/(?:mnt\/)?([A-Za-z])(?:\/(.*))?$/.exec(text);
  if (!match) return undefined;
  // `/c` alone is a word; `/mnt/c` is a drive.
  if (match[2] === undefined && !text.startsWith("/mnt/")) return undefined;
  return `${match[1].toUpperCase()}:\\${(match[2] ?? "").replace(/\//g, "\\")}`;
}

/** The text, then the text without a numeric `:line` or `:line:col` (or `(line,col)`), step by step. */
export function suffixVariants(text: string): string[] {
  const out = [text];
  let trimmed = text;
  for (let i = 0; i < 2; i++) {
    const colon = trimmed.lastIndexOf(":");
    if (colon < 0) break;
    const tail = trimmed.slice(colon + 1);
    if (!/^\d+$/.test(tail)) break;
    trimmed = trimmed.slice(0, colon);
    out.push(trimmed);
  }
  // tsc and MSBuild say it as `file.ts(12,5)`.
  const parens = /^(.+)\(\d+(?:,\d+)?\)$/.exec(text);
  if (parens) out.push(parens[1]);
  return out;
}

/** The absolute path a spelling stands for from where the shell is, or undefined when it cannot be told. */
function resolve(spelled: string, where: Where): string | undefined {
  const { flavor, cwd, home } = where;
  if (spelled === "~" || /^~[\\/]/.test(spelled)) {
    if (!home) return undefined;
    return normalize(join(home, spelled.slice(2), flavor), flavor);
  }
  if (flavor === "windows") {
    if (spelled.startsWith("/") && !spelled.startsWith("//")) {
      // WSL's and Git Bash's drives; any other rooted POSIX path means nothing here.
      const drive = drivePath(spelled);
      return drive ? normalize(drive, flavor) : undefined;
    }
    // Rooted on a drive nobody named.
    if (spelled.startsWith("\\") && !spelled.startsWith("\\\\")) return undefined;
  }
  if (isAbsolute(spelled, flavor)) return normalize(spelled, flavor);
  if (!cwd) return undefined;
  return normalize(join(cwd, spelled, flavor), flavor);
}

/**
 * Every absolute path a printed one may stand for, the likeliest first: the
 * path whole (a file may really be called `a:1`), then without a trailing
 * `:line[:col]`.
 */
export function expand(text: string, where: Where): string[] {
  const out: string[] = [];
  for (const variant of suffixVariants(text)) {
    const path = variant === "" ? undefined : resolve(variant, where);
    if (path !== undefined && !out.includes(path)) out.push(path);
  }
  return out;
}

// ---------------------------------------------------------------- words

/** Whether a word is a POSIX path, where a backslash before a blank escapes it rather than ends a folder. */
function posixStyle(word: string): boolean {
  return /^(?:~|\.{1,2})?\//.test(word) && !DRIVE_ROOT.test(word);
}

/**
 * The run of text around `start..<end` that a shell would read as one word:
 * no blanks in it, except a blank a backslash escapes. On Windows a backslash
 * before a blank is more often a folder ending (`cd C:\x\ && …`), so there
 * it escapes only inside a POSIX-looking path.
 */
export function shellWord(text: string, start: number, end: number, flavor: Flavor): [number, number] {
  const run = (escapes: boolean): [number, number] => {
    let low = start;
    while (low > 0 && (!isSpace(text[low - 1]) || (escapes && low >= 2 && text[low - 2] === "\\"))) low--;
    let high = end;
    while (high < text.length && (!isSpace(text[high]) || (escapes && high > 0 && text[high - 1] === "\\"))) high++;
    return [low, high];
  };
  if (flavor === "posix") return run(true);
  const plain = run(false);
  return posixStyle(text.slice(plain[0], plain[1])) ? run(true) : plain;
}

/** What a word dropped into a shell stands for: `\x` is `x` (only `\ ` where the word is no POSIX path). */
export function unescape(text: string, flavor: Flavor): string {
  if (flavor === "windows" && !posixStyle(text)) return text.replace(/\\ /g, " ");
  let out = "";
  let escaping = false;
  for (const char of text) {
    if (escaping) {
      out += char;
      escaping = false;
    } else if (char === "\\") escaping = true;
    else out += char;
  }
  return out;
}

/**
 * How many characters of wrapping the text begins and ends with. The dots of
 * `./x` and `../x` are the path's own, and so is the `(12,5)` a compiler puts
 * after a file name.
 */
function wrapped(text: string): [number, number] {
  let lead = 0;
  while (lead < text.length && WRAPPING.has(text[lead]) && !/^\.{1,2}[\\/]/.test(text.slice(lead, lead + 3))) lead++;
  if (lead === text.length) return [lead, 0];
  const position = /\(\d+(?:,\d+)?\)([:;,.!?]*)$/.exec(text);
  if (position) return [lead, position[1].length];
  let tail = 0;
  while (tail < text.length - lead && WRAPPING.has(text[text.length - 1 - tail])) tail++;
  return [lead, tail];
}

/** The word at a cell of a line, as a shell would read it, or undefined on a blank. */
export function wordAt(text: string, column: number, flavor: Flavor): [number, number] | undefined {
  if (column < 0 || column >= text.length) return undefined;
  if (isSpace(text[column]) && !(column > 0 && text[column - 1] === "\\")) return undefined;
  return shellWord(text, column, column + 1, flavor);
}

// ---------------------------------------------------------------- shapes

const SCHEME = /^[A-Za-z][A-Za-z0-9+.-]+:(?:\/\/|[^\\/\d])/;
const BARE_FILE = /^([\p{L}\p{N}_@+-][\p{L}\p{N}_@+.-]*)\.(\p{L}[\p{L}\p{N}]{0,9})$/u;
const DOT_FILE = /^\.\p{L}[\p{L}\p{N}_.-]+$/u;

/** A name with an extension, or a dot file: `README.md`, `.gitignore`; not `e.g.` nor `v1.2`. */
function isBareFile(shape: string): boolean {
  const match = BARE_FILE.exec(shape);
  if (match) return match[1].length > 1 || match[2].length > 1;
  return DOT_FILE.test(shape);
}

/** Whether a word is rooted: where it points does not depend on where the shell is. */
function rooted(word: string): boolean {
  return /^(?:[A-Za-z]:[\\/]|\\\\[^\\/\s]|\/|~[\\/]|\.{1,2}[\\/])/.test(word);
}

/**
 * Whether a word has the look of a path worth asking the file system about:
 * rooted, explicitly relative, or a folder and a file name with a dot in it.
 * Prose is full of slashes (`and/or`, `24/7`) and dots; those do not pass.
 */
export function looksLikePath(word: string, flavor: Flavor): boolean {
  const shape = word.replace(/(?::\d+){1,2}$|\(\d+(?:,\d+)?\)$/, "");
  if (shape.length < 2 || SCHEME.test(shape)) return false;
  const windows = flavor === "windows";
  if (windows && (DRIVE_ROOT.test(shape) || UNC_ROOT.test(shape))) return true;
  if (windows ? /^~[\\/]./.test(shape) : /^~\/./.test(shape)) return true;
  if (windows ? /^\.{1,2}[\\/]./.test(shape) : /^\.{1,2}\/./.test(shape)) return true;
  if (shape.startsWith("/")) {
    if (shape.startsWith("//") || shape.length < 3) return false;
    return !windows || drivePath(shape) !== undefined;
  }
  const separator = windows ? /[\\/]/ : /\//;
  const parts = shape.split(separator);
  if (parts.length > 1) {
    // `a/b/c` and `lib/` are paths; `and/or` is not, but `src/app.ts` is.
    if (parts.length > 2 || parts[parts.length - 1] === "") return true;
    return /\.[\p{L}\p{N}]/u.test(parts[parts.length - 1]);
  }
  return isBareFile(shape);
}

// ---------------------------------------------------------------- candidates

interface Part {
  piece: Piece;
  text: string;
}

/** How many rows, and how many words, a path may be pieced together from. */
const MORE_ROWS = 3;
const MORE_WORDS = 5;

/** Characters no file name has on Windows; a word with one of them does not go on into the next. */
const NOT_IN_A_NAME = /[<>"|?*]/;

/** A word that starts something else, which a path with a space in it does not run into. */
function startsAnother(word: string): boolean {
  return /^(?:[A-Za-z]:[\\/]|\\\\|~[\\/]|\.{1,2}[\\/]|[A-Za-z][A-Za-z0-9+.-]+:\/\/)/.test(word);
}

/**
 * The pieces of text to try for the word at `lines[line]` between `low` and
 * `high`, longest first: the word with the words a program broke the path
 * from (on the row above when the word starts its row, on the row below when
 * it ends it); and, for a rooted path, with the words after it that a space
 * in a folder's name would have put there.
 */
function candidatesAround(lines: string[], line: number, low: number, high: number, where: Where): Candidate[] {
  const { flavor } = where;
  const text = lines[line];
  const middle: Part = { piece: { line, start: low, end: high }, text: text.slice(low, high) };

  const before: Part[] = [];
  if (text.slice(0, low).trim() === "") {
    // Up the screen while the word starts its row and the row above ends in
    // one; past the first, only rows that are that word alone.
    let index = line;
    while (index > 0 && before.length < MORE_ROWS) {
      index--;
      const above = lines[index].replace(/\s+$/, "");
      if (above === "") break;
      const [l, h] = shellWord(above, above.length - 1, above.length, flavor);
      before.unshift({ piece: { line: index, start: l, end: h }, text: above.slice(l, h) });
      if (above.slice(0, l).trim() !== "") break;
    }
  }
  const after: Part[] = [];
  if (text.slice(high).trim() === "") {
    let index = line;
    while (index < lines.length - 1 && after.length < MORE_ROWS) {
      index++;
      const below = lines[index];
      const first = below.search(/\S/);
      if (first < 0) break;
      const [l, h] = shellWord(below, first, first + 1, flavor);
      after.push({ piece: { line: index, start: l, end: h }, text: below.slice(l, h) });
      if (below.slice(h).trim() !== "") break;
    }
  }

  // The words after it on its own row, for a folder with a space in its name:
  // the longest reach first, then shorter, then none.
  const spaced: Part[][] = [[]];
  if (rooted(middle.text) && before.length === 0 && after.length === 0) {
    let cursor = high;
    const extra: Part[] = [];
    while (extra.length < MORE_WORDS) {
      const gap = text.slice(cursor).search(/\S/);
      if (gap < 0) break;
      const from = cursor + gap;
      const [l, h] = shellWord(text, from, from + 1, flavor);
      if (l < cursor || startsAnother(text.slice(l, h)) || NOT_IN_A_NAME.test(text.slice(l, h))) break;
      // The blank between the words is part of the name, said as it stands.
      extra.push({ piece: { line, start: cursor, end: h }, text: text.slice(cursor, h) });
      spaced.unshift([...extra]);
      cursor = h;
    }
  }

  const found: Candidate[] = [];
  const seen = new Set<string>();
  const add = (parts: Part[]) => {
    const joined = parts.map((p) => p.text).join("");
    const [lead, tail] = wrapped(joined);
    for (const cut of [false, true]) {
      if (cut && lead === 0 && tail === 0) continue;
      const body = cut ? joined.slice(lead, joined.length - tail) : joined;
      const paths: string[] = [];
      for (const spelled of [unescape(body, flavor), body]) {
        if (spelled === "") continue;
        for (const path of expand(spelled, where)) if (!paths.includes(path)) paths.push(path);
      }
      const pieces = parts.map((p) => ({ ...p.piece }));
      if (cut) {
        pieces[0].start += Math.min(lead, pieces[0].end - pieces[0].start);
        const last = pieces[pieces.length - 1];
        last.end -= Math.min(tail, last.end - last.start);
      }
      const shown = pieces.filter((p) => p.end > p.start);
      if (paths.length === 0 || shown.length === 0) continue;
      const key = shown.map((p) => `${p.line}:${p.start}:${p.end}`).join("|") + "=" + paths.join("\u0000");
      if (seen.has(key)) continue;
      seen.add(key);
      found.push({ pieces: shown, paths });
    }
  };

  for (let b = before.length; b >= 0; b--) {
    for (let a = after.length; a >= 0; a--) {
      const head = before.slice(before.length - b);
      const foot = after.slice(0, a);
      for (const extra of b === 0 && a === 0 ? spaced : [[]]) add([...head, middle, ...extra, ...foot]);
    }
  }
  return found;
}

/** The runs of quoted text that look like a path: the quotes say where it ends, spaces and all. */
function quotedRuns(text: string, flavor: Flavor): [number, number][] {
  const runs: [number, number][] = [];
  const quoted = /(["'`])([^"'`\n]{2,}?)\1/g;
  let match: RegExpExecArray | null;
  while ((match = quoted.exec(text))) {
    const inner = match[2];
    // Only a run with a blank in it: a quoted word is read as any other word.
    // And `it's /tmp/x and don't` has a run between its apostrophes: a path
    // with spaces that is quoted begins rooted or ends in a file's extension.
    if (!/\s/.test(inner) || inner !== inner.trim() || !looksLikePath(inner, flavor)) continue;
    if (!rooted(inner) && !/\.\p{L}[\p{L}\p{N}]{0,9}$/u.test(inner)) continue;
    const start = match.index + 1;
    runs.push([start, start + inner.length]);
  }
  return runs;
}

/**
 * The words of `lines[row]` that look like paths, each with the pieces of text
 * to try for it. `lines` are the logical lines around the row (a row the
 * terminal wrapped by itself is one line with the rest of its row): the rows
 * above and below let a path a program broke be pieced back together.
 */
export function pathTokens(lines: string[], row: number, where: Where): PathToken[] {
  const text = lines[row] ?? "";
  if (text.length === 0 || text.length > 4000) return [];
  const { flavor } = where;
  const tokens: PathToken[] = [];

  for (const [start, end] of quotedRuns(text, flavor)) {
    const body = text.slice(start, end);
    const paths: string[] = [];
    for (const spelled of [unescape(body, flavor), body]) {
      for (const path of expand(spelled, where)) if (!paths.includes(path)) paths.push(path);
    }
    if (paths.length > 0) {
      const piece = { line: row, start, end };
      tokens.push({ at: piece, candidates: [{ pieces: [piece], paths }] });
    }
  }

  let at = 0;
  while (at < text.length) {
    if (isSpace(text[at])) {
      at++;
      continue;
    }
    const [low, high] = shellWord(text, at, at + 1, flavor);
    at = Math.max(high, at + 1);
    const word = text.slice(low, high);
    const [lead, tail] = wrapped(word);
    if (!looksLikePath(word.slice(lead, word.length - tail), flavor) && !looksLikePath(word, flavor)) continue;
    const candidates = candidatesAround(lines, row, low, high, where);
    if (candidates.length > 0) tokens.push({ at: { line: row, start: low, end: high }, candidates });
  }
  // Where two cover the same text the longer comes first, and is the one kept.
  return tokens.sort((a, b) => a.at.start - b.at.start || b.at.end - a.at.end);
}

/**
 * The token under a point of `lines[row]`, for a right-click: the path-like
 * one if the point is on it, else the plain word there — whatever its shape,
 * since the menu asks the file system and is only for what exists.
 */
export function tokenAt(lines: string[], row: number, column: number, where: Where): PathToken | undefined {
  const known = pathTokens(lines, row, where).find((t) => column >= t.at.start && column < t.at.end);
  if (known) return known;
  const word = wordAt(lines[row] ?? "", column, where.flavor);
  if (!word) return undefined;
  const candidates = candidatesAround(lines, row, word[0], word[1], where);
  return candidates.length > 0 ? { at: { line: row, start: word[0], end: word[1] }, candidates } : undefined;
}

/** Every spelling of a token's candidates in one list, each with the candidate it came from. */
export function flatten(token: PathToken): { path: string; candidate: Candidate }[] {
  const out: { path: string; candidate: Candidate }[] = [];
  const seen = new Set<string>();
  for (const candidate of token.candidates) {
    for (const path of candidate.paths) {
      if (seen.has(path)) continue;
      seen.add(path);
      out.push({ path, candidate });
    }
  }
  return out;
}

// ---------------------------------------------------------------- opening

/**
 * What Windows runs when a file is opened, rather than shows: programs,
 * scripts, installers, shortcuts. Opened, a `.py` or a `.js` printed in a
 * conversation would run; so these are shown in the Explorer instead, and the
 * person double-clicks if that is what they meant.
 */
const RUNS_WHEN_OPENED = new Set([
  "exe", "com", "scr", "pif", "bat", "cmd", "lnk", "url", "vbs", "vbe", "js", "jse", "wsf", "wsh", "ws",
  "msi", "msp", "msu", "mst", "cpl", "hta", "jar", "jnlp", "py", "pyw", "pyz", "rb", "pl", "sh", "bash",
  "zsh", "ps1", "psm1", "psd1", "ps1xml", "reg", "inf", "gadget", "appref-ms", "application",
  "settingcontent-ms", "scf", "sct", "shs", "chm", "msc", "xll", "wll", "ocx", "command", "app", "dll",
  "sys", "appx", "msix", "appxbundle", "msixbundle",
]);

/** Whether opening this file would run it, or leave the shell to guess (no extension). A folder is opened, never run. */
export function runsWhenOpened(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return true;
  return RUNS_WHEN_OPENED.has(name.slice(dot + 1).toLowerCase());
}
