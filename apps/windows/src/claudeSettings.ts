// Claude Code's settings file (`settings.json`), as the bar needs it touched:
//
//  - The status line. The bar learns Claude Code's model and effort only from
//    the status line command (tools/keep-statusline.ps1, which the installer
//    carries). On opening, the app sets it up in a file that has none — the
//    key added and nothing else in the file changed — and leaves alone one
//    that has another.
//  - The defaults. `/model <id>` saves the model as the default for new
//    sessions, which the sessions opened without --model follow live; after
//    the one pick that still goes that way (the 1M-context Opus, which
//    `/model`'s list does not have), the keys it wrote are put back as they
//    were — the rest of the file as it stands then. A port of
//    `typeKeepingDefaults` in apps/macos/Sources/Keep/UI/MainWindowController.swift.
//
// Pure functions of the file's text; store.ts reads and writes the file
// through the app (src-tauri/src/claude_settings.rs).

type Json = Record<string, unknown>;

const isObject = (v: unknown): v is Json => typeof v === "object" && v !== null && !Array.isArray(v);

/** The object a settings file holds, or null for one that is not (or not there). */
export function parseSettings(text: string | null): Json | null {
  if (text === null) return null;
  try {
    const json: unknown = JSON.parse(text.replace(/^﻿/, ""));
    return isObject(json) ? json : null;
  } catch {
    return null;
  }
}

/** A value as text in which two equal values read the same: the keys of every object in order. */
export function canonical(value: unknown): string {
  const sorted = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(sorted);
    if (isObject(v)) return Object.fromEntries(Object.keys(v).sort().map((k) => [k, sorted(v[k])]));
    return v;
  };
  return JSON.stringify(sorted(value)) ?? "null";
}

// ---------------------------------------------------------------- the status line

/**
 * The status line command for the script at `script`: Windows PowerShell,
 * which every Windows has, with the path quoted and in forward slashes —
 * Claude Code runs it through Git Bash, where a backslash can escape what
 * follows it, or else through cmd, and both take this.
 */
export function statusLineCommand(script: string): string {
  return `powershell -NoProfile -ExecutionPolicy Bypass -File "${script.replace(/\\/g, "/")}"`;
}

/**
 * Characters Git Bash expands inside double quotes (`$`, a backquote) or cmd
 * does (`%`), and the quote itself: a path with one is left out rather than
 * risked.
 */
const UNSAFE_IN_PATH = /["$`%]/;

export type StatusLinePlan =
  /** The file to write: the one there with the status line added, or a new one with only it. */
  | { kind: "write"; text: string; command: string }
  /** It is set to the script already. */
  | { kind: "ours" }
  /** Another status line is set: the person's, left alone. */
  | { kind: "other"; command: string }
  /** Not a file to touch: not JSON, not an object, or a path no shell would take as it is. */
  | { kind: "unreadable"; why: string };

/**
 * What to do with a settings file (its text, or null when there is none) so
 * Claude Code runs the status line script at `script`: nothing when it does
 * already or runs another one; else the text with `statusLine` added before
 * the closing brace, in the file's own indentation and line ends, every other
 * byte as it was.
 */
export function planStatusLine(text: string | null, script: string): StatusLinePlan {
  if (UNSAFE_IN_PATH.test(script)) {
    return { kind: "unreadable", why: `o caminho do script tem caracteres que o shell interpretaria: ${script}` };
  }
  const command = statusLineCommand(script);
  const value = { type: "command", command };
  if (text === null || text.replace(/^\uFEFF/, "").trim() === "") {
    const block = JSON.stringify(value, null, 2).replace(/\n/g, "\n  ");
    return { kind: "write", text: `{\n  "statusLine": ${block}\n}\n`, command };
  }
  const settings = parseSettings(text);
  if (settings === null) return { kind: "unreadable", why: "não é um objeto JSON" };
  if ("statusLine" in settings) {
    const current = settings.statusLine;
    if (isObject(current) && current.command === command) return { kind: "ours" };
    const said = isObject(current) && typeof current.command === "string" ? current.command : canonical(current);
    return { kind: "other", command: said };
  }
  const close = text.lastIndexOf("}");
  const head = text.slice(0, close).replace(/\s+$/, "");
  const tail = text.slice(close);
  const empty = head.replace(/^\uFEFF/, "").trim() === "{";
  let added: string;
  if (!/[\r\n]/.test(head)) {
    // All on one line: the key goes on it too.
    added = `${head}${empty ? "" : ","}"statusLine":${JSON.stringify(value)}${tail}`;
  } else {
    const newline = text.includes("\r\n") ? "\r\n" : "\n";
    const indent = /\n([ \t]+)"/.exec(text)?.[1] ?? "  ";
    const entry = [
      `${indent}"statusLine": {`,
      `${indent}${indent}"type": "command",`,
      `${indent}${indent}"command": ${JSON.stringify(command)}`,
      `${indent}}`,
    ].join(newline);
    added = `${head}${empty ? "" : ","}${newline}${entry}${newline}${tail}`;
  }
  // What was there must read the same, and the status line must be the one asked for.
  const check = parseSettings(added);
  const kept =
    check !== null &&
    canonical(check.statusLine) === canonical(value) &&
    Object.keys(check).length === Object.keys(settings).length + 1 &&
    Object.keys(settings).every((k) => canonical(check[k]) === canonical(settings[k]));
  return kept
    ? { kind: "write", text: added, command }
    : { kind: "unreadable", why: "não deu para acrescentar sem mexer no resto" };
}

// ---------------------------------------------------------------- the defaults

/** The keys `/model` and `/effort` with an argument save as Claude Code's default for new sessions. */
export const DEFAULT_KEYS = ["model", "effortLevel", "modelSettings"] as const;

/** What those keys hold in a settings file, as text that tells two apart (`{"model":"x"}`); absent ones are left out. */
export function defaultsOf(text: string | null): string {
  const settings = parseSettings(text) ?? {};
  const picked: Json = {};
  for (const key of DEFAULT_KEYS) if (key in settings) picked[key] = settings[key];
  return canonical(picked);
}

/**
 * The settings file with those keys put back as `before` (what `defaultsOf`
 * said earlier) had them — present with their values, absent removed — and
 * everything else as the file has it now. Null when there is nothing to put
 * back, or no file to put it in.
 */
export function restoreDefaults(text: string | null, before: string): string | null {
  const settings = parseSettings(text);
  if (settings === null || text === null || defaultsOf(text) === before) return null;
  let wanted: Json;
  try {
    const parsed: unknown = JSON.parse(before);
    if (!isObject(parsed)) return null;
    wanted = parsed;
  } catch {
    return null;
  }
  const next: Json = { ...settings };
  for (const key of DEFAULT_KEYS) {
    if (key in wanted) next[key] = wanted[key];
    else delete next[key];
  }
  const newline = text.includes("\r\n") ? "\r\n" : "\n";
  const body = JSON.stringify(next, null, 2).replace(/\n/g, newline);
  const bom = text.startsWith("﻿") ? "﻿" : "";
  return bom + body + (/\r?\n$/.test(text) ? newline : "");
}
