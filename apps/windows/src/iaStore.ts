// The AI layer's live state — the footer's lines, each tab's AI — and what
// the footer and the tabs' menus do.
//
// The core decides and does everything that touches an account (docs/ia.md):
// this keeps what it last said and asks it again on the contract's clock —
// usage every minute (the five minutes between readings of an account are
// the core's), the login files every two seconds, the tabs every five while
// the window is in front, the order of priority every half minute, the
// worktrees' index every two minutes. None of them overlaps itself.

import { computed, signal } from "@preact/signals";
import * as api from "./api";
import type { CoreAnswer, TabInfo } from "./api";
import {
  FOLLOW_ORDER,
  adopt,
  fresh,
  holding,
  isValidKey,
  lineId,
  menuRows,
  moved,
  orderKeyOf,
  programKind,
  readStoredLines,
  runsAI,
  type Engine,
  type MenuRow,
  type ProgramKind,
  type TabAI,
  type UsageLine,
} from "./ia";
import type { RootTab } from "./layout";
import {
  activeTab,
  activeWorkspace,
  ask,
  daemonError,
  gui,
  iaInfo,
  info,
  loaded,
  notice,
  refresh,
  say,
  select,
  workspaces,
} from "./store";
import { viewKey } from "./terminals";

// ------------------------------------------------------------------ state

const LINES_KEY = "keep.ia.linhas";
const FOLDED_KEY = "keep.ia.rodapeRecolhido";

export const iaAvailable = computed(() => iaInfo.value?.available === true);
/** The footer's lines, in the order of priority. */
export const usageLines = signal<UsageLine[]>([]);
/** A round somebody asked for is running. */
export const measuring = signal(false);
/** What the footer has to say for a moment: a change the core refused. */
export const usageNotice = signal<string | null>(null);
/** Another tool switches the global login (the author's kit): the core defers to it. */
export const externalManager = signal(false);
/** The footer folded to one line per account. A convenience of this machine's. */
export const footerFolded = signal(readFlag(FOLDED_KEY));
/** A clock for "recomeça em…": every half minute is often enough. */
export const usageClock = signal(Date.now());
/** Each tab's AI as the core last reported it, by pane (`viewKey`). */
export const tabAIs = signal<ReadonlyMap<string, TabAI>>(new Map());
/** Choices the core has just said yes to, shown until it reports the tab again. */
const notes = signal<ReadonlyMap<string, { key: string; at: number }>>(new Map());

function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

function persists(): boolean {
  return iaInfo.value !== null && !iaInfo.value.testHome;
}

function remember(): void {
  if (!persists()) return;
  try {
    localStorage.setItem(LINES_KEY, JSON.stringify(usageLines.value));
  } catch {
    /* the footer still works; it opens empty next time */
  }
}

export function toggleFooter(): void {
  footerFolded.value = !footerFolded.value;
  try {
    localStorage.setItem(FOLDED_KEY, footerFolded.value ? "1" : "0");
  } catch {
    /* a convenience */
  }
}

const daemonUp = () => loaded.value && daemonError.value === null;
const inFront = () => document.visibilityState === "visible" && document.hasFocus();

/** What a refusal says, or that this core does not know the question (exit 2). */
export function refusal(answer: CoreAnswer, what: string): string {
  if (answer._saida === 2) return `Esta versão do Keep ainda não sabe ${what}.`;
  return answer.detalhe || answer.motivo || `O keep saiu com ${answer._saida}.`;
}

/** Something said in a box with one button. */
function tell(title: string, message: string): Promise<unknown> {
  return ask({ title, message, buttons: [{ label: "OK", value: "ok", primary: true }] });
}

// ------------------------------------------------------------------ the accounts and their usage

let inFlight = false;
let clickPending = false;
let lastClick = 0;
/** Accounts found while a round was running, owed a reading of their own. */
const owed = new Set<string>();

/** A round's lines, onto what the footer shows: the accounts worth measuring now are handed back. */
function applyLines(answer: CoreAnswer): UsageLine[] {
  if (answer.ok !== true || !Array.isArray(answer.linhas)) return [];
  externalManager.value = answer.gerenteExterno === true;
  const previous = usageLines.value;
  const next = holding(adopt(previous, answer.linhas as UsageLine[]), heldOrder);
  const found = fresh(previous, next);
  if (JSON.stringify(next) !== JSON.stringify(previous)) {
    usageLines.value = next;
    remember();
  }
  return found;
}

