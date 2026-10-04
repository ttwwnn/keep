// Chrome's zoom, for the terminal: a port of TerminalZoom in the macOS app.
//
// Every terminal gets larger or smaller a step at a time, named the way a
// browser names them — as a share of the size the text normally is. The tabs'
// titles follow by the same share; the frame around them stays.

/** Chrome's steps, from half to three times. */
export const ZOOM_LEVELS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];

/** How far from a step a share may be and still count as that step. */
const SLACK = 0.005;

/**
 * The next step from `factor`: larger when `direction` is positive, smaller
 * when negative, `undefined` when there is none that way. A share between two
 * steps goes on to the next one in the direction asked, never back.
 */
export function zoomStep(factor: number, direction: number): number | undefined {
  return direction > 0
    ? ZOOM_LEVELS.find((level) => level > factor + SLACK)
    : [...ZOOM_LEVELS].reverse().find((level) => level < factor - SLACK);
}

/** "110%". Whole numbers, as a browser shows them: two thirds is 67%. */
export const zoomPercent = (factor: number) => `${Math.round(factor * 100)}%`;

/** A share read back from the state file, held to the steps. */
export function normalizeZoom(factor: unknown): number {
  if (typeof factor !== "number" || !Number.isFinite(factor)) return 1;
  let best = 1;
  for (const level of ZOOM_LEVELS) {
    if (Math.abs(level - factor) < Math.abs(best - factor)) best = level;
  }
  return best;
}

/** The font size the text is drawn at, for a base size and a share; to the hundredth. */
export const zoomedSize = (base: number, factor: number) => Math.round(base * factor * 100) / 100;
