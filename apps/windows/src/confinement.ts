// The folder a workspace's AI is held to: what the choice looks like, and
// which folders cannot hold it.
//
// The macOS app keeps the choice in ~/.config/keep/confinement.json
// (WorkspaceConfinement.swift) and so does this one (src-tauri/src/confinement.rs).
// It is the shell side that holds the AI to it when it starts
// (`keep ia confinar`, crates/keep-ia/src/confinar_win.rs): an AI already
// running keeps the access it had, which the button says.

/**
 * Whether the button and the menu items are offered. The AI is held to the
 * folder by a restricted token that `keep ia confinar` puts on the program
 * and proves before it starts it (the part that touches Windows is in
 * crates/keep-ia/src/confinar_win/); if that cannot be shown to hold on a
 * computer, the AI does not start there. Turn this off if the mechanism has to
 * be pulled back: the choice already made stays in the file and the button
 * and the menu items go.
 */
export const CONFINEMENT_ENFORCED = true;

/** The folder of each workspace that has one, by workspace name. */
export type Confinements = Readonly<Record<string, string>>;

/** What a session started later is told, and an open one is not: said where the choice is made. */
export const LATER_SESSIONS =
  "Vale para as sessões da IA iniciadas daqui em diante; as que já estão abertas mantêm o acesso que tinham.";

/** The file as the core and the macOS app write it: a name and a folder, nothing empty. */
export function normalizeConfinements(raw: unknown): Confinements {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return {};
  const out: Record<string, string> = {};
  for (const [workspace, folder] of Object.entries(raw)) {
    if (typeof folder === "string" && folder !== "") out[workspace] = folder;
  }
  return out;
}

export function sameConfinements(a: Confinements, b: Confinements): boolean {
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((k) => a[k] === b[k]);
}

/** How a workspace's folder button looks and what it says. */
export interface ConfineLook {
  /** The folder is shut: the AI is held to one. */
  held: boolean;
  /** Said under the pointer, and read out. */
  title: string;
}

export function confineLook(label: string, folder: string | null): ConfineLook {
  if (folder === null) {
    return { held: false, title: `Restringir a IA de ${label} a uma pasta\n${LATER_SESSIONS}` };
  }
  return { held: true, title: `A IA de ${label} só acessa ${folder}\n${LATER_SESSIONS}` };
}

/**
 * A long path shortened in the middle, for a menu: where it starts and where
 * it ends are what tell folders apart. The whole path is under the pointer.
 */
export function elide(path: string, max = 48): string {
  if (path.length <= max) return path;
  const head = Math.ceil((max - 1) * 0.4);
  return `${path.slice(0, head)}…${path.slice(path.length - (max - 1 - head))}`;
}

/** One line of a menu, by what it does; the window gives each its action. */
export type Choice =
  | "separator"
  | { id: "info" | "choose" | "release"; label: string; disabled?: boolean; help?: string };

/** What the folder button opens when the AI is held to a folder: where, and the two ways out. */
export function heldChoices(folder: string): Choice[] {
  return [
    { id: "info", label: `A IA só acessa ${elide(folder)}`, disabled: true, help: folder },
    "separator",
    { id: "choose", label: LABELS.change },
    { id: "release", label: LABELS.release },
  ];
}

/** What the workspace header's context menu has for the AI's folder, after a separator. */
export function contextChoices(folder: string | null): Choice[] {
  const choices: Choice[] = ["separator", { id: "choose", label: LABELS.restrict }];
  if (folder !== null) choices.push({ id: "release", label: LABELS.releaseToday(elide(folder, 40)), help: folder });
  return choices;
}

/** The labels of the choices, as the macOS app words them. */
export const LABELS = {
  restrict: "Restringir a IA a uma pasta…",
  change: "Trocar a pasta…",
  release: "Liberar a IA",
  releaseToday: (folder: string) => `Liberar a IA (hoje: ${folder})`,
  dialogTitle: (label: string) => `Pasta que a IA de ${label} pode acessar`,
};

/** A path's names in lower case (the file system does not tell cases apart), and whether it is on the network. */
function split(path: string): { network: boolean; names: string[] } {
  let rest = path;
  let network = false;
  const verbatim = /^\\\\\?\\(UNC\\)?/.exec(rest);
  if (verbatim) {
    network = verbatim[1] !== undefined;
    rest = rest.slice(verbatim[0].length);
  } else if (/^[\\/]{2}[^\\/]/.test(rest)) {
    network = true;
  }
  const names = rest
    .split(/[\\/]+/)
    .filter((n) => n !== "" && n !== ".")
    .map((n) => n.toLowerCase());
  return { network, names };
}

/**
 * Why a folder cannot hold the AI, if it cannot: the root of a drive, the
 * user's home, or one that contains it. A permission given there would go
 * through the whole profile, so it holds nothing and takes minutes. The core
 * refuses these too, along with the system's own folders (it knows where
 * they are); this is to say it when the folder is chosen.
 */
export function tooBroad(folder: string, home: string): string | null {
  const wanted = split(folder);
  if (wanted.names.length <= (wanted.network ? 2 : 1)) return "a raiz de uma unidade";
  const mine = split(home).names;
  if (mine.length >= wanted.names.length && wanted.names.every((n, i) => mine[i] === n)) {
    return "a pasta do seu usuário (ou uma que a contém)";
  }
  return null;
}

/** What is said when a folder is refused. */
export function refusal(folder: string, why: string): string {
  return `Escolha a pasta de um projeto: ${folder} é ${why}, e a IA não ficaria presa a nada.`;
}

/** What is said once a folder is chosen. */
export function chosen(label: string, folder: string): string {
  return `A IA de ${label} só acessa ${folder}. ${LATER_SESSIONS}`;
}
