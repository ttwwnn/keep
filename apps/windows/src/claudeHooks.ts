// A hook in Claude Code's settings file (`hooks` in `settings.json`; Codex's
// `hooks.json` has the same shape), added without touching what the person
// has there.
//
// The one the app adds is `keep ia conta-da-aba --gancho` on
// `UserPromptSubmit`: after a hot switch of account (docs/ia.md) the Claude of
// a tab reads the global login in `~/.claude.json` and names the wrong
// account; the hook puts the tab's own in its context. The macOS author has it
// by hand in his settings; on Windows the app sets it up on opening, as it
// does the status line (claudeSettings.ts), with the same care: the hooks the
// person has stay as they are, byte for byte, and the one added is not added
// twice.
//
// Pure functions of the file's text; store.ts reads and writes the file
// through the app (src-tauri/src/claude_settings.rs).

import { UNSAFE_IN_PATH, canonical, parseSettings } from "./claudeSettings";

type Json = Record<string, unknown>;

const isObject = (v: unknown): v is Json => typeof v === "object" && v !== null && !Array.isArray(v);

// ---------------------------------------------------------------- where things are in the text

/** Where a value is in a text, `start` to `end` (exclusive). */
interface Span {
  start: number;
  end: number;
}

const isSpace = (c: string | undefined) => c === " " || c === "\t" || c === "\n" || c === "\r" || c === "﻿";

function skipSpace(text: string, from: number): number {
  let i = from;
  while (i < text.length && isSpace(text[i])) i++;
  return i;
}

/** After the string that opens at `from`. */
function stringEnd(text: string, from: number): number {
  for (let i = from + 1; i < text.length; i++) {
    if (text[i] === "\\") i++;
    else if (text[i] === '"') return i + 1;
  }
  return text.length;
}

/**
 * After the value that starts at `from`. The text is one that parsed, so it
 * is not checked again: this only has to know where things end.
 */
function valueEnd(text: string, from: number): number {
  const c = text[from];
  if (c === '"') return stringEnd(text, from);
  if (c === "{" || c === "[") {
    const close = c === "{" ? "}" : "]";
    let i = skipSpace(text, from + 1);
    if (text[i] === close) return i + 1;
    for (;;) {
      if (c === "{") {
        i = skipSpace(text, stringEnd(text, i));
        i = skipSpace(text, i + 1); // the colon
      }
      i = skipSpace(text, valueEnd(text, i));
      if (text[i] !== ",") return i + 1;
      i = skipSpace(text, i + 1);
    }
  }
  let i = from;
  while (i < text.length && !",}] \t\r\n".includes(text[i])) i++;
  return i;
}

/** The root value: the object the whole file is. */
function rootSpan(text: string): Span {
  const start = skipSpace(text, 0);
  return { start, end: valueEnd(text, start) };
}

/** The value of a member of the object at `object`; null when it has none by that name. */
function memberSpan(text: string, object: Span, key: string): Span | null {
  let i = skipSpace(text, object.start + 1);
  while (text[i] === '"') {
    const keyEnd = stringEnd(text, i);
    const start = skipSpace(text, skipSpace(text, keyEnd) + 1); // past the colon
    const end = valueEnd(text, start);
    if (JSON.parse(text.slice(i, keyEnd)) === key) return { start, end };
    i = skipSpace(text, end);
    if (text[i] !== ",") break;
    i = skipSpace(text, i + 1);
  }
  return null;
}

// ---------------------------------------------------------------- adding to an object or an array

