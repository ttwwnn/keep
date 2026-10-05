// The AI layer as the page reads it: the accounts and their usage, each
// tab's AI, and what the footer and the tabs' menus decide from them. All of
// it comes from the core's answers (`keep ia …`, docs/ia.md); nothing here
// touches an account or a credential.
//
// The rules are the macOS app's (Model/AIUsage.swift, Model/AIChoice.swift),
// so the two windows say the same things about the same accounts.

import { programOf } from "./claude";

export type Engine = "claude" | "codex";

/** An account, without its secret: what the core prints (`Conta`). */
export interface Account {
  engine: Engine;
  /** Its identity at the service: two logins with one `key` are one account. */
  key: string;
  /** The name it is shown under. */
  alias: string;
  /** Every name that turned out to hold this same account. */
  aliases: string[];
  email: string | null;
  plan: string | null;
  /** Claude: the global login's owner. GPT: Codex's own login. */
  isActive: boolean;
  isPreferred: boolean;
  /** Something known to be wrong with the login, said on its line. */
  warning: string | null;
  /** The key that stands for it in the order. */
  orderKey: string;
  hasToken: boolean;
  /** A tab can run on it now. */
  roda?: boolean;
}

/** One allowance window: a bar in the footer. */
export interface UsageWindow {
  label: string;
  title: string;
  /** Spent, from 0 to 100. */
  percent: number;
  /** When it starts over, in unix seconds. */
  resetsAt: number | null;
}

export interface UsageReading {
  windows: UsageWindow[];
  /** At its limit by the general windows. */
  limitReached: boolean;
}

/** One account's line in the footer (`AccountUsage`). */
export interface UsageLine {
  account: Account;
  reading: UsageReading | null;
  /** Unix seconds. */
  measuredAt: number | null;
  problem: string | null;
}

/** What the core answers, before it is read: `ok`, and `motivo` and `detalhe` when not. */
export interface CoreAnswer {
  versao: number;
  ok?: boolean;
  motivo?: string;
  detalhe?: string;
  /** The exit code, added by the app: 2 is a question this core does not know. */
  _saida: number;
  [field: string]: unknown;
}

/** One tab's AI, as `keep ia abas` reports it. */
export interface TabAI {
  workspace: string;
  aba: number;
  /** What runs there now: a tab following the order may be on Codex. */
  agente: Engine;
  /** The choice: `claude:ordem` (following the order, whichever service it is on now), or an account's key. */
  conta: string;
  /** The account it runs on now, when known: what the menu marks with a dash. */
  atual: string | null;
  vinculo: string;
  pid?: number;
  conversa?: string | null;
}

/**
 * Following the order of priority: the core puts the tab on the first
 * account in it that can take work — a GPT one too, when no Claude can —
 * and moves it on when that one cannot. Always this key, whatever it
 * resolves to now: naming the account would fix the tab on it.
 */
export const FOLLOW_ORDER = "claude:ordem";

export const engineTitle = (engine: Engine) => (engine === "claude" ? "Claude" : "GPT");
export const orderPrefix = (engine: Engine) => (engine === "claude" ? "claude" : "gpt");
export const keyOf = (engine: Engine, alias: string) => `${orderPrefix(engine)}:${alias}`;

/** "Claude · reserva": which service, and the name it is known by. */
export const accountName = (account: Account) => `${engineTitle(account.engine)} · ${account.alias}`;

/** Every key that names an account, one per name it has. */
export function accountKeys(account: Account): string[] {
  const names = account.aliases.length > 0 ? account.aliases : [account.alias];
  return names.map((name) => keyOf(account.engine, name));
}

/** The line's identity: the same account keeps its place across reads. */
export const lineId = (line: UsageLine) => `${line.account.engine}:${line.account.key}`;

/** The key that moves in the order for this account. */
export const orderKeyOf = (account: Account) => account.orderKey || keyOf(account.engine, account.alias);

