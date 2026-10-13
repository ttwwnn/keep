// The bar under a Claude Code or Codex conversation: the model and the effort
// it runs with, how many agents it has going, and what a click may change.
//
// A port of ClaudeStatus.swift in the macOS app (apps/macos/Sources/Keep/Model).
// Everything here is a pure function of text (a status file, a screen, Codex's
// model cache), so the rules are tested without a Windows, a daemon or a
// terminal; store.ts only gathers the text.
//
// Where the facts come from:
//  - Claude Code: its status line command (tools/keep-statusline.ps1) saves
//    the JSON Claude Code hands it under the number of the tab's shell; the
//    model and the effort are read from there. The agents are counted off the
//    screen, where Claude Code lists them under the prompt.
//  - Codex: it has no status line command, so the model and the effort are
//    read off its own screen (the footer, the header it prints at the start,
//    the line it prints when the model changes), and its model cache
//    (`models_cache.json`) says what each model is called and the levels it
//    takes.
//
// A pick in the bar walks the program's own picker (pickers.ts).

import type { ClaudeActivity } from "./claude";
import { claudePickerLabel, type CodexModel } from "./pickers";

export type { CodexModel } from "./pickers";

export type StatusProgram = "claude" | "codex";

/** What the bar shows for one pane. */
export interface AiStatus {
  program: StatusProgram;
  /** What identifies the model: Claude Code's id (`claude-opus-5-5[1m]`), Codex's slug (`gpt-6-astra`). */
  modelId: string;
  /** How it is called on screen (`Opus 5.5 (1M)`). */
  modelName: string;
  /** `low`, `medium`, `high`, `xhigh` or `max` (Codex: also `ultra`, `minimal`, `none`); null when none was said. */
  effort: string | null;
  /** How many agents run; null where they are not counted (Codex). */
  agents: number | null;
  /** Whether a command can be typed into the prompt now: the turn is over and nothing is in the way. */
  canType: boolean;
}

/** Said in a menu whose items are off. */
export const CANNOT_TYPE_NOTE = "Disponível com o turno terminado e o prompt vazio";
/** Said in a menu while a pick is being walked through the program's picker. */
export const PICKING_NOTE = "Trocando pelo seletor do programa…";

// ---------------------------------------------------------------- the screen

const RULE = "─".repeat(12);
const isRule = (line: string) => line.trim().startsWith(RULE);

function lastIndex<T>(items: readonly T[], test: (item: T) => boolean, end = items.length): number {
  for (let i = Math.min(end, items.length) - 1; i >= 0; i--) if (test(items[i])) return i;
  return -1;
}

/**
 * How many agents Claude Code has running, as it lists them under the prompt
 * (`◯ general-purpose …` for each) or counts them when the list is folded
 * (`· 2 local agents`).
 */
export function countAgents(screen: string): number {
  const lines = screen.split("\n");
  const rule = lastIndex(lines, isRule);
  if (rule < 0) return 0;
  const footer = lines.slice(rule + 1).map((l) => l.trim());
  const listed = footer.filter((l) => l.startsWith("◯")).length;
  let counted = 0;
  for (const line of footer) {
    const match = /(\d+) local agents?/.exec(line);
    if (match) counted = Math.max(counted, Number(match[1]));
  }
  return Math.max(listed, counted);
}

/** Whether Claude Code's prompt box holds nothing: `❯` and no text after it. */
export function claudePromptIsEmpty(screen: string): boolean {
  const lines = screen.split("\n");
  const rule = lastIndex(lines, isRule);
  if (rule < 0) return false;
  const opening = lastIndex(lines, isRule, rule);
  if (opening < 0) return false;
  const first = lines.slice(opening + 1, rule).find((l) => l.trim() !== "");
  if (first === undefined) return false;
  const line = first.trim();
  return line === "❯" || line === ">";
}

// ---------------------------------------------------------------- Claude Code

const record = (v: unknown): Record<string, unknown> | null =>
  typeof v === "object" && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;

export interface ClaudeFile {
  modelId: string;
  modelName: string;
  effort: string | null;
}

/** `Opus 5.5 (1M context)` as `Opus 5.5 (1M)`. */
export function shortModelName(display: string): string {
  return display.split(" context)").join(")");
}

/**
 * What the status line command saved: Claude Code's JSON, of which the model
 * (`id`, `display_name`) and the effort (`level`) matter. Null for anything
 * that does not say a model — a file half written, an old version.
 */
