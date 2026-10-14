// The scripted check of what the tabs and the panes can do, run with the rest
// of e2e.ts: tabs carried along the row and the sidebar, to another workspace,
// panes carried by their handle, the divider between them, the arrows, a name
// typed in one place, a workspace put away, the vertical tabs and the tabs put
// to sleep.
//
// The pointer is the page's own: the same events it would be sent, dispatched
// from here — on a Windows the system's pointer is out of the script's reach —
// so what is measured is the logic and what is drawn, not the machine's mouse.

import * as api from "./api";
import { SPLIT_RIGHT } from "./api";
import { awake } from "./hibernation";
import { LEAST_COLS } from "./paneActions";
import { settle } from "./sleepWatch";
import { shares } from "./splits";
import { tabKey } from "./state";
import {
  activePane,
  activeTab,
  activeWorkspace,
  answer,
  closePane,
  createWorkspace,
  dialog,
  editing,
  focusPane,
  gui,
  log,
  newTab,
  putAway,
  refresh,
  renameTab,
  select,
  shownWorkspaces,
  split,
  startRename,
  tabLabel,
  tabsByWorkspace,
  terminals,
  toggleSidebar,
  toggleVerticalTabs,
  workspaces,
} from "./store";
import { menu } from "./ui/common";

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;
interface Point {
  x: number;
  y: number;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

async function waitFor<T>(what: () => T | undefined | null | false, timeoutMs: number, wanted: string | (() => string)): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = what();
    if (value) return value as T;
    if (Date.now() > deadline) throw new Error(`${typeof wanted === "string" ? wanted : wanted()}: não aconteceu em ${timeoutMs} ms`);
    await sleep(100);
  }
}

function check(condition: unknown, otherwise: string): asserts condition {
  if (!condition) throw new Error(otherwise);
}

/**
 * A photograph of the window as it stands. On a Windows the harness takes it
 * (`e2eShot`); elsewhere the run says so in the app's log and holds the
 * picture still long enough for whoever is watching the log to take it.
 */
async function photo(name: string, windows: boolean): Promise<void> {
  if (windows) await api.e2eShot(name).catch(() => {});
  else {
    log(`tela ${name}`);
    await sleep(1500);
  }
}

// ------------------------------------------------------------------ the pointer

let pointerId = 40;

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
function line(a: Point, b: Point, steps = 8): Point[] {
  return Array.from({ length: steps }, (_, i) => ({
    x: a.x + ((b.x - a.x) * (i + 1)) / steps,
    y: a.y + ((b.y - a.y) * (i + 1)) / steps,
  }));
}

/**
 * Press on `from`, pull through `via` and let go at the last point. `held` runs
 * with the pointer at the last point and the button still down, to look at what
 * is drawn then.
 */
async function drag(from: Element, via: Point[], held?: () => Promise<void> | void): Promise<void> {
  pointerId += 1;
  const start = middle(from);
  fire(from, "pointerdown", start, 1);
  for (const at of via) {
    fire(window, "pointermove", at, 1);
    await sleep(35);
  }
  await sleep(120);
  if (held) await held();
  fire(window, "pointerup", via[via.length - 1] ?? start, 0);
  await sleep(200);
}

const one = <T extends Element>(selector: string): T => {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`sem ${selector} na tela`);
  return found;
};

const all = <T extends Element>(selector: string): T[] => [...document.querySelectorAll<T>(selector)];

/** The tabs of a workspace, as their ids. */
const idsOf = (workspace: string): number[] => (tabsByWorkspace.value.get(workspace) ?? []).map((t) => t.root.id);

/** The row above the terminal, as drawn: the tab of each, by id. */
const stripIds = (): number[] => all<HTMLElement>(".strip .tab").map((e) => Number((e.dataset.tab ?? "").split("\u0000")[1]));

/** A workspace's rows in the sidebar, as drawn: the tab of each, by id. */
const rowIds = (workspace: string): number[] =>
  all<HTMLElement>(`.sidebar .workspace[data-workspace="${CSS.escape(workspace)}"] .tab-row[data-row]`).map((e) =>
    Number((e.dataset.row ?? "").split("\u0000")[1]),
  );