/** The file's own line ends, and how far it indents. */
function style(text: string): { newline: string; unit: string } {
  return { newline: text.includes("\r\n") ? "\r\n" : "\n", unit: /\n([ \t]+)["[{]/.exec(text)?.[1] ?? "  " };
}

/** A value as the file writes values: indented by the file's unit, continuing lines under `base`. */
function pretty(value: unknown, text: string, base: string): string {
  const { newline, unit } = style(text);
  return JSON.stringify(value, null, unit).replace(/\n/g, newline + base);
}

/**
 * The text with `entry` (`"key": value`, or only the value, for an array)
 * added as the last item of the object or array at `container`: after the
 * last one, in the indentation the others have, and everything else as it
 * was. Written on one line when the container is on one.
 */
function appendTo(text: string, container: Span, entry: (inline: boolean, base: string) => string): string {
  const close = container.end - 1;
  const inner = text.slice(container.start + 1, close);
  const empty = inner.trim() === "";
  const head = text.slice(0, container.start + 1) + (empty ? "" : inner.replace(/\s+$/, ""));
  const tail = text.slice(close);
  if (!/[\r\n]/.test(inner)) return `${head}${empty ? "" : ","}${entry(true, "")}${tail}`;
  const { newline, unit } = style(text);
  // The line the closing bracket is on is indented as the container is; the
  // items, a step further — or as the ones already there.
  const closing = /\n([ \t]*)$/.exec(inner)?.[1] ?? "";
  const indent = /\n([ \t]+)\S/.exec(inner)?.[1] ?? closing + unit;
  return `${head}${empty ? "" : ","}${newline}${indent}${entry(false, indent)}${newline}${closing}${tail}`;
}

const asMember = (key: string, value: unknown, text: string) => (inline: boolean, base: string) =>
  inline ? `${JSON.stringify(key)}:${JSON.stringify(value)}` : `${JSON.stringify(key)}: ${pretty(value, text, base)}`;

const asElement = (value: unknown, text: string) => (inline: boolean, base: string) =>
  inline ? JSON.stringify(value) : pretty(value, text, base);

// ---------------------------------------------------------------- the plan

export interface HookSpec {
  /** The event it answers to: `UserPromptSubmit`. */
  event: string;
  /** What it runs, as the shell Claude Code uses on Windows (Git Bash, or cmd) takes it. */
  command: string;
  /** Seconds Claude Code waits for it. */
  timeout: number;
}

export type HookPlan =
  /** The file to write: the one there with the hook added, or a new one with only it. */
  | { kind: "write"; text: string }
  /** It is there already, as it would be written. */
  | { kind: "ours" }
  /** A hook of the same kind is there already — the person's own, or from another place: left alone. */
  | { kind: "present"; command: string }
  /** Not a file to touch: not JSON, not an object, hooks of a shape this does not know, or an edit that could not be made without touching the rest. */
  | { kind: "unreadable"; why: string };

/** The commands of every hook set for an event, wherever they sit in the groups. */
function commandsOf(groups: unknown[]): string[] {
  const found: string[] = [];
  for (const group of groups) {
    if (!isObject(group) || !Array.isArray(group.hooks)) continue;
    for (const hook of group.hooks) if (isObject(hook) && typeof hook.command === "string") found.push(hook.command);
  }
  return found;
}

/**
 * What to do with a settings file (its text, or null when there is none) so
 * that `spec` runs: nothing when it does already, or when a hook with the
 * same `signature` in its command is there; else a group holding it added to
 * the event's list — the list made when there is none, the `hooks` object
 * made when there is none — after what the person has, every other byte as it
 * was, in the file's own indentation and line ends.
 */
export function planHook(text: string | null, spec: HookSpec, signature: string): HookPlan {
  const group = { hooks: [{ type: "command", command: spec.command, timeout: spec.timeout }] };
  if (text === null || text.replace(/^﻿/, "").trim() === "") {
    const body = JSON.stringify({ hooks: { [spec.event]: [group] } }, null, 2);
    return { kind: "write", text: `${body}\n` };
  }
  const settings = parseSettings(text);
  if (settings === null) return { kind: "unreadable", why: "não é um objeto JSON" };
  let added: string;
  let expected: Json;
  if (!("hooks" in settings)) {
    const value = { [spec.event]: [group] };
    added = appendTo(text, rootSpan(text), asMember("hooks", value, text));
    expected = { ...settings, hooks: value };
  } else if (!isObject(settings.hooks)) {
    return { kind: "unreadable", why: "hooks não é um objeto" };
  } else if (!(spec.event in settings.hooks)) {
    const hooksAt = memberSpan(text, rootSpan(text), "hooks");
    if (!hooksAt) return { kind: "unreadable", why: "não achei hooks no arquivo" };
    added = appendTo(text, hooksAt, asMember(spec.event, [group], text));
    expected = { ...settings, hooks: { ...settings.hooks, [spec.event]: [group] } };
  } else {
    const list = settings.hooks[spec.event];
    if (!Array.isArray(list)) return { kind: "unreadable", why: `hooks.${spec.event} não é uma lista` };
    const commands = commandsOf(list);
    if (commands.includes(spec.command)) return { kind: "ours" };
    const alike = commands.find((c) => c.includes(signature));
    if (alike !== undefined) return { kind: "present", command: alike };
    const hooksAt = memberSpan(text, rootSpan(text), "hooks");
    const listAt = hooksAt && memberSpan(text, hooksAt, spec.event);
    if (!listAt) return { kind: "unreadable", why: `não achei hooks.${spec.event} no arquivo` };
    added = appendTo(text, listAt, asElement(group, text));
    expected = { ...settings, hooks: { ...settings.hooks, [spec.event]: [...list, group] } };
  }
  // What was there must read the same, and the hook must be the one asked for.
  const check = parseSettings(added);
  return check !== null && canonical(check) === canonical(expected)
    ? { kind: "write", text: added }
    : { kind: "unreadable", why: "não deu para acrescentar sem mexer no resto" };
}

// ---------------------------------------------------------------- the account hook

/** What the command ends with, and what tells a hook of this kind whoever wrote it. */
export const ACCOUNT_HOOK_ARGS = "ia conta-da-aba --gancho";

/**
 * `keep ia conta-da-aba --gancho` for the keep at `keep`: the path quoted and
 * in forward slashes, as the status line's — Claude Code runs a hook through
 * Git Bash, where a backslash can escape what follows it, or else through
 * cmd, and both take this.
 */
export function accountHookCommand(keep: string): string {
  return `"${keep.replace(/\\/g, "/")}" ${ACCOUNT_HOOK_ARGS}`;
}

/** What to do with Claude Code's settings so the account hook runs; see `planHook`. */
export function planAccountHook(text: string | null, keep: string): HookPlan {
  if (UNSAFE_IN_PATH.test(keep)) {
    return { kind: "unreadable", why: `o caminho do keep tem caracteres que o shell interpretaria: ${keep}` };
  }
  return planHook(text, { event: "UserPromptSubmit", command: accountHookCommand(keep), timeout: 10 }, ACCOUNT_HOOK_ARGS);
}