export function parseStatusFile(text: string): ClaudeFile | null {
  let json: unknown;
  try {
    // PowerShell 5 writes a byte order mark unless told not to.
    json = JSON.parse(text.replace(/^\uFEFF/, ""));
  } catch {
    return null;
  }
  const root = record(json);
  const model = record(root?.model);
  if (!root || !model) return null;
  const id = typeof model.id === "string" ? model.id : "";
  const display = typeof model.display_name === "string" ? model.display_name : id;
  if (id === "" && display === "") return null;
  const level = record(root.effort)?.level;
  return { modelId: id, modelName: shortModelName(display), effort: typeof level === "string" && level ? level : null };
}

/** The status of a Claude Code pane, or null when its status line has not said (yet). */
export function claudeStatus(
  file: ClaudeFile | null,
  screen: string,
  activity: ClaudeActivity | undefined,
): AiStatus | null {
  if (!file) return null;
  return {
    program: "claude",
    modelId: file.modelId,
    modelName: file.modelName,
    effort: file.effort,
    agents: countAgents(screen),
    canType: activity === "done" && claudePromptIsEmpty(screen),
  };
}

/** The efforts `/effort` takes, lowest first. */
export const CLAUDE_EFFORTS = ["low", "medium", "high", "xhigh", "max"] as const;

/** The models offered for `/model`: what to show, what to type. */
export const CLAUDE_MODELS: readonly { name: string; id: string }[] = [
  { name: "Fable 5.1", id: "claude-fable-5-1" },
  { name: "Opus 5.5", id: "claude-opus-5-5" },
  { name: "Opus 5.5 (1M)", id: "claude-opus-5-5[1m]" },
  { name: "Sonnet 5.5", id: "claude-sonnet-5-5" },
  { name: "Haiku 5.5", id: "claude-haiku-5-5" },
];

// ---------------------------------------------------------------- Codex

const CODEX_EFFORT = "minimal|low|medium|high|xhigh|max|ultra|none";
/** A model's name as Codex prints it (`gpt-5.1-codex-max`, `o3`, `gpt-oss:20b`): it has a digit or a hyphen. */
const CODEX_MODEL = String.raw`[A-Za-z][\w.:/-]*`;

const hasDigitOrHyphen = (token: string) => /[\d-]/.test(token);

// `gpt-6.1-sol high · 100% context left`, `gpt-5.2-codex xhigh fast · 80% left · ~/proj`
const CODEX_FOOTER = new RegExp(String.raw`^(${CODEX_MODEL})\s+(${CODEX_EFFORT})(?:\s+fast)?(?:\s*·|\s{2,}|\s*$)`, "i");
// `gpt-4.1 · 100% context left`: a model that has no effort to say
const CODEX_FOOTER_PLAIN = new RegExp(String.raw`^(${CODEX_MODEL})\s*·\s*\d+% `, "i");
// `│ model:     gpt-5-codex high   /model to change │`
const CODEX_HEADER = new RegExp(String.raw`^│\s*model:\s+(${CODEX_MODEL})(?:\s+(${CODEX_EFFORT})\b)?`, "i");
// `• Model changed to gpt-5.1-codex-max high`
const CODEX_CHANGED = new RegExp(
  String.raw`^[•●◦∙*]?\s*Model changed to\s+(${CODEX_MODEL})(?:\s+(?:with\s+|\(\s*)?(${CODEX_EFFORT})\b)?`,
  "i",
);

const CODEX_CHOICE = /^[❯›»]\s*\d+[.)]\s+/;
const isCodexPrompt = (line: string) => (line.startsWith("›") || line.startsWith("»")) && !CODEX_CHOICE.test(line);

/** What a Codex screen says: the model as it is written there, and the effort. */
export interface CodexSaid {
  model: string;
  effort: string | null;
}

function codexMatch(m: RegExpExecArray | null): CodexSaid | null {
  if (!m || !hasDigitOrHyphen(m[1])) return null;
  return { model: m[1], effort: m[2] ? m[2].toLowerCase() : null };
}

/**
 * The model and the effort a Codex screen says. The footer under the prompt
 * is the live one (`gpt-6.1-sol high · 100% context left`); without it, the
 * last thing Codex printed about it in the conversation: the header box it
 * opens with, or the line it prints when the model changes.
 */
