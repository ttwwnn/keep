// The terminal's part of the scripted check (see e2e.ts): what only a real PTY
// shows — on Windows, ConPTY between a program and the page. The keyboard
// protocol a program turns on has to reach the page through it, be answered,
// and the keys sent back have to reach the program as they were written; the
// title a program sets has to reach the tab; and the paths the page asks the
// system about have to be told as the system has them.

import * as api from "./api";
import { tabsByWorkspace, terminals } from "./store";
import type { TermView } from "./terminals";

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;

interface Helpers {
  waitFor<T>(what: () => T | undefined | null | false, timeoutMs: number, everyMs?: number): Promise<T>;
  /** The workspace the check works in. */
  workspace: string;
  /** The check runs on a real Windows: PowerShell is the shell. */
  windows: boolean;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The last lines of a screen, for a step that did not get what it waited for to say where it was. */
const tail = (text: string) =>
  text
    .split("\n")
    .filter((line) => line.trim() !== "")
    .slice(-8)
    .join("\n");

export async function terminalSteps(step: StepRunner, { waitFor, workspace, windows }: Helpers): Promise<void> {
  const viewOf = (): TermView | undefined => {
    const root = tabsByWorkspace.value.get(workspace)?.[0]?.root;
    return root ? terminals.existing(workspace, root.id) : undefined;
  };
  const atPrompt = (view: TermView) => (windows ? /PS [^\n]*>\s*$/.test(view.allText().trimEnd()) : /[$%❯#]$/.test(view.allText().trimEnd()));

  await step("teclado Kitty: a consulta, as flags e as teclas passam pelo PTY até o programa", async () => {
    const view = viewOf();
    if (!view) throw new Error("sem terminal");
    await waitFor(() => atPrompt(view), 30000);
    const until = async (what: string, check: () => boolean, ms = 20000) => {
      try {
        await waitFor(check, ms);
      } catch {
        throw new Error(`esperei ${ms} ms por ${what}; a tela:\n${tail(view.allText())}`);
      }
    };
    const heard = (hex: string) => view.allText().includes(`sonda> ${hex}`);
    const press = (init: KeyboardEventInit) =>
      view.term.textarea?.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
    try {
      // e2e/sonda-teclado.mjs, by the path run.ps1 gave it: it turns the protocol on, asks, and writes down what it is sent.
      view.term.input(windows ? 'node "$env:KEEP_E2E_SONDA"\r' : 'node "$KEEP_E2E_SONDA"\r', true);

      // Its push (CSI > 5 u) reached the page through the PTY, which answered its question (CSI ? u), and the answer reached it.
      await until("a resposta da página à consulta (ESC [ ? 5 u)", () => heard("1b5b3f3575"));
      if (view.kittyFlags() !== 5) throw new Error(`a página tem as flags ${view.kittyFlags()}, e o programa ligou 5`);

      // Keys with the protocol on, as the program asked: each arrives as the sequence it was written.
      press({ key: "Enter", code: "Enter", shiftKey: true });
      await until("Shift+Enter como ESC [ 1 3 ; 2 u", () => heard("1b5b31333b3275"));
      press({ key: "Escape", code: "Escape" });
      await until("Esc como ESC [ 2 7 u", () => heard("1b5b323775"));
      press({ key: "a", code: "KeyA", ctrlKey: true });
      await until("Ctrl+A como ESC [ 9 7 ; 5 u", () => heard("1b5b39373b3575"));
      press({ key: "c", code: "KeyC", ctrlKey: true });
      await until("Ctrl+C como ESC [ 9 9 ; 5 u, sem interromper o programa", () => heard("1b5b39393b3575"));
      // What types is typed, not written as a sequence.
      view.term.input("z", true);
      await until("a letra z como ela é (7a)", () => heard("7a"));

      // A plain q ends it; its pop (CSI < u) reaches the page the same way, and the keys are the terminal's again.
      view.term.input("q", true);
      await until("o fim da sonda", () => view.allText().includes("sonda: fim"));
      await until("a página sem flags depois do fim", () => view.kittyFlags() === 0);
      await until("o prompt de volta", () => atPrompt(view));
      return "consulta respondida (?5u), Shift+Enter, Esc, Ctrl+A e Ctrl+C como CSI u, a letra como texto, flags de volta a 0";
    } finally {
      // A step that failed leaves the probe, or a REPL where the probe was not found, in the pane: ended, so the steps after it have a shell.
      if (!atPrompt(view)) {
        view.term.input("q", true);
        await sleep(300);
        view.term.input("\x03\x03", true);
        await waitFor(() => atPrompt(view), 10000).catch(() => {});
      }
      // And the protocol off on the page's side, whatever the program did not get to say.
      view.term.write("\x1b[<u");
    }
  });

  await step("título: o que o programa diz chega à aba", async () => {
    const view = viewOf();
    if (!view) throw new Error("sem terminal");
    await waitFor(() => atPrompt(view), 30000);
    const title = "keep-e2e-titulo";
    // The daemon's title of the tab: what the sidebar shows unless the person gave the tab a name, as the check did the first one.
    const titleOf = () => tabsByWorkspace.value.get(workspace)?.[0]?.root.title ?? "";
    const command = windows ? `[Console]::Write("$([char]27)]0;${title}$([char]7)")` : `printf '\\033]0;${title}\\007'`;
    const sent = performance.now();
    view.term.input(`${command}\r`, true);
    try {
      await waitFor(() => titleOf().includes(title), 5000, 50);
    } catch {
      throw new Error(`o título da aba é “${titleOf()}”; a tela:\n${tail(view.allText())}`);
    }
    return `o título da aba passou a “${titleOf()}” em ${Math.round(performance.now() - sent)} ms`;
  });

  await step("caminhos: o sistema confirma o que existe, e não o que não existe nem os nomes de dispositivo", async () => {
    const folder = windows ? "C:\\Windows\\System32" : "/tmp";
    const file = windows ? "C:\\Windows\\System32\\drivers\\etc\\hosts" : "/etc/hosts";
    const missing = windows ? "C:\\Windows\\keep-e2e-nao-existe" : "/keep-e2e-nao-existe";
    const groups = [[folder], [missing, file], [missing], ...(windows ? [["CON"], ["NUL"], ["C:\\Windows\\CON"]] : [])];
    const hits = await api.findPaths(groups);
    const said = JSON.stringify(hits);
    if (!hits[0] || !hits[0].dir) throw new Error(`a pasta não foi confirmada: ${said}`);
    if (!hits[1] || hits[1].dir || hits[1].index !== 1) throw new Error(`o arquivo não foi achado entre os dois: ${said}`);
    if (hits[2] !== null) throw new Error(`o que não existe foi confirmado: ${said}`);
    if (windows && hits.slice(3).some((hit) => hit !== null)) throw new Error(`um nome de dispositivo foi confirmado: ${said}`);
    return said;
  });
}
