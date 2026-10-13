// Keep for Windows: the window over the daemon. See store.ts for what it
// knows, ui/ for what it shows, input.ts for the keyboard.

import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import "./styles-appearance.css";
import "./styles-confine.css";
import "./styles-palette.css";
import "./styles-quick.css";
import "./styles-tabs.css";
import { render } from "preact";
import { startAppearance } from "./appearance";
import { startConfinement } from "./confinementStore";
import { installInput, terminalKey } from "./input";
import { restoreTabs } from "./iaChores";
import { startIA } from "./iaStore";
import { bindCommands } from "./palette/bindings";
import { bootQuick } from "./quick/boot";
import { bindQuick } from "./quick/main";
import { watchSleep } from "./sleepWatch";
import { connect, info, start } from "./store";
import { App } from "./ui/App";

async function boot() {
  installInput();
  bindCommands();
  await start({ onKey: terminalKey });
  bindQuick();
  startAppearance();
  render(<App />, document.getElementById("app")!);
  watchSleep();
  await connect({ restore: restoreTabs });
  startConfinement();
  // The AI layer after the daemon: its tabs' AI is asked of the daemon the window found.
  await startIA();
  if (info.value?.e2eReport) {
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
