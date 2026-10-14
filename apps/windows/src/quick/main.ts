// The main window's side of the drop-down terminal: the palette's "Janela
// rápida", the shortcut the settings show, and what is said when the system
// would not give the shortcut. The panel itself is quick/boot.tsx, in a window
// of its own.

import { signal } from "@preact/signals";
import { registerCommand } from "../commands";
import { isMac, log, say } from "../store";
import { quickSetShortcut, quickStatus, quickToggle, type QuickStatus } from "./api";
import { quickAsMac, quickShortcut } from "./chord";
import { refusalNotice } from "./shortcut";

/** The drop-down terminal as the app last told it. */
export const quick = signal<QuickStatus | null>(null);

export async function refreshQuick(): Promise<QuickStatus | null> {
  try {
    const status = await quickStatus();
    quick.value = status;
    quickShortcut.value = status.shortcut;
  } catch (error) {
    log(`janela rápida: ${String(error)}`);
  }
  return quick.value;
}

/** Choose the shortcut. Null when the system gave it; else why not, in words. */
export async function chooseShortcut(shortcut: string): Promise<string | null> {
  try {
    const status = await quickSetShortcut(shortcut);
    quick.value = status;
    quickShortcut.value = status.shortcut;
    return null;
  } catch (error) {
    return String(error);
  }
}

/** Bring the panel in, or put it away. */
export function toggleQuick(): void {
  void quickToggle().catch((error) => {
    log(`janela rápida: ${String(error)}`);
    say("Não deu para abrir a janela rápida.");
  });
}

/** Once, as the window opens: the palette's command, and a word if the shortcut did not come through. */
export function bindQuick(): void {
  quickAsMac.value = isMac.value;
  registerCommand("quickWindow", { run: toggleQuick });
  void refreshQuick().then((status) => {
    if (status && !status.registered) say(refusalNotice(status, isMac.value), 25);
  });
}