export function readCodexModel(screen: string): CodexSaid | null {
  const lines = screen.split("\n").map((l) => l.trim());
  const prompt = lastIndex(lines, isCodexPrompt);
  if (prompt >= 0) {
    for (const line of lines.slice(prompt + 1)) {
      const said = codexMatch(CODEX_FOOTER.exec(line)) ?? codexMatch(CODEX_FOOTER_PLAIN.exec(line));
      if (said) return said;
    }
  }
  const history = prompt >= 0 ? lines.slice(0, prompt) : lines;
  for (let i = history.length - 1; i >= 0; i--) {
    const said = codexMatch(CODEX_HEADER.exec(history[i])) ?? codexMatch(CODEX_CHANGED.exec(history[i]));
    if (said) return said;
  }
  return null;
}

/** What Codex puts in its empty prompt as a hint, dim, in the screen's text like anything typed. */
const CODEX_HINTS =
  /^(?:ask codex to do anything|explain this codebase|summarize recent commits|implement \{feature\}|find and fix a bug in @filename|write tests for @filename|improve documentation in @filename|run \/review on my current changes|use \/skills to list available skills)$/i;

/**
 * Whether Codex's prompt holds nothing: its `›` bare, or with the hint it
 * shows in an empty box. A hint this list does not know reads as a draft,
 * which leaves the menu off rather than typing into somebody's text.
 */
export function codexPromptIsEmpty(screen: string): boolean {
  const lines = screen.split("\n").map((l) => l.trim());
  const prompt = lastIndex(lines, isCodexPrompt);
  if (prompt < 0) return false;
  const text = lines[prompt].slice(1).trim();
  return text === "" || CODEX_HINTS.test(text);
}

/** The efforts Codex's levels go by, for a model whose levels its cache does not say. */
export const CODEX_EFFORTS = ["low", "medium", "high", "xhigh", "max", "ultra"] as const;

/**
 * The models Codex offers, in its order, from its cache
 * (`$CODEX_HOME/models_cache.json`): the ones it lists (`visibility: "list"`),
 * by `priority`, each with its slug, its name on screen and the levels it
 * takes. None for a file that is not one.
 */
export function parseCodexModels(text: string): CodexModel[] {
  let json: unknown;
  try {
    json = JSON.parse(text.replace(/^\uFEFF/, ""));
  } catch {
    return [];
  }
  const list = record(json)?.models;
  if (!Array.isArray(list)) return [];
  const found: { priority: number; at: number; model: CodexModel }[] = [];
  list.forEach((item, at) => {
    const m = record(item);
    if (!m || typeof m.slug !== "string" || m.slug === "" || m.visibility !== "list") return;
    const levels = Array.isArray(m.supported_reasoning_levels) ? m.supported_reasoning_levels : [];
    const efforts = levels
      .map((level) => (typeof level === "string" ? level : record(level)?.effort))
      .filter((effort): effort is string => typeof effort === "string" && effort !== "");
    const name = typeof m.display_name === "string" && m.display_name !== "" ? m.display_name : m.slug;
    const defaultEffort = typeof m.default_reasoning_level === "string" ? m.default_reasoning_level : null;
    found.push({
      priority: typeof m.priority === "number" ? m.priority : 1000,
      at,
      model: { slug: m.slug, name, efforts, defaultEffort },
    });
  });
  return found.sort((a, b) => a.priority - b.priority || a.at - b.at).map((f) => f.model);
}

/** The model a name on screen stands for: its slug or its display name, in any case. */
export function codexModelNamed(name: string, models: readonly CodexModel[]): CodexModel | null {
  const wanted = name.toLowerCase();
  return models.find((m) => m.slug.toLowerCase() === wanted || m.name.toLowerCase() === wanted) ?? null;
}

/**
 * The status of a Codex pane: from its screen, or from what was last read
 * (`kept`) when the screen shows nothing of it now; the cache gives the slug
 * and the name for the model it said.
 */
export function codexStatus(
  screen: string,
  activity: ClaudeActivity | undefined,
  kept?: CodexSaid | null,
  models: readonly CodexModel[] = [],
): AiStatus | null {
  const said = readCodexModel(screen) ?? kept ?? null;
  if (!said) return null;
  const model = codexModelNamed(said.model, models);
  return {
    program: "codex",
    modelId: model?.slug ?? said.model.toLowerCase(),
    modelName: model?.name ?? said.model,
    effort: said.effort,
    agents: null,
    canType: activity === "done" && codexPromptIsEmpty(screen),
  };
}

