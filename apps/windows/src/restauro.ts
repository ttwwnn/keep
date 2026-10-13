// What the core says about the tabs coming back after a restart (`keep
// restauro`, docs/restauro.md) and about the conversations a dropped login,
// network or usage limit stopped (`keep continuar`, docs/restauro.md): the
// window runs both on their clocks and says only what is worth saying.
//
// Pure functions of the core's answers; iaChores.ts asks.

import type { CoreAnswer } from "./api";

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const count = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);
const texts = (v: unknown) => (Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : []);

/** A tab the restore made, and the number it had in the daemon of before. */
export interface Made {
  workspace: string;
  from: number;
  to: number;
}

/** The tabs a restore brought back, under their new numbers. */
export interface Restored {
  made: Made[];
}

/** What one round of `keep restauro` did. */
export interface RestoreRound {
  /** Tabs it made again. */
  tabs: number;
  /** Which, under what numbers; only in the round that made them. */
  made: Made[];
  /** Conversations it brought back. */
  conversations: number;
  /** What it could not do, one line each. */
  failures: string[];
  /** Why it did what it did, or nothing: "já restaurado neste keepd", "nada a restaurar". */
  reason: string;
  /** The core did not answer as it should: this core does not know the question, or it failed. */
  error: string | null;
}

/** The answer to `restauro restaurar` or `restauro manter`. */
export function readRestore(answer: CoreAnswer): RestoreRound {
  const round = isRecord(answer.restaurar) ? answer.restaurar : {};
  let error: string | null = null;
  if (typeof answer.erro === "string") error = answer.erro;
  else if (answer._saida === 2) error = "esta versão do keep não sabe restaurar as abas";
  else if (answer.ok === false) error = answer.detalhe ?? answer.motivo ?? `o keep saiu com ${answer._saida}`;
  const made: Made[] = [];
  if (Array.isArray(round.criadas)) {
    for (const c of round.criadas) {
      if (!isRecord(c) || typeof c.workspace !== "string") continue;
      const [from, to] = [count(c.origem), count(c.aba)];
      if (from > 0 && to > 0) made.push({ workspace: c.workspace, from, to });
    }
  }
  return {
    tabs: count(round.abas),
    made,
    conversations: count(round.conversas),
    failures: texts(round.falhas),
    reason: typeof round.motivo === "string" ? round.motivo : "",
    error,
  };
}

/** A round with nothing to say: nothing came back, nothing failed (the common one, every half minute). */
export const isQuietRestore = (round: RestoreRound) =>
  round.tabs === 0 && round.conversations === 0 && round.failures.length === 0 && round.error === null;

/** The round in a line for the log. */
export function restoreLine(round: RestoreRound): string {
  if (round.error !== null) return `restauro: ${round.error}`;
  const parts = [`${round.tabs} aba(s)`, `${round.conversations} conversa(s)`];
  if (round.reason) parts.push(round.reason);
  if (round.failures.length > 0) parts.push(`falhas: ${round.failures.join("; ")}`);
  return `restauro: ${parts.join(", ")}`;
}

/** What one round of `keep continuar` did. */
export interface ResumeRound {
  /** Conversations told to continue. */
  sent: number;
  /** Of those, the ones whose message reached the transcript. */
  confirmed: number;
  /** Another round is running (or this core does not know the question): nothing to do. */
  skipped: boolean;
  error: string | null;
}

/** The answer to `continuar`. */
export function readResume(answer: CoreAnswer): ResumeRound {
  const motivo = typeof answer.motivo === "string" ? answer.motivo : "";
  let error: string | null = null;
  if (typeof answer.erro === "string") error = answer.erro;
  else if (answer._saida === 2) error = "esta versão do keep não sabe continuar as conversas";
  return {
    sent: count(answer.enviadas),
    confirmed: count(answer.confirmadas),
    skipped: motivo.includes("outra rodada"),
    error,
  };
}

/** A round worth a line in the log: something was sent, or it failed. */
export const isQuietResume = (round: ResumeRound) => round.sent === 0 && round.error === null;

export function resumeLine(round: ResumeRound): string {
  if (round.error !== null) return `continuar: ${round.error}`;
  return `continuar: ${round.sent} conversa(s) mandada(s) continuar, ${round.confirmed} confirmada(s)`;
}

// ------------------------------------------------------------------ what the window keeps by tab number

/**
 * What the window keeps by the number of a tab, which is one daemon's — a new
 * one counts from one again: the order the tabs are listed in, the last one
 * shown in each workspace, and the tab in front.
 */
export interface TabNumbers {
  tabOrder: Record<string, number[]>;
  lastTab: Record<string, number>;
  active: { workspace: string; tab: number } | null;
}

/**
 * The same, under the numbers the restore gave the tabs it made: the tabs
 * come back listed in the order they were, and the one that was in front is in
 * front. What belonged to a tab that did not come back goes with it.
 */
export function remapTabNumbers(was: TabNumbers, made: readonly Made[]): TabNumbers {
  const to = (workspace: string, id: number) => made.find((m) => m.workspace === workspace && m.from === id)?.to;
  const tabOrder: Record<string, number[]> = {};
  for (const [workspace, ids] of Object.entries(was.tabOrder)) {
    const mapped = ids.flatMap((id) => to(workspace, id) ?? []);
    if (mapped.length > 0) tabOrder[workspace] = mapped;
  }
  const lastTab: Record<string, number> = {};
  for (const [workspace, id] of Object.entries(was.lastTab)) {
    const mapped = to(workspace, id);
    if (mapped !== undefined) lastTab[workspace] = mapped;
  }
  const shown = was.active && to(was.active.workspace, was.active.tab);
  return { tabOrder, lastTab, active: was.active && shown ? { workspace: was.active.workspace, tab: shown } : null };
}

/** Nothing of the tabs of before: a daemon with none of them. */
export const noTabNumbers = (): TabNumbers => ({ tabOrder: {}, lastTab: {}, active: null });
