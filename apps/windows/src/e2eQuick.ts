// The drop-down terminal's part of the scripted check (see e2e.ts): the
// window the shortcut and the palette bring in, on the real system — made
// borderless and on top, put where the Mac's panel goes, shown and put away —
// and the workspace behind it, which the daemon holds and the window never
// lists. Only the system can say whether the window has the geometry it was
// given, so that is asked of it here; what the page draws in it is the
// quick page's own, looked at in the photograph.
//
// The focus is not asked: a system that will not hand the keyboard to a
// window that was not the last to get input — a machine nobody is sitting
// at — is not a failure of the window.

import { availableMonitors } from "@tauri-apps/api/window";
import * as api from "./api";
import { quickSetShortcut, quickStatus, quickToggle, type QuickStatus } from "./quick/api";
import { QUICK_WORKSPACE } from "./quickWorkspace";
import { refresh, workspaces } from "./store";

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;

interface Helpers {
  waitForAsync<T>(what: () => Promise<T | undefined | null | false>, timeoutMs: number, everyMs?: number): Promise<T>;
}

type Frame = { x: number; y: number; width: number; height: number };

/** What a pixel or two of the system's own framing may move a window by. */
const SLACK = 3;

const near = (a: number, b: number) => Math.abs(a - b) <= SLACK;
const sameFrame = (a: Frame, b: Frame) => near(a.x, b.x) && near(a.y, b.y) && near(a.width, b.width) && near(a.height, b.height);
const text = (f: Frame) => `${f.width}x${f.height} em (${f.x}, ${f.y})`;

/** The first live tab of the daemon's quick workspace, if it has one. */
async function quickTab(): Promise<number | null> {
  const space = (await api.listAll()).find((w) => w.name === QUICK_WORKSPACE);
  return space?.tabs.find((t) => !t.finished)?.id ?? null;
}

