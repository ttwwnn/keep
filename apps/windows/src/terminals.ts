// The terminals: one xterm.js per tab or pane, each attached to the daemon.
//
// They live outside the component tree. A terminal holds a scrollback and a
// connection that must outlive any one render, so components only hand it a
// place to sit (`mount`), and the same terminal moves when the layout around
// it changes — a split made, a pane closed — instead of being rebuilt.

import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Terminal, type ITheme } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { SearchAddon } from "@xterm/addon-search";
import * as api from "./api";
import { installKitty, type Kitty } from "./kitty";
import { installLinks, type LinkEnv, type Links } from "./links";
import { installOsc52 } from "./osc52";
import { isSafePaste, pasteQuestion, preparePaste, type Question } from "./paste";
import { SchemeReporter } from "./schemeReport";
import { serializeBuffer } from "./serialize";
import type { CursorShape } from "./state";
import { viewerSize } from "./windowModel";

export interface TerminalLook {
  fontFamily: string;
  fontSize: number;
  theme: ITheme;
  /** The theme's background has an alpha that is to show through (xterm draws it opaque otherwise). */
  translucent?: boolean;
  cursor: CursorShape;
  blink: boolean;
  /** Lines of history kept. */
  scrollback: number;
  /** Whether the ground is dark: what a program that asked is told (mode 2031). */
  dark: boolean;
}

export interface TerminalHooks {
  /** The tab's shell ended. */
  ended(view: TermView): void;
  /** The connection broke; the view will be attached again when asked. */
  failed(view: TermView, reason: string): void;
  /** A key reached the terminal: false keeps it from the program. */
  key(view: TermView, event: KeyboardEvent): boolean;
  focused(view: TermView): void;
  /** Where the pane's program works, for the paths it prints; empty when unknown. */
  cwd(view: TermView): string;
  /** The person's home folder. */
  home(): string;
  /** A quiet line for the person. */
  say(message: string): void;
  /** A question the person answers before something goes on: true on the yes. */
  confirm(question: Question): Promise<boolean>;
  /** The program set the tab's title (OSC 0 or 2). */
  titleChanged(view: TermView, title: string): void;
}

export const viewKey = (workspace: string, tab: number) => `${workspace}\u0000${tab}`;

/** When the window last stopped being the one with the keyboard: a copy made in another app after that is not a program's. */
let windowBlurredAt = 0;
if (typeof window !== "undefined") window.addEventListener("blur", () => (windowBlurredAt = Date.now()));

/** How long after a pane is made, or shown, its grid is taken as settling: mounting walks through two or three sizes as the layout resolves. */
const SETTLING_MS = 700;
/** How long the grid stays said, and how long it takes to fade. */
const BADGE_MS = 900;

/** What a repaint is written after: home and clear, as the terminal client does. */
const CLEAR_FOR_REPAINT = "\x1b[H\x1b[2J";

export class TermView {
  key: string;
  readonly element: HTMLDivElement;
  readonly term: Terminal;
  readonly search: SearchAddon;
  private readonly scheme: SchemeReporter;
  /** The addresses and paths in its text that can be clicked. */
  readonly links: Links;
  /** The keyboard protocol the program in it has turned on. */
  private readonly kitty: Kitty;
  /** The last text a program put on the clipboard (OSC 52), and when. */
  programCopied: { text: string; at: number } | null = null;
  /** When the pointer last went down in this terminal. */
  private pressedAt = 0;
  private readonly fit: FitAddon;
  private webgl: WebglAddon | null = null;
  private attachment: api.Attachment | null = null;
  private attaching = false;
  private opened = false;
  private disposed = false;
  private visible = false;
  /** Another window is open: out of sight, this terminal says it is not looking (windowModel.ts `viewerSize`). */
  private severalWindows = false;
  private sent = { cols: 0, rows: 0 };
  private host: HTMLElement | null = null;
  private readonly observer: ResizeObserver;
  /** Says "cols × rows" over the pane while its box is being changed. */
  private readonly badge: HTMLDivElement;
  private badgeTimer: ReturnType<typeof setTimeout> | undefined;
  /** The grid last seen: a change of it, not the first, is what the badge speaks of. */
  private grid: { cols: number; rows: number };
  /** Until when a change of grid is settling rather than being asked for. */
  private settlingUntil = performance.now() + SETTLING_MS;
  /** The shell ended; nothing will come back on this tab. */
  ended = false;
  /** Why the last attach failed, while it has not been retried. */
  failure: string | null = null;

