// The drop-down terminal's page. The window (src-tauri/src/quick.rs) loads the
// same page as the main window with `?quick=1` (main.tsx), and this is what
// that page is then: one terminal, the first live tab of the daemon's `quick`
// workspace, and nothing around it — no sidebar, no row of tabs.
//
// The panel is a visitor in the window's state. It wears the theme, the face
// and the zoom the main window has chosen, read from the state file each time
// it comes in (look.ts), and it saves nothing: the state belongs to the main
// window, and two windows saving it would overwrite each other's changes.
// What it does keep is the shell: a tab of the daemon's, which outlives this
// page and the app. If the shell ends, another is opened.

import { effect, signal } from "@preact/signals";
import { listen } from "@tauri-apps/api/event";
import { render } from "preact";
import { useEffect, useLayoutEffect, useRef } from "preact/hooks";
import * as api from "../api";
import { loadSymbolsFont } from "../fonts";
import { glassTheme } from "../glass";
import { startGlass } from "../glassStore";
import { QUICK_WORKSPACE } from "../quickWorkspace";
import { normalizeState } from "../state";
import { ask, dialog, fontFamily, fontSize, glassAlpha, gui, info, notice, palette, say } from "../store";
import { TerminalManager, viewKey, type TermView, type TerminalLook } from "../terminals";
import { applyPalette, onSystemAppearanceChange } from "../theme";
import { loadThemes } from "../themes";
import { ContextMenu } from "../ui/common";
import { Overlays } from "../ui/Overlays";
import { quickLook, quickReady, quickStatus } from "./api";
import { installQuickInput, openQuickMenu, quickKey } from "./keys";
import { wearTheState } from "./look";
import { guessGrid, nextShell, quickTabIn, type Shell } from "./tab";

/** The shell the panel shows, a tab of the daemon's quick workspace; null until the daemon has said. */
const shell = signal<Shell | null>(null);
/** Whether the panel is in view: one that is not draws nothing and does not keep asking the daemon. */
const shown = signal(false);
/** What to say over the terminal when the daemon does not answer. */
const trouble = signal<string | null>(null);

let terminals: TerminalManager;

function look(): TerminalLook {
  const { cursor, blink, scrollback } = gui.value.terminal;
  return {
    fontFamily: fontFamily.value,
    fontSize: fontSize.value,
    theme: glassTheme(palette.value.terminal, glassAlpha.value),
    translucent: glassAlpha.value < 1,
    cursor,
    blink,
    scrollback,
    dark: palette.value.dark,
  };
}

/** What runs in the panel's tab, and where, as the daemon last said (`pwsh`, `wsl`): how a dropped file's path is to be written, and what a relative one is relative to. */
let shellCommand = "";
let shellCwd = "";

const current = (): TermView | undefined =>
  shell.value === null ? undefined : terminals.existing(QUICK_WORKSPACE, shell.value.id);

/** The terminal gets the keyboard once the panel has its own: a frame later, after the layout. */
function focusTerminal(): void {
  requestAnimationFrame(() => current()?.focus());
}

/** A tab for the shell, when the workspace has none alive: where it starts is only a guess until the terminal attaches. */
async function makeTab(): Promise<number> {
  const host = document.querySelector(".quick-host");
  const grid = guessGrid(host?.clientWidth || 1000, host?.clientHeight || 400, fontSize.value);
  return api.newTab(QUICK_WORKSPACE, info.value?.home || null, grid.cols, grid.rows);
}

let settling: Promise<void> | null = null;
let again = false;

/**
 * Ask the daemon which tab is the panel's, and make one if the shell ended or there never was one (the daemon makes
 * the first for whoever attaches to a workspace with none, as `keep quick` does; the panel asks instead, to
 * attach to a tab it knows). Asked while an answer is on its way, it asks once more when that arrives.
 */
function settle(): Promise<void> {
  if (settling) {
    again = true;
    return settling;
  }
  settling = (async () => {
    do {
      again = false;
      await settleOnce();
    } while (again);
  })().finally(() => (settling = null));
  return settling;
}

async function settleOnce(): Promise<void> {
  try {
    const found = quickTabIn(await api.listAll());
    shellCommand = found?.command ?? "";
    shellCwd = found?.cwd ?? "";
    const id = found?.id ?? (await makeTab());
    const wasDown = trouble.value !== null;
    trouble.value = null;
    // A terminal whose shell ended is let go, even where the daemon gave its number to the shell that took its place.
    const deadHere = terminals.existing(QUICK_WORKSPACE, id)?.ended ?? false;
    // Only the panel's tab keeps a terminal.
    terminals.prune(deadHere ? new Set() : new Set([viewKey(QUICK_WORKSPACE, id)]));
    const next = nextShell(shell.value, id, deadHere);
    if (next !== shell.value) shell.value = next;
    if (wasDown) terminals.reattach();
  } catch (error) {
    trouble.value = String(error);
    // Started again if it went away, as the main window does.
    void api.ensureDaemon().catch(() => {});
  }
}

