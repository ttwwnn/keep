// Placeholder while the interface is built: one terminal on one tab, enough
// to prove the path from the daemon to the page and back.
import "@xterm/xterm/css/xterm.css";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import * as api from "./api";

async function boot() {
  const root = document.getElementById("app")!;
  root.style.cssText = "position:fixed;inset:0;background:#282c34;padding:8px";
  const term = new Terminal({ fontSize: 14, theme: { background: "#282c34", foreground: "#ffffff" } });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(root);
  fit.fit();
  await api.ensureDaemon();
  const tab = await api.newTab("teste", null, term.cols, term.rows);
  const att = await api.attach("teste", tab, term.cols, term.rows, (ev) => {
    if (ev.kind === "repaint") {
      term.reset();
      term.write(ev.data);
    } else if (ev.kind === "output") {
      term.write(ev.data);
    }
  });
  term.onData((d) => void att.input(d));
  term.onResize(({ cols, rows }) => void att.resize(cols, rows));
  window.addEventListener("resize", () => fit.fit());
  (window as unknown as { keepSmoke: unknown }).keepSmoke = { term, att, tab };
}

void boot();