  constructor(
    public workspace: string,
    public tab: number,
    look: TerminalLook,
    private readonly hooks: TerminalHooks,
    platform: { name: string; windowsBuild: number },
    /** What the terminal held when it was put to sleep (see serialize.ts): written before the daemon's repaint. */
    private history = "",
  ) {
    this.key = viewKey(workspace, tab);
    this.element = document.createElement("div");
    this.element.className = "term-root";
    const linkEnv: LinkEnv = {
      flavor: platform.name === "windows" ? "windows" : "posix",
      mac: platform.name === "macos",
      cwd: () => hooks.cwd(this),
      home: () => hooks.home(),
      say: (message) => hooks.say(message),
    };
    this.term = new Terminal({
      fontFamily: look.fontFamily,
      fontSize: look.fontSize,
      theme: look.theme,
      allowTransparency: look.translucent === true,
      scrollback: look.scrollback,
      allowProposedApi: true,
      cursorBlink: look.blink,
      cursorStyle: look.cursor,
      // Hollow while the terminal does not have the keyboard, as Ghostty draws it.
      cursorInactiveStyle: "outline",
      // ConPTY reflows lines itself on a resize; told which build it is,
      // xterm.js leaves the reflowing to it rather than doing it twice.
      windowsPty:
        platform.name === "windows" ? { backend: "conpty", buildNumber: platform.windowsBuild || undefined } : undefined,
      macOptionIsMeta: platform.name === "macos",
      rescaleOverlappingGlyphs: true,
      // A link the program drew (OSC 8) opens as an address in the text does.
      linkHandler: { activate: (event, uri) => this.links.activateDrawn(event, uri) },
    });
    this.links = installLinks(this.term, this.element, linkEnv);
    // A program may write the clipboard, and read it once the person says so.
    installOsc52(this.term, {
      write: (text) => writeText(text).catch(() => navigator.clipboard.writeText(text)),
      read: () => readText(),
      confirm: (question) => hooks.confirm(question),
      answer: (data) => this.term.input(data, false),
      wrote: (text) => (this.programCopied = { text, at: Date.now() }),
    });
    // A program may ask for more of the keyboard than a terminal gives, and is told it can have it.
    this.kitty = installKitty(this.term, (data) => this.term.input(data, false));
    this.fit = new FitAddon();
    this.search = new SearchAddon({ highlightLimit: 2000 });
    this.term.loadAddon(this.fit);
    this.term.loadAddon(this.search);
    const unicode = new Unicode11Addon();
    this.term.loadAddon(unicode);
    this.term.unicode.activeVersion = "11";

    this.element.addEventListener("mousedown", () => (this.pressedAt = Date.now()), true);
    // The browser's own paste (Ctrl+V, Shift+Insert) goes the way any paste does.
    this.element.addEventListener(
      "paste",
      (event) => {
        event.preventDefault();
        event.stopPropagation();
        this.paste(event.clipboardData?.getData("text/plain") ?? "");
      },
      true,
    );

    // The title is the program's, said as it changes: the daemon has it before this does.
    this.term.onTitleChange((title) => this.hooks.titleChanged(this, title));
    this.scheme = new SchemeReporter(this.term, look.dark);

    this.term.attachCustomKeyEventHandler((event) => this.hooks.key(this, event));
    this.term.onData((data) => void this.attachment?.input(data).catch(() => {}));
    this.term.onBinary((data) => {
      const bytes = Uint8Array.from(data, (c) => c.charCodeAt(0) & 0xff);
      void this.attachment?.inputBytes(bytes).catch(() => {});
    });
    this.grid = { cols: this.term.cols, rows: this.term.rows };
    this.badge = document.createElement("div");
    this.badge.className = "size-badge";
    this.element.appendChild(this.badge);
    this.term.onResize(({ cols, rows }) => {
      this.sendSize();
      this.sayGrid(cols, rows);
    });
    this.observer = new ResizeObserver(() => this.fitNow());
  }

  /**
   * The tab went to another workspace and the daemon gave it new ids. The
   * connection holds the tab itself, not its id, and carries on; only the
   * names this view answers to change.
   */
  rekey(workspace: string, tab: number): void {
    this.workspace = workspace;
    this.tab = tab;
    this.key = viewKey(workspace, tab);
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
    // The lines that had scrolled off, as they were; the repaint that follows clears the screen and draws what is there now.
    if (this.history) {
      this.term.write(this.history);
      this.history = "";
    }
    this.term.refresh(0, this.term.rows - 1);
    if (this.visible) this.enableWebgl();
    this.attachIfNeeded();
  }

