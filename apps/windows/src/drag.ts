// Dragging with the pointer: tabs along the row, tabs and workspaces along the
// sidebar, panes by their handle.
//
// With the pointer rather than HTML drag and drop: on Windows the webview
// hands every drag to the window's own file-drop handler — which is what lets
// files dropped on a terminal arrive as their paths — and HTML drag and drop
// inside the page never starts.

export interface Point {
  x: number;
  y: number;
}

export interface DragHandlers {
  /** How far, in pixels, the pointer must go before the press is a drag and not a click. */
  threshold?: number;
  /** The pointer went far enough. Returning false leaves the press a click. */
  start?(at: Point, event: PointerEvent): boolean | void;
  move(at: Point, event: PointerEvent): void;
  /** Over: `dropped` is false when Esc, the system or the window's losing focus ended it instead of letting go. */
  end(at: Point, dropped: boolean): void;
}

let justDragged = false;

let trace: ((line: string) => void) | undefined;

/** Where what a drag did is said — a scripted run's, to tell why one did not do what was asked of it. */
export function traceDrags(sink: ((line: string) => void) | undefined): void {
  trace = sink;
}

const named = (e: Element | null) =>
  e ? `${e.tagName.toLowerCase()}${e.className && typeof e.className === "string" ? `.${e.className.trim().replace(/\s+/g, ".")}` : ""}` : "nada";

/** The click that ends a drag is not a click on what was dragged: asked of by whatever selects on click. */
export function endedADrag(): boolean {
  return justDragged;
}

/**
 * Follow a press of the primary button. It is a click until the pointer has
 * gone `threshold` away; from then on `move` hears every step and `end`
 * hears how it finished. Esc calls it off.
 */
export function trackDrag(down: PointerEvent, handlers: DragHandlers): void {
  if (down.button !== 0) return;
  const holder = down.currentTarget instanceof HTMLElement ? down.currentTarget : null;
  const threshold = handlers.threshold ?? 4;
  const origin: Point = { x: down.clientX, y: down.clientY };
  let last = origin;
  let active = false;
  let over = false;

  trace?.(`pressionado em ${Math.round(origin.x)},${Math.round(origin.y)} sobre ${named(down.target instanceof Element ? down.target : null)}`);
  const at = (e: PointerEvent): Point => ({ x: e.clientX, y: e.clientY });
  const stop = () => {
    over = true;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("blur", cancel);
    window.removeEventListener("keydown", key, true);
    try {
      holder?.releasePointerCapture(down.pointerId);
    } catch {
      // Already let go of with the pointer.
    }
  };
  const finish = (dropped: boolean, why: string) => {
    if (over) return;
    stop();
    trace?.(`fim por ${why} em ${Math.round(last.x)},${Math.round(last.y)}; arrastando: ${active}; solto: ${dropped}`);
    if (!active) return;
    justDragged = true;
    setTimeout(() => (justDragged = false), 0);
    handlers.end(last, dropped);
  };
  function move(e: PointerEvent) {
    if (over || e.pointerId !== down.pointerId) return;
    last = at(e);
    if (!active) {
      if (Math.hypot(last.x - origin.x, last.y - origin.y) < threshold) return;
      active = true;
      if (handlers.start?.(last, e) === false) {
        trace?.("recusado ao começar: o que se media não bate com o que se via");
        active = false;
        stop();
        return;
      }
      trace?.(`começou em ${Math.round(last.x)},${Math.round(last.y)}`);
      try {
        holder?.setPointerCapture(down.pointerId);
      } catch {
        // The window keeps hearing it either way.
      }
    }
    handlers.move(last, e);
  }
  function up(e: PointerEvent) {
    if (e.pointerId !== down.pointerId) return;
    last = at(e);
    finish(true, "pointerup");
  }
  function cancel(e: Event) {
    finish(false, e.type);
  }
  function key(e: KeyboardEvent) {
    if (e.key !== "Escape" || !active) return;
    e.preventDefault();
    e.stopPropagation();
    finish(false, "Esc");
  }
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("blur", cancel);
  window.addEventListener("keydown", key, true);
}

/** A copy of an element hanging from the pointer, as the system's drag image would. */
export interface Ghost {
  follow(at: Point): void;
  remove(): void;
}

export function hangGhost(source: HTMLElement, at: Point): Ghost {
  const rect = source.getBoundingClientRect();
  const grab = { x: at.x - rect.left, y: at.y - rect.top };
  const copy = source.cloneNode(true) as HTMLElement;
  copy.classList.add("drag-ghost");
  for (const attribute of [...copy.getAttributeNames()]) {
    if (attribute.startsWith("data-")) copy.removeAttribute(attribute);
  }
  copy.style.width = `${rect.width}px`;
  copy.style.height = `${rect.height}px`;
  document.body.appendChild(copy);
  const follow = (to: Point) => {
    copy.style.transform = `translate(${Math.round(to.x - grab.x)}px, ${Math.round(to.y - grab.y)}px)`;
  };
  follow(at);
  return { follow, remove: () => copy.remove() };
}

/**
 * How far a list scrolls in a frame for a pointer `distance` from one of its
 * edges (negative: past it): none beyond `reach`, the most at the edge and
 * beyond it, between the two in proportion.
 */
export function scrollStep(distance: number, reach = 36, most = 14): number {
  if (distance >= reach) return 0;
  return Math.ceil((Math.min(reach - distance, reach) / reach) * most);
}

/**
 * Scroll a list while the pointer is near its top or bottom edge, faster the
 * closer it is: a drag toward a workspace out of sight must be able to reach
 * it. What is under a pointer that does not move changes as the list scrolls,
 * so `scrolled` is told each time it did, with where the pointer is, for the
 * drag to look again at what it is over. Returns how to stop.
 */
export function autoScroll(list: HTMLElement, pointer: () => Point | null, scrolled: (at: Point) => void): () => void {
  let frame = 0;
  const step = () => {
    const at = pointer();
    if (at) {
      const box = list.getBoundingClientRect();
      const up = scrollStep(at.y - box.top);
      const down = scrollStep(box.bottom - at.y);
      const before = list.scrollTop;
      // The nearer edge, if the list is too short to have a middle.
      if (up > 0 && up >= down) list.scrollTop -= up;
      else if (down > 0) list.scrollTop += down;
      if (list.scrollTop !== before) scrolled(at);
    }
    frame = requestAnimationFrame(step);
  };
  frame = requestAnimationFrame(step);
  return () => cancelAnimationFrame(frame);
}
