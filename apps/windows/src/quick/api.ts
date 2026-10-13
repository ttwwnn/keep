// The drop-down terminal's questions to the app (src-tauri/src/quick.rs), apart
// from api.ts because nothing else in the window asks them.

import { invoke } from "@tauri-apps/api/core";

/** The drop-down terminal as the app tells it. */
export interface QuickStatus {
  /** The shortcut asked for, as `Control+Alt+Backquote`: the modifiers in that order, then the key's `code`. */
  shortcut: string;
  /** Whether the system gave it. */
  registered: boolean;
  /** If not, what the system said. */
  problem: string | null;
  /** A shortcut that is free now, to offer in its place. */
  suggestion: string | null;
  /** Whether the panel is meant to be in view. */
  visible: boolean;
  /** The window as it stands; null until it is first asked for. */
  window: {
    frame: { x: number; y: number; width: number; height: number };
    /** Where it was last sent to rest: the frame, once it has got there. */
    rest: { x: number; y: number; width: number; height: number } | null;
    alwaysOnTop: boolean;
    focused: boolean;
    shown: boolean;
  } | null;
}

/** Bring the drop-down terminal in, or put it away; whether it is in after. */
export const quickToggle = () => invoke<boolean>("quick_toggle");
export const quickStatus = () => invoke<QuickStatus>("quick_status");
/** Choose the shortcut; the state after. The system may refuse it, and the answer is then why. */
export const quickSetShortcut = (shortcut: string) => invoke<QuickStatus>("quick_set_shortcut", { shortcut });
/** The drop-down terminal's page has drawn: its first showing was waiting for that. */
export const quickReady = () => invoke<void>("quick_ready");
/** The window's state as it stands on the disk, read and nothing more; what the drop-down terminal wears. */
export const quickLook = () => invoke<unknown | null>("quick_look");
