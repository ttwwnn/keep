// The scripted check of what several windows can do, run with the rest of
// e2e.ts in the first window: it opens windows, and they answer it. A window
// the script opened is the same page as the first, started by the same app;
// when the scripted check is on, it listens for what the script asks of it
// (`answerTheScript`) and says what it shows — which workspaces it lists, which
// tab is in front, the marks drawn, what it was told of the app's state — or
// does what it is asked, through the same functions its buttons call.
//
// The pointer is the page's own, as in e2eTabs.ts: a tab is pulled out of the
// row with the events a pointer would send, and let go where the check says,
// on the screen, over a window or over none.

import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { tabKey } from "./state";
import { cascaded, shownElsewhere } from "./windowModel";
import {
  activeTab,
  activeWorkspace,
  answer,
  closeWorkspace,
  createWorkspace,
  dialog,
  gui,
  log,
  newTab,
  putAway,
  renameTab,
  select,
  shownWorkspaces,
  tabsByWorkspace,
  terminals,
  workspaces,
  zoom,
  zoomReset,
} from "./store";
import { peers, severalWindows, windowLabel } from "./windowPeers";
import { hooks, openNewWindow } from "./windowSync";

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;
interface Point {
  x: number;
  y: number;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

async function waitFor<T>(what: () => T | undefined | null | false, timeoutMs: number, wanted: string): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = what();
    if (value) return value as T;
    if (Date.now() > deadline) throw new Error(`${wanted}: não aconteceu em ${timeoutMs} ms`);
    await sleep(100);
  }
}

async function waitForAsync<T>(what: () => Promise<T | undefined | null | false>, timeoutMs: number, wanted: string): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await what().catch(() => null);
    if (value) return value as T;
    if (Date.now() > deadline) throw new Error(`${wanted}: não aconteceu em ${timeoutMs} ms`);
    await sleep(150);
  }
}

function check(condition: unknown, otherwise: string): asserts condition {
  if (!condition) throw new Error(otherwise);
}

// ------------------------------------------------------------------ what a window answers

/** What a window says of itself when the script asks. */
interface Snapshot {
  label: string;
  /** The workspaces it lists, in its order. */
  listed: string[];
  workspace: string | null;
  tab: number | null;
  /** The workspaces put away, as it knows them. */
  hidden: string[];
  zoom: number;
  tabNames: Record<string, string>;
  /** Whether another window has the tab in front of this one in front too: it wears the mark. */
  elsewhere: boolean;
  windows: string[];
  innerWidth: number;
  innerHeight: number;
  screenX: number;
  screenY: number;
}

interface Ask {
  id: number;
  from: string;
  what: "ping" | "snapshot" | "go" | "putAway" | "close";
  workspace?: string;
  tab?: number;
}

const ASK = "keep://e2e-ask";
const ANSWER = "keep://e2e-answer";

const tabIds = (selector: string, attribute: string): number[] =>
  [...document.querySelectorAll<HTMLElement>(selector)].map((e) => Number((e.getAttribute(attribute) ?? "").split("\u0000")[1]));

function snapshot(): Snapshot {
  const workspace = activeWorkspace.value;
  const tab = activeTab.value?.root.id ?? null;
  return {
    label: windowLabel.value,
    listed: shownWorkspaces.value.map((w) => w.name),
    workspace,
    tab,
    hidden: [...gui.value.hidden],
    zoom: gui.value.zoom,
    tabNames: gui.value.tabNames,
    elsewhere: workspace !== null && tab !== null && shownElsewhere(peers.value, windowLabel.value, workspace, tab),
    windows: peers.value.map((p) => p.label),
    innerWidth: window.innerWidth,
    innerHeight: window.innerHeight,
    screenX: window.screenX,
    screenY: window.screenY,
  };
}