export async function quickSteps(step: StepRunner, { waitForAsync }: Helpers): Promise<void> {
  /** The panel in view, at rest. */
  const rested = (status: QuickStatus) => {
    const w = status.window;
    return status.visible && w?.shown === true && w.rest !== null && near(w.frame.y, w.rest.y) ? w : null;
  };
  const rest = () => waitForAsync(async () => rested(await quickStatus()), 15000, 250);
  /**
   * The panel in view and at rest, brought in if it is not: it goes away of itself when something else takes the
   * keyboard, which on a machine somebody is using — or a runner with a notification — can happen between two steps.
   */
  const inView = async () => {
    if (!(await quickStatus()).visible) await quickToggle();
    return rest();
  };
  /** The panel put away: its window hidden once it has slid out. */
  const away = () =>
    waitForAsync(async () => {
      const status = await quickStatus();
      return !status.visible && status.window?.shown === false ? status : null;
    }, 8000, 250);

  await step("janela rápida: o atalho global foi entregue pelo sistema", async () => {
    const status = await quickStatus();
    if (!status.registered) {
      throw new Error(`${status.shortcut} não foi registrado: ${status.problem ?? "sem motivo"}; livre: ${status.suggestion ?? "nenhum"}`);
    }
    return `${status.shortcut} registrado`;
  });

  await step("janela rápida: vem para a frente, no topo da tela, na largura dela e com 42% da altura útil", async () => {
    const window = await inView();
    if (!window.alwaysOnTop) throw new Error("a janela não ficou sempre no topo");
    if (window.rest === null) throw new Error("sem o lugar de repouso");
    // The drawn part, which is what a person sees: the window itself may reach past it (the frame's invisible border).
    if (!sameFrame(window.frame, window.rest)) {
      throw new Error(`o painel está em ${text(window.frame)} e devia estar em ${text(window.rest)} (a janela: ${text(window.outer)})`);
    }
    // Which screen the pointer is on is the system's to say; with one screen it is this one.
    const monitors = await availableMonitors();
    let where = "";
    if (monitors.length === 1) {
      const m = monitors[0];
      const wanted = {
        x: m.position.x,
        y: m.workArea.position.y,
        width: m.size.width,
        height: Math.round(m.workArea.size.height * 0.42),
      };
      if (!sameFrame(window.rest, wanted)) throw new Error(`devia ocupar ${text(wanted)} e ocupa ${text(window.rest)}`);
      where = `; a tela tem ${m.size.width}x${m.size.height}, a área útil ${m.workArea.size.height}`;
    }
    return `${text(window.frame)} (a janela: ${text(window.outer)}), sempre no topo${where}`;
  });

  await step("janela rápida: o workspace quick é do keepd e a janela não o lista", async () => {
    const id = await waitForAsync(quickTab, 15000, 250);
    if ((await api.list()).some((w) => w.name === QUICK_WORKSPACE)) throw new Error("api.list devolveu o workspace quick");
    await refresh();
    if (workspaces.value.some((w) => w.name === QUICK_WORKSPACE)) throw new Error("a janela tem o workspace quick");
    const found = await api.search("a", 5).catch(() => []);
    if (found.some((h) => h.workspace === QUICK_WORKSPACE)) throw new Error("a busca achou linhas do workspace quick");
    return `o keepd tem a aba ${id}; ${workspaces.value.length} workspace(s) na janela`;
  });

  await step("janela rápida: o terminal dela é um shell vivo", async () => {
    const id = await waitForAsync(quickTab, 5000, 250);
    const windows = (await api.startup()).platform === "windows";
    const screen = await waitForAsync(async () => {
      const shown = await api.preview(QUICK_WORKSPACE, id);
      const ready = windows ? shown.includes("PS ") && shown.includes(">") : /[$%❯#]/.test(shown);
      return ready ? shown : null;
    }, 30000, 500);
    // The photograph once the prompt is on the glass: the page has the shell's repaint, and has drawn it.
    await inView();
    await new Promise((resolve) => setTimeout(resolve, 600));
    await api.e2eShot("janela-rapida");
    return screen.split("\n").filter((l) => l.trim()).slice(-1)[0] ?? "";
  });

  await step("janela rápida: o mesmo atalho a põe de volta, e o shell continua onde estava", async () => {
    const id = await waitForAsync(quickTab, 5000, 250);
    await inView();
    if (await quickToggle()) throw new Error("o comando disse que a janela seguia à vista");
    await away();
    if ((await quickTab()) !== id) throw new Error("o shell da janela rápida mudou ao sair");
    if (!(await quickToggle())) throw new Error("não voltou");
    await rest();
    if ((await quickTab()) !== id) throw new Error("a janela voltou para outro shell");
    return `a aba ${id} fica, e é a que volta`;
  });

  await step("janela rápida: o atalho se troca, recusa o que não serve e volta ao que era", async () => {
    const before = (await quickStatus()).shortcut;
    const other = before === "Control+Alt+Backquote" ? "Control+Shift+Backquote" : "Control+Alt+Backquote";
    const changed = await quickSetShortcut(other);
    if (changed.shortcut !== other || !changed.registered) throw new Error(`ficou ${changed.shortcut} (registrado: ${changed.registered})`);
    const refused = await quickSetShortcut("KeyK").then(
      () => null,
      (error) => String(error),
    );
    if (refused === null) throw new Error("aceitou uma letra sozinha como atalho");
    const kept = await quickStatus();
    if (kept.shortcut !== other || !kept.registered) throw new Error(`a recusa mexeu no atalho: ${kept.shortcut}`);
    const back = await quickSetShortcut(before);
    if (back.shortcut !== before || !back.registered) throw new Error(`não voltou: ${back.shortcut}`);
    return `${before} → ${other} → ${before}; "KeyK": ${refused}`;
  });

  await step("janela rápida: sai de vista e deixa a janela como estava", async () => {
    const status = await quickStatus();
    if (status.visible) await quickToggle();
    await away();
    return "fora de vista";
  });
}
