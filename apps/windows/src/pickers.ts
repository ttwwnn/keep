// The pickers of Claude Code and Codex, walked by keys the way a person
// walks them, the screen read before each key. A port of ClaudePicker and
// CodexPicker (apps/macos/Sources/Keep/Model/ClaudeStatus.swift) and of their
// drivers (UI/ClaudeStatusBar.swift) in the macOS app.
//
// - Claude Code: `/model` with no argument opens a list (the arrows to the
//   model, then "s" for this session only) and `/effort` a rule (the arrows
//   along it, then "s"). With an argument, both save the default for new
//   sessions, which the sessions opened without --model follow live; the
//   pickers change this conversation and no other.
// - Codex: `/model` opens a numbered list of models, then one of reasoning
//   levels, with Max and Ultra one step further under "More reasoning…"; a
//   number key chooses at once.
//
// Everything that reads a screen is a pure function of its text. The drivers
// take what they press, read and wait with as an argument (`PickerIO`), so
// they are tested against made-up programs without a terminal.

const ESC = "\x1b";
const UP = "\x1b[A";
const DOWN = "\x1b[B";
const RIGHT = "\x1b[C";
const LEFT = "\x1b[D";

function lastIndex<T>(items: readonly T[], test: (item: T) => boolean): number {
  for (let i = items.length - 1; i >= 0; i--) if (test(items[i])) return i;
  return -1;
}

/** The lines under the last one that says `title`: the picker, without the conversation above it. */
function below(screen: string, title: string): string[] {
  const lines = screen.split("\n");
  return lines.slice(lastIndex(lines, (l) => l.includes(title)) + 1);
}

// ---------------------------------------------------------------- Claude Code

export const CLAUDE_PICKER = {
  modelTitle: "Select model",
  modelSessionOnly: "use this session only",
  effortSessionOnly: "s for this session only",
} as const;

/** The efforts on `/effort`'s rule, lowest first. */
const CLAUDE_LEVELS = ["low", "medium", "high", "xhigh", "max"];

/**
 * The item of `/model`'s list for a model id, or null when it has none: the
 * 1M-context Opus is not listed (its "Opus 5.5" is the 200k one).
 */
export function claudePickerLabel(id: string): string | null {
  switch (id) {
    case "claude-fable-5-1":
      return "Fable 5.1";
    case "claude-opus-5-5":
      return "Opus 5.5";
    case "claude-sonnet-5-5":
      return "Sonnet 5.5";
    case "claude-haiku-5-5":
      return "Haiku 5.5";
    default:
      return null;
  }
}

const CLAUDE_ITEM = /^\s*(❯)?\s*[↑↓]?\s*\d+\.\s+(\S.*)$/;

/**
 * The items of `/model`'s list, in order, and which one the pointer is on:
 * `   ❯ 4.  Sonnet 5.5 ✔           Most efficient…` is "Sonnet 5.5",
 * pointed. The label is what comes before the first double blank. Only the
 * lines under the list's title count: a numbered line above it is the
 * conversation's.
 */
export function claudeModelItems(screen: string): { label: string; pointed: boolean }[] {
  const items: { label: string; pointed: boolean }[] = [];
  for (const line of below(screen, CLAUDE_PICKER.modelTitle)) {
    const match = CLAUDE_ITEM.exec(line);
    if (!match) continue;
    const label = match[2].trim().split("  ")[0].replace(/✔/g, "").trim();
    items.push({ label, pointed: match[1] !== undefined });
  }
  return items;
}

/** Arrow presses from the pointer to `label` (+ down, − up), or null when the list shows no pointer or no such item. */
export function claudeModelSteps(label: string, screen: string): number | null {
  const items = claudeModelItems(screen);
  const from = items.findIndex((i) => i.pointed);
  const to = items.findIndex((i) => i.label === label);
  return from < 0 || to < 0 ? null : to - from;
}

/**
 * Arrow presses (+ right, − left) that take `/effort`'s marker (▲) to
 * `level`, read off the levels written under the marker's rule: the marker
 * is on the level whose name is centred nearest to it.
 */
export function claudeEffortSteps(level: string, screen: string): number | null {
  const lines = screen.split("\n");
  const row = lastIndex(lines, (l) => l.includes("▲"));
  if (row < 0 || row + 1 >= lines.length) return null;
  const marker = Array.from(lines[row]).indexOf("▲");
  const names = Array.from(lines[row + 1]);
  const spans: { name: string; start: number; end: number }[] = [];
  for (let i = 0; i < names.length; ) {
    if (!/\p{L}/u.test(names[i])) {
      i++;
      continue;
    }
    const start = i;
    while (i < names.length && /\p{L}/u.test(names[i])) i++;
    spans.push({ name: names.slice(start, i).join(""), start, end: i });
  }
  const levels = spans.filter((s) => CLAUDE_LEVELS.includes(s.name));
  const target = levels.findIndex((s) => s.name === level);
  if (target < 0) return null;
  let current = 0;
  let nearest = Infinity;
  levels.forEach((s, i) => {
    const distance = Math.abs(Math.floor((s.start + s.end) / 2) - marker);
    if (distance < nearest) {
      nearest = distance;
      current = i;
    }
  });
  return target - current;
}

