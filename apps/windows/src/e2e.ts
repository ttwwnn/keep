// The scripted check a test runs on a real machine — the CI's Windows — when
// it starts the app with KEEP_E2E_REPORT set: the same functions the buttons
// and keys call, against a real daemon and a real shell, and a report of
// each step for the test to read.

import * as api from "./api";
import { SPLIT_RIGHT } from "./api";
import { parseStatusFile } from "./aiStatus";
import { busyMark, programOf, readActivity } from "./claude";
import { defaultsOf, parseSettings, statusLineCommand } from "./claudeSettings";
import { iaName, lineId } from "./ia";
import { chooseAccount, e2eHooks, iaAvailable, measureNow, moveOrder, signIn, usageLines } from "./iaStore";
import {
  activeTab,
  answer,
  closeTab,
  dialog,
  fontSize,
  gui,
  info,
  newTab,
  newWorkspace,
  picking,
  refresh,
  renameTab,
  select,
  split,
  statuses,
  statusLineSetup,
  tabBusy,
  tabLabel,
  tabsByWorkspace,
  terminals,
  trashing,
  workspaces,
  zoom,
} from "./store";
import { viewKey } from "./terminals";
import { menu } from "./ui/common";

interface Step {
  name: string;
  ok: boolean;
  ms: number;
  detail: string;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function waitFor<T>(what: () => T | undefined | null | false, timeoutMs: number, everyMs = 150): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = what();
    if (value) return value as T;
    if (Date.now() > deadline) throw new Error(`não aconteceu em ${timeoutMs} ms`);
    await sleep(everyMs);
  }
}

/** The same, for a question that has to be asked of something that answers later. */
async function waitForAsync<T>(what: () => Promise<T | undefined | null | false>, timeoutMs: number, everyMs = 500): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await what().catch(() => null);
    if (value) return value as T;
    if (Date.now() > deadline) throw new Error(`não aconteceu em ${timeoutMs} ms`);
    await sleep(everyMs);
  }
}

const WORKSPACE = "e2e";

function firstTab() {
  return (tabsByWorkspace.value.get(WORKSPACE) ?? [])[0];
}

function screenOf(tab: number): string {
  return terminals.existing(WORKSPACE, tab)?.allText() ?? "";
}

