// The page's side of the app: every question to the daemon, and attached tabs.
//
// One function per command in src-tauri/src/lib.rs, typed the way the Rust
// side serialises them. Nothing here keeps state; see model.ts for that.

import { Channel, invoke } from "@tauri-apps/api/core";

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
  /** The pipe or socket the daemon listens on. */
  address: string;
  version: string;
  /** Set when a test started the app: run the scripted check and report. */
  e2eReport: string | null;
  /** Set when that check covers the AI layer: "falso" (a stand-in core) or "real". */
  e2eIa: string | null;
}

export interface Shell {
  id: string;
  label: string;
  command: string;
}

export const SPLIT_RIGHT = 1;
export const SPLIT_DOWN = 2;

export const startup = () => invoke<Startup>("startup");
export const ensureDaemon = () => invoke<void>("ensure_daemon");
/** Which daemon answers (its process and start time), or null where unknown. */
export const daemonIdentity = () => invoke<string | null>("daemon_identity");
export const list = () => invoke<WorkspaceInfo[]>("list");

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
export const search = (query: string, limit = 50, workspace?: string, tab?: number) =>
  invoke<SearchHit[]>("search", { query, limit, workspace: workspace ?? null, tab: tab ?? null });

export const loadState = () => invoke<unknown | null>("load_state");
export const saveState = (state: unknown) => invoke<void>("save_state", { state });
export const shells = () => invoke<{ chosen: string; available: Shell[] }>("shells");
export const chooseShell = (command: string) => invoke<void>("choose_shell", { command });
export const e2eReport = (report: string) => invoke<void>("e2e_report", { report });

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
export const pathExists = (path: string) => invoke<boolean>("path_exists", { path });
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
