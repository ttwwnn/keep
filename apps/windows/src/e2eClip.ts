// The clipboard's part of the scripted check (see e2e.ts): the files the
// Explorer copies have to reach the page as their paths, and be typed in the
// pane as the shell there reads them; text has to go on being pasted as it was.
// The shell puts them on the clipboard itself (Set-Clipboard -Path writes the
// same CF_HDROP the Explorer's Ctrl+C does), so only the Windows run has them.
// And the other thing only a real PTY shows: that mode 2027, switched on by a
// program, gets through ConPTY to the page, which then clusters (graphemes.ts).

import * as api from "./api";
import { tabsByWorkspace, terminals } from "./store";
import { CLUSTERED, PLAIN } from "./graphemes";
import type { TermView } from "./terminals";

type StepRunner = (name: string, run: () => Promise<string>) => Promise<void>;

interface Helpers {
  waitFor<T>(what: () => T | undefined | null | false, timeoutMs: number, everyMs?: number): Promise<T>;
  workspace: string;
  windows: boolean;
}

/** The last lines of a screen, for a step that did not get what it waited for to say where it was. */
const tail = (text: string) =>
  text
    .split("\n")
    .filter((line) => line.trim() !== "")
    .slice(-8)
    .join("\n");

export async function clipSteps(step: StepRunner, { waitFor, workspace, windows }: Helpers): Promise<void> {
  await step("colar: arquivos copiados viram caminhos digitados; texto continua texto", async () => {
    if (!windows) {
      const none = await api.clipboardFiles();
      if (none.length !== 0) throw new Error(`fora do Windows o comando devolveu ${JSON.stringify(none)}`);
      return "fora do Windows: nenhum arquivo, como deve ser";
    }
    const root = tabsByWorkspace.value.get(workspace)?.[0]?.root;
    const view: TermView | undefined = root ? terminals.existing(workspace, root.id) : undefined;
    if (!view) throw new Error("sem terminal");
    const atPrompt = () => /PS [^\n]*>\s*$/.test(view.allText().trimEnd());
    const until = async (what: string, check: () => boolean, ms = 20000) => {
      try {
        await waitFor(check, ms);
      } catch {
        throw new Error(`esperei ${ms} ms por ${what}; a tela:\n${tail(view.allText())}`);
      }
    };
    await until("o prompt", atPrompt, 30000);

    // A file with a blank in its name, so that the escape shows; the marker is built so that the echo of the command does not contain it.
    view.term.input(
      "$f = Join-Path $env:TEMP 'keep e2e clip.txt'; Set-Content -Path $f -Value x; Set-Clipboard -Path $f; Write-Host ('keep-e2e-' + 'clip-pronto')\r",
      true,
    );
    await until("a cópia do arquivo", () => view.allText().includes("keep-e2e-clip-pronto") && atPrompt());

    const paths = await api.clipboardFiles();
    if (paths.length !== 1 || !paths[0].endsWith("keep e2e clip.txt")) {
      throw new Error(`o comando devolveu ${JSON.stringify(paths)}; a tela:\n${tail(view.allText())}`);
    }
    // Pasted as the app pastes when asked for it: PowerShell reads a blank in a name inside double quotes.
    await view.pasteFromSystem();
    await until('o caminho entre aspas no prompt', () => view.allText().includes('keep e2e clip.txt" '));
    view.term.input("\x03", true);
    await until("o prompt depois de descartar a linha", atPrompt);

    // Text on the clipboard is pasted as text.
    view.term.input("Set-Clipboard -Value ('texto-' + 'e2e-clip'); Write-Host ('keep-e2e-' + 'texto-pronto')\r", true);
    await until("a cópia do texto", () => view.allText().includes("keep-e2e-texto-pronto") && atPrompt());
    if ((await api.clipboardFiles()).length !== 0) throw new Error("o texto copiado ainda mostra arquivos");
    await view.pasteFromSystem();
    await until("o texto no prompt", () => view.allText().includes("texto-e2e-clip"));
    view.term.input("\x03", true);
    await until("o prompt no fim", atPrompt);
    return `${paths[0]} digitado entre aspas; o texto, como texto`;
  });

  await step("grafemas: o modo 2027 que o programa liga chega à página pelo PTY", async () => {
    const root = tabsByWorkspace.value.get(workspace)?.[0]?.root;
    const view: TermView | undefined = root ? terminals.existing(workspace, root.id) : undefined;
    if (!view) throw new Error("sem terminal");
    const atPrompt = () => (windows ? /PS [^\n]*>\s*$/.test(view.allText().trimEnd()) : /[$%❯#]$/.test(view.allText().trimEnd()));
    await waitFor(() => atPrompt(), 30000);
    const say = (on: boolean) =>
      windows ? `[Console]::Write("$([char]27)[?2027${on ? "h" : "l"}")` : `printf '\\033[?2027${on ? "h" : "l"}'`;
    try {
      view.term.input(`${say(true)}\r`, true);
      try {
        await waitFor(() => view.term.unicode.activeVersion === CLUSTERED, 5000, 50);
      } catch {
        throw new Error(`a página ficou na regra “${view.term.unicode.activeVersion}” depois de CSI ? 2027 h; a tela:\n${tail(view.allText())}`);
      }
      view.term.input(`${say(false)}\r`, true);
      try {
        await waitFor(() => view.term.unicode.activeVersion === PLAIN, 5000, 50);
      } catch {
        throw new Error(`a página ficou na regra “${view.term.unicode.activeVersion}” depois de CSI ? 2027 l; a tela:\n${tail(view.allText())}`);
      }
      return "ligado: a regra com grafemas; desligado: a regra do Unicode 11";
    } finally {
      // A step that failed leaves the mode on the page's side whatever the program said.
      view.term.write("\x1b[?2027l");
    }
  });
}
