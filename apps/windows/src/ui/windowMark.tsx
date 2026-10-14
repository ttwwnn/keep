// What marks a tab another window has in front. The macOS app draws the SF
// Symbol `macwindow.on.rectangle` after the title for the same thing — a
// window over another —, in the strip and in the sidebar alike: the row is
// the workspace's tabs, not this window's, so without it a tab pulled out
// into a window of its own looks like a tab that never left.

const stroke = {
  fill: "none",
  stroke: "currentColor",
  "stroke-width": 1.2,
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
} as const;

/** After a tab's title: this tab is in front in another window. */
export const WindowMark = () => (
  <svg
    class="window-mark"
    width="12"
    height="12"
    viewBox="0 0 12 12"
    role="img"
    aria-label="aberta em outra janela"
  >
    <title>Aberta em outra janela</title>
    <path d="M4.2 1.9h5.3a.9.9 0 0 1 .9.9v4.4" {...stroke} />
    <rect x="1.5" y="3.9" width="7.3" height="6" rx="1.3" {...stroke} />
    <path d="M1.5 5.9h7.3" {...stroke} />
  </svg>
);