/** What the page holds just now, for a step that did not find what it was waiting for to say. */
const now = (): string =>
  [
    `terminais: ${terminals.all().map((v) => `${v.workspace}/${v.tab}`)}`,
    `acordadas: ${[...awake.value].map((k) => k.replace("\u0000", "/"))}`,
    `campos: ${all("input.inline-editor").map((e) => e.parentElement?.className)}`,
    `editando: ${JSON.stringify(editing.value)}`,
  ].join("; ");

const A = "e2e-abas";
const B = "e2e-destino";
const H = "e2e-sono";

/** Text written straight into a tab's terminal and scrolled up into its scrollback. */
function marked(workspace: string, tab: number, text: string): void {
  const view = terminals.existing(workspace, tab);
  check(view, `a aba ${tab} de ${workspace} não tem terminal`);
  view.term.write(`\r\n${text}\r\n${"\r\n".repeat(view.term.rows + 4)}`);
}

export async function tabsSteps(step: StepRunner, windows: boolean, home: string): Promise<void> {
  let tabs: number[] = [];

  await step("abas: arrastar uma aba pela fileira muda a ordem", async () => {
    await createWorkspace(A, null);
    await newTab(A);
    await newTab(A);
    tabs = await waitFor(() => (idsOf(A).length === 3 ? idsOf(A) : null), 20000, "o workspace com três abas");
    const [a, b, c] = tabs;
    select(A, a);
    await waitFor(() => stripIds().length === 3, 5000, "três abas na fileira");
    const strip = all<HTMLElement>(".strip .tab");
    const from = middle(strip[0]);
    const to = middle(strip[2]);
    await drag(strip[0], line(from, { x: to.x + 6, y: from.y }, 10), async () => {
      check(document.querySelector(".strip.reordering .tab.carried"), "a aba levada não está marcada");
      await photo("abas-arrasto", windows);
    });
    await waitFor(() => idsOf(A).join() === [b, c, a].join(), 3000, "a nova ordem").catch(() => {
      throw new Error(`a ordem é ${idsOf(A)}, não ${[b, c, a]}`);
    });
    check(stripIds().join() === [b, c, a].join(), `a fileira mostra ${stripIds()}`);
    check(gui.value.tabOrder[A]?.join() === [b, c, a].join(), `o estado guarda ${gui.value.tabOrder[A]}`);
    check(activeTab.value?.root.id === a, "a aba em frente mudou com o arrasto");
    return `${[a, b, c]} → ${idsOf(A)}`;
  });

  await step("abas: arrastar uma aba na barra lateral reordena, e sobre outro workspace a move com o terminal", async () => {
    const [b, c, a] = idsOf(A);
    // The last row to the first place.
    let rows = all<HTMLElement>(`.sidebar .workspace[data-workspace="${A}"] .tab-row`);
    const last = middle(rows[2]);
    const first = middle(rows[0]);
    await drag(rows[2], line(last, { x: first.x, y: first.y - 4 }, 8), () => photo("abas-barra-arrasto", windows));
    await waitFor(() => idsOf(A).join() === [a, b, c].join(), 3000, "a ordem pela barra lateral").catch(() => {
      throw new Error(`a ordem é ${idsOf(A)}, não ${[a, b, c]}`);
    });
    check(rowIds(A).join() === [a, b, c].join(), `a barra lateral mostra ${rowIds(A)}`);

    // Another workspace to take it, and something written in the terminal that only that terminal has.
    await createWorkspace(B, null);
    select(A, a);
    const view = await waitFor(() => terminals.existing(A, b), 10000, "o terminal da aba que vai mudar");
    marked(A, b, "marcador-aba-movida");
    await waitFor(() => view.allText().includes("marcador-aba-movida"), 5000, "o marcador no terminal");
    await photo("abas-barra", windows);

    rows = all<HTMLElement>(`.sidebar .workspace[data-workspace="${A}"] .tab-row`);
    const header = one<HTMLElement>(`.sidebar .workspace[data-workspace="${B}"] .workspace-row`);
    const source = rows.find((r) => r.dataset.row === tabKey(A, b));
    check(source, "sem a linha da aba");
    const there = idsOf(B);
    await drag(source, line(middle(source), middle(header), 10), async () => {
      check(document.querySelector(".workspace-row.offered"), "o cabeçalho de destino não acendeu");
      await photo("abas-para-outro-workspace", windows);
    });
    const moved = await waitFor(() => idsOf(B).find((id) => !there.includes(id)), 10000, "a aba no workspace de destino");
    check(idsOf(A).join() === [a, c].join(), `sobraram ${idsOf(A)} em ${A}`);
    check(idsOf(B).at(-1) === moved, "a aba movida não ficou no fim");
    const same = await waitFor(() => terminals.existing(B, moved), 5000, () => `o terminal religado ao novo nome (${now()})`);
    check(same === view, "o terminal foi refeito em vez de religado");
    check(same.allText().includes("marcador-aba-movida"), "o terminal perdeu o que tinha");
    check(!terminals.existing(A, b), "o terminal ficou com o nome antigo");
    return `aba ${b} de ${A} agora é ${moved} em ${B}, com o mesmo terminal`;
  });

  await step("painéis: a alça troca dois painéis de lugar, e o nome da aba segue", async () => {
    select(A, tabs[2]);
    renameTab(A, tabs[2], "Painéis");
    await split(SPLIT_RIGHT);
    await waitFor(() => (activeTab.value?.panes.length ?? 0) === 2, 10000, "a aba com dois painéis");
    const root = activeTab.value!.root.id;
    const [left, right] = (await waitFor(() => (all(".tab-view.shown .pane").length === 2 ? all<HTMLElement>(".tab-view.shown .pane") : null), 5000, "dois painéis na tela")).map((e) =>
      Number(e.dataset.pane),
    );
    check(left === root, "o painel da esquerda não é o primeiro");
    const grips = await waitFor(() => (all(".tab-view.shown .pane-grip").length === 2 ? all<HTMLElement>(".tab-view.shown .pane-grip") : null), 5000, "as duas alças");
    const target = one<HTMLElement>(`.tab-view.shown .pane[data-pane="${left}"]`);
    const hand = middle(grips[1]);
    await drag(grips[1], line(hand, middle(target), 10), async () => {
      check(document.querySelector(".pane-landing"), "o pouso não aparece");
      check(document.querySelector(".pane-card"), "o cartão não aparece");
      await photo("paineis-troca", windows);
    });
    // The seat on the left is the other pane's now, and the tab, which is the root's, carries on under the heir.
    await waitFor(() => activeTab.value?.panes[0]?.id === right, 5000, "os painéis trocados").catch(() => {
      throw new Error(`os painéis estão em ${activeTab.value?.panes.map((p) => p.id)}`);
    });
    check(activeTab.value?.root.id === right, "a raiz da aba não passou ao outro painel");
    const label = tabLabel(A, activeTab.value!, 0);
    check(label === "Painéis", `a aba se chama “${label}”`);
    return `painéis ${left},${right} → ${activeTab.value?.panes.map((p) => p.id)}; a aba segue “${label}”`;
  });

  await step("painéis: o divisor se arrasta, com limite, e o selo diz o tamanho", async () => {
    const tab = activeTab.value!;
    const first = tab.panes[0].id;
    const view = terminals.existing(A, first);
    check(view, "sem terminal no primeiro painel");
    const before = view.term.cols;
    const divider = one<HTMLElement>(".tab-view.shown .split-divider");
    const at = middle(divider);
    await drag(divider, line(at, { x: at.x + 160, y: at.y }, 8), async () => {
      check(document.querySelector(".size-badge.shown"), "o selo do tamanho não apareceu");
      await photo("paineis-divisor", windows);
    });
    await sleep(300);
    check(view.term.cols > before, `o painel tem ${view.term.cols} colunas, tinha ${before}`);
    check(Object.keys(shares.value).length > 0, "a razão do divisor não ficou guardada");
    // Past the edge it stops where a few columns are left.
    const far = middle(one<HTMLElement>(".tab-view.shown .split-divider"));
    await drag(one<HTMLElement>(".tab-view.shown .split-divider"), line(far, { x: far.x + 4000, y: far.y }, 6));
    await sleep(300);
    const other = terminals.existing(A, tab.panes[1].id);
    check(other, "sem terminal no segundo painel");
    const wide = view.term.cols;
    const atLimit = other.term.cols;
    check(atLimit >= LEAST_COLS - 2 && atLimit <= LEAST_COLS + 4, `o outro painel ficou com ${atLimit} colunas, no limite de ${LEAST_COLS}`);
    // A double-click puts it back in the middle.
    one<HTMLElement>(".tab-view.shown .split-divider").dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    await waitFor(() => Math.abs(view.term.cols - other.term.cols) <= 2, 3000, "o divisor de volta ao meio");
    return `${before} → ${wide} colunas no primeiro; no limite o outro ficou com ${atLimit}; no meio, ${view.term.cols} e ${other.term.cols}`;
  });

  await step("painéis: Alt+setas movem o foco e Alt+Shift+setas movem o divisor", async () => {
    const tab = activeTab.value!;
    const [left, right] = tab.panes.map((p) => p.id);
    const press = (code: string, shift = false) =>
      terminals.existing(A, activePane.value!.id)?.term.textarea?.dispatchEvent(
        new KeyboardEvent("keydown", { code, key: code, altKey: true, shiftKey: shift, bubbles: true, cancelable: true }),
      );
    select(A, tab.root.id);
    // Said to the window rather than asked of the page: a window that is not in front is not given focus events.
    focusPane(A, tab.root.id, left);
    await waitFor(() => activePane.value?.id === left, 3000, "o foco no painel da esquerda");
    press("ArrowRight");
    await waitFor(() => activePane.value?.id === right, 3000, "o foco no painel da direita").catch(() => {
      throw new Error(`o foco ficou em ${activePane.value?.id}`);
    });
    press("ArrowLeft");
    await waitFor(() => activePane.value?.id === left, 3000, "o foco de volta no painel da esquerda");
    const view = terminals.existing(A, left)!;
    const before = view.term.cols;
    for (let i = 0; i < 4; i++) {
      press("ArrowRight", true);
      await sleep(60);
    }
    await sleep(300);
    check(view.term.cols > before, `Alt+Shift+→ não alargou o painel (${before} → ${view.term.cols})`);
    return `foco ${left} → ${right} → ${left}; ${before} → ${view.term.cols} colunas`;
  });

  await step("painéis: a alça, solta fora, faz uma aba nova do painel", async () => {
    const tab = activeTab.value!;
    const count = idsOf(A).length;
    const grips = all<HTMLElement>(".tab-view.shown .pane-grip");
    const sidebar = one<HTMLElement>(".sidebar").getBoundingClientRect();
    // The sidebar's bare ground, under everything in it.
    const bare = { x: sidebar.left + sidebar.width / 2, y: sidebar.bottom - 260 };
    await drag(grips[1], line(middle(grips[1]), bare, 8));
    await waitFor(() => idsOf(A).length === count + 1, 10000, "a aba nova do painel").catch(() => {
      throw new Error(`o workspace tem ${idsOf(A).length} abas, não ${count + 1}`);
    });
    check((tabsByWorkspace.value.get(A) ?? []).every((t) => t.panes.length === 1), "ainda há uma aba com painéis");
    return `${count} → ${idsOf(A).length} abas (a aba ${tab.root.id} deixou de ter painéis)`;
  });

  await step("painéis: o painel segurado sobre uma aba da fileira a abre, e soltar nela junta o painel", async () => {
    const x = (await newTab(A))!;
    const y = (await newTab(A))!;
    select(A, x);
    await split(SPLIT_RIGHT);
    await waitFor(() => tabsByWorkspace.value.get(A)?.find((t) => t.root.id === x)?.panes.length === 2, 10000, "a aba com dois painéis");
    const grips = await waitFor(() => (all(".tab-view.shown .pane-grip").length === 2 ? all<HTMLElement>(".tab-view.shown .pane-grip") : null), 5000, "as duas alças");
    const pane = tabsByWorkspace.value.get(A)!.find((t) => t.root.id === x)!.panes[1].id;
    const strip = () => all<HTMLElement>(".strip .tab").find((e) => e.dataset.tab === tabKey(A, y));
    const tab = strip();
    check(tab, "sem a aba de destino na fileira");
    await drag(grips[1], line(middle(grips[1]), middle(tab), 10), async () => {
      // Held over the tab for a moment, it opens.
      await waitFor(() => activeTab.value?.root.id === y, 2000, "a aba abrir sob o painel");
      await photo("paineis-mola", windows);
    });
    await waitFor(() => tabsByWorkspace.value.get(A)?.find((t) => t.root.id === y)?.panes.some((p) => p.id === pane), 10000, "o painel na outra aba");
    const left = tabsByWorkspace.value.get(A)?.find((t) => t.root.id === x);
    check(left && left.panes.length === 1, `a aba de origem ficou com ${left?.panes.length} painéis`);
    check(activeTab.value?.root.id === y, "a aba de destino não ficou em frente");
    return `o painel ${pane} saiu da aba ${x} e entrou na ${y}, que abriu sob ele`;
  });

  await step("painéis: fechar o painel raiz passa o nome e o lugar da aba ao herdeiro", async () => {
    const x = (await newTab(A))!;
    select(A, x);
    renameTab(A, x, "Herdada");
    await split(SPLIT_RIGHT);
    const heir = await waitFor(() => tabsByWorkspace.value.get(A)?.find((t) => t.root.id === x)?.panes[1]?.id, 10000, "a aba com dois painéis");
    const place = idsOf(A).indexOf(x);
    const closing = closePane(A, x);
    await waitFor(() => dialog.value, 3000, "a pergunta de fechar o painel");
    answer("close");
    await closing;
    const tab = await waitFor(() => tabsByWorkspace.value.get(A)?.find((t) => t.root.id === heir), 10000, "o herdeiro como aba");
    check(tab.panes.length === 1, "o herdeiro ficou com painéis demais");
    check(tabLabel(A, tab, 0) === "Herdada", `a aba se chama “${tabLabel(A, tab, 0)}”`);
    check(idsOf(A).indexOf(heir) === place, `o herdeiro está no lugar ${idsOf(A).indexOf(heir)}, não no ${place}`);
    check(activeTab.value?.root.id === heir, "a aba em frente não seguiu o herdeiro");
    renameTab(A, heir, "");
    return `o painel ${heir} é a aba ${x} agora, no lugar ${place}, com o nome`;
  });

  await step("workspaces: arrastar o cabeçalho pela barra lateral reordena a lista", async () => {
    const names = () => all<HTMLElement>(".sidebar .workspace").map((e) => e.dataset.workspace);
    check(names().indexOf(A) < names().indexOf(B), `${A} devia vir antes de ${B}: ${names()}`);
    const header = one<HTMLElement>(`.sidebar .workspace[data-workspace="${B}"] .workspace-row`);
    const above = middle(one<HTMLElement>(`.sidebar .workspace[data-workspace="${A}"] .workspace-row`));
    await drag(header, line(middle(header), { x: above.x, y: above.y - 8 }, 10), () => photo("workspaces-arrasto", windows));
    await waitFor(() => names().indexOf(B) < names().indexOf(A), 3000, "a nova ordem dos workspaces");
    check(gui.value.order.indexOf(B) < gui.value.order.indexOf(A), "o estado não guarda a ordem");
    // And back.
    const again = one<HTMLElement>(`.sidebar .workspace[data-workspace="${A}"] .workspace-row`);
    const below = middle(one<HTMLElement>(`.sidebar .workspace[data-workspace="${B}"] .workspace-row`));
    await drag(again, line(middle(again), { x: below.x, y: below.y + 8 }, 10));
    await waitFor(() => names().indexOf(A) < names().indexOf(B), 3000, "a ordem de volta");
    return `${B} subiu e ${A} voltou a subir`;
  });

  await step("renomear: Enter sem mudar não fixa o título, e há um só campo", async () => {
    const root = activeTab.value!.root.id;
    const key = tabKey(A, root);
    if (gui.value.tabNames[key]) renameTab(A, root, "");
    startRename(A, root);
    const editor = await waitFor(() => document.querySelector<HTMLInputElement>(".strip input.inline-editor"), 3000, "o campo do nome na fileira");
    check(all("input.inline-editor").length === 1, `${all("input.inline-editor").length} campos de nome`);
    editor.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await waitFor(() => !document.querySelector("input.inline-editor"), 3000, "o campo fechar com Enter");
    check(gui.value.tabNames[key] === undefined, `o título foi fixado como “${gui.value.tabNames[key]}”`);
    // In the sidebar, the same, and once typed over, it is kept.
    startRename(A, root, "sidebar");
    const field = await waitFor(() => document.querySelector<HTMLInputElement>(".sidebar input.inline-editor"), 3000, "o campo do nome na barra lateral");
    check(all("input.inline-editor").length === 1, "dois campos de nome");
    await photo("renomear-barra", windows);
    field.value = "Nome dado";
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await waitFor(() => gui.value.tabNames[key] === "Nome dado", 3000, "o nome dado guardado");
    // Esc leaves it as it was.
    startRename(A, root);
    const again = await waitFor(() => document.querySelector<HTMLInputElement>(".strip input.inline-editor"), 3000, () => `o campo do nome na fileira, de novo (${now()})`);
    again.value = "outro";
    again.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => !document.querySelector("input.inline-editor"), 3000, "o campo fechar com Esc");
    check(gui.value.tabNames[key] === "Nome dado", "o Esc mudou o nome");
    renameTab(A, root, "");
    return "Enter sem mudar não grava, a barra lateral grava, Esc volta atrás";
  });

  await step("renomear: o clique lento no nome da aba em frente abre o campo, e em outra aba só a escolhe", async () => {
    const [shown, other] = [activeTab.value!.root.id, idsOf(A).find((id) => id !== activeTab.value!.root.id)!];
    const click = (element: Element) => element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1 }));
    const title = (id: number) => {
      const tab = all<HTMLElement>(".strip .tab").find((e) => e.dataset.tab === tabKey(A, id));
      check(tab, `sem a aba ${id} na fileira`);
      return tab.querySelector<HTMLElement>(".tab-title")!;
    };
    click(title(other));
    await sleep(800);
    check(!document.querySelector("input.inline-editor"), "o clique numa aba que não estava em frente abriu o campo");
    await waitFor(() => activeTab.value?.root.id === other, 3000, "a aba clicada em frente");
    click(title(other));
    await waitFor(() => document.querySelector(".strip input.inline-editor"), 2000, "o campo aberto pelo clique lento");
    document.querySelector(".strip input.inline-editor")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => !document.querySelector("input.inline-editor"), 3000, "o campo fechar");
    // Anything done while it waits calls it off.
    click(title(other));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
    await sleep(800);
    check(!document.querySelector("input.inline-editor"), "uma tecla não chamou a espera de volta");
    select(A, shown);
    return "só a aba em frente, só depois da espera, e uma tecla cancela";
  });

  await step("workspaces: remover da barra lateral guarda, e Ir para traz de volta", async () => {
    putAway(B);
    await waitFor(() => !document.querySelector(`.sidebar .workspace[data-workspace="${B}"]`), 3000, "o workspace sair da barra lateral");
    check(workspaces.value.some((w) => w.name === B), "o workspace deixou de existir");
    check(gui.value.hidden.includes(B), "o estado não guarda o workspace");
    check(!shownWorkspaces.value.some((w) => w.name === B), "a lista da barra lateral ainda o tem");
    select(B);
    await waitFor(() => document.querySelector(`.sidebar .workspace[data-workspace="${B}"]`), 3000, "o workspace voltar à barra lateral");
    check(!gui.value.hidden.includes(B), "continua guardado");
    check(activeWorkspace.value === B, "o workspace não ficou em frente");
    return "saiu da barra lateral, seguiu rodando e voltou ao ser visitado";
  });

  await step("abas verticais: a fileira sai e as abas ficam na barra lateral", async () => {
    select(A, idsOf(A)[0]);
    await waitFor(() => stripIds().length > 1, 3000, "a fileira com abas");
    // From the empty stretch of the row itself, as a person would.
    one<HTMLElement>(".strip").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 10, clientY: 10 }));
    const item = await waitFor(() => document.querySelector<HTMLElement>(".context-menu .menu-item"), 3000, "o menu da fileira");
    // The first of the menu's items is the vertical tabs' (the others are the window's: a new one).
    const first = menu.value?.items[0];
    check(first && first !== "separator" && first.label === "Abas só na barra lateral", "o primeiro item do menu da fileira não é o das abas verticais");
    item.click();
    await waitFor(() => stripIds().length === 0, 3000, "a fileira sair"); 
    check(rowIds(A).length === idsOf(A).length, "a barra lateral perdeu abas");
    await photo("abas-verticais", windows);
    // Nowhere to type a name in place: it is asked for.
    toggleSidebar();
    const root = activeTab.value!.root.id;
    startRename(A, root);
    await waitFor(() => dialog.value?.title === "Renomear a aba", 3000, "a pergunta do nome");
    answer("rename", { name: "Pela pergunta" });
    await waitFor(() => gui.value.tabNames[tabKey(A, root)] === "Pela pergunta", 3000, "o nome dado pela pergunta");
    renameTab(A, root, "");
    toggleSidebar();
    toggleVerticalTabs();
    await waitFor(() => stripIds().length === idsOf(A).length, 3000, "a fileira voltar");
    return "sem a fileira, com as abas na barra lateral; sem ela, o nome é perguntado; e de volta";
  });

  await step("hibernação: com mais de seis abas as menos recentes adormecem e acordam com a rolagem", async () => {
    await createWorkspace(H, null);
    const first = await waitFor(() => idsOf(H)[0], 10000, "a primeira aba do workspace do sono");
    await waitFor(() => terminals.existing(H, first), 10000, "o terminal da primeira aba");
    marked(H, first, "marcador-sono");
    await waitFor(() => terminals.existing(H, first)?.allText().includes("marcador-sono"), 5000, "o marcador do sono");
    for (let i = 0; i < 7; i++) await newTab(H);
    await waitFor(() => idsOf(H).length === 8, 30000, "oito abas");
    const chosen = settle();
    check(chosen.length > 0, "nenhuma aba adormeceu");
    // One left awake that something holds on to is allowed: a selection, the keyboard.
    check(awake.value.size <= 7, `${awake.value.size} abas acordadas`);
    check(chosen.includes(tabKey(H, first)), "a mais antiga não foi escolhida");
    await waitFor(() => !terminals.existing(H, first), 5000, "a aba mais antiga adormecer").catch(() => {
      throw new Error("a aba mais antiga ainda tem terminal");
    });
    const front = activeTab.value!.root.id;
    check(terminals.existing(H, front), "a aba em frente ficou sem terminal");
    await refresh();
    select(H, first);
    const view = await waitFor(() => terminals.existing(H, first), 10000, "o terminal da aba acordada");
    await waitFor(() => view.allText().includes("marcador-sono"), 10000, "a rolagem de volta").catch(() => {
      throw new Error("a rolagem não voltou");
    });
    check(awake.value.has(tabKey(H, first)), "a aba acordada não consta como acordada");
    await photo("hibernacao", windows);
    return `${chosen.length} abas dormiram; a mais antiga voltou com a rolagem; ${awake.value.size} acordadas`;
  });

  // What the steps made goes away, and the tab the rest of the script knows is in front again, with its terminal.
  for (const name of [A, B, H]) await api.killWorkspace(name).catch(() => {});
  await refresh();
  const first = idsOf(home)[0];
  if (first !== undefined) {
    select(home, first);
    await waitFor(() => terminals.existing(home, first), 10000, "o terminal da aba de casa").catch(() => {});
  }
}