function QuickApp() {
  const id = shell.value?.id ?? null;
  const born = shell.value?.born ?? 0;
  const visible = shown.value;
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (id === null || !host.current) return;
    terminals.view(QUICK_WORKSPACE, id).mount(host.current);
  }, [id, born]);
  useEffect(() => {
    if (id === null) return;
    // Out of view it gives up its WebGL context, as a tab out of sight does.
    terminals.existing(QUICK_WORKSPACE, id)?.setVisible(visible);
    if (visible) focusTerminal();
  }, [id, born, visible]);
  return (
    <div class="quick-shell">
      <div ref={host} class="quick-host" onContextMenu={(event) => openQuickMenu(event, current())} />
      {trouble.value ? (
        <div class="banner" role="status">
          Sem conexão com o keepd — tentando de novo. <span class="banner-detail">{trouble.value}</span>
        </div>
      ) : (
        notice.value && (
          <div class="banner" role="status">
            {notice.value}
          </div>
        )
      )}
      <ContextMenu />
      <Overlays />
    </div>
  );
}

export async function bootQuick(): Promise<void> {
  const root = document.getElementById("app")!;
  document.documentElement.classList.add("quick");
  try {
    const startup = await api.startup();
    info.value = startup;
    // Made transparent (quick.rs) on Windows, for the bottom corners the page rounds (styles-glass.css).
    if (startup.windowTransparent) document.documentElement.classList.add("quick-round");
    gui.value = normalizeState(await quickLook().catch(() => null));
    // As the main window does before it draws: the catalogue, if a theme was chosen, and the symbols font.
    await Promise.all([gui.value.theme ? loadThemes().catch(() => {}) : undefined, loadSymbolsFont()]);
    applyPalette(palette.value);
    // The palette follows the system while the appearance is automatic: a new state object makes it ask again.
    onSystemAppearanceChange(() => (gui.value = { ...gui.value }));

    terminals = new TerminalManager(
      look(),
      {
        ended: () => void settle(),
        // A tab that went away between two lists also fails to attach; only a connection that fails says the daemon is gone.
        failed: (_view, reason) => {
          if (/conectar|conex[aã]o|connect|pipe|socket/i.test(reason)) trouble.value ??= reason;
        },
        key: quickKey,
        focused: () => {},
        cwd: () => shellCwd,
        command: () => shellCommand,
        home: () => info.value?.home ?? "",
        // A quiet line for the person: over the terminal, where the sidebar's foot would be.
        say: (message) => say(message),
        titleChanged: () => {},
        // The question about a paste goes up in the panel as it does in a window; a second one is a no.
        confirm: async (q) => {
          if (dialog.value) return false;
          const answered = await ask({
            title: q.title,
            message: q.message,
            preview: q.preview,
            buttons: [
              { label: q.no, value: "no" },
              { label: q.yes, value: "yes", primary: true },
            ],
          });
          return answered?.button === "yes";
        },
      },
      { name: startup.platform, windowsBuild: startup.windowsBuild },
    );
    effect(() => applyPalette(palette.value));
    effect(() => terminals.setLook(look()));
    startGlass();
    effect(() => document.documentElement.style.setProperty("--zoom", String(gui.value.zoom)));
    installQuickInput(current, () => shellCommand);

    render(<QuickApp />, root);
    // The window says when it comes and when it goes (quick.rs): in, it wears the state as it is now and takes the keyboard.
    await listen("quick-shown", () => {
      shown.value = true;
      focusTerminal();
      void wearTheState().then(settle).then(focusTerminal);
    });
    await listen("quick-hidden", () => {
      shown.value = false;
    });
    // Brought in before this page was ready to hear of it (the first showing waits only so long): it is in view.
    if ((await quickStatus().catch(() => null))?.visible) {
      shown.value = true;
      focusTerminal();
    }
    await api.ensureDaemon().catch(() => {});
    await settle();
    // While it is in view the daemon is asked once a second, as the main window's is: a shell ended from the outside
    // (`keep kill quick`) is noticed, and the panel does not sit on a dead terminal.
    setInterval(() => {
      if (shown.value) void settle();
    }, 1000);
    // The first showing was waiting for this, so that the panel does not come in blank. Not for a frame to be drawn:
    // a window out of view draws none, and waiting for one would wait for the showing itself.
    await quickReady();
  } catch (error) {
    root.className = "fatal";
    root.textContent = `A janela rápida não conseguiu abrir: ${String(error)}`;
    // Even so: the first showing should not wait out its time for a page that has said what it could.
    void quickReady().catch(() => {});
  }
}
