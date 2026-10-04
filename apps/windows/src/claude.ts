// Where Claude Code (or Codex) stands in a tab, read off its screen.
//
// A port of ClaudeMode and ClaudeActivity in the macOS app
// (apps/macos/Sources/Keep/Model/Snapshots.swift) and of the colours it
// titles a tab in (UI/Glass.swift). Same rules, same reasons: Claude Code
// tells no terminal when its mode changes or its turn ends, but it writes
// both on screen, under its prompt box, the moment they happen — and every
// tab here is attached, so its screen is right there in the terminal's buffer.

/** The permission modes Claude Code gives a colour of its own. Manual has none. */
export type ClaudeMode = "plan" | "acceptEdits" | "bypass" | "auto";

/** Where a turn stands. */
export type ClaudeActivity =
  /** A turn is running: the mode's colour. */
  | "working"
  /** The turn ended with work still running behind it: workflows, agents, shells. */
  | "waitingForWorkflow"
  /** A question, a permission or a dialog waits on an answer. */
  | "waitingForYou"
  /** The turn is over. */
  | "done";

const RULE = "─".repeat(12);

const isRule = (line: string) => line.trim().startsWith(RULE);

function findLastIndex<T>(items: readonly T[], test: (item: T) => boolean, end = items.length): number {
  for (let i = Math.min(end, items.length) - 1; i >= 0; i--) {
    if (test(items[i])) return i;
  }
  return -1;
}

/** Split, leaving out the empty pieces — Swift's `split(separator:)`. */
function splitOmittingEmpty(text: string, separator: string): string[] {
  return text.split(separator).filter((piece) => piece !== "");
}

/** A count and what it counts: `5 shells`, `1 MCP task` — split at the first space. */
function countAndKind(item: string): string[] {
  const match = /^(\S+) +(.+)$/.exec(item);
  return match ? [match[1], match[2]] : [item];
}

/**
 * The mode a screen's footer declares: a mode, `null` for manual (a footer
 * that names none), or `undefined` when the screen cannot say — a dialog in
 * front, or not Claude Code at all — which leaves what was known standing.
 *
 * Only the first line under the rule closing the prompt box is read: the same
 * words anywhere else on screen are conversation, not a mode.
 */
export function declaredMode(text: string): ClaudeMode | null | undefined {
  const lines = text.split("\n");
  const rule = findLastIndex(lines, isRule);
  if (rule < 0) return undefined;
  const opening = findLastIndex(lines, isRule, rule);
  if (opening < 0) return undefined;
  const first = lines.slice(opening + 1, rule).find((l) => l.trim() !== "");
  if (first === undefined) return undefined;
  const opened = first.trim();
  if (!opened.startsWith("❯") && !opened.startsWith(">")) return undefined;
  const footer = lines.slice(rule + 1).find((l) => l.trim() !== "");
  if (footer === undefined) return null;
  const said = footer.trim();
  if (said.startsWith("⏸ plan mode on")) return "plan";
  if (said.startsWith("⏵⏵ accept edits on")) return "acceptEdits";
  if (said.startsWith("⏵⏵ bypass permissions on")) return "bypass";
  if (said.startsWith("⏵⏵ don't ask on")) return "bypass";
  if (said.startsWith("⏵⏵ auto mode on")) return "auto";
  return null;
}

/**
 * The kinds of background work Claude Code counts under its box that bring it
 * back when they end. An Artifact comment monitor waits on people, cloud
 * sessions run on without it, and teams, dreaming and the auto-mode scan are
 * not work a turn handed off.
 */
const BACKGROUND_WORK = new Set([
  "shell",
  "shells",
  "monitor",
  "monitors",
  "local agent",
  "local agents",
  "background dynamic workflow",
  "background dynamic workflows",
  "background task",
  "background tasks",
  "MCP task",
  "MCP tasks",
]);

