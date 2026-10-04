// The scripted check a test runs on a real machine — the CI's Windows — when
// it starts the app with KEEP_E2E_REPORT set: the same functions the buttons
// and keys call, against a real daemon and a real shell, and a report of
// each step for the test to read.

import * as api from "./api";
import { SPLIT_RIGHT } from "./api";
import { readActivity } from "./claude";
import {
  activeTab,
  answer,
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
  tabsByWorkspace,
  terminals,
  workspaces,
  zoom,
} from "./store";

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
