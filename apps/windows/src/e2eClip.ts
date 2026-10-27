// The clipboard's part of the scripted check (see e2e.ts): the files the
// Explorer copies have to reach the page as their paths, and be typed in the
// pane as the shell there reads them; text has to go on being pasted as it was.
// The shell puts them on the clipboard itself (SetFileDropList writes the same
// CF_HDROP the Explorer's Ctrl+C does), so only the Windows run has them.
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
    // The screen with the breaks where the pane wrapped a long line taken out (the pane is narrow).
    const flat = () => view.allText().replace(/\n/g, "");
    await until("o prompt", atPrompt, 30000);
    try {
      // A file with a blank in its name, so that the escape shows; the marker is built so that the echo of the command does not contain it.
      // Windows PowerShell, in a single-threaded apartment as the clipboard wants, puts the file on it (PowerShell 7's Set-Clipboard has no -Path).
      const copy =
        "Add-Type -AssemblyName System.Windows.Forms; $l = New-Object System.Collections.Specialized.StringCollection; " +
        "[void]$l.Add($env:KEEP_E2E_CLIPFILE); [System.Windows.Forms.Clipboard]::SetFileDropList($l)";
      const command = `$env:KEEP_E2E_CLIPFILE = Join-Path $env:TEMP 'keep e2e clip.txt'; Set-Content -Path $env:KEEP_E2E_CLIPFILE -Value x; powershell.exe -NoProfile -STA -Command '${copy}'; Write-Host ('keep-e2e-' + 'clip-pronto')\r`;
      const copied = () => view.allText().includes("keep-e2e-clip-pronto") && atPrompt();
      view.term.input(command, true);
      // The Windows PowerShell that puts the file there stayed past twenty seconds twice in some fifty runs, in a step
      // that takes four; it is the check's own setup, not the paste: what was in front is said, and it is asked again.
      let again = "";
      try {
        await waitFor(copied, 20000);
      } catch {
        again = `; a cópia foi pedida de novo (na 1ª, ${tabsByWorkspace.value.get(workspace)?.[0]?.root.command || "?"} ficou em frente por 20 s)`;
        view.term.input("\x03", true);
        await until("o prompt depois de interromper a cópia", atPrompt);
        view.term.input(command, true);
        await until("a cópia do arquivo", copied, 40000);
      }

      const paths = await api.clipboardFiles();
      if (paths.length !== 1 || !paths[0].endsWith("keep e2e clip.txt")) {
        throw new Error(`o comando devolveu ${JSON.stringify(paths)}; a tela:\n${tail(view.allText())}`);
      }
      // Pasted as the app pastes when asked for it: PowerShell reads a blank in a name inside double quotes.
      await view.pasteFromSystem();
      await until('o caminho entre aspas no prompt', () => flat().includes('keep e2e clip.txt"'));
      view.term.input("\x03", true);
      await until("o prompt depois de descartar a linha", atPrompt);

      // Text on the clipboard is pasted as text.
      view.term.input("Set-Clipboard -Value ('texto-' + 'e2e-clip'); Write-Host ('keep-e2e-' + 'texto-pronto')\r", true);
      await until("a cópia do texto", () => view.allText().includes("keep-e2e-texto-pronto") && atPrompt());
      if ((await api.clipboardFiles()).length !== 0) throw new Error("o texto copiado ainda mostra arquivos");
      await view.pasteFromSystem();
      await until("o texto no prompt", () => flat().includes("texto-e2e-clip"));
      view.term.input("\x03", true);
      await until("o prompt no fim", atPrompt);
      return `${paths[0]} digitado entre aspas; o texto, como texto${again}`;
    } finally {
      // A step that failed leaves what it typed on the line, and the steps after it need a clean prompt.
      if (!atPrompt()) {
        view.term.input("\x03", true);
        await waitFor(() => atPrompt(), 10000).catch(() => {});
      }
    }
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
