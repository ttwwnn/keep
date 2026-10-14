// Moving tabs and workspaces by hand: a tab along the row above the terminal,
// a tab or a workspace along the sidebar, and a tab dropped on another
// workspace. What is dragged slides with the pointer and the others open a
// place for it, as the macOS app's do (TabStripView.carry, the sidebar's
// drop delegates); nothing is kept until it is let go. A tab pulled out of the
// row, up or down past its band, leaves it: let go, it makes a window of its own
// or goes to the one it is let go over.

import { signal } from "@preact/signals";
import { autoScroll, hangGhost, trackDrag, type Ghost, type Point } from "../drag";
import type { RootTab } from "../layout";
import { pitches, slides, slotAt, type Span } from "../reorder";
import { moveId } from "../tabOrder";
import { moveTabTo, moveWorkspace, reorderTabs, shownWorkspaces } from "../store";
import { pulledOut } from "../windowModel";
import { tearOffTab } from "../windowSync";

// ------------------------------------------------------------------ the row above the terminal

/**
 * The tab being carried along the row, and how the others have slid for it.
 * `tearing`: it has been pulled out of the row — a copy of it hangs from the
 * pointer, and the others have closed the gap it left.
 */
export const stripMove = signal<{
  workspace: string;
  from: number;
  to: number;
  dx: number;
  slid: number[];
  tearing?: boolean;
} | null>(null);

/** The box of each element along an axis, from where the move began. */
function spansOf(elements: Element[], axis: "x" | "y", origin = 0): Span[] {
  return elements.map((e) => {
    const r = e.getBoundingClientRect();
    return axis === "x" ? { start: r.left - origin, size: r.width } : { start: r.top - origin, size: r.height };
  });
}

/**
 * Carry a tab along the row. Past the first few pixels the tab follows the
 * pointer sideways — it never leaves the row, and the row puts it first or
 * last when the pointer goes past either end — and the others slide to
 * leave a gap where it would land; let go, the row is kept in that order.
 * Pulled up or down out of the row's band (windowModel.ts `pulledOut`) it is
 * no longer being put in a place of the row: a copy of it hangs from the
 * pointer, the others close the gap, and let go it is torn out — into a new
 * window, or the window it is let go over (windowSync.ts).
 */
export function startStripDrag(event: PointerEvent, workspace: string, tabs: RootTab[], index: number): void {
  if (tabs.length < 2 || (event.target as HTMLElement).closest("button, input")) return;
  const tab = event.currentTarget as HTMLElement;
  const strip = tab.closest<HTMLElement>(".strip");
  if (!strip) return;
  let spans: Span[] = [];
  let pitch: number[] = [];
  let limits = { min: 0, max: 0 };
  let band = { top: 0, bottom: 0 };
  let ghost: Ghost | null = null;
  const start = { x: event.clientX, y: event.clientY };
  trackDrag(event, {
    start() {
      spans = spansOf([...strip.querySelectorAll(".tab")], "x");
      if (spans.length !== tabs.length) return false;
      pitch = pitches(spans);
      const room = strip.getBoundingClientRect();
      limits = { min: room.left - spans[index].start, max: room.right - (spans[index].start + spans[index].size) };
      band = { top: room.top, bottom: room.bottom };
      return true;
    },
    move(at) {
      if (pulledOut(at.y, band)) {
        if (!ghost) ghost = hangGhost(tab, at);
        ghost.follow(at);
        // The gap it left is closed: the ones after it slide into its place.
        stripMove.value = { workspace, from: index, to: index, dx: 0, slid: slides(pitch, index, pitch.length - 1), tearing: true };
        return;
      }
      ghost?.remove();
      ghost = null;
      const dx = Math.min(Math.max(at.x - start.x, limits.min), limits.max);
      // The middle of the carried tab decides the place, as the macOS row does.
      const middle = spans[index].start + spans[index].size / 2 + dx;
      const to = Math.max(0, slotAt(spans, middle));
      stripMove.value = { workspace, from: index, to, dx, slid: slides(pitch, index, to) };
    },
    end(at, dropped) {
      const moved = stripMove.value;
      ghost?.remove();
      ghost = null;
      stripMove.value = null;
      if (!dropped || !moved) return;
      if (moved.tearing) {
        void tearOffTab(workspace, tabs[index], start, at);
        return;
      }
      if (moved.to === moved.from) return;
      reorderTabs(workspace, moveId(tabs.map((t) => t.root.id), tabs[moved.from].root.id, moved.to));
    },
  });
}

// ------------------------------------------------------------------ the sidebar

/** What is being dragged along the sidebar, and where it would land. */
export type SidebarMove =
  | {
      kind: "tab";
      workspace: string;
      root: number;
      from: number;
      /** The place among its own group's tabs it would take. */
      to: number;
      slid: number[];
      /** Another workspace the pointer is over, and the tab of it the dragged one would go before. */
      into: string | null;
      before: number | null;
    }
  | { kind: "workspace"; name: string; from: number; to: number; slid: number[] };

export const sidebarMove = signal<SidebarMove | null>(null);

/** The sidebar's list and the pointer in it, measured from its content so that scrolling does not move the places. */
function inList(list: HTMLElement, at: Point): number {
  return at.y - list.getBoundingClientRect().top + list.scrollTop;
}