/** A key the core takes; refused here before anything runs. */
export const isValidKey = (key: string) => /^(claude|gpt):[^\s/:]+$/.test(key);

/**
 * Whether an account can take work now: nothing wrong with its login, and
 * not at its limit — only 100% of the five-hour or the weekly window takes
 * an account out of the order. Not measured yet: the benefit of the doubt.
 */
export function isAvailable(line: UsageLine): boolean {
  if (line.account.warning) return false;
  const reading = line.reading;
  if (!reading) return true;
  if (reading.limitReached) return false;
  return !reading.windows.some((w) => (w.label === "5h" || w.label === "7d") && w.percent >= 100);
}

/**
 * Whether a tab whose choice is `selected` is on this account. Following the
 * order, it is on the account it runs on now (`running`, as the core said —
 * of either service) or, not knowing that, on the Claude account the global
 * login belongs to.
 */
export function isUsedBy(account: Account, selected: string | null | undefined, running?: string | null): boolean {
  if (!selected) return false;
  if (selected === FOLLOW_ORDER) {
    if (running) return accountKeys(account).includes(running);
    return account.engine === "claude" && account.isActive;
  }
  return accountKeys(account).includes(selected);
}

// ------------------------------------------------------------------ what a tab runs

export type ProgramKind =
  | { kind: "shell" }
  | { kind: "claude" }
  | { kind: "codex" }
  /** Somebody else's program: the menu says so and offers nothing. */
  | { kind: "other"; name: string }
  /** Nothing said: not held against the tab, the core looks for itself. */
  | { kind: "unknown" };