/** `2 shells`, `5 shells, 1 monitor`: one `·` part of the footer, every item a count of running work. */
export function isRunningBackgroundWork(line: string): boolean {
  return splitOmittingEmpty(line, "·").some((part) => {
    const items = splitOmittingEmpty(part, ",").map((i) => i.trim());
    return (
      items.length > 0 &&
      items.every((item) => {
        const words = countAndKind(item);
        return words.length === 2 && /^\d+$/.test(words[0]) && BACKGROUND_WORK.has(words[1]);
      })
    );
  });
}

/** `❯ 1. Yes`: the pointer on one of Claude Code's numbered choices. */
function isChoicePointer(line: string): boolean {
  let rest = line.replace(/^ +/, "");
  if (!rest.startsWith("❯")) return false;
  rest = rest.slice(1).replace(/^ +/, "");
  const digits = /^\d+/.exec(rest);
  return digits !== null && rest.charAt(digits[0].length) === ".";
}

const SPINNER_GLYPHS = new Set(["✻", "✳", "✢", "✶", "✽", "·", "*"]);

const isUppercase = (c: string) => c !== c.toLowerCase() && c === c.toUpperCase();

/** `✻ Baked for 23s`, `✶ Roosting… (16m)`: a spinner glyph and a capitalised word. */
function isStatusLine(line: string): boolean {
  let rest = line.replace(/^ +/, "");
  const glyph = rest.charAt(0);
  if (!SPINNER_GLYPHS.has(glyph)) return false;
  rest = rest.slice(1);
  if (rest.charAt(0) !== " ") return false;
  rest = rest.replace(/^ +/, "");
  return rest.length > 0 && isUppercase(rest.charAt(0));
}

/**
 * Where Claude Code's turn stands, or `undefined` when the screen says nothing
 * that can be told apart — not Claude Code, or caught mid-redraw.
 */
export function readActivity(text: string): ClaudeActivity | undefined {
  const lines = text.split("\n");
  const trimmed = (l: string) => l.trim();
  const isPrompt = (l: string) => {
    const t = trimmed(l);
    return t.startsWith("❯") || t.startsWith(">");
  };
  // A dialog first: Claude Code ends most it draws with `Esc to cancel`.
  const tail = lines.filter((l) => l.trim() !== "").slice(-4);
  if (tail.some((l) => l.includes("Esc to cancel"))) return "waitingForYou";
  // The prompt box: a rule, the prompt, a rule.
  const rule = findLastIndex(lines, isRule);
  const opening = rule >= 0 ? findLastIndex(lines, isRule, rule) : -1;
  const first = opening >= 0 ? lines.slice(opening + 1, rule).find((l) => trimmed(l) !== "") : undefined;
  if (rule < 0 || opening < 0 || first === undefined || !isPrompt(first)) {
    // No box: a plan's approval draws its choices without `Esc to cancel`.
    if (lines.some(isChoicePointer)) return "waitingForYou";
    return undefined;
  }
  const footer = lines.slice(rule + 1);
  // An agent's conversation or the detailed transcript says nothing of the main turn.
  if (
    trimmed(first).startsWith("❯ Message @") ||
    footer.some((l) => l.includes("stop all agents") || l.includes("Showing detailed transcript"))
  ) {
    return undefined;
  }
  if (footer.some((l) => l.toLowerCase().includes("esc to interrupt"))) return "working";
  if (footer.some(isRunningBackgroundWork)) return "waitingForWorkflow";
  const conversation = lines.slice(0, opening);
  if (conversation.some((l) => l.includes("Jump to bottom") || l.includes(" new message"))) return undefined;
  const at = findLastIndex(conversation, isStatusLine);
  if (at < 0) return "done";
  const status = conversation[at];
  if (status.includes("Waiting for") && status.includes("to finish")) {
    return footer.some((l) => trimmed(l).startsWith("◯")) ? "waitingForWorkflow" : "done";
  }
  if (status.includes("…")) return "working";
  return "done";
}

const CODEX_CHOICE = /^[❯›»]\s*\d+[.)]\s+/;
const CODEX_PROMPT_WORDS =
  /\benter\b.{0,16}\b(confirm|submit|continue)\b|\besc(?:ape)?\s+(?:to\s+)?(?:cancel|back)\b/i;
