// Files dropped on a terminal, and what is typed for them: their paths.
//
// The macOS app types each path with the characters a shell reads as syntax
// escaped by a backslash (Display/DroppedFiles.swift), several of them
// separated by blanks; that is what a shell wants at its prompt, and what
// Claude Code looks for to attach an image. Windows has more than one shell
// to say it to: PowerShell and cmd take double quotes (and PowerShell a
// single-quoted string where the path has a `$` or a backtick, which it
// would expand), and a WSL shell wants the Linux path, escaped the Mac's way.

import { quotePaths } from "./text";

export type ShellKind = "powershell" | "wsl" | "other";

/** Which shell a pane is at, by the name of what runs in it (`pwsh`, `wsl`, `claude`…). */
export function shellKind(command: string): ShellKind {
  const name = command.trim().toLowerCase().replace(/\.exe$/, "");
  if (name === "pwsh" || name === "powershell") return "powershell";
  if (name === "wsl" || /^(?:ubuntu|debian|kali|opensuse|sles|oracle|fedora|alpine|arch)/.test(name)) return "wsl";
  return "other";
}

/**
 * The Linux path of a Windows one: a drive is `/mnt/<drive>`, a path under
 * `\\wsl$\<distro>` or `\\wsl.localhost\<distro>` is the path inside it.
 * Undefined for any other share.
 */
export function toWslPath(path: string): string | undefined {
  const drive = /^([A-Za-z]):(?:[\\/](.*))?$/.exec(path);
  if (drive) return `/mnt/${drive[1].toLowerCase()}${drive[2] ? "/" + drive[2].replace(/\\/g, "/") : ""}`;
  const distro = /^\\\\wsl(?:\$|\.localhost)\\[^\\]+(?:\\(.*))?$/i.exec(path);
  if (distro) return "/" + (distro[1] ?? "").replace(/\\/g, "/");
  return undefined;
}

/** Ghostty's set — the characters a shell takes as syntax inside a word — and the caret, a glob operator to zsh. */
const SPECIAL = new Set([..."\\ ()[]{}<>\"'`!#$&;|*?\t^"]);

/** A path with each character a shell would take as syntax escaped by a backslash (macOS's way, and a POSIX shell's). */
export function escapePosix(path: string): string {
  let out = "";
  for (const char of path) {
    if (SPECIAL.has(char)) out += "\\";
    out += char;
  }
  return out;
}

/** A path as PowerShell reads it: bare when it is plain, double-quoted when it has blanks, single-quoted when it has a `$` or a backtick. */
function powershell(path: string): string {
  if (/[$`']/.test(path)) return `'${path.replace(/'/g, "''")}'`;
  return quotePaths([path]);
}

/** What is typed for the dropped paths: one after another, said the way the pane's shell reads them. */
export function dropText(paths: string[], shell: ShellKind): string {
  return paths
    .filter((path) => path.length > 0)
    .map((path) => {
      if (shell === "wsl") {
        const linux = toWslPath(path);
        if (linux !== undefined) return escapePosix(linux);
      }
      return shell === "powershell" ? powershell(path) : quotePaths([path]);
    })
    .join(" ");
}

let lit: HTMLElement | null = null;

/** Light the pane a dragged file is over, which is where its path will be typed, and put out the light on the one before. */
export function lightPane(next: HTMLElement | null): void {
  if (next === lit) return;
  lit?.classList.remove("drop-target");
  next?.classList.add("drop-target");
  lit = next;
}

/** The pane under a point the window reports in pixels of the screen. */
export function paneAt(position: { x: number; y: number }): HTMLElement | null {
  const ratio = window.devicePixelRatio || 1;
  return document.elementFromPoint(position.x / ratio, position.y / ratio)?.closest<HTMLElement>(".pane") ?? null;
}