const SHELLS = new Set(["zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "csh", "nu", "pwsh", "powershell", "cmd"]);

const isVersionNumber = (name: string) => {
  const parts = name.split(".");
  return parts.length >= 2 && parts.every((p) => /^\d+$/.test(p));
};

/** What a tab runs, from the name of the process holding its terminal (and its title). */
export function programKind(command: string, title = ""): ProgramKind {
  let name = programOf(command, title).replace(/\.exe$/i, "");
  if (name.startsWith("-")) name = name.slice(1);
  if (!name) return { kind: "unknown" };
  if (SHELLS.has(name)) return { kind: "shell" };
  if (name === "claude" || isVersionNumber(name)) return { kind: "claude" };
  if (name === "codex") return { kind: "codex" };
  return { kind: "other", name };
}

/** Whether the AI written down for a tab is still the tab's: once back at its shell, it is history. */
export const runsAI = (program: ProgramKind) =>
  program.kind === "claude" || program.kind === "codex" || program.kind === "unknown";

// ------------------------------------------------------------------ the tab's menu

export interface MenuRow {
  kind: "note" | "follow" | "separator" | "account";
  title: string;
  /** What choosing it asks the core for; null for what is not a choice. */
  key: string | null;
  mark: "on" | "mixed" | null;
  enabled: boolean;
  help: string | null;
  /** What the choice is called afterwards, in a question about it. */
  label: string;
}

export const FOLLOW_TITLE = "Seguir a ordem de prioridade";

/**
 * The menu, top to bottom: a tab running some other program is told so
 * first, and nothing in it can be chosen. Then following the order, and one
 * line per account in the order's order — those that cannot take work now
 * say so, and can still be chosen: that is the person's call.
 */
export function menuRows(
  lines: UsageLine[],
  current: string | null,
  program: ProgramKind,
  running?: string | null,
): MenuRow[] {
  const rows: MenuRow[] = [];
  const enabled = program.kind !== "other";
  if (program.kind === "other") {
    rows.push({ kind: "note", title: `Esta aba está rodando ${program.name}`, key: null, mark: null, enabled: false, help: null, label: "" });
  }
  const first = lines.find(isAvailable) ?? lines[0];
  rows.push({
    kind: "follow",
    title: FOLLOW_TITLE,
    key: FOLLOW_ORDER,
    mark: current === FOLLOW_ORDER ? "on" : null,
    enabled,
    help: first ? `Agora: ${accountName(first.account)}` : null,
    label: "a ordem de prioridade",
  });
  if (lines.length === 0) return rows;
  rows.push({ kind: "separator", title: "", key: null, mark: null, enabled: false, help: null, label: "" });
  for (const line of lines) {
    const account = line.account;
    let mark: MenuRow["mark"] = null;
    if (current && accountKeys(account).includes(current)) mark = "on";
    else if (current === FOLLOW_ORDER && isUsedBy(account, current, running)) mark = "mixed";
    rows.push({
      kind: "account",
      title: accountTitle(line),
      key: keyOf(account.engine, account.alias),
      mark,
      enabled,
      help: null,
      label: accountName(account),
    });
  }
  return rows;
}

/** "Claude · reserva · ana@… — no limite": the name, the address, and why it cannot take work. */
export function accountTitle(line: UsageLine): string {
  let title = accountName(line.account);
  if (line.account.email) title += ` · ${line.account.email}`;
  if (!isAvailable(line)) title += ` — ${line.account.warning ?? "no limite"}`;
  return title;
}

/**
 * What the sidebar writes after a tab's title for an AI kept on an account
 * of its own: "claude · reserva", or "codex · outra" for a GPT account other
 * than Codex's own login. Anything else: the program, as before.
 */
export function programLabel(command: string, account: string | null | undefined): string {
  if (!account) return command;
  const colon = account.indexOf(":");
  if (colon < 0) return command;
  const service = account.slice(0, colon);
  const alias = account.slice(colon + 1);
  if (service === "claude" && alias !== "ordem") return `claude · ${alias}`;
  if (service === "gpt" && alias !== "principal") return `codex · ${alias}`;
  return command;
}

// ------------------------------------------------------------------ the core's answers

/** What a refusal says, or that this core does not know the question yet (exit 2). */
export function refusalText(answer: CoreAnswer, what: string): string {
  if (answer._saida === 2) return `Esta versão do Keep ainda não sabe ${what}.`;
  return answer.detalhe || answer.motivo || `O keep saiu com ${answer._saida}.`;
}

export type SwitchOutcome =
  /** Done: what the core did, in its words. */
  | { kind: "done"; text: string }
  /** The tab is at work: ask before interrupting it, and ask the core again if yes. */
  | { kind: "busy"; detail: string }
  /** The account has no login of its own for a tab yet: the core opened one in a tab, which comes forward. */
  | { kind: "login"; detail: string; workspace: string | null; tab: number | null }
  | { kind: "failed"; detail: string };

/** What the answer to `trocar` means for the window. */
export function readSwitch(answer: CoreAnswer | null, error: string, interrupting: boolean): SwitchOutcome {
  if (!answer) return { kind: "failed", detail: error || "O keep não respondeu." };
  if (answer.ok === true) {
    return { kind: "done", text: typeof answer.feito === "string" ? answer.feito : "" };
  }
  const detail = refusalText(answer, "trocar a IA das abas");
  if (answer.motivo === "ocupada" && !interrupting) return { kind: "busy", detail };
  if (answer.motivo === "precisa-login") {
    return {
      kind: "login",
      detail,
      workspace: typeof answer.ws_login === "string" && answer.ws_login ? answer.ws_login : null,
      tab: typeof answer.aba_login === "number" ? answer.aba_login : null,
    };
  }
  return { kind: "failed", detail };
}

// ------------------------------------------------------------------ saying it

/** "17%", whole numbers. */
export const percentText = (value: number) => `${Math.round(Math.max(0, Math.min(999, value)))}%`;

/** How long until a window starts over, in the fewest characters: "41min", "4h10", "1d13h". */
export function untilText(resetsAt: number | null, nowMs: number): string | null {
  if (resetsAt === null || resetsAt === undefined) return null;
  const minutes = Math.ceil((resetsAt * 1000 - nowMs) / 60000);
  if (minutes <= 0) return "agora";
  if (minutes < 60) return `${minutes}min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const rest = minutes % 60;
    return rest === 0 ? `${hours}h` : `${hours}h${String(rest).padStart(2, "0")}`;
  }
  const days = Math.floor(hours / 24);
  const restHours = hours % 24;
  return restHours === 0 ? `${days}d` : `${days}d${restHours}h`;
}

const two = (n: number) => String(n).padStart(2, "0");
const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** "hoje às 01:50", "amanhã às 07:00", "em 28/09 às 07:00". */
export function momentText(atMs: number, nowMs: number): string {
  const at = new Date(atMs);
  const now = new Date(nowMs);
  const time = `${two(at.getHours())}:${two(at.getMinutes())}`;
  if (sameDay(at, now)) return `hoje às ${time}`;
  const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  if (sameDay(at, tomorrow)) return `amanhã às ${time}`;
  return `em ${two(at.getDate())}/${two(at.getMonth() + 1)} às ${time}`;
}

/** "agora", "há 3 min", "há 2h". */
export function agoText(atMs: number, nowMs: number): string {
  const minutes = Math.floor((nowMs - atMs) / 60000);
  if (minutes < 1) return "agora";
  if (minutes < 60) return `há ${minutes} min`;
  return `há ${Math.floor(minutes / 60)}h`;
}

export type Level = "normal" | "attention" | "critical";

/** The Clínica panel's thresholds: attention from 75, critical from 90. */
export const level = (percent: number): Level => (percent >= 90 ? "critical" : percent >= 75 ? "attention" : "normal");

/** A reading older than this, or none, is shown faded. */
export const STALE_AFTER_S = 15 * 60;

/** The line under an account: at its limit, its warning, what went wrong, how old. */
export function noteText(line: UsageLine, nowMs: number): string | null {
  const parts: string[] = [];
  if (line.reading?.limitReached) parts.push("no limite");
  if (line.account.warning) parts.push(line.account.warning);
  if (line.problem) parts.push(line.problem);
  else if (!line.reading) parts.push("medindo…");
  if (line.measuredAt !== null && nowMs / 1000 - line.measuredAt > STALE_AFTER_S) {
    parts.push(`medido ${agoText(line.measuredAt * 1000, nowMs)}`);
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** The tooltip on an account's name. */
export function accountHelp(line: UsageLine, nowMs: number, used: boolean): string {
  const out: string[] = [];
  if (line.account.email) out.push(line.account.email);
  if (line.account.plan) out.push(`Plano ${line.account.plan}`);
  if (line.account.aliases.length > 1) out.push(`Também conhecida como: ${line.account.aliases.join(", ")}`);
  if (used) out.push("Conta usada nesta aba");
  if (line.measuredAt !== null) out.push(`Medido ${agoText(line.measuredAt * 1000, nowMs)}`);
  return out.join("\n");
}

/** The tooltip on a bar. */
export function barHelp(window: UsageWindow, nowMs: number): string {
  let text = `${window.title}: ${percentText(window.percent)} usados`;
  if (window.resetsAt !== null) text += `\nReinicia ${momentText(window.resetsAt * 1000, nowMs)}`;
  return text;
}

// ------------------------------------------------------------------ the order

/** The lines with one moved a place up (-1) or down (1); the same lines when it cannot move. */
export function moved(lines: UsageLine[], id: string, step: number): UsageLine[] {
  const at = lines.findIndex((l) => lineId(l) === id);
  const to = at + step;
  if (at < 0 || step === 0 || to < 0 || to >= lines.length) return lines;
  const next = [...lines];
  next.splice(to, 0, next.splice(at, 1)[0]);
  return next;
}

/** Lines read from the core, in an order being held while the arrows are ahead of it. */
export function holding(lines: UsageLine[], held: string[] | null): UsageLine[] {
  if (!held) return lines;
  const rank = new Map<string, number>();
  held.forEach((id, i) => {
    if (!rank.has(id)) rank.set(id, i);
  });
  const placed = lines.filter((l) => rank.has(lineId(l))).sort((a, b) => rank.get(lineId(a))! - rank.get(lineId(b))!);
  return [...placed, ...lines.filter((l) => !rank.has(lineId(l)))];
}

/** Lines as the core read them, each keeping what was last measured of it when this read has nothing. */
export function adopt(previous: UsageLine[], next: UsageLine[]): UsageLine[] {
  const before = new Map(previous.map((l) => [lineId(l), l]));
  return next.map((line) => {
    const old = before.get(lineId(line));
    if (!old || line.reading || line.measuredAt !== null || line.problem) return line;
    return { ...line, reading: old.reading, measuredAt: old.measuredAt, problem: old.problem };
  });
}

/** The accounts worth measuring now: new ones, and ones that have just been given a token. */
export function fresh(previous: UsageLine[], next: UsageLine[]): UsageLine[] {
  const before = new Map(previous.map((l) => [lineId(l), l]));
  return next.filter((line) => {
    const old = before.get(lineId(line));
    return !old || (!old.account.hasToken && line.account.hasToken);
  });
}

/** Lines kept from a run before, whatever was stored: what does not read is dropped. */
export function readStoredLines(raw: string | null): UsageLine[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    if (!Array.isArray(value)) return [];
    return value.filter(
      (l): l is UsageLine =>
        typeof l === "object" &&
        l !== null &&
        typeof l.account === "object" &&
        l.account !== null &&
        (l.account.engine === "claude" || l.account.engine === "codex") &&
        typeof l.account.key === "string" &&
        typeof l.account.alias === "string" &&
        Array.isArray(l.account.aliases),
    );
  } catch {
    return [];
  }
}

// ------------------------------------------------------------------ the Jev's credit

/**
 * What OpenRouter says of the credit the Jev spends, in dollars (`keep ia
 * jev`): what the account bought and spent, and the ceiling and spending of
 * the Jev's own key — no ceiling means it spends from the whole credit. The
 * macOS app's `JevCredit`, with its rules.
 */
export interface JevCredit {
  total: number;
  used: number;
  keyLimit: number | null;
  keyUsed: number | null;
  usedToday: number | null;
  usedThisMonth: number | null;
}

/**
 * The Jev's line of the footer: the last credit that came back, and why the
 * latest attempt did not, when it did not. `credit` already counts `pending`:
 * what this machine's decisions spent that OpenRouter had not counted when it
 * was measured, from the book the Jev's skill keeps.
 */
export interface JevLine {
  credit: JevCredit | null;
  measuredAt: number | null;
  problem: string | null;
  pending: number | null;
}

export const accountLeft = (c: JevCredit) => Math.max(0, c.total - c.used);
export const keyLeft = (c: JevCredit) => (c.keyLimit === null ? null : Math.max(0, c.keyLimit - (c.keyUsed ?? 0)));
/** What the Jev can still spend: the smaller of the account's credit and its key's ceiling. */
export const available = (c: JevCredit) => {
  const key = keyLeft(c);
  return key === null ? accountLeft(c) : Math.min(key, accountLeft(c));
};
const share = (part: number, whole: number) => (whole > 0 ? Math.min(100, (part / whole) * 100) : 100);

/** "US$ 9,98": two places, a comma, and "< US$ 0,01" for what rounds to nothing. */
export function dollars(value: number): string {
  const v = Math.max(0, value);
  if (v > 0 && v < 0.01) return "< US$ 0,01";
  return `US$ ${v.toFixed(2).replace(".", ",")}`;
}

/** One of the Jev's two counters: the bar is what is spent, the figure the dollars beside it. */
export interface CreditRow {
  label: string;
  spent: number;
  figure: number;
  /** How the figure reads aloud: "restam", "gastou". */
  says: string;
  help: string;
}

/**
 * The account — what is left of what it bought — and the Jev's key: with a
 * ceiling, what is left of it; without one, what it spent ("gasto"), its bar
 * that share of the whole credit.
 */
export function creditRows(c: JevCredit): CreditRow[] {
  const rows: CreditRow[] = [
    {
      label: "conta",
      spent: share(c.used, c.total),
      figure: accountLeft(c),
      says: "restam",
      help: `Crédito da conta no OpenRouter: ${dollars(c.used)} gastos de ${dollars(c.total)}`,
    },
  ];
  const left = keyLeft(c);
  if (left !== null && c.keyLimit !== null) {
    rows.push({
      label: "chave",
      spent: c.keyLimit > 0 ? Math.min(100, ((c.keyUsed ?? 0) / c.keyLimit) * 100) : 100,
      figure: left,
      says: "restam",
      help: `Teto da chave do Jev: ${dollars(c.keyUsed ?? 0)} gastos de ${dollars(c.keyLimit)}`,
    });
  } else if (c.keyUsed !== null) {
    rows.push({
      label: "gasto",
      spent: c.total > 0 ? Math.min(100, (c.keyUsed / c.total) * 100) : 0,
      figure: c.keyUsed,
      says: "gastou",
      help:
        `Gasto da chave do Jev, que não tem teto e usa todo o crédito da conta: ${dollars(c.keyUsed)}` +
        (c.usedToday !== null ? ` (hoje ${dollars(c.usedToday)})` : ""),
    });
  }
  return rows;
}

/** The line under the Jev: what went wrong, or that it is measuring, and how old. */
export function jevNote(line: JevLine, nowMs: number): string | null {
  const parts: string[] = [];
  if (line.problem) parts.push(line.problem);
  else if (!line.credit) parts.push("medindo…");
  if (line.measuredAt !== null && nowMs / 1000 - line.measuredAt > STALE_AFTER_S) {
    parts.push(`medido ${agoText(line.measuredAt * 1000, nowMs)}`);
  }
  return parts.length ? parts.join(" · ") : null;
}

/** The Jev's tooltip. */
export function jevHelp(line: JevLine, nowMs: number): string {
  const lines = ["Crédito que o Jev gasta no OpenRouter"];
  const c = line.credit;
  if (c) {
    lines.push(`Pode gastar ainda: ${dollars(available(c))}`);
    lines.push(`Conta: ${dollars(accountLeft(c))} de ${dollars(c.total)}`);
    const left = keyLeft(c);
    if (left !== null && c.keyLimit !== null) lines.push(`Chave: ${dollars(left)} de ${dollars(c.keyLimit)}`);
    else if (c.keyUsed !== null) lines.push(`Chave: sem teto, gastou ${dollars(c.keyUsed)}`);
    if (c.usedToday !== null) lines.push(`Gasto hoje: ${dollars(c.usedToday)}`);
    if (c.usedThisMonth !== null) lines.push(`Gasto no mês: ${dollars(c.usedThisMonth)}`);
  }
  if (line.pending !== null && line.pending > 0) {
    lines.push(`Inclui ${dollars(line.pending)} de decisões desta máquina que o OpenRouter ainda não contou`);
  }
  if (line.measuredAt !== null) lines.push(`Medido ${agoText(line.measuredAt * 1000, nowMs)}`);
  return lines.join("\n");
}

/** The Jev as the core answered (`"jev": null` = no key here), or as stored: what does not read is no Jev. */
export function readJev(value: unknown): JevLine | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  const num = (x: unknown) => (typeof x === "number" && Number.isFinite(x) ? x : null);
  const c = v.credit as Record<string, unknown> | null | undefined;
  const credit =
    c && typeof c === "object" && num(c.total) !== null && num(c.used) !== null
      ? {
          total: num(c.total) as number,
          used: num(c.used) as number,
          keyLimit: num(c.keyLimit),
          keyUsed: num(c.keyUsed),
          usedToday: num(c.usedToday),
          usedThisMonth: num(c.usedThisMonth),
        }
      : null;
  const problem = typeof v.problem === "string" ? v.problem : null;
  if (!credit && !problem) return null;
  return { credit, measuredAt: num(v.measuredAt), problem, pending: num(v.pending) };
}
