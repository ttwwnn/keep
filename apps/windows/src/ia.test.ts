// The AI layer's rules: what the footer says, what a tab's menu offers and
// marks, how the order moves, and what the close question adds about
// worktrees — the macOS app's rules, which these mirror.
import { describe, expect, it } from "vitest";
import {
  FOLLOW_ORDER,
  accountHelp,
  accountTitle,
  adopt,
  agoText,
  fresh,
  holding,
  isAvailable,
  isUsedBy,
  isValidKey,
  iaName,
  level,
  lineId,
  menuRows,
  momentText,
  moved,
  nameToKeep,
  noteText,
  numberOf,
  numberedName,
  percentText,
  programKind,
  provenKey,
  tabAccountLabel,
  readJev,
  readStoredLines,
  readSwitch,
  refusalText,
  untilText,
  available,
  creditRows,
  dollars,
  jevHelp,
  jevNote,
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

describe("an account is its place in the list", () => {
  const lines = [ana, bia, gpt];
  it("IA·1, IA·2, IA·3: from 1, never by the order's own names", () => {
    expect(iaName(1)).toBe("IA·1");
    expect(iaName(12)).toBe("IA·12");
    expect(numberedName(3, gpt.account)).toBe("IA·3 · GPT");
    expect(numberedName(2, bia.account)).toBe("IA·2 · Claude");
  });
  it("the place of the account a key names, by any of its names or its order key", () => {
    expect(numberOf("claude:ana", lines)).toBe(1);
    expect(numberOf("claude:trabalho", lines)).toBe(2);
    expect(numberOf("gpt:principal", lines)).toBe(3);
    const twoNames = line({ key: "u-7", alias: "x", aliases: ["x", "y"], orderKey: "claude:x" });
    expect(numberOf("claude:y", [ana, twoNames])).toBe(2);
    expect(numberOf("claude:ninguem", lines)).toBeNull();
    expect(numberOf(FOLLOW_ORDER, lines)).toBeNull();
    expect(numberOf(null, lines)).toBeNull();
  });
  it("the place moves with the order", () => {
    expect(numberOf("gpt:principal", moved(lines, lineId(gpt), -1))).toBe(2);
    expect(numberOf("claude:trabalho", moved(lines, lineId(gpt), -1))).toBe(3);
  });
  it("the footer's tooltip starts with it, and says no other name", () => {
    const twoNames = line({ key: "u-7", alias: "x", aliases: ["x", "y"], email: null, plan: null }, null, { measuredAt: null });
    expect(accountHelp(twoNames, 2, 0, false)).toBe("IA·2 · Claude");
    expect(accountHelp(ana, 1, 0, true)).toBe("IA·1 · Claude\nana@exemplo.com\nPlano Max 20x\nConta usada nesta aba");
  });
});

describe("a tab's menu", () => {
  const lines = [ana, bia, gpt];
  it("following the order always asks for claude:ordem, whatever it resolves to now", () => {
    const rows = menuRows([bia, gpt], null, { kind: "shell" });
    expect(rows[0]).toMatchObject({ kind: "follow", key: FOLLOW_ORDER, mark: null, enabled: true, help: "Agora: IA·2" });
    // None can take work: the first.
    expect(menuRows([bia], null, { kind: "shell" })[0].help).toBe("Agora: IA·1");
    expect(menuRows([], null, { kind: "shell" })[0].help).toBeNull();
  });
  it("each account by its place, its address and why it cannot take work; spoken of as IA·n", () => {
    const rows = menuRows(lines, FOLLOW_ORDER, { kind: "claude" });
    expect(rows.filter((r) => r.kind === "account").map((r) => [r.title, r.label, r.key])).toEqual([
      ["IA·1 · ana@exemplo.com", "IA·1", "claude:ana"],
      ["IA·2 · bia@exemplo.com — no limite", "IA·2", "claude:trabalho"],
      ["IA·3 · ana@exemplo.com", "IA·3", "gpt:principal"],
    ]);
    expect(rows.some((r) => /ana$|trabalho|principal|·\s*Claude/.test(r.title))).toBe(false);
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
    expect(rows[3]).toMatchObject({ title: "IA·2 · bia@exemplo.com — no limite", key: "claude:trabalho", enabled: true });
    expect(accountTitle(line({ warning: "sem login próprio: entre pelo menu da aba" }), 4)).toBe(
      "IA·4 · ana@exemplo.com — sem login próprio: entre pelo menu da aba",
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
  it("the sidebar says only the number of the account an AI tab runs on, and the program of any other", () => {
    const lines = [ana, bia, gpt];
    const claude = { kind: "claude" } as const;
    const codex = { kind: "codex" } as const;
    // Where it runs, as the core says, over what it was put on.
    expect(tabAccountLabel(claude, "claude", FOLLOW_ORDER, "claude:trabalho", lines)).toBe("(IA·2)");
    expect(tabAccountLabel(codex, "codex", FOLLOW_ORDER, "gpt:principal", lines)).toBe("(IA·3)");
    // Put on an account, the core not having said.
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, lines)).toBe("(IA·2)");
    expect(tabAccountLabel(codex, "codex", "gpt:principal", null, lines)).toBe("(IA·3)");
    // Following the order, where to unknown: the Claude login the tabs share.
    expect(tabAccountLabel(claude, "claude", FOLLOW_ORDER, null, lines)).toBe("(IA·1)");
    expect(tabAccountLabel(claude, "claude", FOLLOW_ORDER, null, [bia, gpt])).toBe("");
    // No account known: nothing.
    expect(tabAccountLabel(claude, "claude", "claude:ninguem", null, lines)).toBe("");
    expect(tabAccountLabel(claude, "claude", null, null, lines)).toBe("");
    expect(tabAccountLabel(claude, "claude", FOLLOW_ORDER, null, [])).toBe("");
    // Another program: its name.
    expect(tabAccountLabel({ kind: "shell" }, "pwsh", null, null, lines)).toBe("— pwsh");
    expect(tabAccountLabel({ kind: "other", name: "vim" }, "vim", null, null, lines)).toBe("— vim");
    expect(tabAccountLabel({ kind: "unknown" }, "", null, null, lines)).toBe("");
  });
  it("a Claude switched and not answering on the new account yet says both: (IA·1→2)", () => {
    const lines = [ana, bia, gpt];
    const claude = { kind: "claude" } as const;
    // Put on IA·2 (what the core or the note says), still answering on IA·1 (proven).
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, lines, "claude:ana")).toBe("(IA·1→2)");
    expect(tabAccountLabel(claude, "claude", FOLLOW_ORDER, "claude:trabalho", lines, "claude:ana")).toBe("(IA·1→2)");
    // Answering on the account it is on: one number. Nothing proven: one number.
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, lines, "claude:trabalho")).toBe("(IA·2)");
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, lines, null)).toBe("(IA·2)");
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, lines)).toBe("(IA·2)");
    // A proof of an account the list does not have says nothing.
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, lines, "claude:sumiu")).toBe("(IA·2)");
    // The arrow follows the footer's order too, and a plain program never gets one.
    expect(tabAccountLabel(claude, "claude", "claude:trabalho", null, [bia, ana, gpt], "claude:ana")).toBe("(IA·2→1)");
    expect(tabAccountLabel({ kind: "shell" }, "pwsh", null, null, lines, "claude:ana")).toBe("— pwsh");
    // No account to name, no arrow either.
    expect(tabAccountLabel(claude, "claude", "claude:ninguem", null, lines, "claude:ana")).toBe("");
  });
  it("what the core proves is a key, never the order", () => {
    expect(provenKey("claude:ana")).toBe("claude:ana");
    expect(provenKey("gpt:principal")).toBe("gpt:principal");
    expect(provenKey(FOLLOW_ORDER)).toBeNull();
    expect(provenKey("ana")).toBeNull();
    expect(provenKey(null)).toBeNull();
    expect(provenKey(7)).toBeNull();
  });
  it("a tab keeps the name it had through a switch, unless it was named meanwhile", () => {
    expect(nameToKeep("✳ Revisão do PR", undefined)).toBe("✳ Revisão do PR");
    expect(nameToKeep("  pwsh ", undefined)).toBe("pwsh");
    // The person's name wins; the fallback is not a name; nothing is nothing.
    expect(nameToKeep("✳ Revisão do PR", "Meu nome")).toBeNull();
    expect(nameToKeep("aba 3", undefined)).toBeNull();
    expect(nameToKeep("   ", undefined)).toBeNull();
  });
  it("the number follows the footer's order", () => {
    const lines = [ana, bia, gpt];
    const codex = { kind: "codex" } as const;
    expect(tabAccountLabel(codex, "codex", "gpt:principal", null, lines)).toBe("(IA·3)");
    expect(tabAccountLabel(codex, "codex", "gpt:principal", null, moved(lines, lineId(gpt), -1))).toBe("(IA·2)");
    expect(tabAccountLabel(codex, "codex", "gpt:principal", null, [gpt, ana, bia])).toBe("(IA·1)");
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

describe("the answer to a switch", () => {
  const answer = (over: Record<string, unknown>) => ({ versao: 1, _saida: 1, ...over });
  it("done, in the core's words", () => {
    expect(readSwitch(answer({ ok: true, _saida: 0, feito: "retomou GPT · principal" }), "", false)).toEqual({
      kind: "done",
      text: "retomou GPT · principal",
    });
  });
  it("a busy tab is asked about once; asked again with the yes, it is a failure to say", () => {
    const busy = answer({ ok: false, motivo: "ocupada", detalhe: "A aba está trabalhando." });
    expect(readSwitch(busy, "", false)).toEqual({ kind: "busy", detail: "A aba está trabalhando." });
    expect(readSwitch(busy, "", true)).toEqual({ kind: "failed", detail: "A aba está trabalhando." });
  });
  it("an account without a login of its own sends the person to the tab the core opened", () => {
    const login = answer({ ok: false, motivo: "precisa-login", detalhe: "Abri a aba", ws_login: "e2e", aba_login: 7 });
    expect(readSwitch(login, "", false)).toEqual({ kind: "login", detail: "Abri a aba", workspace: "e2e", tab: 7 });
    expect(readSwitch(answer({ ok: false, motivo: "precisa-login", detalhe: "x" }), "", false)).toMatchObject({ workspace: null, tab: null });
  });
  it("anything else is said as the core said it; an older core, as not known yet", () => {
    expect(readSwitch(answer({ ok: false, motivo: "sem-conta", detalhe: "Não há a conta Claude · x." }), "", false)).toEqual({
      kind: "failed",
      detail: "Não há a conta Claude · x.",
    });
    expect(readSwitch(answer({ ok: false, motivo: "uso", _saida: 2 }), "", false)).toEqual({
      kind: "failed",
      detail: "Esta versão do Keep ainda não sabe trocar a IA das abas.",
    });
    expect(readSwitch(null, "o keep passou de 40 s", false)).toEqual({ kind: "failed", detail: "o keep passou de 40 s" });
    expect(refusalText(answer({ ok: false }), "mudar a ordem")).toBe("O keep saiu com 1.");
  });
});

describe("the Jev's credit", () => {
  const credit = (over = {}) => ({
    total: 10,
    used: 0.0246,
    keyLimit: 5 as number | null,
    keyUsed: 0.0042 as number | null,
    usedToday: 0.0018 as number | null,
    usedThisMonth: 0.0042 as number | null,
    ...over,
  });
  it("dollars with a comma and two places, and what rounds to nothing said as such", () => {
    expect(dollars(9.9754)).toBe("US$ 9,98");
    expect(dollars(0)).toBe("US$ 0,00");
    expect(dollars(0.004)).toBe("< US$ 0,01");
    expect(dollars(-1)).toBe("US$ 0,00");
    expect(dollars(1500.5)).toBe("US$ 1500,50");
  });
  it("what the Jev can still spend is the smaller of the account's credit and its key's ceiling", () => {
    expect(available(credit())).toBeCloseTo(4.9958, 9);
    expect(available(credit({ keyLimit: null }))).toBeCloseTo(9.9754, 9);
    expect(available(credit({ used: 12, keyLimit: null }))).toBe(0);
  });
  it("two counters: the account, and the key's ceiling — or, without one, what the whole account spent", () => {
    const comTeto = creditRows(credit());
    expect(comTeto.map((r) => [r.label, r.says])).toEqual([["conta", "restam"], ["chave", "restam"]]);
    expect(comTeto[1].figure).toBeCloseTo(4.9958, 9);
    // US$ 10 bought, 3 cents spent by every key, 1 cent of it by the Jev's
    const semTeto = creditRows(credit({ used: 0.0317, keyLimit: null, keyUsed: 0.0109 }));
    expect(semTeto.map((r) => [r.label, r.says])).toEqual([["conta", "restam"], ["gasto", "gastou"]]);
    expect(semTeto[1].figure).toBeCloseTo(0.0317, 9);
    expect(dollars(semTeto[1].figure)).toBe("US$ 0,03");
    expect(semTeto[1].spent).toBeCloseTo(0.317, 9);
    expect(semTeto[0].figure + semTeto[1].figure).toBeCloseTo(10, 9);
    expect(semTeto[1].help).toContain("a chave do Jev gastou US$ 0,01");
    expect(creditRows(credit({ used: 0.0317, keyLimit: null, keyUsed: null })).map((r) => r.label)).toEqual([
      "conta",
      "gasto",
    ]);
  });
  it("reads the core's answer: no key is no Jev; a reading kept through a problem", () => {
    expect(readJev(null)).toBeNull();
    expect(readJev({ credit: null, measuredAt: null, problem: null })).toBeNull();
    const line = readJev({ credit: credit(), measuredAt: 1791084861.45, problem: null });
    expect(line?.credit?.keyLimit).toBe(5);
    expect(readJev({ credit: { total: "10", used: 1 }, problem: "sem rede" })).toEqual({
      credit: null,
      measuredAt: null,
      problem: "sem rede",
      pending: null,
    });
    expect(line?.pending).toBeNull();
    expect(readJev({ credit: credit(), measuredAt: 1, problem: null, pending: 0.0002 })?.pending).toBe(0.0002);
  });
  it("the note and the tooltip say it as the macOS footer does", () => {
    const now = Date.UTC(2026, 9, 5, 12);
    const line = { credit: credit({ keyLimit: null }), measuredAt: now / 1000 - 3600, problem: null, pending: null };
    expect(jevNote(line, now)).toBe("medido há 1h");
    expect(jevNote({ credit: null, measuredAt: null, problem: null, pending: null }, now)).toBe("medindo…");
    expect(jevHelp(line, now)).toContain("Chave: sem teto, gastou < US$ 0,01");
    expect(jevHelp(line, now)).not.toContain("ainda não contou");
    expect(jevHelp({ ...line, pending: 0.0002 }, now)).toContain(
      "Inclui < US$ 0,01 de decisões desta máquina que o OpenRouter ainda não contou",
    );
  });
});
