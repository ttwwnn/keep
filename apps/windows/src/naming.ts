// Naming a tab or a workspace by clicking on its name.
//
// Finder's way: the chosen item's name, clicked once, opens for typing after
// the double-click interval has gone by with nothing else done. Waiting is
// what keeps the first click of a double-click from being taken for a single
// one; and anything done meanwhile — a key, another press, the pointer
// moving off the name — means the click was not asking for a name.

/** Windows' own double-click time, which the page cannot ask for. */
export const SLOW_CLICK_MS = 500;

/** Whether a click may start the wait: on the name of what was already chosen, and not the end of a drag. */
export function mayArmRename(click: { wasChosen: boolean; onName: boolean; dragged: boolean; count: number }): boolean {
  return click.wasChosen && click.onName && !click.dragged && click.count === 1;
}

/** How the wait learns of something being done: it hands over what to call, and gets back how to stop listening. */
export type Watch = (cancel: () => void) => () => void;

/** The keyboard, a press anywhere and the window losing the pointer's attention, as the page sees them. */
const watchWindow: Watch = (cancel) => {
  const events = ["keydown", "pointerdown", "blur"] as const;
  for (const name of events) window.addEventListener(name, cancel, true);
  return () => {
    for (const name of events) window.removeEventListener(name, cancel, true);
  };
};

/** One rename waiting out its interval at a time. */
export class SlowRename {
  private timer: ReturnType<typeof setTimeout> | undefined;
  private unwatch: (() => void) | undefined;

  constructor(private readonly watch: Watch = watchWindow) {}

  get waiting(): boolean {
    return this.timer !== undefined;
  }

  /**
   * Wait, then `begin` — if `stillChosen` still says the click was on what is
   * in front. Arming again calls the first off.
   */
  arm(begin: () => void, stillChosen: () => boolean, delay = SLOW_CLICK_MS): void {
    this.disarm();
    this.unwatch = this.watch(() => this.disarm());
    this.timer = setTimeout(() => {
      this.disarm();
      if (stillChosen()) begin();
    }, delay);
  }

  /** Call off a rename that is waiting. */
  disarm(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.unwatch?.();
    this.unwatch = undefined;
  }
}

/** The one wait there is: a name is opened for typing in one place at a time. */
export const slowRename = new SlowRename();
