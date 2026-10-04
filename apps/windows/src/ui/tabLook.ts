// How a tab is titled, the same in the strip and in the sidebar.

import { useEffect, useRef } from "preact/hooks";
import {
  BREATH_PERIOD,
  activityColor,
  attentionFill,
  attentionGlow,
  attentionTitleInk,
  css,
  lastBreath,
  modeColor,
} from "../claude";
import type { RootTab } from "../layout";
import { now, palette, tabActivity, tabBusy, tabLabel, tabMode, tabWaitingSince } from "../store";

export interface TabLook {
  label: string;
  /** The title's ink, when its state gives it one. */
  color: string | undefined;
  /** Waiting on you: the orange badge with the raised hand. */
  wantsYou: boolean;
  /** Running: the ✳ in front. */
  atWork: boolean;
  /** The badge breathes for its first minute, while out of sight. */
  breathing: boolean;
  fill: string;
}

export function tabLook(workspace: string, tab: RootTab, index: number, selected: boolean): TabLook {
  const dark = palette.value.dark;
  const activity = tabActivity(workspace, tab);
  const mode = tabMode(workspace, tab);
  const wantsYou = activity === "waitingForYou";
  const atWork = tabBusy(workspace, tab);
  let label = tabLabel(workspace, tab, index);
  if (!wantsYou && atWork && !label.startsWith("✳")) label = `✳ ${label}`;
  let color: string | undefined;
  if (wantsYou) color = css(attentionTitleInk(selected));
  else if (activity && activity !== "working") color = css(activityColor(activity, dark)!);
  else if (mode) color = css(modeColor(mode, dark));
  const since = tabWaitingSince(workspace, tab);
  const until = lastBreath(since);
  const breathing = wantsYou && !selected && until !== undefined && now.value / 1000 < until;
  return { label, color, wantsYou, atWork, breathing, fill: css(attentionFill(dark)) };
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