/** In a window the script opened: do what it asks and say what it shows. */
export async function answerTheScript(): Promise<void> {
  // As itself, so that what the script says to one window is heard by that one.
  await getCurrentWebviewWindow().listen<Ask>(ASK, async ({ payload }) => {
    let data: unknown = null;
    try {
      switch (payload.what) {
        case "go":
          select(payload.workspace ?? "", payload.tab);
          break;
        case "putAway":
          putAway(payload.workspace ?? "");
          break;
        case "close":
          // Said before it goes: the answer leaves with the window still there.
          await emitTo(payload.from, ANSWER, { id: payload.id, data: null });
          await getCurrentWindow().close();
          return;
        case "ping":
        case "snapshot":
          data = snapshot();
          break;
      }
    } catch (error) {
      data = { error: String(error) };
    }
    // After the page has drawn what was asked.
    await sleep(payload.what === "go" || payload.what === "putAway" ? 400 : 0);
    if (payload.what === "go" || payload.what === "putAway") data = snapshot();
    await emitTo(payload.from, ANSWER, { id: payload.id, data });
  });
}

// ------------------------------------------------------------------ what the script asks

let asked = 0;
const waiting = new Map<number, (data: unknown) => void>();
let listening: Promise<unknown> | null = null;

async function ask(label: string, what: Ask["what"], rest: Partial<Ask> = {}, timeoutMs = 15000): Promise<Snapshot & { error?: string }> {
  listening ??= getCurrentWebviewWindow().listen<{ id: number; data: unknown }>(ANSWER, ({ payload }) => waiting.get(payload.id)?.(payload.data));
  await listening;
  const id = ++asked;
  const answer = new Promise<unknown>((resolve, reject) => {
    waiting.set(id, resolve);
    setTimeout(() => reject(new Error(`${label} não respondeu a "${what}" em ${timeoutMs} ms`)), timeoutMs);
  });
  await emitTo(label, ASK, { id, from: windowLabel.value, what, ...rest } satisfies Ask);
  try {
    return (await answer) as Snapshot & { error?: string };
  } finally {
    waiting.delete(id);
  }
}

/** A window that has opened and started: it answers. */
async function awaitWindow(label: string): Promise<Snapshot> {
  await waitFor(() => peers.value.some((p) => p.label === label), 20000, `a janela ${label} na lista`);
  return waitForAsync(() => ask(label, "ping", {}, 1500), 40000, `a janela ${label} responder`);
}

async function seen(label: string, what: (s: Snapshot) => boolean, wanted: string, timeoutMs = 10000): Promise<Snapshot> {
  return waitForAsync(async () => {
    const s = await ask(label, "snapshot");
    return what(s) ? s : null;
  }, timeoutMs, wanted);
}

// ------------------------------------------------------------------ the pointer

let pointerId = 70;

function fire(target: EventTarget, type: string, at: Point, buttons: number): void {
  target.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      composed: true,
      pointerId,
      pointerType: "mouse",
      isPrimary: true,
      button: 0,
      buttons,
      clientX: at.x,
      clientY: at.y,
    }),
  );
}

const middle = (element: Element): Point => {
  const box = element.getBoundingClientRect();
  return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
};

/** Points along a straight line, the first one step from `a` and the last at `b`. */
function line(a: Point, b: Point, steps = 10): Point[] {
  return Array.from({ length: steps }, (_, i) => ({
    x: a.x + ((b.x - a.x) * (i + 1)) / steps,
    y: a.y + ((b.y - a.y) * (i + 1)) / steps,
  }));
}

/** Press on a tab of the row, pull it through `via` and let go at the last point. */
async function pull(tab: Element, via: Point[], held?: () => void): Promise<void> {
  pointerId += 1;
  fire(tab, "pointerdown", middle(tab), 1);
  for (const at of via) {
    fire(window, "pointermove", at, 1);
    await sleep(35);
  }
  await sleep(150);
  held?.();
  fire(window, "pointerup", via[via.length - 1], 0);
  await sleep(300);
}

/**
 * Bring this window to the front and wait until it is looked at. A window that another covers does not draw (a
 * page that is not seen is not painted, which the system takes care of), so what the check reads off the screen
 * is read from the one in front.
 */