/** One round of readings: every account due, or `only` this one, now. */
async function measure(clicked: boolean, only?: string): Promise<void> {
  if (!iaAvailable.value) return;
  if (inFlight) {
    if (only) owed.add(only);
    return;
  }
  inFlight = true;
  if (clicked) measuring.value = true;
  try {
    const args = ["ia", "uso"];
    if (clicked) args.push("--agora");
    if (only) args.push(`--conta=${only}`);
    args.push("--json");
    applyLines(await api.iaAsk(args));
  } catch {
    // The footer keeps what it had; the next tick asks again.
  } finally {
    inFlight = false;
    measuring.value = false;
    const next = owed.values().next();
    if (!next.done) {
      owed.delete(next.value);
      void measure(false, next.value);
    } else if (clickPending) {
      clickPending = false;
      measureNow();
    }
  }
}

/** The footer's refresh: every account, now — within reason. */
export function measureNow(): void {
  if (Date.now() - lastClick < 15_000) return;
  if (inFlight) {
    // Not dropped: run as soon as the round in flight lands.
    clickPending = true;
    measuring.value = true;
    return;
  }
  lastClick = Date.now();
  void measure(true);
}

let signature: string | null = null;
let looking = false;

/**
 * A glance at the files the logins live in. Changed — a login added or
 * renewed, the order rewritten — the accounts are read again, without the
 * network, and whatever is new among them measured now.
 */
async function look(force: boolean, measureFound = true): Promise<void> {
  if (!iaAvailable.value || looking) return;
  looking = true;
  try {
    const now = await api.iaSignature();
    if (!force && now === signature) return;
    signature = now;
    const found = applyLines(await api.iaAsk(["ia", "uso", "--cache", "--json"]));
    if (measureFound) for (const line of found) void measure(false, orderKeyOf(line.account));
  } catch {
    /* asked again in two seconds */
  } finally {
    looking = false;
  }
}

// ------------------------------------------------------------------ the order

let movesPending = 0;
/** The order shown ahead of the core while the arrows are, line by line, and until when. */
let heldOrder: string[] | null = null;
let heldUntil = 0;
/** One move at a time, in the order clicked: each is a step from where the last left it. */
let moves: Promise<void> = Promise.resolve();
let noticeTicket = 0;

function sayInFooter(text: string): void {
  const ticket = ++noticeTicket;
  usageNotice.value = text;
  setTimeout(() => {
    if (noticeTicket === ticket) usageNotice.value = null;
  }, 12_000);
}

/** One place up (-1) or down (1) the order: shown at once, asked of the core behind it. */
export function moveOrder(line: UsageLine, step: number): void {
  const next = moved(usageLines.value, lineId(line), step);
  if (next === usageLines.value) return;
  const key = orderKeyOf(line.account);
  if (!isValidKey(key)) return;
  usageLines.value = next;
  heldOrder = next.map(lineId);
  heldUntil = Date.now() + 10_000;
  movesPending += 1;
  noticeTicket += 1;
  usageNotice.value = null;
  moves = moves.then(async () => {
    try {
      const answer = await api.iaAsk(["ia", "ordem", "mover", key, step < 0 ? "cima" : "baixo", "--json"]);
      if (answer.ok !== true) throw new Error(refusal(answer, "mudar a ordem"));
    } catch (error) {
      // Taken back by reading the order as the core has it.
      heldOrder = null;
      sayInFooter(`Não deu para mudar a ordem: ${error instanceof Error ? error.message : String(error)}`);
      void look(true, false);
    } finally {
      movesPending -= 1;
      if (movesPending === 0) releaseLater();
    }
  });
}

/** Stop holding the order once the last move is answered and the hold has run out: the core's word is the last. */
function releaseLater(): void {
  const until = heldUntil;
  setTimeout(() => {
    if (heldUntil !== until || movesPending > 0 || heldOrder === null) return;
    heldOrder = null;
    void look(true, false);
  }, Math.max(0, until - Date.now()) + 50);
}

// ------------------------------------------------------------------ signing in

let signingIn = false;

/** A login to another account, in a new tab of the workspace in front; that tab comes forward. */
export async function signIn(engine: Engine): Promise<void> {
  if (signingIn) return;
  signingIn = true;
  const workspace =
    activeWorkspace.value ?? workspaces.value[0]?.name ?? (info.value?.home.split(/[\\/]/).pop() || "keep");
  try {
    const answer = await api.iaAsk(["ia", "entrar", engine === "claude" ? "claude" : "gpt", `--ws=${workspace}`, "--json"]);
    if (answer.ok === true && typeof answer.ws === "string" && typeof answer.aba === "number") {
      await refresh();
      select(answer.ws, answer.aba);
      return;
    }
    await tell("Não deu para abrir o login", refusal(answer, "abrir o login"));
  } catch (error) {
    await tell("Não deu para abrir o login", String(error));
  } finally {
    signingIn = false;
  }
}

// ------------------------------------------------------------------ each tab's AI

let readingTabs = false;