const CODEX_WORKING = /^(?:[•●◦∙*]\s*)?(?:Working|Workflow)(?:\s*\(.*\)|\s*…|\s*\.{3})?\s*$/;

/**
 * Codex's turn, read the same way. Its final `Working` above the prompt is
 * the workflow's blue; older mentions in the transcript do not keep it.
 */
export function readCodexActivity(text: string): ClaudeActivity | undefined {
  const rawLines = text.split("\n");
  const lines = rawLines.map((l) => l.trim());
  if (lines.some((l) => l.includes("Jump to bottom"))) return undefined;
  const choice = (line: string) => CODEX_CHOICE.test(line);
  const input = findLastIndex(lines, (l) => (l.startsWith("»") || l.startsWith("›")) && !choice(l));
  const footer = input >= 0 ? lines.slice(input + 1) : lines;
  const tail = footer.filter((l) => l !== "").slice(-12);
  if (tail.some((line) => choice(line) || CODEX_PROMPT_WORDS.test(line))) return "waitingForYou";
  if (input < 0) return undefined;
  // Codex puts queued USER messages below Working, under a heading of its
  // own. That is not an assistant reply.
  let queue: number | undefined;
  const queueStart = findLastIndex(
    rawLines,
    (l) => l.startsWith("• Messages to be submitted after next tool call"),
    input,
  );
  if (queueStart >= 0) {
    const between = rawLines.slice(queueStart + 1, input);
    if (between.every((l) => l.trim() === "" || /^\s/.test(l))) queue = queueStart;
  }
  const end = queue ?? input;
  const conversation = lines.slice(0, end).filter((l) => l !== "" && !l.startsWith(RULE));
  if (conversation.length > 0 && conversation[conversation.length - 1].startsWith("└ Tip:")) conversation.pop();
  const last = conversation[conversation.length - 1];
  if (last === undefined) return queue === undefined ? "done" : "working";
  if (CODEX_WORKING.test(last)) return "waitingForWorkflow";
  if (queue !== undefined || lines.slice(input + 1).some((l) => l.toLowerCase().includes("esc to interrupt"))) {
    return "working";
  }
  // A direct question in the final assistant reply also needs an answer —
  // never inferred from quoted examples or the user's own box.
  const user = findLastIndex(rawLines, (l) => l.startsWith("›") || l.startsWith("»"), end);
  const reply = findLastIndex(rawLines, (l) => l.startsWith("• "), end);
  if (reply >= 0 && reply > user) {
    const message = lines.slice(reply, end);
    const fences = message.filter((l) => l.startsWith("```")).length;
    if (last.endsWith("?") && ![">", "$", "↳", "›", "»"].some((p) => last.startsWith(p)) && fences % 2 === 0) {
      return "waitingForYou";
    }
  }
  return "done";
}

// ---------------------------------------------------------------- what runs

/** The marks Claude Code writes at the head of every title: its `✳` and its spinner. */
const SPINNER_MARKS = new Set(["✳", "◐", "◑", "◒", "◓", "✻", "✽"]);

function isVersionNumber(name: string): boolean {
  const parts = name.split(".");
  return parts.length >= 2 && parts.every((p) => /^\d+$/.test(p));
}

/**
 * What runs in a tab, with a guess for when the daemon's name says nothing:
 * Claude Code's installer names each release after its version (`2.1.281`),
 * and npm installs run it under `node` — behind a title wearing its marks, or
 * saying "Claude Code", either is Claude Code.
 */
export function programOf(command: string, title: string): string {
  const wearsMarks = title.length > 0 && SPINNER_MARKS.has(Array.from(title)[0]);
  const name = command.toLowerCase();
  if (name) {
    if (wearsMarks && isVersionNumber(name)) return "claude";
    if ((name === "node" || name === "bun") && (wearsMarks || /claude code/i.test(title))) return "claude";
    return name;
  }
  return wearsMarks ? "claude" : "";
}

export const isAiProgram = (program: string) => program === "claude" || program === "codex";