  /** The terminal's text, scrollback and colours, as the sequences that draw it again. */
  snapshot(): string {
    return serializeBuffer(this.term.buffer.normal, this.term.cols);
  }

  get isAttached(): boolean {
    return this.attachment !== null;
  }

  /**
   * Whether letting go of this terminal would lose something the daemon
   * cannot give back: a selection, a composition under way, or the keyboard
   * itself.
   */
  holdsUnsavedState(): boolean {
    if (this.term.hasSelection()) return true;
    if (this.element.contains(document.activeElement)) return true;
    return this.element.querySelector(".composition-view.active") !== null;
  }

  /** Attach again after a failure, or for the first time. */
  attachIfNeeded(): void {
    if (this.disposed || this.ended || this.attachment || this.attaching || !this.opened) return;
    this.attaching = true;
    const { cols, rows } = this.viewing();
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

  /** The size this viewer tells the daemon: the terminal's, unless it is out of sight while other windows may be looking. */
  private viewing(): { cols: number; rows: number } {
    return viewerSize(this.visible, this.severalWindows, this.term.cols, this.term.rows);
  }

  /** The size the daemon was last told this viewer has (0×0: not looking). */
  get told(): { cols: number; rows: number } {
    return this.sent;
  }

  private sendSize(): void {
    const { cols, rows } = this.viewing();
    if (!this.attachment || (cols === this.sent.cols && rows === this.sent.rows)) return;
    this.sent = { cols, rows };
    void this.attachment.resize(cols, rows).catch(() => {});
  }

  /**
   * Say how big the pane is now, if its box really changed the grid: points
   * are not the unit that matters, and a few pixels of divider often leave the
   * columns and rows as they were. Not while the pane is settling, and not out
   * of sight.
   */
  private sayGrid(cols: number, rows: number): void {
    const changed = cols !== this.grid.cols || rows !== this.grid.rows;
    this.grid = { cols, rows };
    if (!changed || !this.visible || performance.now() < this.settlingUntil) return;
    this.badge.textContent = `${cols} × ${rows}`;
    this.badge.classList.add("shown");
    clearTimeout(this.badgeTimer);
    this.badgeTimer = setTimeout(() => this.badge.classList.remove("shown"), BADGE_MS);
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
      // A pane catching up after being revealed is not being resized in front of anyone.
      this.settlingUntil = performance.now() + SETTLING_MS;
      this.enableWebgl();
      this.fitNow();
      this.term.refresh(0, this.term.rows - 1);
    } else {
      this.disableWebgl();
    }
    // With other windows open, going out of sight or coming back changes what the daemon is told.
    this.sendSize();
  }

  /** Whether another window is open: a terminal out of sight stops counting in the size of its tab. */
  setSeveralWindows(several: boolean): void {
    if (this.severalWindows === several) return;
    this.severalWindows = several;
    this.sendSize();
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
    // Before the theme: a background with an alpha is drawn opaque until the terminal is told it may show through.
    const translucent = look.translucent === true;
    if (options.allowTransparency !== translucent) options.allowTransparency = translucent;
    options.theme = look.theme;
    if (options.cursorStyle !== look.cursor) options.cursorStyle = look.cursor;
    if (options.cursorBlink !== look.blink) options.cursorBlink = look.blink;
    if (options.scrollback !== look.scrollback) options.scrollback = look.scrollback;
    this.scheme.setDark(look.dark);
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

  /**
   * Text as if pasted: bracketed when the program asked for it, stripped of
   * the control characters a tty acts on, and asked about first when it would
   * run itself (a line break where the program did not ask for bracketed
   * paste — see paste.ts).
   */
  paste(text: string): void {
    if (text === "") return;
    if (isSafePaste(text, this.term.modes.bracketedPasteMode)) {
      this.term.paste(preparePaste(text));
      return;
    }
    void this.hooks.confirm(pasteQuestion(text)).then((yes) => {
      if (yes) this.term.paste(preparePaste(text));
      // A beat later: the key that answered is still being delivered, and
      // WebKit hands its keypress to whatever has the focus by then — the
      // terminal, which would send an Escape to the program.
      setTimeout(() => this.focus(), 50);
    });
  }

  /**
   * The text a program that selects with the mouse itself — Claude Code in full
   * screen — copied when the button was let go: the pointer went down here
   * after the window last had the keyboard, and the copy came after that. Not
   * an old clipboard, which cutting out of the prompt would delete from text
   * nobody selected.
   */
  selectionTheProgramCopied(): string | undefined {
    const copied = this.programCopied;
    if (!copied || copied.text === "") return undefined;
    return copied.at >= this.pressedAt && this.pressedAt > windowBlurredAt ? copied.text : undefined;
  }

  /** The Kitty keyboard flags the program has on; 0 while it has asked for none. */
  kittyFlags(): number {
    return this.kitty.flags();
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
    clearTimeout(this.badgeTimer);
    this.observer.disconnect();
    this.links.dispose();
    this.kitty.dispose();
    this.release();
    this.disableWebgl();
    this.term.dispose();
    this.element.remove();
  }
}

/** Every terminal the window has, by tab. */
export class TerminalManager {
  private readonly views = new Map<string, TermView>();
  private severalWindows = false;
  /** What the terminals of tabs asleep held, by view: given back to the terminal that takes theirs. */
  private readonly sleeping = new Map<string, string>();