/** Ask the core which AI and account each tab runs on. */
export async function readTabs(): Promise<void> {
  if (!iaAvailable.value || !daemonUp() || readingTabs) return;
  readingTabs = true;
  const asked = Date.now();
  try {
    const answer = await api.iaAsk(["ia", "abas", "--json"]);
    if (answer.ok !== true || !Array.isArray(answer.ia)) return;
    // Tab numbers are one daemon's: an answer about another is about other tabs.
    const keepd = answer.keepd as { pid?: number } | undefined;
    const ours = Number.parseInt(gui.value.daemon.split("-")[0] ?? "", 10);
    if (keepd?.pid && Number.isFinite(ours) && ours > 0 && keepd.pid !== ours) return;
    const next = new Map<string, TabAI>();
    for (const entry of answer.ia as TabAI[]) {
      if (typeof entry?.workspace !== "string" || typeof entry.aba !== "number") continue;
      if (typeof entry.conta !== "string" || !isValidKey(entry.conta)) continue;
      next.set(viewKey(entry.workspace, entry.aba), entry);
    }
    const before = tabAIs.value;
    const same = next.size === before.size && [...next].every(([k, v]) => JSON.stringify(before.get(k)) === JSON.stringify(v));
    if (!same) tabAIs.value = next;
    // A choice noted before this question was asked gives way to what the core says now.
    const kept = new Map([...notes.value].filter(([, note]) => note.at > asked));
    if (kept.size !== notes.value.size) notes.value = kept;
  } catch {
    /* asked again in five seconds */
  } finally {
    readingTabs = false;
  }
}

export interface TabAccount {
  /** The choice: `claude:ordem`, or an account's key; null when no AI runs. */
  key: string | null;
  /** The account it runs on now, when the core said. */
  running: string | null;
  program: ProgramKind;
  /** Where the choice came from: what was just asked, the core, or the default for the program. */
  source: "note" | "core" | "default" | null;
}

/** The AI and account a pane runs on, as far as the window knows. */
export function tabAccount(workspace: string, pane: TabInfo): TabAccount {
  const id = viewKey(workspace, pane.id);
  const entry = tabAIs.value.get(id);
  // The core looked at the process holding the tab (List3): a Claude Code run
  // by `node`, or a tab that follows the order and runs on Codex now, is what
  // the core says it is. Without its word, the daemon's name for the process.
  const program: ProgramKind = entry ? { kind: entry.agente } : programKind(pane.command, pane.title);
  if (!runsAI(program)) return { key: null, running: null, program, source: null };
  const note = notes.value.get(id);
  if (note) return { key: note.key, running: null, program, source: "note" };
  if (entry) return { key: entry.conta, running: entry.atual, program, source: "core" };
  const key = program.kind === "claude" ? FOLLOW_ORDER : program.kind === "codex" ? "gpt:principal" : null;
  return { key, running: null, program, source: key ? "default" : null };
}

/**
 * The account the tab in front is on, for the footer's dot: only one the core
 * reported (or just said yes to) for the program actually on screen — an old
 * Claude record must not light a new Codex.
 */
export const selectedAccount = computed<{ key: string | null; running: string | null }>(() => {
  const ws = activeWorkspace.value;
  const tab = activeTab.value;
  const none = { key: null, running: null };
  if (!ws || !tab) return none;
  const account = tabAccount(ws, tab.root);
  if (account.source !== "note" && account.source !== "core") return none;
  if (account.program.kind !== "claude" && account.program.kind !== "codex") return none;
  const prefix = account.program.kind === "codex" ? "gpt:" : "claude:";
  // Following the order, the account in use is the one it runs on now, of
  // whichever service; not knowing that, only Claude's global login.
  if (account.key === FOLLOW_ORDER) {
    if (account.running) return account.running.startsWith(prefix) ? { key: account.key, running: account.running } : none;
    return prefix === "claude:" ? { key: account.key, running: null } : none;
  }
  return account.key?.startsWith(prefix) ? { key: account.key, running: account.running } : none;
});

const switching = new Set<string>();

/**
 * Put a tab's AI on another account. A tab at work is not interrupted without
 * a yes: the core says it is busy, the question goes up, and a yes asks
 * again, allowed to interrupt. An account without a login of its own for
 * tabs gets one in a tab the core opens, which comes forward; anything else
 * refused is said, in the core's words.
 */