/**
 * The question Claude Code asks before switching models in a conversation
 * with a cache ("Switch model? … the full history gets re-read"), the
 * pointer on "1. Yes, switch to …".
 */
export function claudeAsksToSwitch(screen: string): boolean {
  return screen.includes("Switch model?") && /❯\s*1\.\s*Yes, switch to/.test(screen);
}

// ---------------------------------------------------------------- Codex

export const CODEX_PICKER = {
  modelsTitle: "Select Model and Effort",
  levelsTitle: "Select Reasoning Level",
  advancedTitle: "Advanced Reasoning",
  moreReasoning: "More reasoning",
} as const;

/** How a level is called in Codex's picker: `Low`, …, `Extra high`, `Max`, `Ultra`. */
export function codexLevelLabel(effort: string): string {
  if (effort === "xhigh") return "Extra high";
  return effort.charAt(0).toUpperCase() + effort.slice(1);
}

/**
 * The number of the choice called `label` (`› 2. GPT-6-Astra (current)` is
 * GPT-6-Astra's 2), under the last line that says `title`, or null when no
 * choice there is called that. The name must end where the label does:
 * `High` is not `Extra high`, and `GPT-6-Sol` is not `GPT-6.1-Sol`.
 */
export function codexChoiceNumber(label: string, screen: string, title: string): number | null {
  const wanted = label.toLowerCase();
  for (const raw of below(screen, title)) {
    let line = raw.trim();
    if (line !== "" && "›>❯»".includes(line.charAt(0))) line = line.slice(1).trim();
    const dot = line.indexOf(".");
    if (dot <= 0 || !/^\d+$/.test(line.slice(0, dot))) continue;
    const name = line.slice(dot + 1).trim().toLowerCase();
    if (!name.startsWith(wanted)) continue;
    const rest = name.slice(wanted.length);
    if (rest === "" || rest.startsWith(" ") || rest.startsWith("(") || rest.startsWith("…")) {
      return Number(line.slice(0, dot));
    }
  }
  return null;
}

/** One of the models Codex offers, as its cache lists them: the slug it goes by, the name on screen, its levels. */
export interface CodexModel {
  slug: string;
  name: string;
  efforts: string[];
  defaultEffort: string | null;
}

// ---------------------------------------------------------------- walking them

/** What a driver presses, reads and waits with: a pane's keyboard and screen, or a test's. */
export interface PickerIO {
  /** Bytes into the program, as if typed. */
  key(keys: string): Promise<void>;
  /** The pane's screen as it stands. */
  screen(): Promise<string>;
  sleep(ms: number): Promise<void>;
  /** A line for the app's log. */
  log(line: string): void;
}

/** How often a screen is looked at while a step waits for it. */
const LOOK_MS = 100;

/**
 * The screen once `test` holds for it and it has stopped changing (the same
 * twice in a row: a frame half drawn is not read), looked at for five
 * seconds; failing that, the last one `test` held for, or null.
 */
async function screenShowing(io: PickerIO, test: (screen: string) => boolean, looks = 50): Promise<string | null> {
  let seen: string | null = null;
  for (let i = 0; i < looks; i++) {
    const shown = await io.screen();
    if (test(shown)) {
      if (shown === seen) return shown;
      seen = shown;
    } else {
      seen = null;
    }
    await io.sleep(LOOK_MS);
  }
  return seen;
}

/**
 * The screen once it differs from `before` (the pointer or the marker moved)
 * and has settled, or as it is after a second — a key pressed the moment a
 * picker opens can be lost, and is then pressed again rather than counted.
 * Null when the picker (`test`) is gone.
 */
async function screenChanged(io: PickerIO, before: string, test: (screen: string) => boolean): Promise<string | null> {
  let last: string | null = null;
  let candidate: string | null = null;
  for (let i = 0; i < 10; i++) {
    await io.sleep(LOOK_MS);
    const shown = await io.screen();
    if (!test(shown)) continue;
    last = shown;
    if (shown === before) continue;
    if (shown === candidate) return shown;
    candidate = shown;
  }
  return candidate ?? last;
}

/** Back out with Esc — `times` of them, a pause apart — and say why. */
async function giveUp(io: PickerIO, why: string, times = 1): Promise<string> {
  io.log(`picker: ${why}; backing out`);
  for (let i = 0; i < times; i++) {
    await io.key(ESC);
    if (times > 1) await io.sleep(300);
  }
  return why;
}