  constructor(
    private look: TerminalLook,
    private readonly hooks: TerminalHooks,
    private readonly platform: { name: string; windowsBuild: number },
  ) {}

  view(workspace: string, tab: number): TermView {
    const key = viewKey(workspace, tab);
    let view = this.views.get(key);
    if (!view) {
      view = new TermView(workspace, tab, this.look, this.hooks, this.platform, this.sleeping.get(key));
      view.setSeveralWindows(this.severalWindows);
      this.sleeping.delete(key);
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

  /**
   * Tabs the daemon re-filed: each view answers to its new workspace and id,
   * scrollback and connection kept. Done all at once, so a new key is never
   * taken by a view that has not left its old one.
   */
  rekey(moves: { from: [string, number]; to: [string, number] }[]): void {
    const taken = moves.flatMap((m) => {
      const view = this.views.get(viewKey(...m.from));
      return view ? [{ view, to: m.to }] : [];
    });
    for (const { view } of taken) this.views.delete(view.key);
    for (const { view, to } of taken) {
      view.rekey(...to);
      this.views.set(view.key, view);
    }
    // What tabs asleep held goes with them.
    const kept = moves.flatMap((m) => {
      const history = this.sleeping.get(viewKey(...m.from));
      return history === undefined ? [] : [{ from: viewKey(...m.from), to: viewKey(...m.to), history }];
    });
    for (const { from } of kept) this.sleeping.delete(from);
    for (const { to, history } of kept) this.sleeping.set(to, history);
  }

  /**
   * Put a tab's terminal to sleep: its screen, its scrollback and its
   * connection are let go of. The shell is the daemon's and runs on; showing
   * the tab builds the terminal again, repainted from the daemon.
   */
  sleep(workspace: string, tab: number): void {
    const key = viewKey(workspace, tab);
    const view = this.views.get(key);
    if (!view) return;
    // The daemon repaints the screen and no more: the scrollback goes with the terminal unless it is kept.
    if (view.ended) this.sleeping.delete(key);
    else this.sleeping.set(key, view.snapshot());
    this.views.delete(key);
    view.dispose();
  }

  /** Let go of the terminals whose tabs the daemon no longer has. */
  prune(alive: Set<string>): void {
    for (const [key, view] of this.views) {
      if (!alive.has(key)) {
        view.dispose();
        this.views.delete(key);
      }
    }
    for (const key of [...this.sleeping.keys()]) if (!alive.has(key)) this.sleeping.delete(key);
  }

  setLook(look: TerminalLook): void {
    this.look = look;
    for (const view of this.views.values()) view.setLook(look);
  }

  /** Other windows are open, or are not any more. */
  setSeveralWindows(several: boolean): void {
    this.severalWindows = several;
    for (const view of this.views.values()) view.setSeveralWindows(several);
  }

  /** The daemon is back: attach whatever lost its connection. */
  reattach(): void {
    for (const view of this.views.values()) view.attachIfNeeded();
  }

  disposeAll(): void {
    for (const view of this.views.values()) view.dispose();
    this.views.clear();
    this.sleeping.clear();
  }
}