export async function chooseAccount(
  workspace: string,
  tab: number,
  key: string,
  label: string,
  tabName: string,
  interrupting = false,
): Promise<void> {
  if (!isValidKey(key)) return;
  const id = viewKey(workspace, tab);
  if (switching.has(id)) return;
  switching.add(id);
  const working = `Passando a aba “${tabName}” para ${label}…`;
  say(working, 45);
  let answer: CoreAnswer | null = null;
  let error = "";
  try {
    const args = ["ia", "trocar", `--ws=${workspace}`, `--aba=${tab}`, `--para=${key}`];
    if (interrupting) args.push("--interromper");
    args.push("--json");
    answer = await api.iaAsk(args);
  } catch (e) {
    error = String(e);
  } finally {
    switching.delete(id);
    if (notice.value === working) notice.value = null;
  }
  if (answer?.ok === true) {
    notes.value = new Map(notes.value).set(id, { key, at: Date.now() });
    const done = typeof answer.feito === "string" && answer.feito ? answer.feito : `em ${label}`;
    say(`Aba “${tabName}”: ${done}`);
    void refresh();
    setTimeout(() => void readTabs(), 1500);
    return;
  }
  if (answer?.motivo === "ocupada" && !interrupting) {
    const result = await ask({
      title: `A aba “${tabName}” está ocupada`,
      message: `${refusal(answer, "trocar a IA das abas")}\n\nPassar agora para ${label} interrompe o que ela está fazendo.`,
      buttons: [
        { label: "Cancelar", value: "cancel" },
        { label: "Interromper e trocar agora", value: "go", danger: true, primary: true },
      ],
    });
    if (result?.button === "go") await chooseAccount(workspace, tab, key, label, tabName, true);
    return;
  }
  if (answer?.motivo === "precisa-login") {
    // Not a failure: the account has no login of its own for a tab yet, the
    // core opened one in a tab, and switches this one when it is through.
    const where = typeof answer.ws_login === "string" && answer.ws_login ? answer.ws_login : workspace;
    if (typeof answer.aba_login === "number") {
      await refresh();
      select(where, answer.aba_login);
    }
    await tell(`Falta aprovar ${label} no navegador`, refusal(answer, "trocar a IA das abas"));
    return;
  }
  await tell("Não deu para trocar a IA da aba", answer ? refusal(answer, "trocar a IA das abas") : error);
}

/** The tab's menu as it stands now, and what choosing a row of it does. */
export function accountMenu(workspace: string, tab: RootTab, tabName: string): { rows: MenuRow[]; choose: (row: MenuRow) => void } {
  // Fresh for the next time it opens; this one is built from what is known now.
  void readTabs();
  const account = tabAccount(workspace, tab.root);
  const rows = menuRows(usageLines.value, account.key, account.program, account.running);
  const choose = (row: MenuRow) => {
    // What the tab is on already is no change: nothing is asked.
    if (!row.key || row.key === account.key) return;
    void chooseAccount(workspace, tab.root.id, row.key, row.label, tabName);
  };
  return { rows, choose };
}

// ------------------------------------------------------------------ the core's own rounds

let syncing = false;

/** Tabs that follow the order move to the first account that can take work (the core decides). */
async function synchronize(): Promise<void> {
  if (!iaAvailable.value || !daemonUp() || syncing) return;
  syncing = true;
  try {
    const answer = await api.iaAsk(["ia", "sincronizar", "--json"]);
    if (answer.ok === true && Array.isArray(answer.alteradas) && answer.alteradas.length > 0) {
      void refresh();
      void readTabs();
    }
  } catch {
    /* the next round */
  } finally {
    syncing = false;
  }
}

let indexing = false;

/** Who made which worktree, written down while the conversations are alive to say. */
async function indexWorktrees(): Promise<void> {
  if (!iaAvailable.value || !daemonUp() || indexing) return;
  indexing = true;
  try {
    await api.iaAsk(["worktrees", "indexar", "--json"]);
  } catch {
    /* the next round */
  } finally {
    indexing = false;
  }
}

// ------------------------------------------------------------------ start

let started = false;

/** Ask whether the core is here, show what was known last time, and start the clocks. */
export async function startIA(): Promise<void> {
  if (started) return;
  started = true;
  iaInfo.value = await api.iaInfo().catch(() => null);
  if (!iaAvailable.value) return;
  if (persists()) {
    try {
      usageLines.value = readStoredLines(localStorage.getItem(LINES_KEY));
    } catch {
      /* opens empty */
    }
  }
  // The accounts first, then a round over them.
  await look(true, false);
  void measure(false);
  void readTabs();
  setInterval(() => void measure(false), 60_000);
  setInterval(() => void look(false), 2_000);
  setInterval(() => {
    if (inFront()) void readTabs();
  }, 5_000);
  setInterval(() => void synchronize(), 30_000);
  setInterval(() => void indexWorktrees(), 120_000);
  setInterval(() => (usageClock.value = Date.now()), 30_000);
}

/** For the scripted check: the rounds it waits on. */
export const e2eHooks = { measure, look, readTabs, synchronize };
