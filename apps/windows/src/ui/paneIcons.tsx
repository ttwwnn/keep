// What marks a tab that holds panes, and the handle a pane is moved by.
// Drawn as lines, as icons.tsx's are: the macOS app draws `rectangle.split.2x1`
// for the same thing, a glyph borrowed from a font being at the mercy of
// whichever face has it.

const stroke = {
  fill: "none",
  stroke: "currentColor",
  "stroke-width": 1.2,
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
} as const;

/** After a tab's title: this tab is divided into panes. */
export const PaneMark = () => (
  <svg class="pane-mark" width="12" height="12" viewBox="0 0 12 12" role="img" aria-label="com painéis">
    <rect x="1.5" y="2.5" width="9" height="7" rx="1.6" {...stroke} />
    <path d="M6 2.5v7" {...stroke} />
  </svg>
);

/** The three dots of a pane's handle. */
export const GripDots = () => (
  <svg width="21" height="3" viewBox="0 0 21 3" aria-hidden="true">
    <circle cx="1.5" cy="1.5" r="1.5" fill="currentColor" />
    <circle cx="9.5" cy="1.5" r="1.5" fill="currentColor" />
    <circle cx="17.5" cy="1.5" r="1.5" fill="currentColor" />
  </svg>
);
