// The terminals: one xterm.js per tab or pane, each attached to the daemon.
//
// They live outside the component tree. A terminal holds a scrollback and a
// connection that must outlive any one render, so components only hand it a
// place to sit (`mount`), and the same terminal moves when the layout around
// it changes — a split made, a pane closed — instead of being rebuilt.

import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SearchAddon } from "@xterm/addon-search";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as api from "./api";

export interface TerminalLook {
  fontFamily: string;
  fontSize: number;
  theme: ITheme;
}

export interface TerminalHooks {
  /** The tab's shell ended. */
  ended(view: TermView): void;
  /** The connection broke; the view will be attached again when asked. */
  failed(view: TermView, reason: string): void;
  /** A key reached the terminal: false keeps it from the program. */
  key(view: TermView, event: KeyboardEvent): boolean;
  focused(view: TermView): void;
}

export const viewKey = (workspace: string, tab: number) => `${workspace}\u0000${tab}`;

/** What a repaint is written after: home and clear, as the terminal client does. */
const CLEAR_FOR_REPAINT = "\x1b[H\x1b[2J";

function openLink(uri: string) {
  void openUrl(uri).catch(() => {});
}

export class TermView {
  readonly key: string;
  readonly element: HTMLDivElement;
  readonly term: Terminal;
  readonly search: SearchAddon;
  private readonly fit: FitAddon;
  private webgl: WebglAddon | null = null;
  private attachment: api.Attachment | null = null;
  private attaching = false;
  private opened = false;
  private disposed = false;
  private visible = false;
  private sent = { cols: 0, rows: 0 };
  private host: HTMLElement | null = null;
  private readonly observer: ResizeObserver;
  /** The shell ended; nothing will come back on this tab. */
  ended = false;
  /** Why the last attach failed, while it has not been retried. */
  failure: string | null = null;

  constructor(
    readonly workspace: string,
    readonly tab: number,
    look: TerminalLook,
    private readonly hooks: TerminalHooks,
    platform: { name: string; windowsBuild: number },
  ) {
    this.key = viewKey(workspace, tab);
    this.element = document.createElement("div");
    this.element.className = "term-root";
    this.term = new Terminal({
      fontFamily: look.fontFamily,
      fontSize: look.fontSize,
      theme: look.theme,
      scrollback: 10000,
      allowProposedApi: true,
      cursorBlink: false,
      // ConPTY reflows lines itself on a resize; told which build it is,
      // xterm.js leaves the reflowing to it rather than doing it twice.
      windowsPty:
        platform.name === "windows" ? { backend: "conpty", buildNumber: platform.windowsBuild || undefined } : undefined,
      macOptionIsMeta: platform.name === "macos",
      rescaleOverlappingGlyphs: true,
      linkHandler: { activate: (_event, uri) => openLink(uri) },
    });
    this.fit = new FitAddon();
    this.search = new SearchAddon({ highlightLimit: 2000 });
    this.term.loadAddon(this.fit);
    this.term.loadAddon(this.search);
    this.term.loadAddon(new WebLinksAddon((_event, uri) => openLink(uri)));
    const unicode = new Unicode11Addon();
    this.term.loadAddon(unicode);
    this.term.unicode.activeVersion = "11";

    this.term.attachCustomKeyEventHandler((event) => this.hooks.key(this, event));
    this.term.onData((data) => void this.attachment?.input(data).catch(() => {}));
    this.term.onBinary((data) => {
      const bytes = Uint8Array.from(data, (c) => c.charCodeAt(0) & 0xff);
      void this.attachment?.inputBytes(bytes).catch(() => {});
    });
    this.term.onResize(() => this.sendSize());
    this.observer = new ResizeObserver(() => this.fitNow());
  }

  /** Put the terminal in `host`, opening it the first time and attaching it once it has a size. */
  mount(host: HTMLElement): void {
    if (this.disposed) return;
    if (this.host !== host || this.element.parentElement !== host) {
      if (this.host) this.observer.unobserve(this.host);
      host.appendChild(this.element);
      this.host = host;
      this.observer.observe(host);
    }
    if (!this.opened) {
      this.term.open(this.element);
      this.term.textarea?.addEventListener("focus", () => this.hooks.focused(this));
      this.opened = true;
    }
    this.fitNow();
    this.term.refresh(0, this.term.rows - 1);
    if (this.visible) this.enableWebgl();
    this.attachIfNeeded();
  }

  get isAttached(): boolean {
    return this.attachment !== null;
  }

  /** Attach again after a failure, or for the first time. */
  attachIfNeeded(): void {
    if (this.disposed || this.ended || this.attachment || this.attaching || !this.opened) return;
    this.attaching = true;
    const { cols, rows } = this.term;
    api
      .attach(this.workspace, this.tab, cols, rows, (event) => this.receive(event))
      .then((attachment) => {
        this.attaching = false;
        if (this.disposed) {
          void attachment.detach();
          return;
        }
        this.attachment = attachment;
        this.failure = null;
        this.sent = { cols, rows };
        // The size may have changed while the attach was on its way.
        this.sendSize();
      })
      .catch((error) => {
        this.attaching = false;
        this.failure = String(error);
        this.hooks.failed(this, this.failure);
      });
  }