async function front(): Promise<void> {
  await getCurrentWindow().setFocus();
  await waitFor(() => document.visibilityState === "visible" && document.hasFocus(), 8000, "esta janela à frente");
  await sleep(300);
}

/** A tab of the row, found by what it says it is: its key holds a character a selector cannot spell. */
const stripTab = (workspace: string, root: number) =>
  [...document.querySelectorAll<HTMLElement>(".strip .tab")].find((e) => e.dataset.tab === tabKey(workspace, root));

/** The same in the sidebar. */
const sidebarRow = (workspace: string, root: number) =>
  [...document.querySelectorAll<HTMLElement>(".sidebar .tab-row[data-row]")].find((e) => e.dataset.row === tabKey(workspace, root));

// ------------------------------------------------------------------ the steps

const SPACE = "e2e-janelas";

/** The same size, give or take what a window's edges take. */
const alike = (a: number, b: number) => Math.abs(a - b) <= 2;

/** What the windows say of themselves, for the message of a step that did not come out. */
const peersText = () => JSON.stringify(peers.value.map((p) => ({ [p.label]: { carries: p.placement.carries.length, showing: p.placement.showing } })));

const idsOf = (workspace: string): number[] => (tabsByWorkspace.value.get(workspace) ?? []).map((t) => t.root.id);

/** A photograph of window `n`: on a Windows the harness takes it of the screen; elsewhere the run says so in the log. */
async function photo(name: string, windows: boolean, n = 1): Promise<void> {
  if (windows) await api.e2eShot(name).catch(() => {});
  else {
    log(`tela ${name}:${n}`);
    await sleep(1500);
  }
}

