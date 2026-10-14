// Claude Code's question when it opens in a folder it has not been in:
//
//   Quick safety check: Is this a project you created or one you trust?
//   ❯ No, exit
//     Yes, I trust this folder
//
// The macOS app answers it for the person, who typed `claude` there
// (ClaudeTrustDialog in Model/ClaudeStatus.swift, answered by ClaudeModeWatcher
// in Daemon/DaemonPoller.swift): the pointer down to the yes and, when the
// next look finds it there, Enter — never Enter on the no, and never a key
// before the question is up (an early key is read as Esc, which is "No, exit"
// too). Newer versions list the choices numbered, the yes first.

/** Where the pointer (`❯`) stands, and where the other choice is from it. */
export type Pointer = { on: "yes" } | { on: "no"; yes: "below" | "above" };

const YES = "Yes, I trust this folder";
const NO = "No, exit";

/**
 * Whether the question is on the screen, and where its pointer is. Undefined
 * when it is not the question — the words quoted in a conversation, say, have
 * no pointer in front of them.
 */
export function trustPointer(screen: string): Pointer | undefined {
  if (!screen.includes(YES) || !screen.includes(NO)) return undefined;
  const lines = screen.split("\n").filter((line) => line.trim() !== "");
  const yes = lines.findIndex((line) => line.includes(YES));
  for (let at = 0; at < lines.length; at++) {
    const line = lines[at].trim();
    if (!line.startsWith("❯")) continue;
    if (line.includes(YES)) return { on: "yes" };
    if (line.includes(NO)) return { on: "no", yes: yes > at ? "below" : "above" };
  }
  return undefined;
}

const DOWN = "\x1b[B";
const UP = "\x1b[A";
const ENTER = "\r";

/** The key that moves the pointer to the yes, or confirms it where it already is. */
export function trustKey(pointer: Pointer): string {
  if (pointer.on === "yes") return ENTER;
  return pointer.yes === "below" ? DOWN : UP;
}

/** A pane showing the question this moment. */
export interface Showing {
  key: string;
  pointer: Pointer;
}

/** How long the question is given to go away after Enter, and to be drawn again after a move of the pointer. */
const AFTER_ENTER_MS = 10_000;
const AFTER_MOVE_MS = 1_500;
const AFTER_MOVE_TO_YES_MS = 500;

/**
 * What to send, each second, to the panes showing the question: remembers what
 * was sent to each and when, so a key the question has not taken yet is not
 * sent again, and Enter goes once.
 */
export class TrustAnswerer {
  private readonly sent = new Map<string, { at: number; on: "yes" | "no" }>();

  step(showing: Showing[], now: number): { key: string; keys: string }[] {
    const here = new Set(showing.map((s) => s.key));
    for (const key of [...this.sent.keys()]) if (!here.has(key)) this.sent.delete(key);
    const out: { key: string; keys: string }[] = [];
    for (const { key, pointer } of showing) {
      const last = this.sent.get(key);
      if (last) {
        const since = now - last.at;
        // After Enter, the question gets ten seconds to go away; after the
        // arrow, a moment to be drawn with the pointer moved.
        if (last.on === "yes" && since < AFTER_ENTER_MS) continue;
        if (last.on === "no" && since < (pointer.on === "yes" ? AFTER_MOVE_TO_YES_MS : AFTER_MOVE_MS)) continue;
      }
      out.push({ key, keys: trustKey(pointer) });
      this.sent.set(key, { at: now, on: pointer.on });
    }
    return out;
  }
}

/**
 * Where the shell's own watcher (`keep ia confiar`, started by the `claude`
 * function) says it is answering, so that two answers do not overshoot: a
 * file named for the shell, in the user's `.local/state/keep/trust`.
 */
export function trustMarker(home: string, shellPid: number, windows: boolean): string {
  const separator = windows ? "\\" : "/";
  return [home.replace(/[\\/]+$/, ""), ".local", "state", "keep", "trust", String(shellPid)].join(separator);
}

/** A pane running Claude Code, looked at this second. */
export interface TrustPane {
  key: string;
  screen: string;
  /** The tab's shell, which names the marker of the shell's own watcher; 0 when unknown. */
  shellPid: number;
  /** Type into the pane. */
  send(keys: string): Promise<void>;
}

export interface TrustIO {
  home: string;
  windows: boolean;
  markerExists(path: string): Promise<boolean>;
  log(line: string): void;
  now(): number;
}

/** Looks at the panes running Claude Code once a second and answers the question where it is up. */
export class TrustWatch {
  private readonly answerer = new TrustAnswerer();
  private looking = false;

  async look(panes: TrustPane[], io: TrustIO): Promise<void> {
    // One look at a time: the marker questions are asked of the disk.
    if (this.looking) return;
    this.looking = true;
    try {
      const showing: Showing[] = [];
      const byKey = new Map<string, TrustPane>();
      for (const pane of panes) {
        const pointer = pane.screen === "" ? undefined : trustPointer(pane.screen);
        if (!pointer) continue;
        // The shell's own watcher answers it: two arrows would overshoot.
        const watched =
          pane.shellPid > 0 && (await io.markerExists(trustMarker(io.home, pane.shellPid, io.windows)).catch(() => false));
        if (watched) continue;
        showing.push({ key: pane.key, pointer });
        byKey.set(pane.key, pane);
      }
      for (const { key, keys } of this.answerer.step(showing, io.now())) {
        io.log(`trust question in ${key.replace("\u0000", "/")}: ${keys === "\r" ? "yes" : "pointer toward the yes"}`);
        void byKey.get(key)?.send(keys).catch(() => {});
      }
    } finally {
      this.looking = false;
    }
  }
}
