// The few glyphs the chrome draws, as lines rather than a font: they take
// the colour of the text around them and stay sharp at any zoom.

const stroke = {
  fill: "none",
  stroke: "currentColor",
  "stroke-width": 1.2,
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
} as const;

export const Plus = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d="M6 2v8M2 6h8" {...stroke} />
  </svg>
);

export const Minus = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d="M2 6h8" {...stroke} />
  </svg>
);

export const Close = ({ size = 10 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 10 10" aria-hidden="true">
    <path d="M2 2l6 6M8 2l-6 6" {...stroke} />
  </svg>
);

export const Chevron = ({ open }: { open: boolean }) => (
  <svg
    width="10"
    height="10"
    viewBox="0 0 10 10"
    aria-hidden="true"
    style={{ transform: open ? "rotate(90deg)" : "none", transition: "transform 150ms ease-out" }}
  >
    <path d="M3.5 2l3 3-3 3" {...stroke} />
  </svg>
);

export const Sidebar = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
    <rect x="1.5" y="2.5" width="11" height="9" rx="2" {...stroke} />
    <path d="M5.5 2.5v9" {...stroke} />
  </svg>
);

export const Gear = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
    <circle cx="7" cy="7" r="2" {...stroke} />
    <path
      d="M7 1.5v1.6M7 10.9v1.6M1.5 7h1.6M10.9 7h1.6M3.1 3.1l1.1 1.1M9.8 9.8l1.1 1.1M3.1 10.9l1.1-1.1M9.8 4.2l1.1-1.1"
      {...stroke}
    />
  </svg>
);

export const Hand = () => (
  <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
    <path
      d="M4 6.5V2.8a.8.8 0 0 1 1.6 0V6M5.6 5.5V2a.8.8 0 0 1 1.6 0v3.5M7.2 5.5V2.6a.8.8 0 0 1 1.6 0v4.6M4 6.3l-.8-1a.8.8 0 0 0-1.3.9l1.6 2.6c.7 1.2 1.7 1.9 3 1.9h.5c1.4 0 2.6-1.2 2.6-2.6V5.3"
      fill="currentColor"
      stroke="currentColor"
      stroke-width="0.6"
      stroke-linejoin="round"
    />
  </svg>
);

/** Windows' own window buttons, drawn to its proportions. */
export const WinMinimize = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <path d="M0 5h10" stroke="currentColor" stroke-width="1" />
  </svg>
);

export const WinMaximize = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <rect x="0.5" y="0.5" width="9" height="9" rx="1" fill="none" stroke="currentColor" stroke-width="1" />
  </svg>
);

export const WinRestore = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <rect x="0.5" y="2.5" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="1" />
    <path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" fill="none" stroke="currentColor" stroke-width="1" />
  </svg>
);

export const WinClose = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" stroke-width="1" />
  </svg>
);

export const Up = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d="M3 7.5l3-3 3 3" {...stroke} />
  </svg>
);

export const Down = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d="M3 4.5l3 3 3-3" {...stroke} />
  </svg>
);
