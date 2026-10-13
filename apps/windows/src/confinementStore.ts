// The folders the AI of each workspace is held to: read from the file and
// changed from the workspace's header.
//
// What the choice does is the core's (`keep ia confinar`, started in front of
// the AI by the shell function, the account switch and the restore): this
// keeps the choice, and the window reads the file again every few seconds,
// since the core, the macOS app or a hand can change it as well.

import { signal } from "@preact/signals";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as api from "./api";
import {
  LABELS,
  LATER_SESSIONS,
  chosen,
  normalizeConfinements,
  refusal,
  sameConfinements,
  tooBroad,
  type Confinements,
} from "./confinement";
import { info, log, say } from "./store";

/** The folder of each workspace that has one, by workspace name. */
export const confinements = signal<Confinements>({});

export async function loadConfinements(): Promise<void> {
  try {
    const next = normalizeConfinements(await api.confinementAll());
    if (!sameConfinements(next, confinements.value)) confinements.value = next;
  } catch {
    // Asked again in a few seconds.
  }
}

let started = false;

export function startConfinement(): void {
  if (started) return;
  started = true;
  void loadConfinements();
  setInterval(() => void loadConfinements(), 3000);
}

async function save(workspace: string, folder: string | null): Promise<boolean> {
  try {
    confinements.value = normalizeConfinements(await api.confinementSet(workspace, folder));
    return true;
  } catch (error) {
    log(`confinement ${workspace}: ${String(error)}`);
    say(`Não foi possível ${folder === null ? "liberar" : "restringir"} a IA: ${String(error)}`);
    return false;
  }
}

/** Ask for the folder the workspace's AI may use, and hold it to the one chosen. */
export async function chooseFolder(workspace: string, label: string): Promise<void> {
  const picked = await openDialog({
    directory: true,
    multiple: false,
    title: LABELS.dialogTitle(label),
    defaultPath: confinements.value[workspace],
  }).catch(() => null);
  if (typeof picked !== "string") return;
  const why = tooBroad(picked, info.value?.home ?? "");
  if (why) {
    say(refusal(picked, why));
    return;
  }
  if (await save(workspace, picked)) say(chosen(label, picked));
}

/** Let the workspace's AI go where it likes again. */
export async function release(workspace: string, label: string): Promise<void> {
  if (await save(workspace, null)) say(`A IA de ${label} está livre. ${LATER_SESSIONS}`);
}