/** A title with the busy marks its program put there taken off. */
export function plainTitle(title: string, fallback: string): string {
  const chars = Array.from(title);
  let at = 0;
  while (at < chars.length && (SPINNER_MARKS.has(chars[at]) || /\s/.test(chars[at]))) at++;
  const rest = chars.slice(at).join("").trim();
  return rest === "" ? fallback : rest;
}

/** Whether a tab is doing something rather than having something open. */
export function isAtWork(activity: ClaudeActivity | undefined, busy: boolean): boolean {
  switch (activity) {
    case "working":
    case "waitingForWorkflow":
      return true;
    case "done":
    case "waitingForYou":
      return false;
    default:
      return busy;
  }
}

// ---------------------------------------------------------------- colours

export type Rgb = readonly [number, number, number];

/** The colour Claude Code gives a mode under its prompt, from its own themes. */
export function modeColor(mode: ClaudeMode, dark: boolean): Rgb {
  switch (mode) {
    case "plan":
      return dark ? [72, 150, 140] : [0, 102, 102];
    case "acceptEdits":
      return dark ? [175, 135, 255] : [135, 0, 255];
    case "bypass":
      return dark ? [255, 107, 128] : [171, 43, 63];
    case "auto":
      return dark ? [255, 193, 7] : [150, 108, 30];
  }
}

/** Printed on the badge of a tab waiting on you, at either end of a breath. */
export const ATTENTION_INK: Rgb = [33, 22, 12];

/**
 * A title's ink by where its turn stands: none while it runs (the mode's
 * colour), a calm blue while work it handed off runs, the badge's ink while
 * it waits on you, Claude Code's own grey once it is over.
 */
export function activityColor(activity: ClaudeActivity, dark: boolean): Rgb | null {
  switch (activity) {
    case "working":
      return null;
    case "waitingForWorkflow":
      return dark ? [122, 180, 232] : [37, 99, 235];
    case "waitingForYou":
      return ATTENTION_INK;
    case "done":
      return dark ? [153, 153, 153] : [102, 102, 102];
  }
}

/** The badge's ground: Claude Code's vivid orange. */
export const attentionFill = (dark: boolean): Rgb => (dark ? [255, 120, 20] : [255, 106, 0]);

/** The far end of a breath: the same orange with light let into it. */
export const attentionGlow = (dark: boolean): Rgb => (dark ? [255, 178, 112] : [255, 166, 96]);

export const selectionEdge = (dark: boolean): Rgb => (dark ? [255, 255, 255] : [40, 30, 20]);

function blend(a: Rgb, b: Rgb, fraction: number): Rgb {
  return [
    Math.round(a[0] + (b[0] - a[0]) * fraction),
    Math.round(a[1] + (b[1] - a[1]) * fraction),
    Math.round(a[2] + (b[2] - a[2]) * fraction),
  ];
}

/** Selected waiting titles keep full contrast; the other orange badges are softer. */
export const attentionTitleInk = (selected: boolean): Rgb =>
  selected ? ATTENTION_INK : blend(ATTENTION_INK, [150, 70, 15], 0.22);

/** One breath, fill to glow and back, in seconds. */
export const BREATH_PERIOD = 1.8;

/** How long a badge breathes once its tab starts waiting. */
export const BREATHES_FOR = 60;

/** Where the breath is at a moment (seconds), on one clock for every badge. */
export function breathPhase(seconds: number): number {
  const t = (((seconds % BREATH_PERIOD) + BREATH_PERIOD) % BREATH_PERIOD) / BREATH_PERIOD;
  return (1 - Math.cos(2 * Math.PI * t)) / 2;
}

/** The badge's colour at a phase of the breath. */
export function breathColor(dark: boolean, phase: number): Rgb {
  return blend(attentionFill(dark), attentionGlow(dark), Math.max(0, Math.min(1, phase)));
}

/** When a badge that started waiting at `since` (seconds) stops breathing: the end of a breath. */
export function lastBreath(since: number | undefined): number | undefined {
  if (since === undefined) return undefined;
  const end = since + BREATHES_FOR;
  return Math.ceil(end / BREATH_PERIOD) * BREATH_PERIOD;
}

export const css = (c: Rgb, alpha = 1) =>
  alpha === 1 ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${alpha})`;
