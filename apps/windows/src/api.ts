// The page's side of the app: every question to the daemon, and attached tabs.
//
// One function per command in src-tauri/src/lib.rs, typed the way the Rust
// side serialises them. Nothing here keeps state; see model.ts for that.

import { Channel, invoke } from "@tauri-apps/api/core";
import { withoutQuick, withoutQuickHits } from "./quickWorkspace";

export interface TabInfo {
  id: number;
  cols: number;
  rows: number;
  /** How many viewers are attached, this window included. */
  clients: number;
  finished: boolean;
  /** What the program inside called itself (OSC 0/2); empty when it has not. */
  title: string;
  /** A command runs, as opposed to the shell waiting at its prompt. */
  busy: boolean;
  /** The tab this one is a pane of, or 0 for a tab of its own. */
  splitOf: number;
  /** How it sits against that tab: 1 to the right, 2 below. */
  splitDir: number;
  /** Where its foreground process works; empty when unknown. */
  cwd: string;
  /** When a byte last went either way, in unix milliseconds. */
  lastActive: number;
  /** The running command's name, or the shell's at a prompt ("claude", "pwsh"). */
  command: string;
  /** The tab's shell, which names its Claude Code status file; 0 from a daemon that predates it. */
  shellPid: number;
}

export interface WorkspaceInfo {
  name: string;
  tabs: TabInfo[];
}

export interface SearchHit {
  workspace: string;
  tab: number;
  line: number;
  total: number;
  text: string;
  matchStart: number;
  matchLen: number;
  before: string[];
  after: string[];
}

export interface Startup {
  /** "windows", "macos", "linux". */
  platform: string;
  /** The Windows build (e.g. 26100), for xterm's ConPTY handling; 0 elsewhere. */
  windowsBuild: number;
  home: string;
  /** The login name, for the workspace a fresh daemon starts with. */
  user: string;
  /** The pipe or socket the daemon listens on. */
  address: string;
  version: string;
  /** Set when a test started the app: run the scripted check and report. */
  e2eReport: string | null;
  /** Set when that check covers the AI layer: "falso" (a stand-in core) or "real". */
  e2eIa: string | null;
  /** Whether this window was made transparent, which the translucent ground needs (src-tauri/src/glass.rs). */
  windowTransparent: boolean;
}

export interface Shell {
  id: string;
  label: string;
  command: string;
}

export const SPLIT_RIGHT = 1;
export const SPLIT_DOWN = 2;

export const startup = () => invoke<Startup>("startup");
/** Put a material behind the windows that were made transparent ("none", "mica" or "acrylic"); `dark` is the ground's. */
export const glassApply = (blur: string, dark: boolean) => invoke<void>("glass_apply", { blur, dark });
/** Open the app again (the daemon and the tabs stay): a window is made transparent only when it is made. */
export const restartApp = () => invoke<void>("restart_app");
export const ensureDaemon = () => invoke<void>("ensure_daemon");
/** Which daemon answers (its process and start time), or null where unknown. */
export const daemonIdentity = () => invoke<string | null>("daemon_identity");
/** The workspaces as the window shows them: the daemon's, without the drop-down terminal's (quickWorkspace.ts). */
export const list = () => invoke<WorkspaceInfo[]>("list").then(withoutQuick);
/** Every workspace the daemon holds, the drop-down terminal's included: what only that terminal asks for. */
export const listAll = () => invoke<WorkspaceInfo[]>("list");

export const newTab = (
  workspace: string,
  cwd: string | null,
  cols: number,
  rows: number,
  splitOf = 0,
  splitDir = 0,
) => invoke<number>("new_tab", { workspace, cwd, cols, rows, splitOf, splitDir });

export const closeTab = (workspace: string, tab: number) => invoke<void>("close_tab", { workspace, tab });
export const killWorkspace = (workspace: string) => invoke<void>("kill_workspace", { workspace });
export const moveTab = (workspace: string, tab: number, to: string) =>
  invoke<number>("move_tab", { workspace, tab, to });
export const rearrange = (
  workspace: string,
  moves: { tab: number; splitOf: number; splitDir: number }[],
) => invoke<void>("rearrange", { workspace, moves });
export const preview = (workspace: string, tab: number) => invoke<string>("preview", { workspace, tab });
/** A screen with its colours (the sequences the daemon repaints with) and the grid it is held on. */
export interface Screen {
  cols: number;
  rows: number;
  text: string;
}
/** The same screen as `preview`, with its colours; a daemon that predates it refuses, and `preview` is the fallback. */
export const previewVt = (workspace: string, tab: number) => invoke<Screen>("preview_vt", { workspace, tab });
/** The folders the person goes to, from zoxide, best first; none where it is not installed. */
export const directories = () => invoke<string[]>("directories");
export const search = (query: string, limit = 50, workspace?: string, tab?: number) =>
  invoke<SearchHit[]>("search", { query, limit, workspace: workspace ?? null, tab: tab ?? null }).then(withoutQuickHits);

