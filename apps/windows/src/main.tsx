// Keep for Windows: the window over the daemon. See store.ts for what it
// knows, ui/ for what it shows, input.ts for the keyboard.

import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import { render } from "preact";
import { installInput, terminalKey } from "./input";
import { connect, info, start } from "./store";
import { App } from "./ui/App";

async function boot() {
  installInput();
  await start({ onKey: terminalKey });
  render(<App />, document.getElementById("app")!);
  await connect();
  if (info.value?.e2eReport) {
    const { runE2E } = await import("./e2e");
    // After the first layout, so every terminal has a size to attach with.
    setTimeout(() => void runE2E(), 300);
  }
}

boot().catch((error) => {
  // Nothing drawn yet: say why where the window would have been.
  const root = document.getElementById("app");
  if (root) {
    root.className = "fatal";
    root.textContent = `O Keep não conseguiu abrir: ${String(error)}`;
  }
});
