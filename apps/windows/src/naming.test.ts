// Finder's slow click: when it starts the wait, what calls it off, and what
// it opens.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mayArmRename, SLOW_CLICK_MS, SlowRename, type Watch } from "./naming";

describe("a click that may ask for a name", () => {
  const click = { wasChosen: true, onName: true, dragged: false, count: 1 };
  it("is a single click on the name of what was already chosen", () => {
    expect(mayArmRename(click)).toBe(true);
  });
  it("is not on something that was not chosen yet: that click chooses", () => {
    expect(mayArmRename({ ...click, wasChosen: false })).toBe(false);
  });
  it("is not beside the name, nor the end of a drag, nor half a double-click", () => {
    expect(mayArmRename({ ...click, onName: false })).toBe(false);
    expect(mayArmRename({ ...click, dragged: true })).toBe(false);
    expect(mayArmRename({ ...click, count: 2 })).toBe(false);
  });
});

describe("the wait", () => {
  let cancel: (() => void) | undefined;
  let watching = 0;
  const watch: Watch = (call) => {
    cancel = call;
    watching += 1;
    return () => {
      watching -= 1;
    };
  };
  beforeEach(() => {
    vi.useFakeTimers();
    cancel = undefined;
    watching = 0;
  });
  afterEach(() => vi.useRealTimers());

  it("opens the name once the interval has passed with nothing done", () => {
    const begin = vi.fn();
    const wait = new SlowRename(watch);
    wait.arm(begin, () => true);
    vi.advanceTimersByTime(SLOW_CLICK_MS - 1);
    expect(begin).not.toHaveBeenCalled();
    expect(wait.waiting).toBe(true);
    vi.advanceTimersByTime(1);
    expect(begin).toHaveBeenCalledTimes(1);
    expect(wait.waiting).toBe(false);
    expect(watching).toBe(0);
  });
  it("is called off by a key, a press or the window losing focus", () => {
    const begin = vi.fn();
    const wait = new SlowRename(watch);
    wait.arm(begin, () => true);
    cancel?.();
    vi.advanceTimersByTime(SLOW_CLICK_MS * 2);
    expect(begin).not.toHaveBeenCalled();
    expect(wait.waiting).toBe(false);
    expect(watching).toBe(0);
  });
  it("does not open for something no longer in front", () => {
    const begin = vi.fn();
    new SlowRename(watch).arm(begin, () => false);
    vi.advanceTimersByTime(SLOW_CLICK_MS);
    expect(begin).not.toHaveBeenCalled();
  });
  it("arming again calls the first off, and listens once", () => {
    const first = vi.fn();
    const second = vi.fn();
    const wait = new SlowRename(watch);
    wait.arm(first, () => true);
    vi.advanceTimersByTime(100);
    wait.arm(second, () => true);
    expect(watching).toBe(1);
    vi.advanceTimersByTime(SLOW_CLICK_MS);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });
});