export async function windowsSteps(step: StepRunner, windows: boolean): Promise<void> {
  let second = "";
  let tabs: number[] = [];

  await step("janelas: Nova janela abre uma segunda janela vazia, do tamanho da primeira", async () => {
    const win = getCurrentWindow();
    // A window that fills the screen (the Mac's zoomed one) is opened as it is; the step needs one that does not.
    if (await win.isFullscreen()) await win.setFullscreen(false);
    if (await win.isMaximized()) {
      await win.unmaximize();
      await waitForAsync(async () => !(await win.isMaximized()), 5000, "a janela deixar de estar maximizada");
      // The system takes its time to give the window its size back: until it stands still.
      let size = "";
      let since = Date.now();
      await waitFor(() => {
        const now = `${window.innerWidth}×${window.innerHeight}`;
        if (now !== size) {
          size = now;
          since = Date.now();
        }
        return Date.now() - since >= 700;
      }, 8000, "a janela parar de mudar de tamanho");
    }
    await createWorkspace(SPACE, null);
    await newTab(SPACE);
    await newTab(SPACE);
    tabs = await waitFor(() => (idsOf(SPACE).length === 3 ? idsOf(SPACE) : null), 20000, "o workspace com três abas");
    select(SPACE, tabs[0]);
    check(!severalWindows.value, "já havia mais de uma janela");
    // Where the new one is to be: the size of this one, stepped down and across.
    const [at, size, scale] = await Promise.all([win.outerPosition(), win.innerSize(), win.scaleFactor()]);
    const wanted = cascaded({ x: at.x, y: at.y, width: size.width, height: size.height }, scale);
    const opened = await openNewWindow();
    check(opened, "a janela não abriu");
    second = opened.label;
    // Started and listening, before anything is checked of it: the steps that follow ask it things.
    const start = await awaitWindow(second);
    // As the app put it, before the system had a say: a window manager that zooms every new window is the system's.
    const put = opened.frame;
    check(put !== null, "o app não disse onde pôs a janela");
    check(alike(put.width, wanted.width) && alike(put.height, wanted.height), `${put.width}×${put.height}, não ${wanted.width}×${wanted.height}`);
    check(alike(put.x, wanted.x) && alike(put.y, wanted.y), `em ${put.x},${put.y}, não em ${wanted.x},${wanted.y}`);
    check(start.listed.length === 0, `a janela nova lista ${start.listed.join(", ")}`);
    check(start.workspace === null && start.tab === null, `a janela nova mostra ${start.workspace}/${start.tab}`);
    check(start.innerWidth > 0 && start.innerHeight > 0, "a janela nova não tem tamanho");
    check(severalWindows.value && peers.value.length === 2, "a lista de janelas não tem duas");
    return `${second}: vazia, posta em ${put.x},${put.y} com ${put.width}×${put.height} (a primeira, ${at.x},${at.y} com ${size.width}×${size.height}), agora ${start.innerWidth}×${start.innerHeight}`;
  });

  await step("janelas: Ir para adota o workspace na janela nova, e a mesma aba leva a marca nas duas", async () => {
    select(SPACE, tabs[0]);
    await ask(second, "go", { workspace: SPACE, tab: tabs[0] });
    await seen(second, (s) => s.listed.join() === SPACE && s.tab === tabs[0], "a janela nova mostrar a aba");
    check(shownWorkspaces.value.some((w) => w.name === SPACE), "a primeira deixou de listar o workspace");
    check(shownWorkspaces.value.length > 1 || workspaces.value.length === 1, "a primeira listou só o workspace");
    // Each says the tab is in front elsewhere, and this one draws the mark, in the row and in the sidebar.
    await seen(second, (s) => s.elsewhere, "a janela nova saber que a aba está em frente em outra");
    check(shownElsewhere(peers.value, windowLabel.value, SPACE, tabs[0]), "esta janela não sabe que a aba está em frente na outra");
    await front();
    await waitFor(() => stripTab(SPACE, tabs[0])?.querySelector(".window-mark"), 10000, "a marca na fileira daqui").catch((e) => {
      throw new Error(`${e.message}; janelas: ${peersText()}; fileira: ${tabIds(".strip .tab", "data-tab")}`);
    });
    await waitFor(() => sidebarRow(SPACE, tabs[0])?.querySelector(".window-mark"), 5000, "a marca na barra lateral daqui");
    await photo("janelas-marca", windows, 1);
    // Two viewers on the one tab.
    const viewers = await waitForAsync(async () => {
      const tab = (await api.list()).find((w) => w.name === SPACE)?.tabs.find((t) => t.id === tabs[0]);
      return tab && tab.clients >= 2 ? tab.clients : null;
    }, 10000, "dois visualizadores na aba");
    return `${SPACE} na ${second}; a marca, na fileira e na barra lateral daqui e no estado das duas; ${viewers} visualizadores`;
  });

  await step("janelas: o terminal fora de vista numa janela diz ao keepd que não está olhando", async () => {
    const [a, b] = tabs;
    const view = await waitFor(() => terminals.existing(SPACE, a), 10000, "o terminal da aba");
    await waitFor(() => view.isAttached, 10000, "o terminal anexado");
    check(view.told.cols > 0 && view.told.rows > 0, `à vista, o terminal diz ${view.told.cols}×${view.told.rows}`);
    const real = { ...view.told };
    // The tab goes out of sight in this window, and stays in the other one's.
    select(SPACE, b);
    await waitFor(() => view.told.cols === 0 && view.told.rows === 0, 5000, "o terminal fora de vista dizer 0×0");
    select(SPACE, a);
    await waitFor(() => view.told.cols === real.cols && view.told.rows === real.rows, 5000, "o terminal de volta dizer o tamanho dele");
    return `à vista ${real.cols}×${real.rows}; fora de vista 0×0; de volta ${view.told.cols}×${view.told.rows}`;
  });

  await step("janelas: o que se muda para todas (nome, zoom) chega à outra janela", async () => {
    const [a] = tabs;
    renameTab(SPACE, a, "Compartilhada");
    zoom(1);
    const level = gui.value.zoom;
    const there = await seen(second, (s) => s.tabNames[tabKey(SPACE, a)] === "Compartilhada" && s.zoom === level, "o nome e o zoom na outra janela");
    // Put back, and the other window hears that too.
    zoomReset();
    await seen(second, (s) => s.zoom === 1, "o zoom de volta na outra janela");
    renameTab(SPACE, a, "");
    await seen(second, (s) => !(tabKey(SPACE, a) in s.tabNames), "o nome tirado na outra janela");
    return `nome e zoom ${there.zoom} → 1`;
  });

  await step("janelas: guardar tira da lista de uma janela só; na última, guarda de verdade", async () => {
    // The second window's list has the workspace; this one's has it too.
    check(shownWorkspaces.value.some((w) => w.name === SPACE), "a primeira não lista o workspace");
    // Out of the second: another lists it, so nothing is put away.
    await ask(second, "putAway", { workspace: SPACE });
    const out = await seen(second, (s) => !s.listed.includes(SPACE), "o workspace sair da lista da janela nova");
    check(!gui.value.hidden.includes(SPACE), "guardou, e outra janela ainda o listava");
    check(out.hidden.length === gui.value.hidden.length, "as janelas discordam do que está guardado");
    // Out of the last one listing it: put away for everybody — once this window knows the second no longer lists it.
    await waitFor(() => !peers.value.find((p) => p.label === second)?.placement.carries.includes(SPACE), 5000, "esta janela saber que a outra não lista mais");
    putAway(SPACE);
    await waitFor(() => gui.value.hidden.includes(SPACE), 5000, "o workspace guardado");
    const away = await seen(second, (s) => s.hidden.includes(SPACE), "a outra janela saber que está guardado");
    // Gone to from the second: listed there alone — the first remembered its place, and does not list it again.
    await ask(second, "go", { workspace: SPACE, tab: tabs[0] });
    const back = await seen(second, (s) => s.listed.includes(SPACE) && !s.hidden.includes(SPACE), "a janela nova listar de novo");
    await waitFor(() => !gui.value.hidden.includes(SPACE), 5000, "o workspace não mais guardado, daqui");
    check(!shownWorkspaces.value.some((w) => w.name === SPACE), "a primeira voltou a listar o que a outra pegou");
    return `guardado em duas, solto numa; ${away.hidden.length} guardado(s) → ${back.hidden.length}`;
  });

  await step("janelas: arrancar uma aba da fileira abre uma janela com ela, e a janela de origem segue para a vizinha", async () => {
    // Back in this window, the first tab in front, and the three in the row.
    select(SPACE, tabs[0]);
    await front();
    await waitFor(() => stripTab(SPACE, tabs[0]) && stripTab(SPACE, tabs[2]), 5000, "as abas na fileira");
    const tab = stripTab(SPACE, tabs[0])!;
    const strip = document.querySelector(".strip")!.getBoundingClientRect();
    const from = middle(tab);
    // Down out of the row, past the threshold, and let go over no window of the app: where the system would say
    // is the screen's and the windows' (src-tauri tests it), and the check says it for itself.
    const asks = hooks.windowUnder;
    hooks.windowUnder = async () => null;
    let hung = false;
    try {
      await pull(tab, line(from, { x: from.x, y: strip.bottom + 150 }), () => {
        hung = document.querySelector(".strip.reordering .tab.torn") !== null && document.querySelector(".drag-ghost") !== null;
      });
    } finally {
      hooks.windowUnder = asks;
    }
    check(hung, "a aba não saiu da fileira: sem a cópia pendurada no ponteiro");
    const third = await waitFor(() => peers.value.find((p) => p.label !== windowLabel.value && p.label !== second)?.label, 20000, "a janela da aba");
    const start = await awaitWindow(third);
    check(start.listed.join() === SPACE && start.tab === tabs[0], `a janela da aba lista ${start.listed} e mostra ${start.tab}`);
    // This one went on to the tab beside it, and the torn one wears the mark.
    await waitFor(() => activeTab.value?.root.id === tabs[1], 5000, "a janela de origem seguir para a aba vizinha");
    await waitFor(() => shownElsewhere(peers.value, windowLabel.value, SPACE, tabs[0]), 10000, "esta janela saber que a aba arrancada está em outra");
    await front();
    await waitFor(() => stripTab(SPACE, tabs[0])?.querySelector(".window-mark"), 10000, "a marca na aba arrancada");
    check(idsOf(SPACE).join() === tabs.join(), "a aba saiu do workspace");
    await photo("janelas-arrancada", windows, 3);
    // And closed again, which leaves nothing put away: the workspace is listed by the others too.
    await ask(third, "close");
    await waitFor(() => !peers.value.some((p) => p.label === third), 10000, "a janela da aba sair da lista");
    return `${third} nasceu com a aba ${tabs[0]}; a origem foi para a ${tabs[1]}`;
  });

  await step("janelas: soltar a aba arrancada sobre outra janela a leva para lá", async () => {
    select(SPACE, tabs[1]);
    await front();
    await waitFor(() => stripTab(SPACE, tabs[2]), 5000, "as abas na fileira");
    // Let go over the second window.
    const tab = stripTab(SPACE, tabs[2])!;
    const strip = document.querySelector(".strip")!.getBoundingClientRect();
    const from = middle(tab);
    const asks = hooks.windowUnder;
    hooks.windowUnder = async () => second;
    try {
      await pull(tab, line(from, { x: from.x, y: strip.bottom + 150 }));
    } finally {
      hooks.windowUnder = asks;
    }
    await seen(second, (s) => s.tab === tabs[2], "a aba na outra janela").catch(async (e) => {
      const now = await ask(second, "snapshot");
      throw new Error(`${e.message}; a janela tem ${JSON.stringify(now.listed)} ${now.workspace}/${now.tab}; ${peersText()}`);
    });
    check(peers.value.length === 2, `há ${peers.value.length} janelas, e devia seguir 2`);
    check(activeTab.value?.root.id === tabs[1], "a janela de origem mudou de aba sem ser a arrancada");
    return `a aba ${tabs[2]} foi para a ${second}`;
  });

  await step("janelas: fechar a janela guarda o que só ela listava", async () => {
    // Both list the workspace; this one lets go of it, and the second is the only one that lists it.
    check(shownWorkspaces.value.some((w) => w.name === SPACE), "a primeira não lista o workspace");
    putAway(SPACE);
    check(!gui.value.hidden.includes(SPACE), "guardou, e a outra janela ainda o listava");
    check(!shownWorkspaces.value.some((w) => w.name === SPACE), "a primeira ainda lista o workspace");
    await ask(second, "close");
    await waitFor(() => peers.value.length === 1, 10000, "a janela sair da lista");
    await waitFor(() => gui.value.hidden.includes(SPACE), 5000, "o workspace guardado");
    check(!shownWorkspaces.value.some((w) => w.name === SPACE), "o workspace voltou à lista da primeira");
    // Alone again, a terminal out of sight tells the daemon its own size, as it always did.
    select(SPACE, tabs[0]);
    const view = await waitFor(() => terminals.existing(SPACE, tabs[0]), 10000, "o terminal");
    select(SPACE, tabs[1]);
    await sleep(500);
    check(view.told.cols > 0, "sozinha, a aba fora de vista ainda diz 0×0");
    // Gone to, it comes back to the only window there is.
    select(SPACE, tabs[0]);
    check(shownWorkspaces.value.some((w) => w.name === SPACE) && !gui.value.hidden.includes(SPACE), "o workspace não voltou");
    return `guardado ao fechar; sozinha, a aba fora de vista diz ${view.told.cols}×${view.told.rows}`;
  });

  await step("janelas: encerrar o workspace da prova", async () => {
    const closing = closeWorkspace(SPACE);
    // The question is answered by the dialog's own button.
    await waitFor(() => dialog.value, 5000, "a pergunta de fechar o workspace");
    answer("close");
    await closing;
    await waitFor(() => !workspaces.value.some((w) => w.name === SPACE), 10000, "o workspace sumir");
    return "encerrado";
  });
}
