// The folder a workspace's AI is held to, on its header, between the name and
// the +: open, a click chooses one; shut, a menu changes it or lets the AI go.
// The same two choices are in the header's context menu.

import { CONFINEMENT_ENFORCED, confineLook, contextChoices, heldChoices, type Choice } from "../confinement";
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

/**
 * The same folder, filled in: the AI is held to one. Drawn as a shape without
 * an outline, at the outline's outer edge: the ink at rest is translucent, and
 * a fill under a stroke of the same ink would show the stroke brighter.
 */
const FOLDER_SOLID =
  "M.75 3.6A1.75 1.75 0 0 1 2.5 1.85h2.3a1.25 1.25 0 0 1 1 .55l.6.8h3.1a1.75 1.75 0 0 1 1.75 1.75V9a1.75 1.75 0 0 1-1.75 1.75h-7A1.75 1.75 0 0 1 .75 9z";

const FolderFilled = () => (
  <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
    <path d={FOLDER_SOLID} fill="currentColor" stroke="none" />
  </svg>
);

/** The window's action for each choice a menu offers. */
function menuOf(name: string, label: string, choices: Choice[]): (MenuItem | "separator")[] {
  const run = { info: () => {}, choose: () => void chooseFolder(name, label), release: () => void release(name, label) };
  return choices.map((c) =>
    c === "separator" ? c : { label: c.label, disabled: c.disabled, help: c.help, action: run[c.id] },
  );
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
        openMenuAt(box.left, box.bottom + 4, menuOf(name, label, heldChoices(folder)));
      }}
    >
      {look.held ? <FolderFilled /> : <Folder />}
    </button>
  );
}

/** The header's context menu entries for the AI's folder: after a separator, ready to join the menu. */
export function confineContextItems(name: string): (MenuItem | "separator")[] {
  if (!CONFINEMENT_ENFORCED) return [];
  return menuOf(name, workspaceLabel(name), contextChoices(confinements.value[name] ?? null));
}