/** What reading the state found wrong when the app opened (an unreadable file, and what was done about it), once. */
export const stateWarning = () => invoke<string | null>("state_warning");
export const shells = () => invoke<{ chosen: string; available: Shell[] }>("shells");
export const chooseShell = (command: string) => invoke<void>("choose_shell", { command });
/**
 * The font families installed here, named by the fonts' own files: for each
 * the names it goes by, best first (the typographic family, then the legacy
 * one). The page keeps the monospaced ones (fonts.ts).
 */
export const installedFonts = () => invoke<string[][]>("installed_fonts");
export const e2eReport = (report: string) => invoke<void>("e2e_report", { report });

/** What a window needs before it draws, from src-tauri's windows.rs. */
export interface WindowBoot {
  /** This window: `main`, `w1`, `w2`… */
  label: string;
  /** How many changes to the app's part of the state have been made: the order they are told in. */
  seq: number;
  /** The app's part of the state: what every window shares. */
  shared: unknown;
  /** This window's own part; null for a window that has none yet. */
  window: unknown | null;
  /** The first window's part, for a window with none to take the furniture from. */
  inherit: unknown | null;
  /** The windows that are open, oldest first (see windowModel.ts `readPeers`), and how many times that list has changed. */
  windows: { version: number; list: unknown };
  /** The windows of the last run that are not open: asked of the first window only. */
  saved: { label: string; window: unknown; frame: WindowFrame | null }[];
}

/** A window's place on the screen, in physical pixels. */
export interface WindowFrame {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized?: boolean;
}

export interface OpenWindow {
  /** The label to give it (a window of the last run); else the lowest free slot. */
  label?: string;
  frame?: WindowFrame;
  maximized?: boolean;
  /** What it says of itself until it speaks. */
  placement?: unknown;
  /** The part of the state it starts with, when it is not a blank window. */
  record?: unknown;
}

export const windowBoot = () => invoke<WindowBoot>("window_boot");
/**
 * Tell the app, all at once, what changed in what the windows share (windowState.ts `Op`), what this window
 * lists and shows, and its own part of the state; the order of the changes. Every window hears what made a
 * difference (`keep://sync`).
 */
export const windowSync = (ops: unknown[], placement: unknown | null, record: unknown | null) =>
  invoke<number>("window_sync", { ops, placement, record });
/** The window that was opened, and where the app put it (before the system had a say). */
export interface OpenedWindow {
  label: string;
  frame: WindowFrame | null;
}
export const windowOpen = (request: OpenWindow) => invoke<OpenedWindow>("window_open", { request });
/** The window under a point of the screen (physical pixels), if any, other than `exclude`. */
export const windowUnder = (x: number, y: number, exclude: string | null) => invoke<string | null>("window_under", { x, y, exclude });
export const windowShow = (label: string) => invoke<void>("window_show", { label });

/** Whether the core is here for the AI layer, and the home it reads the logins from. */
export interface IaInfo {
  available: boolean;
  /** A test pointed the core at a made-up home: nothing is kept between runs. */
  testHome: boolean;
  home: string;
}

/** What the core printed for one question, with its exit code in `_saida`. */
export interface CoreAnswer {
  versao: number;
  ok?: boolean;
  motivo?: string;
  detalhe?: string;
  _saida: number;
  [field: string]: unknown;
}

export const iaInfo = () => invoke<IaInfo>("ia_info");
/** One question to the core: `["ia", "uso", "--json"]`, `["worktrees", "listar", …]`. */
export const iaAsk = (args: string[]) => invoke<CoreAnswer>("ia", { args });
/** The login files as they stand: a change is a login made or an order rewritten. */
export const iaSignature = () => invoke<string>("ia_signature");
/** Send listed worktrees to the Recycle Bin, once these processes are gone; what did not go. */
export const iaTrash = (items: { caminho: string; nasceu_ns: string | null }[], pids: number[]) =>
  invoke<string[]>("ia_trash", { items, pids });
/** The worktrees of tabs that closed without the question, to the Recycle Bin by the core's say; what did not go. */
export const iaSweep = () => invoke<string[]>("ia_sweep");
/** What Claude Code's status line saved for each of these shells: its JSON text, by shell pid. */
export const claudeStatuses = (shells: number[]) => invoke<Record<string, string>>("claude_statuses", { shells });
/** Claude Code's settings file as it stands, and the status line script installed with the app. */
export interface ClaudeSettings {
  path: string;
  /** The file's text; null when there is no file. */
  text: string | null;
  /** Whether Claude Code's folder is there: where it is not, it has not run here. */
  folder: boolean;
  /** The `keep-statusline.ps1` installed with the app; null when it is not there. */
  script: string | null;
  /** The core's command line (`keep.exe`), which the account hook runs; null when it is not there. */
  keep: string | null;
}
export const claudeSettings = () => invoke<ClaudeSettings>("claude_settings");
/** Write Claude Code's settings file if it still holds `expected` (null: no file); false when it changed meanwhile. */
export const writeClaudeSettings = (expected: string | null, text: string) =>
  invoke<boolean>("write_claude_settings", { expected, text });
