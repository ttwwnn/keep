// The scripted check's steps for the look (see e2e.ts): what only a real
// machine says — the fonts it lists, the title its window takes — and the
// theme carried all the way from the choice to the terminal and the window.

import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { applyTheme, clearTheme } from "./appearance";
import { listMonospaceFonts } from "./fonts";
import { gui, select, tabLabel, tabsByWorkspace, terminals, workspaceLabel } from "./store";
import { loadThemes, themeColors } from "./themes";
import { windowTitle } from "./windowTitle";

type Step = (name: string, run: () => Promise<string>) => Promise<void>;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** `what` asked again until it answers, for at most `timeoutMs`. */
async function until<T>(what: () => Promise<T | null>, timeoutMs: number): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await what();
    if (value !== null) return value;
    if (Date.now() > deadline) throw new Error(`não aconteceu em ${timeoutMs} ms`);
    await sleep(150);
  }
}

/** Faces every machine this runs on has at least one of: Windows's, a Mac's, a Linux's. */
const KNOWN = ["Consolas", "Courier New", "Lucida Console", "Cascadia Mono", "Menlo", "Monaco", "DejaVu Sans Mono", "Liberation Mono"];

export async function appearanceSteps(step: Step, workspace: string): Promise<void> {
  const firstTab = () => (tabsByWorkspace.value.get(workspace) ?? [])[0];

  await step("aparência: o comando lista as fontes e a página fica com as monoespaçadas", async () => {
    const listed = await api.installedFonts();
    if (listed.length < 5) throw new Error(`só ${listed.length} famílias`);
    const mono = await listMonospaceFonts();
    const known = KNOWN.filter((name) => mono.includes(name));
    if (known.length === 0) throw new Error(`nenhuma das conhecidas entre ${mono.length}: ${mono.slice(0, 8).join(", ")}`);
    return `${listed.length} famílias, ${mono.length} monoespaçadas (entre elas ${known.join(", ")})`;
  });

  await step("aparência: um tema veste o terminal e a janela, e sai", async () => {
    const tab = firstTab();
    const view = tab && terminals.existing(workspace, tab.root.id);
    if (!view) throw new Error("sem terminal");
    await loadThemes();
    const dracula = themeColors("Dracula");
    if (!dracula) throw new Error("sem o Dracula no catálogo");
    const ground = () => getComputedStyle(document.documentElement).getPropertyValue("--ground").trim();
    const chosen = () => gui.value.theme;
    applyTheme("Dracula");
    if (view.term.options.theme?.background !== dracula.background) throw new Error(`o terminal ficou em ${view.term.options.theme?.background}`);
    if (ground() !== dracula.background) throw new Error(`a janela ficou em ${ground()}`);
    if (chosen() !== "Dracula") throw new Error(`o estado guardou "${chosen()}"`);
    clearTheme();
    if (chosen() !== "") throw new Error("o tema não saiu do estado");
    if (!["#282c34", "#ffffff"].includes(ground())) throw new Error(`o Keep não voltou: ${ground()}`);
    return `Dracula ${dracula.background} no terminal e na janela; depois ${ground()}`;
  });

  await step("aparência: o título da janela é o da aba em frente", async () => {
    const tab = firstTab();
    if (!tab) throw new Error("sem aba");
    select(workspace, tab.root.id);
    const expected = windowTitle(workspaceLabel(workspace), tabLabel(workspace, tab, 0));
    const title = await until(async () => {
      const now = await getCurrentWindow().title();
      return now === expected ? now : null;
    }, 5000);
    return title;
  });
}
