// The picker's part of the scripted check (see e2e.ts): the card as a person
// uses it — opened by its key, typed into, moved through, chosen from — against
// the tabs the check opened. What the page draws is looked at in the page, so a
// card that does not come up in the real WebView fails here and not in a
// screenshot somebody has to open.

import * as api from "./api";
import { fontPreview } from "./fonts";
import { activePane, activeTab, focusPane, info, isMac, overlay, select, tabsByWorkspace } from "./store";
import { themePreview } from "./themes";

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;

interface Helpers {
  waitFor<T>(what: () => T | undefined | null | false, timeoutMs: number, everyMs?: number): Promise<T>;
  /** The workspace the check works in. */
  workspace: string;
}

const rows = (kind = "") => [...document.querySelectorAll<HTMLElement>(`.picker .pk-row${kind}`)];
const field = () => document.querySelector<HTMLInputElement>(".picker .pk-field");
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";

export async function paletteSteps(step: StepRunner, { waitFor, workspace }: Helpers): Promise<void> {
  const tabs = () => tabsByWorkspace.value.get(workspace) ?? [];
  /** Type into the field as a person does: the page hears an input event. */
  const type = async (typed: string) => {
    const input = await waitFor(field, 3000);
    input.value = typed;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  };
  /** A key pressed in whatever has the keyboard in the card. */
  const press = (target: Element | null, key: string, init: KeyboardEventInit = {}) =>
    target?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
  const open = async (card: "picker" | "palette" | "find" | "search") => {
    overlay.value = card;
    await waitFor(field, 3000);
  };
  /** When each wait of a step was passed, so that one that fails says how far it had got. */
  const trace: string[] = [];
  const began = performance.now();
  /** `waitFor` that says what it was waiting for, since a step with several waits is told apart by that alone. */
  const until = async <T>(what: string, check: () => T | undefined | null | false, ms = 3000): Promise<T> => {
    try {
      const found = await waitFor(check, ms);
      trace.push(`${what} ${Math.round(performance.now() - began)}`);
      return found;
    } catch {
      throw new Error(`esperei ${ms} ms por ${what} (passou: ${trace.join("; ") || "nada"})`);
    }
  };
  const putAway = async () => {
    overlay.value = null;
    await waitFor(() => !document.querySelector(".picker"), 3000);
  };

  await step("Ir para: lista as abas, filtra pelo workspace e vai até a escolhida", async () => {
    const all = tabs();
    if (all.length < 2) throw new Error("preciso de duas abas");
    // Visited in this order: the first, then the second, which is in front.
    select(workspace, all[0].root.id);
    select(workspace, all[1].root.id);
    await open("picker");
    const listed = await waitFor(() => (rows(".running").length > 0 ? rows(".running") : null), 5000);
    const headings = [...document.querySelectorAll(".picker .pk-heading")].map((h) => text(h));
    if (!headings.includes("terminais")) throw new Error(`os cabeçalhos: ${headings.join(" | ")}`);
    // The tab in front is not a place to go: of this workspace's, one fewer is listed.
    const ours = listed.filter((r) => text(r.querySelector(".pk-context")).startsWith(workspace));
    if (ours.length !== all.length - 1) throw new Error(`${ours.length} linhas do workspace para ${all.length} abas: ${listed.map((r) => text(r)).join(" | ")}`);
    await type(workspace);
    await waitFor(() => rows(".running").length > 0, 3000);
    press(field(), "Enter");
    await waitFor(() => !document.querySelector(".picker"), 3000);
    await waitFor(() => activeTab.value?.root.id === all[0].root.id, 3000);
    return `${listed.length} linha(s), ${ours.length} do workspace; foi para a aba ${all[0].root.id}: ${text(ours[0])}`;
  });

  await step("ações: Ctrl+K abre o painel da linha, Esc o fecha e o seletor segue aberto", async () => {
    await open("picker");
    await waitFor(() => rows(".running").length > 0, 5000);
    // The window's modifier: Ctrl on Windows, ⌘ on the Mac the app is developed on.
    press(field(), "k", { code: "KeyK", ctrlKey: !isMac.value, metaKey: isMac.value });
    const panel = await waitFor(() => document.querySelector(".picker .pk-actions"), 3000);
    const entries = [...panel.querySelectorAll(".pk-action .pk-action-label")].map((e) => text(e));
    if (!entries.some((e) => e.startsWith("Ir para este terminal")) || !entries.some((e) => e.startsWith("Fechar este terminal"))) {
      throw new Error(`as ações: ${entries.join(" | ")}`);
    }
    if (info.value?.platform === "windows") await api.e2eShot("seletor-acoes");
    press(document.querySelector(".picker .pk-actions-field"), "Escape");
    await waitFor(() => !document.querySelector(".picker .pk-actions"), 3000);
    if (!document.querySelector(".picker")) throw new Error("o Esc do painel fechou o seletor também");
    await putAway();
    return entries.join(" | ");
  });

  await step("paleta: lista os comandos e executa Nova aba", async () => {
    await open("palette");
    const commands = await waitFor(() => (rows(".command").length >= 20 ? rows(".command") : null), 3000);
    const dim = commands.filter((c) => c.classList.contains("disabled")).map((c) => text(c));
    await type("nova aba");
    const before = tabs().length;
    const row = await waitFor(() => rows(".command").find((r) => text(r).startsWith("Nova aba") && r.classList.contains("selected")), 3000);
    if (info.value?.platform === "windows") await api.e2eShot("paleta");
    press(field(), "Enter");
    await waitFor(() => tabs().length === before + 1, 15000);
    await putAway();
    return `${commands.length} comandos (cinza: ${dim.join(", ") || "nenhum"}); ${text(row)}`;
  });

  await step("paleta: Mudar o tema… e Mudar a fonte… põem a lista no lugar do cartão, e o Esc volta à paleta", async () => {
    const said: string[] = [];
    trace.length = 0;
    // The themes are eight hundred rows, drawn again on every key: the list is narrowed first, as a person who knows the name would.
    const lists = [
      { typed: "mudar o tema", title: "Mudar o tema", list: "tema", looking: themePreview, narrow: "dracula" },
      { typed: "mudar a fonte", title: "Mudar a fonte", list: "fonte", looking: fontPreview, narrow: "" },
    ];
    try {
      for (const { typed, title, list, looking, narrow } of lists) {
        await open("palette");
        await type(typed);
        // The typed words have been applied once the row they name is the one the selection is on.
        await until(`a linha "${title}" selecionada`, () => rows(".command").find((r) => r.classList.contains("selected") && text(r).startsWith(title)));
        press(field(), "Enter");
        const input = await until(`o campo da lista de ${list}`, () => document.querySelector<HTMLInputElement>(".catalog .catalog-input"));
        if (document.querySelector(".picker")) throw new Error(`o cartão da paleta ficou sob a lista de ${list}`);
        const names = await until(`as linhas da lista de ${list}`, () => (document.querySelectorAll(".catalog .catalog-row").length > 1 ? document.querySelectorAll(".catalog .catalog-row") : null), 8000);
        if (narrow) {
          input.value = narrow;
          input.dispatchEvent(new Event("input", { bubbles: true }));
          await until(`a lista de ${list} só com o que tem "${narrow}"`, () => document.querySelectorAll(".catalog .catalog-row").length < 10);
        }
        // Looked at, not chosen: the window wears the row the selection is on. A key that comes in the very frame the list was drawn in can be undone by the list settling, so it is pressed again, slowly, if nothing was worn.
        let pressedAt = -Infinity;
        await until(
          `a lista de ${list} vestir o que a seta tocou`,
          () => {
            if (looking.value === null && performance.now() - pressedAt > 1000) {
              pressedAt = performance.now();
              press(input, "ArrowDown");
            }
            return looking.value !== null;
          },
          8000,
        );
        const seen = looking.value;
        press(input, "Escape");
        await until("a paleta de volta no lugar da lista", () => document.querySelector(".picker.pk-palette .pk-field") && !document.querySelector(".catalog"));
        await until(`o que se viu na lista de ${list} deixar de estar vestido`, () => looking.value === null);
        const back = field();
        await until("o teclado de volta no campo da paleta", () => document.activeElement === back);
        if (back?.value) throw new Error(`a paleta voltou com "${back.value}" no campo`);
        await putAway();
        said.push(`${list}: ${names.length} linhas, viu "${seen}" e voltou`);
      }
    } finally {
      // A step that fails leaves nothing on screen for the steps after it.
      overlay.value = null;
      themePreview.value = null;
      fontPreview.value = null;
    }
    return said.join("; ");
  });

  await step("busca: acha a linha que o shell escreveu, no painel e em tudo, e vai até ela", async () => {
    const first = tabs()[0];
    select(workspace, first.root.id);
    focusPane(workspace, first.root.id, first.root.id);
    await waitFor(() => activePane.value?.id === first.root.id, 3000);
    await open("find");
    await type("keep-e2e-42");
    const hits = await waitFor(() => (rows(".hit").length > 0 ? rows(".hit") : null), 5000);
    const counter = text(document.querySelector(".picker .pk-counter"));
    if (!/^1 de \d+$/.test(counter)) throw new Error(`o contador: "${counter}"`);
    await putAway();
    await open("search");
    await type("keep-e2e-42");
    const everywhere = await waitFor(() => (rows(".hit").length > 0 ? rows(".hit") : null), 5000);
    if (info.value?.platform === "windows") await api.e2eShot("busca");
    press(field(), "Enter");
    await waitFor(() => !document.querySelector(".picker"), 3000);
    await waitFor(() => activeTab.value?.root.id === first.root.id, 3000);
    return `${hits.length} no painel, ${everywhere.length} em tudo; ${text(everywhere[0])}`;
  });
}
