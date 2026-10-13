// Colour arithmetic for the themes: hex in, hex out, and the few operations
// the macOS app does on a terminal's background to colour the window around
// it (KeepWindow.recessed, GhosttyApp.isDark, PickerView.deepened).

/** Red, green and blue, each 0 to 255. */
export type Rgb = readonly [number, number, number];

/** `#rrggbb` or `rrggbb`, as a theme file spells a colour; anything else is not a colour. */
export function parseHex(text: string): Rgb | undefined {
  const hex = text.trim().replace(/^#/, "");
  if (!/^[0-9a-fA-F]{6}$/.test(hex)) return undefined;
  const value = parseInt(hex, 16);
  return [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
}

const channel = (n: number) => Math.min(255, Math.max(0, Math.round(n)));

/** `#rrggbb`, in lower case; channels outside 0..255 are held to it. */
export function toHex(rgb: Rgb): string {
  return `#${rgb.map((n) => channel(n).toString(16).padStart(2, "0")).join("")}`;
}

/**
 * How much light a colour gives off, 0 to 1: the weighted sum of its encoded
 * channels, as the macOS app takes it. Not the linear-light luminance: what
 * is dark enough to want white ink is decided by this figure there, and the
 * two apps must decide it alike.
 */
export function luminance([r, g, b]: Rgb): number {
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

/** Whether a terminal ground is dark: less than half the light. */
export function isDark(rgb: Rgb): boolean {
  return luminance(rgb) < 0.5;
}

/** Every channel times `factor`: taken toward black, so the hue is the colour's own. */
export function scale([r, g, b]: Rgb, factor: number): Rgb {
  return [channel(r * factor), channel(g * factor), channel(b * factor)];
}

/** `from` taken `share` of the way to `to` (0 is `from`, 1 is `to`). */
export function mix(from: Rgb, to: Rgb, share: number): Rgb {
  return [
    channel(from[0] + (to[0] - from[0]) * share),
    channel(from[1] + (to[1] - from[1]) * share),
    channel(from[2] + (to[2] - from[2]) * share),
  ];
}

/** `rgba(r, g, b, a)`, for the chrome's translucent grounds. */
export function rgba([r, g, b]: Rgb, alpha: number): string {
  return `rgba(${channel(r)}, ${channel(g)}, ${channel(b)}, ${alpha})`;
}

const linear = (n: number) => {
  const c = n / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};

/** WCAG's contrast ratio between two colours, 1 (the same) to 21 (black on white). */
export function contrast(a: Rgb, b: Rgb): number {
  const light = (rgb: Rgb) => 0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2]);
  const [high, low] = [light(a), light(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
}
