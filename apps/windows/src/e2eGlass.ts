// The scripted check's steps for the translucent ground (see e2e.ts): what only the real system says. Off, the
// window is the one it always was; asked for in a window that cannot have it, it asks for the app to be opened
// again; the page wears the alpha in the real webview; and the panel's window is transparent for its corners,
// with the materials applied and cleared without harm. The look of it all is not asked: that is for eyes.

import * as api from "./api";
import { setGlassOpacity } from "./appearance";
import { effectiveBlur, needsRestart } from "./glass";
import { wearGlass } from "./glassStore";
import { gui, info, palette, tabsByWorkspace, terminals } from "./store";

type Step = (name: string, run: () => Promise<string>) => Promise<void>;

const root = () => document.documentElement;

export async function glassSteps(step: Step, workspace: string): Promise<void> {
  const view = () => {
    const tab = (tabsByWorkspace.value.get(workspace) ?? [])[0];
    return tab ? terminals.existing(workspace, tab.root.id) : undefined;
  };

  await step("fundo translúcido: desligado, a janela é a de sempre", async () => {
    const glass = gui.value.glass;
    if (glass.opacity !== 1 || glass.blur !== "none") throw new Error(`o padrão não é opaco: ${JSON.stringify(glass)}`);
    if (root().classList.contains("glass")) throw new Error("a página veste o fundo translúcido sem pedido");
    if (info.value?.windowTransparent) throw new Error("a janela foi feita transparente sem pedido");
    if (view()?.term.options.allowTransparency) throw new Error("o terminal deixa o fundo passar sem pedido");
    return "opaco, sem janela transparente";
  });

  await step("fundo translúcido: pedido numa janela opaca, pede para reabrir e nada muda na tela", async () => {
    setGlassOpacity(0.8);
    try {
      if (gui.value.glass.opacity !== 0.8) throw new Error(`o estado guardou ${gui.value.glass.opacity}`);
      if (!needsRestart(gui.value.glass, info.value?.windowTransparent === true)) throw new Error("não pediu para reabrir");
      if (root().classList.contains("glass")) throw new Error("a página ficou translúcida numa janela que não pode");
      if (view()?.term.options.allowTransparency) throw new Error("o terminal ficou translúcido numa janela que não pode");
    } finally {
      setGlassOpacity(1);
    }
    return "pede para reabrir; a tela fica como estava";
  });

  await step("fundo translúcido: a página veste o alfa e o fundo do corpo some", async () => {
    wearGlass(palette.value, 0.8);
    try {
      if (!root().classList.contains("glass")) throw new Error("sem a classe");
      const ground = getComputedStyle(root()).getPropertyValue("--ground-glass").trim();
      if (!/^rgba\(\d+, \d+, \d+, 0\.8\)$/.test(ground)) throw new Error(`o fundo ficou em "${ground}"`);
      const body = getComputedStyle(document.body).backgroundColor;
      if (body !== "rgba(0, 0, 0, 0)") throw new Error(`o corpo ficou em ${body}, e o inline devia ceder ao !important`);
      const bar = document.querySelector(".titlebar");
      const strip = bar && getComputedStyle(bar).backgroundColor;
      if (strip !== ground) throw new Error(`a barra de título ficou em ${strip} e devia estar em ${ground}`);
    } finally {
      wearGlass(palette.value, 1);
    }
    if (root().classList.contains("glass") || getComputedStyle(root()).getPropertyValue("--ground-glass").trim() !== "") {
      throw new Error("não largou o fundo translúcido");
    }
    return "classe, variável e cores vistas no webview";
  });

  await step("fundo translúcido: a janela rápida é transparente, o app não, e os materiais entram e saem", async () => {
    const before = await api.glassStatus();
    const windows = info.value?.platform === "windows";
    if (before.transparent.includes("main")) throw new Error(`a janela principal foi feita transparente: ${before.transparent.join(", ")}`);
    if (windows && !before.transparent.includes("quick")) throw new Error(`a janela rápida devia ser transparente: ${before.transparent.join(", ") || "nenhuma"}`);
    // The windows that cannot show a material are not touched; the panel can, and takes them without harm.
    for (const blur of ["acrylic", "mica", "none"] as const) {
      await api.glassApply(blur, true);
      const now = await api.glassStatus();
      // What the system lacks is none; and an app that has never worn one says nothing yet.
      const wanted = effectiveBlur({ opacity: 0.8, blur }, info.value?.platform ?? "", info.value?.windowsBuild ?? 0);
      const shown = now.applied ?? "none";
      if (shown !== wanted) throw new Error(`pediu ${blur}, o app diz ${shown} e devia dizer ${wanted}`);
    }
    return `transparentes: ${before.transparent.join(", ") || "nenhuma"}; materiais pedidos e retirados`;
  });
}