function listSpans(list: HTMLElement, elements: Element[]): Span[] {
  const top = list.getBoundingClientRect().top - list.scrollTop;
  return spansOf(elements, "y", top);
}

/** What the pointer is over in another workspace: its name, and the tab it is on, if it is on one. */
function overWorkspace(at: Point): { workspace: string; before: number | null } | null {
  const element = document.elementFromPoint(at.x, at.y);
  const section = element?.closest<HTMLElement>(".workspace");
  const workspace = section?.dataset.workspace;
  if (!section || workspace === undefined) return null;
  const row = element?.closest<HTMLElement>(".tab-row[data-row]");
  const root = row?.dataset.row?.split("\u0000")[1];
  return { workspace, before: root === undefined ? null : Number(root) };
}

/**
 * Carry a tab of the sidebar. Over its own group the others open a gap, live;
 * over another workspace's header or tab that one lights, and letting go
 * there moves the tab to it, panes and shells intact, before the tab it was
 * dropped on. Anywhere else, or Esc, nothing happens.
 */
export function startSidebarTabDrag(event: PointerEvent, workspace: string, tabs: RootTab[], index: number): void {
  if (tabs.length === 0 || (event.target as HTMLElement).closest("button, input")) return;
  const row = event.currentTarget as HTMLElement;
  const list = row.closest<HTMLElement>(".sidebar-list");
  const group = row.closest<HTMLElement>(".workspace");
  if (!list || !group) return;
  let spans: Span[] = [];
  let pitch: number[] = [];
  let ghost: Ghost | null = null;
  let stopScrolling: (() => void) | null = null;
  let pointer: Point | null = null;
  // The place it would take among its own group's tabs: kept while the pointer is elsewhere.
  let place = index;
  const dragged = tabs[index];
  trackDrag(event, {
    start(at) {
      spans = listSpans(list, [...group.querySelectorAll(".tab-row")]);
      if (spans.length !== tabs.length) return false;
      pitch = pitches(spans);
      ghost = hangGhost(row, at);
      stopScrolling = autoScroll(list, () => pointer);
      return true;
    },
    move(at) {
      pointer = at;
      ghost?.follow(at);
      const y = inList(list, at);
      const inGroup = y >= spans[0].start && y < spans[spans.length - 1].start + spans[spans.length - 1].size;
      const elsewhere = inGroup ? null : overWorkspace(at);
      const into = elsewhere && elsewhere.workspace !== workspace ? elsewhere : null;
      if (inGroup) place = Math.max(0, slotAt(spans, y));
      sidebarMove.value = {
        kind: "tab",
        workspace,
        root: dragged.root.id,
        from: index,
        to: place,
        slid: slides(pitch, index, place),
        into: into?.workspace ?? null,
        before: into?.before ?? null,
      };
    },
    end(at, dropped) {
      const moved = sidebarMove.value;
      stopScrolling?.();
      ghost?.remove();
      sidebarMove.value = null;
      if (!dropped || moved?.kind !== "tab") return;
      if (moved.into !== null) {
        void moveTabTo(workspace, dragged, moved.into, moved.before);
        return;
      }
      const y = inList(list, at);
      const onGroup = y >= spans[0].start && y < spans[spans.length - 1].start + spans[spans.length - 1].size;
      if (onGroup && moved.to !== moved.from) {
        reorderTabs(workspace, moveId(tabs.map((t) => t.root.id), dragged.root.id, moved.to));
      }
    },
  });
}

/**
 * Carry a workspace along the sidebar by its header: the groups slide to
 * leave a gap, and the order is kept when it is let go.
 */
export function startWorkspaceDrag(event: PointerEvent, name: string): void {
  if ((event.target as HTMLElement).closest("button, input")) return;
  const header = event.currentTarget as HTMLElement;
  const list = header.closest<HTMLElement>(".sidebar-list");
  if (!list) return;
  const names = shownWorkspaces.value.map((w) => w.name);
  const index = names.indexOf(name);
  let spans: Span[] = [];
  let pitch: number[] = [];
  let ghost: Ghost | null = null;
  let stopScrolling: (() => void) | null = null;
  let pointer: Point | null = null;
  trackDrag(event, {
    start(at) {
      spans = listSpans(list, [...list.querySelectorAll(".workspace")]);
      if (spans.length !== names.length || index < 0) return false;
      pitch = pitches(spans);
      ghost = hangGhost(header, at);
      stopScrolling = autoScroll(list, () => pointer);
      return true;
    },
    move(at) {
      pointer = at;
      ghost?.follow(at);
      const to = Math.max(0, slotAt(spans, inList(list, at)));
      sidebarMove.value = { kind: "workspace", name, from: index, to, slid: slides(pitch, index, to) };
    },
    end(_at, dropped) {
      const moved = sidebarMove.value;
      stopScrolling?.();
      ghost?.remove();
      sidebarMove.value = null;
      if (!dropped || moved?.kind !== "workspace" || moved.to === moved.from) return;
      const rest = names.filter((n) => n !== name);
      moveWorkspace(name, rest[moved.to] ?? null);
    },
  });
}
