// The folder a workspace's AI is held to, on its header, between the name and
// the +: open, a click chooses one; shut, a menu changes it or lets the AI go.
// The same two choices are in the header's context menu.

import { CONFINEMENT_ENFORCED, LABELS, confineLook } from "../confinement";
import { chooseFolder, confinements, release } from "../confinementStore";
import { workspaceLabel } from "../store";
import { openMenuAt, type MenuItem } from "./common";

const stroke = {
  fill: "none",
  stroke: "currentColor",
  "stroke-width": 1.2,
  "stroke-linecap": "round",
  "stroke-linejoin": "round",
} as const;

/** The outline of a folder, the glyph of a workspace whose AI goes where it likes. */
const FOLDER = "M1.5 3.6a1 1 0 0 1 1-1h2.3a1 1 0 0 1 .8.4l.6.8h3.3a1 1 0 0 1 1 1v4.2a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1z";

const Folder = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d={FOLDER} {...stroke} />
  </svg>
);

/** The same folder, filled in: the AI is held to one. */
const FolderFilled = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d={FOLDER} {...stroke} fill="currentColor" />
  </svg>
);

/** The menu of a workspace whose AI is held to `folder`. */
function heldMenu(name: string, label: string, folder: string): (MenuItem | "separator")[] {
  return [
    { label: confineLook(label, folder).info ?? "", disabled: true, action: () => {} },
    "separator",
    { label: LABELS.change, action: () => void chooseFolder(name, label) },
    { label: LABELS.release, action: () => void release(name, label) },
  ];
}

/** The folder button of a workspace's header. */
export function ConfineButton(props: { name: string }) {
  if (!CONFINEMENT_ENFORCED) return null;
  const { name } = props;
  const label = workspaceLabel(name);
  const folder = confinements.value[name] ?? null;
  const look = confineLook(label, folder);
  return (
    <button
      class={`hdr-btn confine ${look.held ? "held" : ""}`}
      title={look.title}
      aria-label={look.title.split("\n")[0]}
      onClick={(e) => {
        e.stopPropagation();
        if (folder === null) {
          void chooseFolder(name, label);
          return;
        }
        const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
        openMenuAt(box.left, box.bottom + 4, heldMenu(name, label, folder));
      }}
    >
      {look.held ? <FolderFilled /> : <Folder />}
    </button>
  );
}

/** The header's context menu entries for the AI's folder: after a separator, ready to join the menu. */
export function confineContextItems(name: string): (MenuItem | "separator")[] {
  if (!CONFINEMENT_ENFORCED) return [];
  const label = workspaceLabel(name);
  const folder = confinements.value[name] ?? null;
  const items: (MenuItem | "separator")[] = [
    "separator",
    { label: LABELS.restrict, action: () => void chooseFolder(name, label) },
  ];
  if (folder !== null) items.push({ label: LABELS.releaseToday(folder), action: () => void release(name, label) });
  return items;
}