export function sameStatus(a: AiStatus | undefined, b: AiStatus | undefined): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.program === b.program &&
    a.modelId === b.modelId &&
    a.modelName === b.modelName &&
    a.effort === b.effort &&
    a.agents === b.agents &&
    a.canType === b.canType
  );
}

// ---------------------------------------------------------------- the clicks

/** What a pick in the bar does. */
export type BarAction =
  /** Claude Code's `/model` list: the pointer taken to this item, then "s" — this conversation only. */
  | { kind: "claudeModel"; label: string }
  /** Claude Code's `/effort` rule: the marker taken to this level, then "s" — this conversation only. */
  | { kind: "claudeEffort"; level: string }
  /**
   * Typed into the prompt, then Enter. `keepDefaults`: what it saves as
   * Claude Code's default for new sessions is put back afterwards.
   */
  | { kind: "type"; command: string; keepDefaults: boolean }
  /** Codex's own picker, walked to this model and level. */
  | { kind: "codex"; model: CodexModel; effort: string };

/** One choice in the bar's menus: what to show, whether it is the one in use, what picking it does. */
export interface Choice {
  label: string;
  current: boolean;
  action: BarAction;
}

/**
 * The effort menu. Claude Code's five levels, each through its `/effort`
 * rule; Codex's, the levels its cache says the model takes (all of them when
 * it does not say), each through its picker with the model kept.
 */
export function effortChoices(status: AiStatus, models: readonly CodexModel[] = []): Choice[] {
  if (status.program === "codex") {
    const model = codexModelNamed(status.modelId, models) ?? {
      slug: status.modelId,
      name: status.modelName,
      efforts: [],
      defaultEffort: null,
    };
    const efforts = model.efforts.length > 0 ? model.efforts : [...CODEX_EFFORTS];
    return efforts.map((effort) => ({
      label: effort,
      current: effort === status.effort,
      action: { kind: "codex", model, effort },
    }));
  }
  return CLAUDE_EFFORTS.map((level) => ({
    label: level,
    current: level === status.effort,
    action: { kind: "claudeEffort", level },
  }));
}

/**
 * The model menu. Claude Code's models through its `/model` list — but the
 * 1M-context Opus, which the list does not have, by `/model <id>` with the
 * default it saves put back. Codex's, the ones its cache lists, through its
 * picker; with no cache, its picker opened for the person to choose in.
 */
export function modelChoices(status: AiStatus, models: readonly CodexModel[] = []): Choice[] {
  if (status.program === "codex") {
    if (models.length === 0) {
      return [
        {
          label: "Escolher no seletor do Codex (/model)",
          current: false,
          action: { kind: "type", command: "/model", keepDefaults: false },
        },
      ];
    }
    return models.map((model) => ({
      label: model.name,
      current: model.slug === status.modelId,
      action: { kind: "codex", model, effort: codexEffortFor(model, status.effort) },
    }));
  }
  return CLAUDE_MODELS.map((m) => {
    const label = claudePickerLabel(m.id);
    const action: BarAction = label
      ? { kind: "claudeModel", label }
      : { kind: "type", command: `/model ${m.id}`, keepDefaults: true };
    return { label: m.name, current: m.id === status.modelId, action };
  });
}

/** The level a Codex model is changed to: the one in use when it takes it, else its own default. */
export function codexEffortFor(model: CodexModel, effort: string | null): string {
  if (effort !== null && model.efforts.includes(effort)) return effort;
  return model.defaultEffort ?? "medium";
}

/** The note a menu opens with when its items are off, or null when they are on. */
export function menuNote(status: AiStatus, picking = false): string | null {
  if (picking) return PICKING_NOTE;
  return status.canType ? null : CANNOT_TYPE_NOTE;
}

/** `esforço high` — the chip's text. */
export const effortLabel = (effort: string) => `esforço ${effort}`;

/** `1 agente`, `3 agentes`. */
export const agentsLabel = (n: number) => (n === 1 ? "1 agente" : `${n} agentes`);

/** The pause between the typed command and its Enter, as on macOS: the prompt's own menu opens in between. */
export const ENTER_DELAY_MS = 200;