  private receive(event: api.TabEvent): void {
    switch (event.kind) {
      case "repaint":
        // The whole screen from the top. Cleared first, as the terminal
        // client does, so a repaint never lands below what was there; the
        // scrollback is kept.
        this.term.write(CLEAR_FOR_REPAINT);
        this.term.write(event.data);
        break;
      case "output":
        this.term.write(event.data);
        break;
      case "ended":
        this.ended = true;
        this.release();
        this.hooks.ended(this);
        break;
      case "error":
        this.attachment = null;
        this.failure = event.reason;
        this.hooks.failed(this, event.reason);
        break;
      case "attached":
        break;
    }
  }

  private sendSize(): void {
    const { cols, rows } = this.term;
    if (!this.attachment || (cols === this.sent.cols && rows === this.sent.rows)) return;
    this.sent = { cols, rows };
    void this.attachment.resize(cols, rows).catch(() => {});
  }

  /** Fit the grid to the host. A host with no size yet is left for later. */
  fitNow(): void {
    if (!this.opened || !this.host) return;
    const { clientWidth, clientHeight } = this.host;
    if (clientWidth < 20 || clientHeight < 20) return;
    try {
      this.fit.fit();
    } catch {
      // A renderer that has not measured its font yet; the next resize fits.
    }
  }

  /**
   * On screen or off. Only what is on screen draws with WebGL: a browser
   * gives a page a handful of WebGL contexts, and a window can hold dozens
   * of tabs.
   */
  setVisible(visible: boolean): void {
    if (this.visible === visible) return;
    this.visible = visible;
    if (visible) {
      this.enableWebgl();
      this.fitNow();
      this.term.refresh(0, this.term.rows - 1);
    } else {
      this.disableWebgl();
    }
  }

  private enableWebgl(): void {
    if (this.webgl || !this.opened || this.disposed) return;
    try {
      const addon = new WebglAddon();
      addon.onContextLoss(() => {
        // Lost to the GPU or to the browser's limit: back to the DOM renderer.
        addon.dispose();
        if (this.webgl === addon) this.webgl = null;
      });
      this.term.loadAddon(addon);
      this.webgl = addon;
    } catch {
      this.webgl = null;
    }
  }

  private disableWebgl(): void {
    const addon = this.webgl;
    this.webgl = null;
    try {
      addon?.dispose();
    } catch {
      // Already gone with its context.
    }
  }

  setLook(look: TerminalLook): void {
    const options = this.term.options;
    if (options.fontFamily !== look.fontFamily) options.fontFamily = look.fontFamily;
    if (options.fontSize !== look.fontSize) options.fontSize = look.fontSize;
    options.theme = look.theme;
    this.fitNow();
  }

  get fontSize(): number {
    return this.term.options.fontSize ?? 0;
  }

  focus(): void {
    this.term.focus();
  }

  /** The screen as it stands — the live one, not wherever the view is scrolled to. */
  screenText(): string {
    const buffer = this.term.buffer.active;
    const lines: string[] = [];
    for (let i = buffer.baseY; i < buffer.baseY + this.term.rows; i++) {
      lines.push(buffer.getLine(i)?.translateToString(true) ?? "");
    }
    return lines.join("\n");
  }

  /** Everything the terminal holds, scrollback included. */
  allText(): string {
    const buffer = this.term.buffer.active;
    const lines: string[] = [];
    for (let i = 0; i < buffer.length; i++) lines.push(buffer.getLine(i)?.translateToString(true) ?? "");
    return lines.join("\n");
  }

  /** Text as if pasted: bracketed when the program asked for it. */
  paste(text: string): void {
    this.term.paste(text);
  }

  /** Bytes as if typed. */
  type(data: string): Promise<void> {
    return this.attachment ? this.attachment.input(data) : Promise.reject(new Error("aba não anexada"));
  }

  /** Let go of the tab: the daemon may reap it once nobody watches. */
  private release(): void {
    const attachment = this.attachment;
    this.attachment = null;
    void attachment?.detach().catch(() => {});
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.observer.disconnect();
    this.release();
    this.disableWebgl();
    this.term.dispose();
    this.element.remove();
  }
}

/** Every terminal the window has, by tab. */
export class TerminalManager {
  private readonly views = new Map<string, TermView>();

  constructor(
    private look: TerminalLook,
    private readonly hooks: TerminalHooks,
    private readonly platform: { name: string; windowsBuild: number },
  ) {}

  view(workspace: string, tab: number): TermView {
    const key = viewKey(workspace, tab);
    let view = this.views.get(key);
    if (!view) {
      view = new TermView(workspace, tab, this.look, this.hooks, this.platform);
      this.views.set(key, view);
    }
    return view;
  }

  existing(workspace: string, tab: number): TermView | undefined {
    return this.views.get(viewKey(workspace, tab));
  }

  all(): TermView[] {
    return [...this.views.values()];
  }

  /** Let go of the terminals whose tabs the daemon no longer has. */
  prune(alive: Set<string>): void {
    for (const [key, view] of this.views) {
      if (!alive.has(key)) {
        view.dispose();
        this.views.delete(key);
      }
    }
  }

  setLook(look: TerminalLook): void {
    this.look = look;
    for (const view of this.views.values()) view.setLook(look);
  }

  /** The daemon is back: attach whatever lost its connection. */
  reattach(): void {
    for (const view of this.views.values()) view.attachIfNeeded();
  }

  disposeAll(): void {
    for (const view of this.views.values()) view.dispose();
    this.views.clear();
  }
}