/** Open a picker by its slash command: typed, a pause for the prompt's own menu, then Enter. */
async function open(io: PickerIO, command: string): Promise<void> {
  await io.key(command);
  await io.sleep(250);
  await io.key("\r");
}

/**
 * Claude Code's model, for this conversation only: `/model`, the arrows to
 * the item, then "s"; the question it asks before switching in a
 * conversation with a cache is answered yes — the pick was the person's.
 * Null when it was done; why it backed out otherwise.
 */
export async function chooseClaudeModel(io: PickerIO, label: string): Promise<string | null> {
  const inPicker = (s: string) => s.includes(CLAUDE_PICKER.modelSessionOnly);
  await open(io, "/model");
  let shown = await screenShowing(io, inPicker);
  if (shown === null || !shown.includes(CLAUDE_PICKER.modelTitle)) return giveUp(io, "no model picker");
  for (let i = 0; i < 25; i++) {
    const steps = claudeModelSteps(label, shown);
    if (steps === null) return giveUp(io, `${label} not in the picker`);
    if (steps === 0) break;
    await io.key(steps > 0 ? DOWN : UP);
    const next = await screenChanged(io, shown, inPicker);
    if (next === null) return giveUp(io, "picker gone");
    shown = next;
  }
  if (claudeModelSteps(label, shown) !== 0) return giveUp(io, `pointer never reached ${label}`);
  await io.key("s");
  // The question, when the conversation has a cache. The picker gone twice
  // in a row without it is its absence: one look may catch a frame half drawn.
  let gone = 0;
  for (let i = 0; i < 20 && gone < 2; i++) {
    await io.sleep(250);
    const text = await io.screen();
    if (claudeAsksToSwitch(text)) {
      await io.key("\r");
      io.log(`picker: model ${label}, this session only (switch confirmed)`);
      return null;
    }
    gone = text.includes(CLAUDE_PICKER.modelSessionOnly) ? 0 : gone + 1;
  }
  io.log(`picker: model ${label}, this session only`);
  return null;
}

/**
 * Claude Code's effort, for this conversation only: `/effort`, the arrows
 * along its rule to the level, then "s".
 */
export async function chooseClaudeEffort(io: PickerIO, level: string): Promise<string | null> {
  const inPicker = (s: string) => s.includes(CLAUDE_PICKER.effortSessionOnly);
  await open(io, "/effort");
  let shown = await screenShowing(io, inPicker);
  if (shown === null) return giveUp(io, "no effort picker");
  for (let i = 0; i < 10; i++) {
    const steps = claudeEffortSteps(level, shown);
    if (steps === null) return giveUp(io, `${level} not on the rule`);
    if (steps === 0) break;
    await io.key(steps > 0 ? RIGHT : LEFT);
    const next = await screenChanged(io, shown, inPicker);
    if (next === null) return giveUp(io, "picker gone");
    shown = next;
  }
  if (claudeEffortSteps(level, shown) !== 0) return giveUp(io, `marker never reached ${level}`);
  await io.key("s");
  io.log(`picker: effort ${level}, this session only`);
  return null;
}

/**
 * Codex's model and level through its own picker: `/model`, the model's
 * number, then the level's — by way of "More reasoning…" for the ones under
 * it. Each step waits for the screen it needs; one that does not come backs
 * out with Esc rather than pressing on blind.
 */
export async function chooseCodex(io: PickerIO, model: CodexModel, effort: string): Promise<string | null> {
  const showing = (title: string) => (s: string) => s.includes(title);
  await open(io, "/model");
  const list = await screenShowing(io, showing(CODEX_PICKER.modelsTitle));
  if (list === null) return giveUp(io, "no model list", 3);
  const n = codexChoiceNumber(model.name, list, CODEX_PICKER.modelsTitle);
  if (n === null || n >= 10) return giveUp(io, `${model.name} not in the list`, 3);
  await io.key(String(n));
  const levels = await screenShowing(io, showing(CODEX_PICKER.levelsTitle));
  if (levels === null) return giveUp(io, "no levels", 3);
  const label = codexLevelLabel(effort);
  const m = codexChoiceNumber(label, levels, CODEX_PICKER.levelsTitle);
  if (m !== null && m < 10) {
    await io.key(String(m));
    io.log(`picker: ${model.slug} ${effort}`);
    return null;
  }
  const more = codexChoiceNumber(CODEX_PICKER.moreReasoning, levels, CODEX_PICKER.levelsTitle);
  if (more === null || more >= 10) return giveUp(io, `${label} not offered`, 3);
  await io.key(String(more));
  const advanced = await screenShowing(io, showing(CODEX_PICKER.advancedTitle));
  const a = advanced === null ? null : codexChoiceNumber(label, advanced, CODEX_PICKER.advancedTitle);
  if (a === null || a >= 10) return giveUp(io, `${label} not under More reasoning`, 3);
  await io.key(String(a));
  io.log(`picker: ${model.slug} ${effort}`);
  return null;
}
