// The AI layer's rules: what the footer says, what a tab's menu offers and
// marks, how the order moves, and what the close question adds about
// worktrees — the macOS app's rules, which these mirror.
import { describe, expect, it } from "vitest";
import {
  FOLLOW_ORDER,
  accountTitle,
  adopt,
  agoText,
  fresh,
  holding,
  isAvailable,
  isUsedBy,
  isValidKey,
  level,
  lineId,
  menuRows,
  momentText,
  moved,
  noteText,
  percentText,
  programKind,
  programLabel,
  readStoredLines,
  untilText,
  type Account,
  type UsageLine,
} from "./ia";
import { describeWorktree, readListing, tilde, worktreeNote, type Listing } from "./worktrees";

const account = (over: Partial<Account> = {}): Account => ({
  engine: "claude",
  key: "u-1",
  alias: "ana",
  aliases: ["ana"],
  email: "ana@exemplo.com",
  plan: "Max 20x",
  isActive: true,
  isPreferred: true,
  warning: null,
  orderKey: "claude:ana",
  hasToken: true,
  ...over,
});

const line = (over: Partial<Account> = {}, reading: UsageLine["reading"] = null, extra: Partial<UsageLine> = {}): UsageLine => ({
  account: account(over),
  reading,
  measuredAt: null,
  problem: null,
  ...extra,
});

const ana = line();
const bia = line({ key: "u-2", alias: "trabalho", aliases: ["trabalho"], email: "bia@exemplo.com", isActive: false, isPreferred: false, orderKey: "claude:trabalho" }, {
  windows: [
    { label: "5h", title: "Sessão (5h)", percent: 96, resetsAt: null },
    { label: "7d", title: "Semanal (7 dias)", percent: 100, resetsAt: null },
  ],
  limitReached: true,
});
const gpt = line({ engine: "codex", key: "acct-1", alias: "principal", aliases: ["principal"], plan: "Pro", orderKey: "gpt:principal" });

describe("which accounts can take work", () => {
  it("only 100% of the 5h or the weekly window takes one out; unmeasured is given the benefit of the doubt", () => {
    expect(isAvailable(ana)).toBe(true);
    expect(isAvailable(bia)).toBe(false);
    const fableFull = line({}, { windows: [{ label: "Fable", title: "Semanal — Fable", percent: 100, resetsAt: null }], limitReached: false });
    expect(isAvailable(fableFull)).toBe(true);
    expect(isAvailable(line({ warning: "login recusado: entre de novo com /login" }))).toBe(false);
  });
  it("keys are refused before anything runs", () => {
    expect(isValidKey("claude:ordem")).toBe(true);
    expect(isValidKey("gpt:principal")).toBe(true);
    expect(isValidKey("claude:a b")).toBe(false);
    expect(isValidKey("--para=x")).toBe(false);
    expect(isValidKey("claude:../x")).toBe(false);
  });
});

describe("a tab's menu", () => {
  const lines = [ana, bia, gpt];
  it("following the order always asks for claude:ordem, whatever it resolves to now", () => {
    const rows = menuRows([bia, gpt], null, { kind: "shell" });
    expect(rows[0]).toMatchObject({ kind: "follow", key: FOLLOW_ORDER, mark: null, enabled: true, help: "Agora: GPT · principal" });
  });
  it("marks the choice, and with a dash the account a tab following the order runs on now — of either service", () => {
    const onOrder = menuRows(lines, FOLLOW_ORDER, { kind: "codex" }, "gpt:principal");
    expect(onOrder.map((r) => r.mark)).toEqual(["on", null, null, null, "mixed"]);
    const unknownRunning = menuRows(lines, FOLLOW_ORDER, { kind: "claude" });
    expect(unknownRunning.map((r) => r.mark)).toEqual(["on", null, "mixed", null, null]);
    const fixed = menuRows(lines, "claude:trabalho", { kind: "claude" });
    expect(fixed.map((r) => r.mark)).toEqual([null, null, null, "on", null]);
  });
  it("says why an account cannot take work, and still lets it be chosen", () => {
    const rows = menuRows(lines, FOLLOW_ORDER, { kind: "claude" });
    expect(rows[3]).toMatchObject({ title: "Claude · trabalho · bia@exemplo.com — no limite", key: "claude:trabalho", enabled: true });
    expect(accountTitle(line({ warning: "sem login próprio: entre pelo menu da aba" }))).toBe(
      "Claude · ana · ana@exemplo.com — sem login próprio: entre pelo menu da aba",
    );
  });
  it("another program: said first, and nothing can be chosen", () => {
    const rows = menuRows(lines, null, { kind: "other", name: "vim" });
    expect(rows[0]).toMatchObject({ kind: "note", title: "Esta aba está rodando vim", enabled: false });
    expect(rows.filter((r) => r.kind !== "separator").every((r) => !r.enabled)).toBe(true);
  });
  it("no accounts: only following the order", () => {
    expect(menuRows([], null, { kind: "shell" }).map((r) => r.kind)).toEqual(["follow"]);
  });
  it("the account in use for the footer's dot", () => {
    expect(isUsedBy(ana.account, FOLLOW_ORDER)).toBe(true);
    expect(isUsedBy(ana.account, FOLLOW_ORDER, "claude:trabalho")).toBe(false);
    expect(isUsedBy(gpt.account, FOLLOW_ORDER, "gpt:principal")).toBe(true);
    expect(isUsedBy(bia.account, "claude:trabalho")).toBe(true);
    expect(isUsedBy(bia.account, null)).toBe(false);
  });
});

