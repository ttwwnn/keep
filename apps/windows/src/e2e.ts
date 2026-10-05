// The scripted check a test runs on a real machine — the CI's Windows — when
// it starts the app with KEEP_E2E_REPORT set: the same functions the buttons
// and keys call, against a real daemon and a real shell, and a report of
// each step for the test to read.

import * as api from "./api";
import { SPLIT_RIGHT } from "./api";
import { busyMark, programOf, readActivity } from "./claude";
import { lineId } from "./ia";
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
  refresh,
  renameTab,
  select,
  split,
  tabBusy,
  tabLabel,
  tabsByWorkspace,
  terminals,
  trashing,
  workspaces,
  zoom,
} from "./store";
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
  };
  await api.e2eReport(JSON.stringify(report, null, 2));
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
    const figures = lines.map(
      (l) => `${l.account.alias}: ${l.reading?.windows.map((w) => `${w.label} ${Math.round(w.percent)}%`).join(", ") ?? l.problem}`,
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
    if (!rows.some((r) => r.includes("Claude · semlogin") && r.includes("sem login próprio"))) {
      throw new Error(`sem a conta sem login: ${rows.join(" | ")}`);
    }
    await api.e2eShot("menu");
    return rows.join(" | ");
  });

  await step("IA: uma conta sem login próprio abre o login dela", async () => {
    const item = [...document.querySelectorAll<HTMLElement>(".context-menu .menu-item")].find((e) =>
      (e.textContent ?? "").includes("Claude · semlogin"),
    );
    if (!item) throw new Error("sem a linha da conta no menu");
    item.click();
    const said = await waitFor(() => dialog.value, 45000);
    const title = said.title;
    const message = said.message ?? "";
    await api.e2eShot("precisa-login");
    answer("ok");
    if (!title.startsWith("Falta aprovar")) throw new Error(`${title}: ${message}`);
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
