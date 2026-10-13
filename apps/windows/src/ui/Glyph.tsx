// The picker's glyphs, drawn as lines so they take the colour of the text
// around them and stay sharp at any zoom (the macOS app uses SF Symbols for the
// same). One 14-point box, one stroke.

import type { VNode } from "preact";
import type { GlyphName } from "../palette/glyphs";

const line = {
  fill: "none",
  stroke: "currentColor",
  "stroke-width": 1.2,
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
} as const;

const A = "M1.8 11L5 3.2 8.2 11M2.9 8.6h4.2";
const FOLDER = "M1.8 4.2a1 1 0 0 1 1-1h2.6l1.2 1.4h4.6a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H2.8a1 1 0 0 1-1-1z";
const frame = () => <rect x="1.5" y="2.5" width="11" height="9" rx="2" {...line} />;

const SHAPES: Record<GlyphName, VNode> = {
  terminal: (
    <>
      {frame()}
      <path d="M4.3 6.2l2 1.4-2 1.4M7.8 9h2.2" {...line} />
    </>
  ),
  // A terminal at work is filled, as the Mac's is: the one thing in a row that is taken in without reading.
  terminalBusy: (
    <>
      <rect x="1.5" y="2.5" width="11" height="9" rx="2" {...line} fill="currentColor" />
      <path d="M4.3 6.2l2 1.4-2 1.4M7.8 9h2.2" {...line} stroke="var(--ground)" />
    </>
  ),
  folder: <path d={FOLDER} {...line} />,
  folderPlus: (
    <>
      <path d={FOLDER} {...line} />
      <path d="M7 6.6v3M5.5 8.1h3" {...line} />
    </>
  ),
  go: <path d="M3 3v3.2a1.3 1.3 0 0 0 1.3 1.3H11M8.7 5.2l2.3 2.3-2.3 2.3" {...line} />,
  newTab: (
    <>
      <rect x="1.5" y="3" width="11" height="8.5" rx="2" {...line} />
      <path d="M7 5.6v3.3M5.35 7.25h3.3" {...line} />
    </>
  ),
  copy: (
    <>
      <rect x="4.5" y="4.5" width="7" height="7" rx="1.5" {...line} />
      <path d="M9.5 4.5v-.6a1.3 1.3 0 0 0-1.3-1.3H3.8a1.3 1.3 0 0 0-1.3 1.3v4.4a1.3 1.3 0 0 0 1.3 1.3h.7" {...line} />
    </>
  ),
  close: <path d="M3.5 3.5l7 7M10.5 3.5l-7 7" {...line} />,
  sidebar: (
    <>
      {frame()}
      <path d="M5.5 2.5v9" {...line} />
    </>
  ),
  verticalTabs: (
    <>
      {frame()}
      <path d="M4.3 5.2h.01M4.3 7h.01M4.3 8.8h.01M6.2 5.2h4M6.2 7h4M6.2 8.8h4" {...line} />
    </>
  ),
  splitRight: (
    <>
      {frame()}
      <path d="M7 2.5v9" {...line} />
    </>
  ),
  splitDown: (
    <>
      {frame()}
      <path d="M1.5 7h11" {...line} />
    </>
  ),
  closePane: (
    <>
      {frame()}
      <path d="M5.2 5.2l3.6 3.6M8.8 5.2l-3.6 3.6" {...line} />
    </>
  ),
  newWindow: (
    <>
      {frame()}
      <path d="M1.5 5.2h11M7 6.8v3M5.5 8.3h3" {...line} />
    </>
  ),
  hide: (
    <>
      <path d="M1.5 7s2-3.5 5.5-3.5S12.5 7 12.5 7s-2 3.5-5.5 3.5S1.5 7 1.5 7z" {...line} />
      <circle cx="7" cy="7" r="1.5" {...line} />
      <path d="M2.5 11.5l9-9" {...line} />
    </>
  ),
  kill: (
    <>
      <path d="M7.9 1.8L4.2 7.4h2.6l-.7 4.8 3.7-5.6H7.2z" {...line} />
      <path d="M2.2 2.2l9.6 9.6" {...line} />
    </>
  ),
  settings: (
    <>
      <circle cx="7" cy="7" r="2" {...line} />
      <path d="M7 1.5v1.6M7 10.9v1.6M1.5 7h1.6M10.9 7h1.6M3.1 3.1l1.1 1.1M9.8 9.8l1.1 1.1M3.1 10.9l1.1-1.1M9.8 4.2l1.1-1.1" {...line} />
    </>
  ),
  fullscreen: (
    <path d="M2 5V2.5a.5.5 0 0 1 .5-.5H5M12 5V2.5a.5.5 0 0 0-.5-.5H9M2 9v2.5a.5.5 0 0 0 .5.5H5M12 9v2.5a.5.5 0 0 1-.5.5H9" {...line} />
  ),
  image: (
    <>
      <rect x="1.5" y="2.5" width="11" height="9" rx="1.5" {...line} />
      <circle cx="5" cy="5.6" r=".9" {...line} />
      <path d="M1.8 10.2l3.2-3 2.4 2.3 1.8-1.7 3 2.9" {...line} />
    </>
  ),
  theme: (
    <>
      <path d="M7 1.8a5.2 5.2 0 1 0 0 10.4c.9 0 1.2-.6.9-1.2-.3-.6-.1-1.4.7-1.4h1.1A2.4 2.4 0 0 0 12.2 7 5.2 5.2 0 0 0 7 1.8z" {...line} />
      <path d="M4.6 6.4h.01M6.5 4.4h.01M9.2 5h.01" {...line} />
    </>
  ),
  font: <path d="M2.5 3.5h9M7 3.5v7.5M5.5 11h3" {...line} />,
  fontBigger: (
    <>
      <path d={A} {...line} />
      <path d="M11 4.4v3.2M9.4 6h3.2" {...line} />
    </>
  ),
  fontSmaller: (
    <>
      <path d={A} {...line} />
      <path d="M9.4 6h3.2" {...line} />
    </>
  ),
  reset: <path d="M3.2 7a3.9 3.9 0 1 0 1.2-2.8M3.2 2.6v2.6h2.6" {...line} />,
  undo: <path d="M4.2 5.3H9a2.6 2.6 0 0 1 0 5.2H5.5M6.2 3.2L4 5.3l2.2 2.1" {...line} />,
  // ⌘, the Commands button's: a square, and a loop at each corner.
  command: (
    <path
      d="M5.2 5.2h3.6v3.6H5.2zM5.2 5.2V3.9a1.3 1.3 0 1 0-1.3 1.3h1.3M8.8 5.2V3.9a1.3 1.3 0 1 1 1.3 1.3H8.8M5.2 8.8v1.3a1.3 1.3 0 1 1-1.3-1.3h1.3M8.8 8.8v1.3a1.3 1.3 0 1 0 1.3-1.3H8.8"
      {...line}
    />
  ),
};

export function Glyph(props: { name: GlyphName; size?: number }) {
  const size = props.size ?? 14;
  return (
    <svg class="glyph" width={size} height={size} viewBox="0 0 14 14" aria-hidden="true">
      {SHAPES[props.name]}
    </svg>
  );
}
