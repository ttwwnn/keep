// Keep for Windows: the window over the daemon. See store.ts for what it
// knows, ui/ for what it shows, input.ts for the keyboard.

import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import "./styles-appearance.css";
import "./styles-confine.css";
import "./styles-glass.css";
import "./styles-palette.css";
import "./styles-quick.css";
import "./styles-tabs.css";
import "./styles-terminal.css";
import "./styles-windows.css";
import { render } from "preact";
import { startAppearance } from "./appearance";
import { startConfinement } from "./confinementStore";
import { startGlass } from "./glassStore";
import { installInput, terminalKey } from "./input";
import { restoreTabs } from "./iaChores";
import { startIA } from "./iaStore";
import { bindCommands } from "./palette/bindings";
import { bootQuick } from "./quick/boot";
import { bindQuick } from "./quick/main";
import { watchSleep } from "./sleepWatch";
import { connect, info, start } from "./store";
import { App } from "./ui/App";
import { MAIN_LABEL } from "./windowModel";
import { isLeader, windowLabel } from "./windowPeers";
import { openSavedWindows, startWindows } from "./windowSync";

async function boot() {
  installInput();
  bindCommands();
  const booted = await start({ onKey: terminalKey });
  bindQuick();
  startAppearance();
  startGlass();
  await startWindows(booted);
  render(<App />, document.getElementById("app")!);
  watchSleep();
  // A window the scripted check opened answers it (e2eWindows.ts).
  if (info.value?.e2eReport && windowLabel.value !== MAIN_LABEL) void import("./e2eWindows").then((m) => m.answerTheScript());
  await connect({
    // The tabs back after a restart are the oldest window's to ask for; the windows of the last run open once they are.
    restore: () => (isLeader.value ? restoreTabs() : Promise.resolve(null)),
    windows: (remap) => openSavedWindows(booted, remap),
  });
  startConfinement();
  // The AI layer after the daemon: its tabs' AI is asked of the daemon the window found.
  await startIA();
  // The scripted check runs in the first window; the others are for it to open.
  if (info.value?.e2eReport && windowLabel.value === MAIN_LABEL) {
    const { runE2E } = await import("./e2e");
    // After the first layout, so every terminal has a size to attach with.
    setTimeout(() => void runE2E(), 300);
  }
}

// The drop-down terminal's window shows this same page in its quick mode (src-tauri/src/quick.rs).
const quick = new URLSearchParams(location.search).has("quick");
(quick ? bootQuick() : boot()).catch((error) => {
  // Nothing drawn yet: say why where the window would have been.
  const root = document.getElementById("app");
  if (root) {
    root.className = "fatal";
    root.textContent = `O Keep não conseguiu abrir: ${String(error)}`;
  }
});