export async function runE2E(): Promise<void> {
  const steps: Step[] = [];
  const windows = info.value?.platform === "windows";
  const step = async (name: string, run: () => Promise<string>) => {
    const started = performance.now();
    try {
      const detail = await run();
      steps.push({ name, ok: true, ms: Math.round(performance.now() - started), detail });
    } catch (error) {
      steps.push({ name, ok: false, ms: Math.round(performance.now() - started), detail: String(error) });
    }
  };

  await step("o daemon responde", async () => {
    await refresh();
    await waitFor(() => workspaces.value !== undefined, 5000);
    return `${workspaces.value.length} workspace(s) antes do teste`;
  });

  await step("criar o workspace pelo diálogo do botão", async () => {
    const created = newWorkspace();
    await waitFor(() => dialog.value, 3000);
    answer("create", { name: WORKSPACE, cwd: "" });
    await created;
    const tab = await waitFor(() => firstTab(), 10000);
    return `aba ${tab.root.id} criada`;
  });

  await step("o prompt do shell aparece", async () => {
    const tab = firstTab();
    if (!tab) throw new Error("sem aba");
    const text = await waitFor(() => {
      const s = screenOf(tab.root.id);
      const ready = windows ? s.includes("PS ") && s.includes(">") : /[$%❯#]/.test(s);
      return ready ? s : null;
    }, 30000);
    return text.split("\n").filter((l) => l.trim()).slice(-1)[0] ?? "";
  });

  await step("digitar um comando e ver o resultado", async () => {
    const tab = firstTab();
    const view = tab && terminals.existing(WORKSPACE, tab.root.id);
    if (!view) throw new Error("sem terminal");
    const command = windows ? "Write-Output ('keep-e2e-' + (6*7))" : "echo keep-e2e-$((6*7))";
    // The way a key reaches the shell: xterm.js's input, as if typed.
    view.term.input(command, true);
    view.term.input("\r", true);
    await waitFor(() => screenOf(tab.root.id).includes("keep-e2e-42"), 20000);
    return "keep-e2e-42 na tela";
  });

  await step("abrir a segunda aba", async () => {
    const id = await newTab(WORKSPACE);
    await waitFor(() => (workspaces.value.find((w) => w.name === WORKSPACE)?.tabs.length ?? 0) >= 2, 10000);
    return `aba ${id}; o daemon tem ${workspaces.value.find((w) => w.name === WORKSPACE)?.tabs.length} abas`;
  });

  await step("renomear a primeira aba", async () => {
    const tab = firstTab();
    if (!tab) throw new Error("sem aba");
    select(WORKSPACE, tab.root.id);
    renameTab(WORKSPACE, tab.root.id, "Primeira");
    const titles = await waitFor(() => {
      const shown = [...document.querySelectorAll(".strip .tab-title")].map((e) => e.textContent ?? "");
      return shown.some((t) => t.includes("Primeira")) ? shown : null;
    }, 3000);
    return titles.join(" | ");
  });

  await step("aumentar o zoom um degrau", async () => {
    const before = fontSize.value;
    zoom(1);
    const tab = firstTab();
    const view = tab && terminals.existing(WORKSPACE, tab.root.id);
    await waitFor(() => view && view.fontSize > before, 3000);
    return `${before}px → ${view?.fontSize}px (${Math.round(gui.value.zoom * 100)}%)`;
  });

  await step("dividir à direita", async () => {
    const tab = firstTab();
    if (!tab) throw new Error("sem aba");
    select(WORKSPACE, tab.root.id);
    await split(SPLIT_RIGHT);
    await waitFor(() => (firstTab()?.panes.length ?? 0) === 2, 10000);
    const panes = await waitFor(() => {
      const shown = document.querySelectorAll(".tab-view.shown .pane");
      return shown.length === 2 ? shown.length : null;
    }, 5000);
    return `${panes} painéis na aba em frente (${activeTab.value?.panes.map((p) => p.id).join(", ")})`;
  });

  await step("ler o estado do Claude Code de uma tela", async () => {
    const rule = "─".repeat(40);
    const screen = ["⏺ Pronto.", "", "✻ Baked for 3s", "", rule, "❯ ", rule, "  ⏵⏵ bypass permissions on"].join("\n");
    const read = readActivity(screen);
    if (read !== "done") throw new Error(`leu ${String(read)}`);
    return "done";
  });

  if (info.value?.e2eIa) await aiSteps(step, info.value.e2eIa === "real");

  if (windows) {
    await step("a marca da aba em trabalho: Claude Code laranja, Codex com a sua", async () => {
      const seen: string[] = [];
      for (const program of ["claude", "codex"]) {
        const id = await newTab(WORKSPACE);
        if (id === null) throw new Error("não abriu a aba");
        const find = () => (tabsByWorkspace.value.get(WORKSPACE) ?? []).find((t) => t.root.id === id);
        await waitFor(() => {
          const s = screenOf(id);
          return find() && s.includes("PS ") && s.includes(">");
        }, 30000);
        terminals.existing(WORKSPACE, id)?.term.input(`cmd /c "$env:KEEP_E2E_FALSOS\\${program}.cmd"\r`, true);
        const tab = await waitFor(() => {
          const t = find();
          return t && programOf(t.root.command, t.root.title) === program && tabBusy(WORKSPACE, t) ? t : null;
        }, 30000);
        select(WORKSPACE, tab.root.id);
        const mark = busyMark(program);
        seen.push(`${program}: ${mark.glyph} ${mark.color}`);
      }
      await api.e2eShot("marcas");
      return seen.join("; ");
    });

    await barSteps(step);
    if (iaAvailable.value) await sidebarSteps(step);
  }

  const tab = firstTab();
  const report = {
    ok: steps.every((s) => s.ok),
    steps,
    screen: tab ? screenOf(tab.root.id) : "",
    version: info.value?.version,
    platform: info.value?.platform,
    windowsBuild: info.value?.windowsBuild,
    renderer: document.querySelector(".tab-view.shown canvas") ? "webgl" : "dom",
    barra,
  };
  await api.e2eReport(JSON.stringify(report, null, 2));
}

// ---------------------------------------------------------------- the bar

/** What the bar's steps leave for run.ps1 to check from outside: the shell, the settings file. */
const barra: { shellPid?: number; settings?: string | null; script?: string | null; statusLine?: string } = {};

/** The bar under a pane as drawn: which program, and its chips' text. */
function barOf(pane: number): { program: string; model: string; effort: string; agents: string } | null {
  const bar = document.querySelector<HTMLElement>(`.ai-status-bar[data-bar-pane="${pane}"]`);
  if (!bar) return null;
  const chip = (name: string) => bar.querySelector(`[data-chip="${name}"]`)?.textContent?.trim() ?? "";
  return { program: bar.dataset.program ?? "", model: chip("model"), effort: chip("effort"), agents: chip("agents") };
}

/** The keys of the last change the stand-in in a pane was walked through, as it says on its first line. */
function lastWalk(pane: number): string | null {
  const screen = terminals.existing(WORKSPACE, pane)?.screenText() ?? "";
  return /e2e: (.*)$/m.exec(screen)?.[1]?.trim() ?? null;
}

/**
 * Open a tab, start a stand-in program in it (e2e/barra-falsa.mjs, by the
 * claude.cmd and codex.cmd run.ps1 wrote in KEEP_E2E_BARRA, so the daemon
 * names it as it does the real ones) and bring it to the front.
 */
async function openStandIn(program: "claude" | "codex"): Promise<number> {
  const id = await newTab(WORKSPACE);
  if (id === null) throw new Error("não abriu a aba");
  const find = () => (tabsByWorkspace.value.get(WORKSPACE) ?? []).find((t) => t.root.id === id);
  await waitFor(() => {
    const s = screenOf(id);
    return find() && s.includes("PS ") && s.includes(">");
  }, 30000);
  terminals.existing(WORKSPACE, id)?.term.input(`cmd /c "$env:KEEP_E2E_BARRA\\${program}.cmd"\r`, true);
  await waitFor(() => {
    const t = find();
    return t && programOf(t.root.command, t.root.title) === program ? t : null;
  }, 30000);
  select(WORKSPACE, id);
  return id;
}

/**
 * Pick in a chip's menu as a click does: the chip, then the item. Only once
 * the bar can type (the turn over, the prompt empty) and no picker is being
 * walked. The menu's items are returned as shown.
 */
async function pickInBar(pane: number, chip: "model" | "effort", item: string): Promise<string[]> {
  const key = viewKey(WORKSPACE, pane);
  await waitFor(() => statuses.value.get(key)?.canType && !picking.value.has(key), 20000);
  menu.value = null;
  const button = await waitFor(
    () => document.querySelector<HTMLElement>(`.ai-status-bar[data-bar-pane="${pane}"] [data-chip="${chip}"]`),
    5000,
  );
  button.click();
  const rows = await waitFor(() => {
    const shown = [...document.querySelectorAll<HTMLElement>(".context-menu .menu-item")];
    return menu.value && shown.length > 0 ? shown : null;
  }, 3000);
  const labels = rows.map((r) => r.querySelector(".menu-label")?.textContent ?? "");
  const target = rows.find((r) => r.querySelector(".menu-label")?.textContent === item);
  if (!target) throw new Error(`sem "${item}" no menu: ${labels.join(" | ")}`);
  if ((target as HTMLButtonElement).disabled) throw new Error(`"${item}" desligado no menu: ${labels.join(" | ")}`);
  target.click();
  return labels;
}

/** Wait for the change a pick started: the bar saying `wanted`, and the stand-in the keys it was walked through. */
async function afterPick(pane: number, chip: "model" | "effort", wanted: string, keys: string): Promise<string> {
  const read = () => barOf(pane)?.[chip] ?? "";
  try {
    await waitFor(() => read() === wanted, 30000);
  } catch {
    throw new Error(`a barra diz "${read()}", não "${wanted}"; as teclas: ${lastWalk(pane)}`);
  }
  const walked = await waitFor(() => lastWalk(pane), 5000).catch(() => null);
  if (walked !== keys) throw new Error(`as teclas foram ${walked}, não ${keys}`);
  return `${wanted} — ${keys}`;
}

/**
 * The bar under Claude Code and Codex on a real Windows, against stand-ins
 * that draw their screens and pickers (e2e/barra-falsa.mjs): the status line
 * the app set up in Claude Code's settings, run as Claude Code runs it, saving
 * the stand-in's model and effort under the tab's shell; the bar showing them,
 * and Codex's off its screen and its cache; and each pick walking the
 * program's own picker by keys — Claude Code's for this conversation only,
 * the 1M Opus by its id with the default put back.
 */
async function barSteps(step: StepRunner): Promise<void> {
  let claude = 0;
  let codex = 0;

  await step("barra: a statusLine automática no settings.json do Claude Code", async () => {
    const setup = await statusLineSetup;
    const settings = await api.claudeSettings();
    barra.settings = settings.text;
    barra.script = settings.script;
    if (settings.script === null) throw new Error("o app não achou o keep-statusline.ps1 instalado");
    const line = parseSettings(settings.text)?.statusLine as { type?: string; command?: string } | undefined;
    if (line?.type !== "command" || line.command !== statusLineCommand(settings.script)) {
      throw new Error(`statusLine: ${JSON.stringify(line)} (${setup})`);
    }
    if (!(await api.pathExists(settings.script))) throw new Error(`o script não está em ${settings.script}`);
    barra.statusLine = line.command;
    return `${setup}`;
  });

  await step("barra: o keep-statusline.ps1 grava o JSON do Claude falso no arquivo do shell da aba", async () => {
    claude = await openStandIn("claude");
    const shell = await waitFor(
      () => (tabsByWorkspace.value.get(WORKSPACE) ?? []).find((t) => t.root.id === claude)?.root.shellPid || null,
      10000,
    );
    barra.shellPid = shell;
    const text = await waitForAsync(async () => {
      const files = await api.claudeStatuses([shell]);
      const found = files[String(shell)];
      return found?.includes("keep-e2e-claude") ? found : null;
    }, 90000);
    const said = parseStatusFile(text);
    return `${shell}.json: ${said?.modelId} ${said?.effort}`;
  });

  await step("barra: o modelo, o esforço e os agentes do Claude falso", async () => {
    const wanted = { program: "claude", model: "Opus 5.5 (1M) ▾", effort: "esforço xhigh ▾", agents: "3 agentes" };
    const bar = await waitFor(() => {
      const b = barOf(claude);
      return b && JSON.stringify(b) === JSON.stringify(wanted) ? b : null;
    }, 30000).catch(() => {
      throw new Error(`a barra: ${JSON.stringify(barOf(claude))}`);
    });
    await api.e2eShot("barra-claude");
    return `${bar.model} · ${bar.effort} · ${bar.agents}`;
  });

  await step("barra: o esforço do Claude pelo /effort dele, só nesta conversa", async () => {
    const before = defaultsOf((await api.claudeSettings()).text);
    const items = await pickInBar(claude, "effort", "max");
    if (items.join("|") !== "low|medium|high|xhigh|max") throw new Error(`o menu: ${items.join(" | ")}`);
    const done = await afterPick(claude, "effort", "esforço max ▾", "/effort<CR><ESC>[Cs");
    const after = defaultsOf((await api.claudeSettings()).text);
    if (after !== before) throw new Error(`o padrão mudou: ${before} → ${after}`);
    return done;
  });

  await step("barra: o modelo do Claude pelo /model dele, a troca confirmada", async () => {
    const before = defaultsOf((await api.claudeSettings()).text);
    const items = await pickInBar(claude, "model", "Sonnet 5.5");
    const done = await afterPick(claude, "model", "Sonnet 5.5 ▾", "/model<CR><ESC>[B<ESC>[Bs<CR>");
    const after = defaultsOf((await api.claudeSettings()).text);
    if (after !== before) throw new Error(`o padrão mudou: ${before} → ${after}`);
    return `${items.join(" | ")} — ${done}`;
  });

  await step("barra: o Opus 5.5 (1M) por /model <id>, e o padrão salvo devolvido", async () => {
    const before = defaultsOf((await api.claudeSettings()).text);
    await pickInBar(claude, "model", "Opus 5.5 (1M)");
    // What the stand-in saves as the default, as Claude Code does, seen before it is put back.
    const saved = await waitForAsync(async () => {
      const now = defaultsOf((await api.claudeSettings()).text);
      return now.includes("claude-opus-5-5[1m]") ? now : null;
    }, 10000, 100);
    const done = await afterPick(claude, "model", "Opus 5.5 (1M) ▾", "/model claude-opus-5-5[1m]<CR>");
    await waitForAsync(async () => defaultsOf((await api.claudeSettings()).text) === before, 15000, 250).catch(() => {
      throw new Error(`o padrão não voltou a ${before}`);
    });
    const settings = await api.claudeSettings();
    if (parseSettings(settings.text)?.statusLine === undefined) throw new Error("a devolução levou a statusLine");
    return `${done}; salvo ${saved}, devolvido ${before}`;
  });

  await step("barra: o modelo e o esforço do Codex falso, com o nome do cache", async () => {
    codex = await openStandIn("codex");
    const wanted = { program: "codex", model: "GPT-6-Astra ▾", effort: "esforço max ▾", agents: "" };
    const bar = await waitFor(() => {
      const b = barOf(codex);
      return b && JSON.stringify(b) === JSON.stringify(wanted) ? b : null;
    }, 30000).catch(() => {
      throw new Error(`a barra: ${JSON.stringify(barOf(codex))}`);
    });
    await api.e2eShot("barra-codex");
    return `${bar.model} · ${bar.effort}`;
  });

  await step("barra: o esforço do Codex pelo seletor dele", async () => {
    const items = await pickInBar(codex, "effort", "high");
    if (items.join("|") !== "low|medium|high|xhigh|max|ultra") throw new Error(`o menu: ${items.join(" | ")}`);
    return afterPick(codex, "effort", "esforço high ▾", "/model<CR>23");
  });

  await step("barra: o modelo do Codex pelo seletor, com o esforço mantido", async () => {
    const items = await pickInBar(codex, "model", "GPT-6-Sol");
    if (items.join("|") !== "GPT-6.1-Sol|GPT-6-Astra|GPT-6-Sol") throw new Error(`o menu: ${items.join(" | ")}`);
    const done = await afterPick(codex, "model", "GPT-6-Sol ▾", "/model<CR>33");
    if (barOf(codex)?.effort !== "esforço high ▾") throw new Error(`o esforço: ${barOf(codex)?.effort}`);
    return done;
  });

  await step("barra: o Ultra do Codex por More reasoning", async () => {
    await pickInBar(codex, "effort", "ultra");
    const done = await afterPick(codex, "effort", "esforço ultra ▾", "/model<CR>352");
    await api.e2eShot("barra-codex-ultra");
    return done;
  });
}

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;

/** The menu's rows as shown: the mark, then the label. */
function shownMenu(): string[] {
  return [...document.querySelectorAll(".context-menu .menu-item")].map((e) =>
    [...e.querySelectorAll(".menu-mark, .menu-label")].map((x) => x.textContent ?? "").join(" ").trim(),
  );
}

/** Open the menu of a tab's AI by its chevron in the strip, as a click does. */
async function openTabMenu(tab: number): Promise<string[]> {
  menu.value = null;
  // Found by its data, not by a selector: a workspace may be named anything.
  const chevron = await waitFor(
    () =>
      [...document.querySelectorAll<HTMLElement>(".strip .ai-chevron")].find(
        (e) => e.dataset.iaWs === WORKSPACE && e.dataset.iaTab === String(tab),
      ),
    5000,
  );
  chevron.click();
  return waitFor(() => (menu.value && shownMenu().length > 0 ? shownMenu() : null), 3000);
}

/**
 * The AI layer: the footer on the made-up home that run.ps1 wrote, against
 * made-up services; the tabs' menu, a switch that asks first, a login and a
 * close that sends a worktree to the Recycle Bin — against a stand-in core
 * (e2e/ia-falso.mjs) until the core answers those itself (`real`).
 */
async function aiSteps(step: StepRunner, real: boolean): Promise<void> {
  await step("IA: o rodapé lista as contas da casa falsa", async () => {
    if (!iaAvailable.value) throw new Error("o app não achou o keep.exe");
    const lines = await waitFor(
      () => (usageLines.value.length >= 3 && usageLines.value.every((l) => l.reading || l.problem) ? usageLines.value : null),
      60000,
    );
    const names = await waitFor(() => {
      const shown = [...document.querySelectorAll(".usage-footer .usage-name")].map((e) => e.textContent ?? "");
      return shown.length >= 3 ? shown : null;
    }, 5000);
    // Each account by its place and its service, never by the order's own names.
    const named = names.filter((n) => !/^\d+ · (Claude|GPT)$/.test(n.trim()));
    if (named.length > 0) throw new Error(`nomes no rodapé fora de "n · serviço": ${named.join(" | ")}`);
    const figures = lines.map(
      (l, i) => `${iaName(i + 1)}: ${l.reading?.windows.map((w) => `${w.label} ${Math.round(w.percent)}%`).join(", ") ?? l.problem}`,
    );
    return `${names.join(" | ")} — ${figures.join("; ")}`;
  });
  await step("IA: foto do rodapé", async () => {
    await api.e2eShot("rodape");
    return "telas/rodape.png";
  });

  await step("IA: medir agora", async () => {
    const before = Math.max(...usageLines.value.map((l) => l.measuredAt ?? 0));
    measureNow();
    const after = await waitFor(() => {
      const latest = Math.max(...usageLines.value.map((l) => l.measuredAt ?? 0));
      return latest > before ? latest : null;
    }, 30000);
    return `medido de novo (${Math.round(after - before)} s depois)`;
  });

  await step("IA: subir o GPT na ordem pelas setas", async () => {
    const gpt = usageLines.value.find((l) => l.account.engine === "codex");
    if (!gpt) throw new Error("sem conta do GPT");
    const at = usageLines.value.indexOf(gpt);
    moveOrder(gpt, -1);
    const shown = await waitFor(() => (usageLines.value.findIndex((l) => lineId(l) === lineId(gpt)) === at - 1 ? true : null), 2000);
    // What the core wrote, asked of it directly.
    const order = await waitForAsync(async () => {
      const answer = await api.iaAsk(["ia", "ordem", "--json"]);
      const list = (answer.ordem as string[]) ?? [];
      return list.indexOf("gpt:principal") === at - 1 ? list : null;
    }, 15000);
    return `${shown ? "na tela e " : ""}no núcleo: ${order.join(" ")}`;
  });

  const tabs = tabsByWorkspace.value.get(WORKSPACE) ?? [];
  const first = tabs[0];
  const second = tabs[1];
  if (!first || !second) {
    await step("IA: abas do workspace", async () => {
      throw new Error(`o roteiro precisa de duas abas em ${WORKSPACE}`);
    });
    return;
  }
  select(WORKSPACE, first.root.id);
  const firstName = tabLabel(WORKSPACE, first, 0);
  // The worktree run.ps1 made goes with the second tab when it closes.
  if (!real) await api.e2eFake({ worktreeAlvo: { workspace: WORKSPACE, aba: second.root.id } });
  await e2eHooks.readTabs();

  await step("IA: o menu da aba lista a ordem e as contas", async () => {
    const rows = await openTabMenu(first.root.id);
    if (!rows[0]?.includes("Seguir a ordem de prioridade")) throw new Error(`sem seguir a ordem: ${rows.join(" | ")}`);
    // The account with no login of its own, by its place and its address.
    if (!rows.some((r) => /IA·\d+ · carla@exemplo\.com — sem login próprio/.test(r))) {
      throw new Error(`sem a conta sem login: ${rows.join(" | ")}`);
    }
    if (rows.some((r) => /semlogin|trabalho|principal|Claude · |GPT · /.test(r))) {
      throw new Error(`um apelido no menu: ${rows.join(" | ")}`);
    }
    await api.e2eShot("menu");
    return rows.join(" | ");
  });

  await step("IA: uma conta sem login próprio abre o login dela", async () => {
    const item = [...document.querySelectorAll<HTMLElement>(".context-menu .menu-item")].find((e) =>
      (e.textContent ?? "").includes("carla@exemplo.com"),
    );
    if (!item) throw new Error("sem a linha da conta no menu");
    item.click();
    const said = await waitFor(() => dialog.value, 45000);
    const title = said.title;
    const message = said.message ?? "";
    await api.e2eShot("precisa-login");
    answer("ok");
    if (!/^Falta aprovar IA·\d+ no navegador$/.test(title)) throw new Error(`${title}: ${message}`);
    const shown = activeTab.value?.root.id;
    if (shown === first.root.id) throw new Error("a aba do login não veio para a frente");
    return `${title} — aba do login ${shown}: ${message.slice(0, 160)}`;
  });

  await step("IA: uma conta que não existe é recusada com as palavras do núcleo", async () => {
    select(WORKSPACE, first.root.id);
    void chooseAccount(WORKSPACE, first.root.id, "claude:ninguem", "Claude · ninguem", firstName);
    const said = await waitFor(() => dialog.value, 45000);
    const text = `${said.title}: ${said.message ?? ""}`;
    answer("ok");
    if (!said.title.startsWith("Não deu para trocar")) throw new Error(text);
    return text;
  });

  await step("IA: entrar em outra conta abre a aba do login", async () => {
    const before = new Set((tabsByWorkspace.value.get(WORKSPACE) ?? []).map((t) => t.root.id));
    await signIn("claude");
    if (dialog.value) {
      const said = `${dialog.value.title}: ${dialog.value.message ?? ""}`;
      answer(null);
      throw new Error(said);
    }
    const shown = activeTab.value?.root.id;
    if (shown === undefined || before.has(shown)) throw new Error(`a aba em frente (${shown}) não é nova`);
    await sleep(1500);
    await api.e2eShot("entrar");
    return `aba do login: ${shown}`;
  });

  await step("IA: fechar a aba manda a worktree para a Lixeira", async () => {
    const tab = (tabsByWorkspace.value.get(WORKSPACE) ?? []).find((t) => t.root.id === second.root.id);
    if (!tab) throw new Error("a segunda aba sumiu");
    const closing = closeTab(WORKSPACE, tab, tabLabel(WORKSPACE, tab, 1));
    const question = await waitFor(() => dialog.value, 15000);
    const message = question.message ?? "";
    if (!real && !message.includes("vai para a Lixeira")) throw new Error(`a pergunta não fala da worktree: ${message}`);
    await api.e2eShot("fechar");
    answer("close");
    await closing;
    await waitFor(() => trashing.value === 0, 90000, 300);
    if (dialog.value?.title === "Worktrees") {
      const said = dialog.value.message ?? "";
      answer(null);
      throw new Error(said);
    }
    return message.split("\n").filter((l) => l.trim()).slice(0, 4).join(" / ");
  });
}

// ---------------------------------------------------------------- the sidebar's account

/** What the sidebar writes after a tab's title, as drawn. */
function sidebarSuffix(tab: number): string | null {
  const row = [...document.querySelectorAll<HTMLElement>(".sidebar .tab-row")].find((r) =>
    r.querySelector(`.ai-chevron[data-ia-ws="${WORKSPACE}"][data-ia-tab="${tab}"]`),
  );
  if (!row) return null;
  return row.querySelector(".tab-program")?.textContent?.trim() ?? "";
}

/**
 * The sidebar names an AI tab's account by its place in the footer's list
 * only — "(IA·3)", never the AI or the order's own names — and the number
 * follows the list when its order changes: the Codex stand-in's tab, on the
 * GPT account, as that account goes one place down the list and back.
 */
async function sidebarSteps(step: StepRunner): Promise<void> {
  await step("barra lateral: a aba de IA diz só (IA·n), e o número segue a ordem do rodapé", async () => {
    const codexTab = [...(tabsByWorkspace.value.get(WORKSPACE) ?? [])]
      .reverse()
      .find((t) => programOf(t.root.command, t.root.title) === "codex" && statuses.value.has(viewKey(WORKSPACE, t.root.id)));
    if (!codexTab) throw new Error("sem a aba do Codex falso");
    const gpt = usageLines.value.findIndex((l) => l.account.engine === "codex");
    if (gpt < 0) throw new Error("sem a conta do GPT no rodapé");
    const wanted = (at: number) => `(IA·${at + 1})`;
    const seen: string[] = [];
    const expect = async (at: number) => {
      const shown = await waitFor(() => (sidebarSuffix(codexTab.root.id) === wanted(at) ? wanted(at) : null), 15000).catch(() => {
        throw new Error(`a barra lateral diz "${sidebarSuffix(codexTab.root.id)}", não "${wanted(at)}"`);
      });
      seen.push(shown);
    };
    await expect(gpt);
    const by = gpt < usageLines.value.length - 1 ? 1 : -1;
    moveOrder(usageLines.value[gpt], by);
    await expect(gpt + by);
    await api.e2eShot("barra-lateral-ia");
    moveOrder(usageLines.value[gpt + by], -by);
    await expect(gpt);
    // After a title: an AI's account by its number only, another program by its name — no account's own name.
    const suffixes = [...document.querySelectorAll(".sidebar .tab-program")].map((e) => e.textContent?.trim() ?? "");
    const odd = suffixes.filter((t) => !/^\(IA·\d+\)$/.test(t) && !/^— [^·]+$/.test(t));
    const named = suffixes.filter((t) => /principal|reserva|semlogin|trabalho/i.test(t));
    if (odd.length > 0 || named.length > 0) throw new Error(`na barra lateral: ${[...odd, ...named].join(" | ")}`);
    return `${seen.join(" → ")}; todas: ${suffixes.join(" | ")}`;
  });
}