describe("what a tab runs", () => {
  it("names on Windows and elsewhere", () => {
    expect(programKind("pwsh")).toEqual({ kind: "shell" });
    expect(programKind("cmd.exe")).toEqual({ kind: "shell" });
    expect(programKind("-zsh")).toEqual({ kind: "shell" });
    expect(programKind("claude")).toEqual({ kind: "claude" });
    expect(programKind("2.1.289")).toEqual({ kind: "claude" });
    expect(programKind("codex.exe")).toEqual({ kind: "codex" });
    expect(programKind("node", "✳ Claude Code")).toEqual({ kind: "claude" });
    expect(programKind("vim")).toEqual({ kind: "other", name: "vim" });
    expect(programKind("")).toEqual({ kind: "unknown" });
  });
  it("the sidebar names the account only for an AI kept on one of its own", () => {
    expect(programLabel("claude", "claude:reserva")).toBe("claude · reserva");
    expect(programLabel("claude", FOLLOW_ORDER)).toBe("claude");
    expect(programLabel("codex", "gpt:outra")).toBe("codex · outra");
    expect(programLabel("codex", "gpt:principal")).toBe("codex");
    expect(programLabel("pwsh", null)).toBe("pwsh");
  });
});

describe("saying it", () => {
  const now = new Date(2026, 9, 4, 12, 0, 0).getTime();
  it("figures and resets in the fewest characters", () => {
    expect(percentText(16.6)).toBe("17%");
    expect(percentText(-3)).toBe("0%");
    expect(untilText(null, now)).toBeNull();
    expect(untilText(now / 1000 - 5, now)).toBe("agora");
    expect(untilText(now / 1000 + 41 * 60, now)).toBe("41min");
    expect(untilText(now / 1000 + 4 * 3600 + 10 * 60, now)).toBe("4h10");
    expect(untilText(now / 1000 + 2 * 3600, now)).toBe("2h");
    expect(untilText(now / 1000 + 37 * 3600, now)).toBe("1d13h");
  });
  it("moments and ages", () => {
    expect(momentText(new Date(2026, 9, 4, 13, 5).getTime(), now)).toBe("hoje às 13:05");
    expect(momentText(new Date(2026, 9, 5, 7, 0).getTime(), now)).toBe("amanhã às 07:00");
    expect(momentText(new Date(2026, 9, 28, 7, 0).getTime(), now)).toBe("em 28/10 às 07:00");
    expect(agoText(now - 30_000, now)).toBe("agora");
    expect(agoText(now - 3 * 60_000, now)).toBe("há 3 min");
    expect(agoText(now - 2 * 3600_000, now)).toBe("há 2h");
  });
  it("colour only near the limit", () => {
    expect(level(74.9)).toBe("normal");
    expect(level(75)).toBe("attention");
    expect(level(90)).toBe("critical");
  });
  it("the line under an account", () => {
    expect(noteText(ana, now)).toBe("medindo…");
    expect(noteText(line({}, null, { problem: "sem rede; tenta de novo em 1 min" }), now)).toBe("sem rede; tenta de novo em 1 min");
    const old = line({}, { windows: [], limitReached: true }, { measuredAt: now / 1000 - 20 * 60 });
    expect(noteText(old, now)).toBe("no limite · medido há 20 min");
  });
});

