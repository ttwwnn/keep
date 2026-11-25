// How a tab is titled, the same in the strip and in the sidebar.

import { useEffect, useRef } from "preact/hooks";
import {
  BREATH_PERIOD,
  activityColor,
  busyMark,
  attentionFill,
  attentionGlow,
  attentionTitleInk,
  css,
  lastBreath,
  modeColor,
  programOf,
} from "../claude";
import { isDormant } from "../iaChores";
import type { RootTab } from "../layout";
import { viewKey } from "../terminals";
import { RESTING, windowIdle } from "../theme";
import { now, palette, unseenDone, tabActivity, tabBusy, tabLabel, tabMode, tabWaitingSince } from "../store";

export interface TabLook {
  label: string;
  /** The title's ink, when its state gives it one. */
  color: string | undefined;
  /** Waiting on you: the orange badge with the raised hand. */
  wantsYou: boolean;
  /** Running: the mark in front, by what runs there. */
  atWork: boolean;
  mark: { glyph: string; color: string } | undefined;
  /** The badge breathes for its first minute, while out of sight. */
  breathing: boolean;
  /** Its conversation is out of memory (twelve hours alone, or the zzz): the title is struck through. */
  asleep: boolean;
  /** Its turn ended out of sight and nobody has entered it since: the lime capsule, black ink. */
  unseen: boolean;
  fill: string;
}

// The sidebar's resting hue for each mode, the macOS app's mapping: its teal is green, its violet blue.
const RESTING_MODE = { plan: RESTING.green, acceptEdits: RESTING.blue, bypass: RESTING.red, auto: RESTING.yellow };

/**
 * `resting`: the sidebar's rows, which wear the five resting hues while the window is out of use. `held`: a workspace's
 * header whose tabs below are still going (`holdsHeader`): its turn over out of sight is not the lime yet.
 */
export function tabLook(workspace: string, tab: RootTab, index: number, selected: boolean, resting = false, held = false): TabLook {
  const dark = palette.value.dark;
  const idle = resting && dark && windowIdle.value;
  const activity = tabActivity(workspace, tab);
  const mode = tabMode(workspace, tab);
  const wantsYou = activity === "waitingForYou";
  const atWork = tabBusy(workspace, tab);
  const label = tabLabel(workspace, tab, index);
  const mark = !wantsYou && atWork ? busyMark(programOf(tab.root.command, tab.root.title)) : undefined;
  const asleep = isDormant(workspace, tab);
  const unseen =
    !held && !wantsYou && !selected && activity === "done" && tab.panes.some((p) => unseenDone.value.has(viewKey(workspace, p.id)));
  let color: string | undefined;
  if (wantsYou) color = css(attentionTitleInk(selected));
  else if (unseen) color = "rgb(20, 20, 20)";
  else if (idle && activity === "waitingForWorkflow") color = RESTING.blue;
  else if (idle && (!activity || activity === "working") && mode) color = RESTING_MODE[mode];
  // At rest, the Finder's way: the chosen tab's name in the selection's blue, a sleeping one faint, the rest in the one
  // grey (a finished turn's too).
  else if (idle) color = selected ? RESTING.selectedText : asleep ? RESTING.faint : RESTING.text;
  else if (activity && activity !== "working") color = css(activityColor(activity, dark)!);
  else if (mode) color = css(modeColor(mode, dark));
  const since = tabWaitingSince(workspace, tab);
  const until = lastBreath(since);
  const breathing = wantsYou && !selected && !idle && until !== undefined && now.value / 1000 < until;
  return { label, color, wantsYou, atWork, mark, breathing, asleep, unseen, fill: idle ? RESTING.orange : css(attentionFill(dark)) };
}

/**
 * Breathe an element's background between the badge's orange and its glow,
 * on one clock for every badge: the animation's start is set to the last beat
 * of the shared period, so badges drawn at different moments pulse together.
 */
export function useBreath(breathing: boolean) {
  const ref = useRef<HTMLElement | null>(null);
  const dark = palette.value.dark;
  useEffect(() => {
    const element = ref.current;
    if (!element || !breathing || typeof element.animate !== "function") return;
    const period = BREATH_PERIOD * 1000;
    const animation = element.animate(
      [
        { backgroundColor: css(attentionFill(dark)) },
        { backgroundColor: css(attentionGlow(dark)) },
        { backgroundColor: css(attentionFill(dark)) },
      ],
      { duration: period, iterations: Infinity, easing: "ease-in-out" },
    );
    const timeline = document.timeline.currentTime;
    if (typeof timeline === "number") animation.startTime = Math.floor(timeline / period) * period;
    return () => animation.cancel();
  }, [breathing, dark]);
  return ref;
}
