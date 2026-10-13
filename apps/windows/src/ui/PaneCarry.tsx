// Carrying a pane by its handle: the three dots at the top of the pane, in the
// place a title bar would be if a pane had one. It is the only part of a
// terminal that is not the terminal — everywhere else the mouse belongs to the
// program inside — so it is what a pane is picked up by.
//
// Picked up, a card with the pane's text follows the pointer, and the pane
// under it shows where the carried one would land: a half for a side, nearly
// all of it for a trade. Let go over a pane, the move is made; over a tab of
// the row, the pane joins that tab; anywhere else in the window, it becomes a
// tab of its own. Held over a tab of the row for a moment, that tab opens —
// the spring — so the pane can be dropped into it.

import { signal } from "@preact/signals";
import * as api from "../api";
import { trackDrag, type Point } from "../drag";
import { holds } from "../hibernation";
import { anchorPane, detachPane, movePane } from "../paneActions";
import { dropSide, landingRect, type DropSide, type Rect } from "../paneMoves";
import { tabKey } from "../state";
import { activeTab, focusedPane, gui, select } from "../store";
import { GripDots } from "./paneIcons";

/** What is being carried: where the pointer is, the pane's text, and where it would land. */
export const paneCarry = signal<{ pane: number; x: number; y: number; text: string; landing: Rect | null } | null>(null);

/** How long a pane held over a tab waits before the tab opens. */
const SPRING_MS = 450;
/** The lines of the pane's screen the card shows. */
const CARD_LINES = 26;

const rectOf = (r: DOMRect): Rect => ({ x: r.left, y: r.top, width: r.width, height: r.height });
const contains = (r: DOMRect, at: Point) => at.x >= r.left && at.x < r.right && at.y >= r.top && at.y < r.bottom;

/** The pane of the tab in front under a point, the side aimed at, and the paint for it. */
function aim(at: Point, carried: number | null): { target: number; side: DropSide; rect: Rect } | null {
  for (const element of document.querySelectorAll<HTMLElement>(".tab-view.shown .pane[data-pane]")) {
    const id = Number(element.dataset.pane);
    if (id === carried) continue;
    const box = element.getBoundingClientRect();
    if (!contains(box, at)) continue;
    const side = dropSide((at.x - box.left) / Math.max(1, box.width), (at.y - box.top) / Math.max(1, box.height));
    return { target: id, side, rect: landingRect(side, rectOf(box)) };
  }
  return null;
}

/** The tab of the workspace under a point: in the row above the terminal or, with the tabs in the sidebar alone, in the sidebar. */
function tabUnder(at: Point, workspace: string): number | null {
  const rows = gui.value.verticalTabs ? ".sidebar .tab-row[data-row]" : ".strip .tab[data-tab]";
  for (const element of document.querySelectorAll<HTMLElement>(rows)) {
    if (!contains(element.getBoundingClientRect(), at)) continue;
    const [name, root] = (element.dataset.tab ?? element.dataset.row ?? "").split("\u0000");
    return name === workspace ? Number(root) : null;
  }
  return null;
}

/** Run a pane's drag to its end. */
function carry(event: PointerEvent, workspace: string, root: number, pane: number): void {
  let spring: { root: number; timer: ReturnType<typeof setTimeout> } | null = null;
  let landing: ReturnType<typeof aim> = null;
  const cancelSpring = () => {
    if (spring) clearTimeout(spring.timer);
    spring = null;
  };
  trackDrag(event, {
    start(at) {
      // The drag can land on any tab: none is put to sleep meanwhile.
      holds.value += 1;
      paneCarry.value = { pane, x: at.x, y: at.y, text: "", landing: null };
      // The card shows the pane's text rather than a picture of it: the daemon already holds it.
      void api
        .preview(workspace, pane)
        .then((text) => {
          const lines = text.split("\n").slice(-CARD_LINES).join("\n");
          if (paneCarry.value?.pane === pane) paneCarry.value = { ...paneCarry.value, text: lines };
        })
        .catch(() => {});
    },
    move(at) {
      // A pane cannot land on itself — but once the drag has crossed into another tab, every pane there is somewhere it could go.
      const inSource = activeTab.value?.root.id === root;
      landing = aim(at, inSource ? pane : null);
      const over = tabUnder(at, workspace);
      if (over === null || over === activeTab.value?.root.id) {
        cancelSpring();
      } else if (spring?.root !== over) {
        cancelSpring();
        spring = { root: over, timer: setTimeout(() => select(workspace, over), SPRING_MS) };
      }
      paneCarry.value = { pane, x: at.x, y: at.y, text: paneCarry.value?.text ?? "", landing: landing?.rect ?? null };
    },
    end(at, dropped) {
      holds.value -= 1;
      cancelSpring();
      const target = landing;
      landing = null;
      paneCarry.value = null;
      if (!dropped) return;
      const area = document.querySelector<HTMLElement>(".terminal-area");
      if (area && contains(area.getBoundingClientRect(), at)) {
        if (target) void movePane(workspace, pane, target.target, target.side);
        return;
      }
      // Over a tab of the row: the pane joins it, beside what is there already.
      const over = tabUnder(at, workspace);
      if (over !== null && over !== root) {
        void movePane(workspace, pane, anchorPane(workspace, over, focusedPane.value[tabKey(workspace, over)]), "right");
        return;
      }
      // Let go outside the window, it is a drag that did not happen; anywhere else in it, a tab of its own.
      if (at.x < 0 || at.y < 0 || at.x > window.innerWidth || at.y > window.innerHeight) return;
      void detachPane(workspace, pane);
    },
  });
}

/** The handle, at the top of a pane that has others beside it. */
export function PaneGrip(props: { workspace: string; root: number; pane: number }) {
  const dragging = paneCarry.value?.pane === props.pane;
  return (
    <div
      class={`pane-grip ${dragging ? "dragging" : ""}`}
      title="Arrastar o painel"
      aria-label="Arrastar o painel"
      onPointerDown={(e) => carry(e, props.workspace, props.root, props.pane)}
    >
      <GripDots />
    </div>
  );
}

/** The card under the pointer and the paint on the pane it would land on. */
export function PaneCarryOverlay() {
  const carrying = paneCarry.value;
  if (!carrying) return null;
  const { landing, x, y, text } = carrying;
  return (
    <>
      {landing && (
        <div
          class="pane-landing"
          style={{ left: `${landing.x}px`, top: `${landing.y}px`, width: `${landing.width}px`, height: `${landing.height}px` }}
        />
      )}
      {/* Held below and right of the pointer, out from under it, the way a dragged thing hangs off the hand carrying it. */}
      <div class="pane-card" style={{ transform: `translate(${Math.round(x - 24)}px, ${Math.round(y - 16)}px)` }}>
        <pre>{text}</pre>
      </div>
    </>
  );
}