describe("the order", () => {
  const lines = [ana, bia, gpt];
  it("a move is one place, and none past the ends", () => {
    expect(moved(lines, lineId(gpt), -1).map(lineId)).toEqual([lineId(ana), lineId(gpt), lineId(bia)]);
    expect(moved(lines, lineId(ana), -1)).toBe(lines);
    expect(moved(lines, lineId(gpt), 1)).toBe(lines);
  });
  it("an order held while the core catches up wins over a read made before it wrote", () => {
    const held = [lineId(gpt), lineId(ana), lineId(bia)];
    expect(holding(lines, held).map(lineId)).toEqual(held);
    const newcomer = line({ key: "u-9", alias: "nova", aliases: ["nova"], orderKey: "claude:nova" });
    expect(holding([...lines, newcomer], held).map(lineId)).toEqual([...held, lineId(newcomer)]);
  });
  it("a read without figures keeps the last ones; new accounts and new tokens are measured at once", () => {
    const measured = line({}, { windows: [], limitReached: false }, { measuredAt: 100 });
    expect(adopt([measured], [ana])[0].measuredAt).toBe(100);
    expect(fresh([ana], [ana, gpt]).map(lineId)).toEqual([lineId(gpt)]);
    const tokenless = line({ hasToken: false });
    expect(fresh([tokenless], [ana]).map(lineId)).toEqual([lineId(ana)]);
  });
  it("what was kept from last time is read leniently", () => {
    expect(readStoredLines(JSON.stringify([ana, { nada: 1 }, null]))).toEqual([ana]);
    expect(readStoredLines("{")).toEqual([]);
    expect(readStoredLines(null)).toEqual([]);
  });
});

describe("worktrees in the close question", () => {
  const home = "C:\\Users\\Ana";
  const listing: Listing = {
    versao: 1,
    abas: [{ alvo: "e2e:1", pids: [42] }],
    lixeira: [
      { caminho: "C:\\Users\\ana\\projetos\\wt-x", ramo: "wt-x", alteracoes: 2, commits_so_aqui: 1, processos: [], nasceu_ns: "1791000000000000000" },
    ],
    mantidas: [{ caminho: "D:\\repo\\wt-y", motivo: "travada" }],
    avisos: ["alterações de wt-z aproximadas"],
  };
  it("lists what goes and what stays, from the home", () => {
    const note = worktreeNote({ kind: "listing", listing }, "desta aba", home);
    expect(note).toContain("A worktree desta aba vai para a Lixeira:\n• ~\\projetos\\wt-x (ramo wt-x; 2 arquivos alterados; 1 commit só nela)");
    expect(note).toContain("Fica onde está:\n• D:\\repo\\wt-y — travada");
    expect(note).toContain("alterações de wt-z aproximadas");
    expect(note.startsWith("\n\n")).toBe(true);
  });
  it("says when it could not check, and nothing when there is nothing", () => {
    expect(worktreeNote({ kind: "failed", why: "o keep passou de 4.5 s" }, "deste painel", home)).toBe(
      "\n\nNão consegui verificar as worktrees deste painel (o keep passou de 4.5 s); nenhuma será movida.",
    );
    expect(worktreeNote({ kind: "off" }, "desta aba", home)).toBe("");
    expect(worktreeNote({ kind: "listing", listing: { versao: 1, lixeira: [], mantidas: [] } }, "desta aba", home)).toBe("");
  });
  it("a core that does not know worktrees is no core for them", () => {
    expect(readListing({ versao: 1, _saida: 2 })).toEqual({ kind: "off" });
    expect(readListing(null, "falhou")).toEqual({ kind: "failed", why: "falhou" });
    expect(readListing({ versao: 1, _saida: 0, lixeira: [] }).kind).toBe("listing");
  });
  it("one folder's facts", () => {
    expect(describeWorktree({ caminho: "C:\\x", head: "abc1234", processos: [{ pid: 7, nome: "node" }, { pid: 8 }] }, home)).toBe(
      "C:\\x (sem ramo, em abc1234; ainda rodando dentro: node, pid 8)",
    );
    expect(tilde("C:/Users/ana/x", home)).toBe("~/x");
    expect(tilde("C:\\Users\\Anabela", home)).toBe("C:\\Users\\Anabela");
  });
});