/** Codex's model cache when it is not the version `known`: its stamp (null: no cache) and, when it changed, its text. */
export const codexModels = (known: string | null) =>
  invoke<{ stamp: string | null; text: string | null }>("codex_models", { known });
/** The names file the core shares with the window, when it is not the version `known`: its stamp (null: no file) and, when it changed, its text. */
export const namesRead = (known: string | null) =>
  invoke<{ stamp: string | null; text: string | null }>("names_read", { known });
/** Write the names file; the stamp of what was written. */
export const namesWrite = (text: string) => invoke<string | null>("names_write", { text });
/** A line for the app's log. */
export const appLog = (line: string) => invoke<void>("app_log", { line });
/** The folder each workspace's AI is held to, by workspace name (~/.config/keep/confinement.json). */
export const confinementAll = () => invoke<Record<string, string>>("confinement_all");
/** Hold a workspace's AI to a folder, or to none (null); the folders as they stand after. */
export const confinementSet = (workspace: string, folder: string | null) =>
  invoke<Record<string, string>>("confinement_set", { workspace, folder });
export interface Memory {
  used: number;
  total: number;
}

/** The computer's memory in use, in bytes; null where it cannot be read. */
export const memoryNow = () => invoke<Memory | null>("memory_now");
export const memoryMonitor = () => invoke<void>("memory_monitor");
export const pathExists = (path: string) => invoke<boolean>("path_exists", { path });

/** The first spelling of a path the terminal printed that exists: which one, and whether it is a folder. */
export interface PathHit {
  index: number;
  path: string;
  dir: boolean;
}

/** For each group of spellings of one printed path, the first that exists. */
export const findPaths = (groups: string[][]) => invoke<(PathHit | null)[]>("find_paths", { groups });
/** Open a file with what the system opens it with, a folder in the file manager. */
export const openPath = (path: string) => invoke<void>("open_path", { path });
/** Show a file in the file manager, selected. */
export const revealPath = (path: string) => invoke<void>("reveal_path", { path });
/** Open an address in the browser, or the mail program; only pages, mail and FTP are. */
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const e2eShot = (name: string) => invoke<void>("e2e_shot", { name });
export const e2eFake = (state: unknown) => invoke<void>("e2e_fake", { state });

/** What arrives on an attached tab's channel. */
export type TabEvent =
  | { kind: "attached"; tab: number }
  | { kind: "repaint"; data: Uint8Array }
  | { kind: "output"; data: Uint8Array }
  | { kind: "ended" }
  | { kind: "error"; reason: string };

const KIND_ATTACHED = 1;
const KIND_REPAINT = 2;
const KIND_OUTPUT = 3;
const KIND_ENDED = 4;
const KIND_ERROR = 5;

/** Read one channel message: a byte saying what it is, then its payload. */
export function decodeTabEvent(buffer: ArrayBuffer | Uint8Array | number[]): TabEvent | null {
  const bytes =
    buffer instanceof Uint8Array ? buffer : Array.isArray(buffer) ? Uint8Array.from(buffer) : new Uint8Array(buffer);
  if (bytes.length === 0) return null;
  const body = bytes.subarray(1);
  switch (bytes[0]) {
    case KIND_ATTACHED:
      return { kind: "attached", tab: new DataView(body.buffer, body.byteOffset, body.byteLength).getUint32(0, true) };
    case KIND_REPAINT:
      return { kind: "repaint", data: body };
    case KIND_OUTPUT:
      return { kind: "output", data: body };
    case KIND_ENDED:
      return { kind: "ended" };
    case KIND_ERROR:
      return { kind: "error", reason: new TextDecoder().decode(body) };
    default:
      return null;
  }
}

/** A tab this window is attached to. */
export interface Attachment {
  handle: number;
  input(data: string): Promise<void>;
  inputBytes(data: Uint8Array): Promise<void>;
  resize(cols: number, rows: number): Promise<void>;
  detach(): Promise<void>;
}

/**
 * Attach to a tab. `onEvent` receives everything the tab writes from the
 * repaint on; the size given is this viewer's, and the daemon fits the tab to
 * the smallest viewer looking at it (0 by 0 means "not looking").
 */
export async function attach(
  workspace: string,
  tab: number,
  cols: number,
  rows: number,
  onEvent: (event: TabEvent) => void,
): Promise<Attachment> {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (message) => {
    const event = decodeTabEvent(message as ArrayBuffer);
    if (event) onEvent(event);
  };
  const handle = await invoke<number>("attach", { workspace, tab, cols, rows, channel });
  return {
    handle,
    input: (data) => invoke<void>("input", { handle, data }),
    inputBytes: (data) => invoke<void>("input_bytes", { handle, data: Array.from(data) }),
    resize: (c, r) => invoke<void>("resize", { handle, cols: c, rows: r }),
    detach: () => invoke<void>("detach", { handle }),
  };
}
