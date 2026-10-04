// The git worktrees a tab's conversation made, which go to the Recycle Bin
// when the tab is closed (docs/ia.md, "Lixeira de worktrees").
//
// The rule is the person's, from the macOS app: a worktree belongs to the tab
// whose conversation created it, an open tab is in use, and when in doubt it
// is not the tab's. Which worktrees those are is the core's to work out
// (`keep worktrees listar`), and it must be asked before the tab closes:
// after, the conversation in it is gone, and so is the only live link
// between the tab and what it made. This file says what the question adds;
// the move itself is the backend's (`ia_trash`), so it finishes even if the
// window does not.

/** What to ask about: a workspace and the daemon tabs in it being closed. */
export interface WorktreeTarget {
  workspace: string;
  tabs: number[];
}

export const targetArgument = (t: WorktreeTarget) => `${t.workspace}:${t.tabs.join(",")}`;

export interface WorktreeItem {
  caminho: string;
  ramo?: string | null;
  head?: string | null;
  alteracoes?: number | null;
  commits_so_aqui?: number | null;
  processos?: { pid: number; nome?: string | null }[] | null;
  /** The birth the core saw, as text (nanoseconds do not fit a number here). */
  nasceu_ns?: string | null;
}

export interface Listing {
  versao: number;
  abas?: { alvo: string; pids?: number[] | null }[] | null;
  lixeira?: WorktreeItem[] | null;
  mantidas?: { caminho: string; motivo: string }[] | null;
  avisos?: string[] | null;
}

export type WorktreeAnswer =
  /** No core to ask, or one that does not know worktrees: nothing to say, nothing to move. */
  | { kind: "off" }
  | { kind: "listing"; listing: Listing }
  /** The core was asked and did not answer: said in the question, and nothing moves. */
  | { kind: "failed"; why: string };

/** The processes that end with the tabs: waited for before anything moves. */
export const listingPids = (listing: Listing) => (listing.abas ?? []).flatMap((a) => a.pids ?? []);

/** A path under the home, written from "~" — Windows compares paths in any case, with either slash. */
export function tilde(path: string, home: string): string {
  const base = home.replace(/[\\/]+$/, "");
  if (!base) return path;
  const fold = (s: string) => s.replace(/\//g, "\\").toLowerCase();
  if (fold(path) === fold(base)) return "~";
  const head = path.slice(0, base.length);
  const rest = path.slice(base.length);
  return fold(head) === fold(base) && /^[\\/]/.test(rest) ? `~${rest}` : path;
}

/** One folder's line: where, and what in it is not anywhere else. */
export function describeWorktree(item: WorktreeItem, home: string): string {
  const facts: string[] = [];
  if (item.ramo) facts.push(`ramo ${item.ramo}`);
  else if (item.head) facts.push(`sem ramo, em ${item.head}`);
  if (item.alteracoes && item.alteracoes > 0) {
    facts.push(item.alteracoes === 1 ? "1 arquivo alterado" : `${item.alteracoes} arquivos alterados`);
  }
  if (item.commits_so_aqui && item.commits_so_aqui > 0) {
    facts.push(item.commits_so_aqui === 1 ? "1 commit só nela" : `${item.commits_so_aqui} commits só nela`);
  }
  if (item.processos && item.processos.length > 0) {
    facts.push(`ainda rodando dentro: ${item.processos.map((p) => p.nome || `pid ${p.pid}`).join(", ")}`);
  }
  return tilde(item.caminho, home) + (facts.length > 0 ? ` (${facts.join("; ")})` : "");
}

/**
 * What the close question adds: which folders go, which stay and why, or
 * that nothing could be checked. Empty when there is nothing to say.
 * `subject` is what is being closed: "desta aba", "deste painel", "deste workspace".
 */
export function worktreeNote(answer: WorktreeAnswer, subject: string, home: string): string {
  if (answer.kind === "off") return "";
  if (answer.kind === "failed") {
    return `\n\nNão consegui verificar as worktrees ${subject} (${answer.why}); nenhuma será movida.`;
  }
  const parts: string[] = [];
  const going = answer.listing.lixeira ?? [];
  if (going.length > 0) {
    let text =
      going.length === 1
        ? `A worktree ${subject} vai para a Lixeira:`
        : `As ${going.length} worktrees ${subject} vão para a Lixeira:`;
    for (const item of going) text += `\n• ${describeWorktree(item, home)}`;
    // Restoring brings the files back, not the worktree: its entry in the
    // repository is dropped so the branch is free again.
    text +=
      "\nOs arquivos voltam pelo “Restaurar” da Lixeira, já fora do git; " +
      "o ramo continua no repositório e os commits e alterações ficam guardados em refs/keep-lixeira.";
    parts.push(text);
  }
  const kept = answer.listing.mantidas ?? [];
  if (kept.length > 0) {
    let text = kept.length === 1 ? "Fica onde está:" : "Ficam onde estão:";
    for (const item of kept) text += `\n• ${tilde(item.caminho, home)} — ${item.motivo}`;
    parts.push(text);
  }
  parts.push(...(answer.listing.avisos ?? []));
  return parts.length > 0 ? `\n\n${parts.join("\n\n")}` : "";
}

/** The answer to `listar`, read: exit 2 is a core that does not know worktrees. */
export function readListing(answer: { _saida: number; versao?: number; [k: string]: unknown } | null, error?: string): WorktreeAnswer {
  if (error !== undefined) return { kind: "failed", why: error };
  if (!answer || answer._saida === 2) return { kind: "off" };
  if (answer._saida !== 0) return { kind: "failed", why: `o keep saiu com ${answer._saida}` };
  return { kind: "listing", listing: answer as unknown as Listing };
}

/** The line that says what did not go. */
export function trashProblems(problems: string[]): string {
  return `Nem todas as worktrees foram para a Lixeira:\n${problems.map((p) => `• ${p}`).join("\n")}`;
}
