// The scripted check a test runs on a real machine — the CI's Windows — when
// it starts the app with KEEP_E2E_REPORT set: the same functions the buttons
// and keys call, against a real daemon and a real shell, and a report of
// each step for the test to read.

import * as api from "./api";
import { SPLIT_RIGHT } from "./api";
import { readActivity } from "./claude";
import { FOLLOW_ORDER, accountName, lineId } from "./ia";
import { e2eHooks, iaAvailable, measureNow, moveOrder, signIn, tabAccount, usageLines } from "./iaStore";
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
  notice,
  refresh,
  renameTab,
  select,
  split,
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
  const chevron = await waitFor(
    () => document.querySelector<HTMLElement>(`.strip [data-ai-chevron="${WORKSPACE}\u0000${tab}"]`),
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

  if (!real) {
    await api.e2eFake({
      abas: { [`${WORKSPACE}:${first.root.id}`]: { agente: "claude", conta: FOLLOW_ORDER, atual: "claude:ana" } },
      ocupada: { [`${WORKSPACE}:${first.root.id}`]: true },
      abaLogin: second.root.id,
      worktreeAlvo: { workspace: WORKSPACE, aba: second.root.id },
    });
  }
  await e2eHooks.readTabs();

  await step("IA: o menu da aba marca a escolha e a conta em uso", async () => {
    const rows = await openTabMenu(first.root.id);
    if (!real) {
      if (!rows[0]?.startsWith("✓ Seguir a ordem de prioridade")) throw new Error(`sem ✓ em seguir a ordem: ${rows.join(" | ")}`);
      if (!rows.some((r) => r.startsWith("– Claude · ana"))) throw new Error(`sem – na conta em uso: ${rows.join(" | ")}`);
    }
    await api.e2eShot("menu");
    return rows.join(" | ");
  });

  if (!real) {
    await step("IA: trocar para o GPT, que pergunta antes de interromper", async () => {
      const gpt = usageLines.value.find((l) => l.account.engine === "codex")!;
      const item = [...document.querySelectorAll<HTMLElement>(".context-menu .menu-item")].find((e) =>
        (e.textContent ?? "").includes(accountName(gpt.account)),
      );
      if (!item) throw new Error("sem a linha do GPT no menu");
      item.click();
      const question = await waitFor(() => (dialog.value?.title.includes("ocupada") ? dialog.value : null), 15000);
      await api.e2eShot("ocupada");
      answer("go");
      const switched = await waitFor(() => {
        const account = tabAccount(WORKSPACE, first.root);
        return account.key === "gpt:principal" ? account : null;
      }, 20000);
      return `${question.title} → ${switched.key}${notice.value ? ` (${notice.value})` : ""}`;
    });
    await step("IA: o menu depois da troca", async () => {
      await e2eHooks.readTabs();
      const rows = await openTabMenu(first.root.id);
      await api.e2eShot("menu-depois");
      menu.value = null;
      if (!rows.some((r) => r.startsWith("✓ GPT · principal"))) throw new Error(`sem ✓ no GPT: ${rows.join(" | ")}`);
      return rows.join(" | ");
    });
  } else {
    menu.value = null;
  }

  await step("IA: entrar em outra conta abre a aba do login", async () => {
    const before = new Set((tabsByWorkspace.value.get(WORKSPACE) ?? []).map((t) => t.root.id));
    await signIn("claude");
    if (dialog.value) {
      const said = `${dialog.value.title}: ${dialog.value.message ?? ""}`;
      answer(null);
      throw new Error(said);
    }
    const shown = activeTab.value?.root.id;
    if (!real && shown !== second.root.id) throw new Error(`foi para a aba ${shown}, não para ${second.root.id}`);
    const now = (tabsByWorkspace.value.get(WORKSPACE) ?? []).map((t) => t.root.id);
    return `aba em frente: ${shown}; abas novas: ${now.filter((id) => !before.has(id)).join(", ") || "nenhuma"}`;
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
