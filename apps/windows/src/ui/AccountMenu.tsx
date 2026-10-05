// The chevron beside a tab's title that opens the menu of its AI and account,
// in the strip and in the sidebar, and the footer's "+" that opens a login.
// The same menus wherever they open; what they offer is decided in ia.ts and
// done in iaStore.ts.

import type { Engine } from "../ia";
import { accountMenu, iaAvailable, jev, signIn } from "../iaStore";
import type { RootTab } from "../layout";
import { openMenuAt } from "./common";
import * as icon from "./icons";

/** Hanging from the glyph, its edge a little left of it: a pull-down. */
function under(anchor: HTMLElement): { x: number; y: number } {
  const rect = anchor.getBoundingClientRect();
  return { x: rect.left - 4, y: rect.bottom + 3 };
}

/** The menu of a tab's AI and account, built from what is known as it opens. */
export function openAccountMenu(anchor: HTMLElement, workspace: string, tab: RootTab, tabName: string): void {
  const { rows, choose } = accountMenu(workspace, tab, tabName);
  const at = under(anchor);
  openMenuAt(
    at.x,
    at.y,
    rows.map((row) =>
      row.kind === "separator"
        ? "separator"
        : {
            label: row.title,
            mark: row.mark,
            help: row.help ?? undefined,
            disabled: !row.enabled || row.key === null,
            action: () => choose(row),
          },
    ),
    true,
  );
}

/**
 * The footer's "+": a login to another account of either service and, apart
 * from them, the Jev's connection to OpenRouter — "Reconectar" once it has a
 * key, which a new connection replaces.
 */
export function openSignInMenu(anchor: HTMLElement): void {
  const at = under(anchor);
  const entry = (engine: Engine | "openrouter", label: string) => ({ label, action: () => void signIn(engine) });
  openMenuAt(at.x, at.y, [
    entry("claude", "Entrar em outra conta do Claude…"),
    entry("codex", "Entrar em outra conta do GPT…"),
    "separator",
    entry("openrouter", jev.value ? "Reconectar o Jev ao OpenRouter…" : "Conectar o Jev ao OpenRouter…"),
  ]);
}

/**
 * The chevron itself: quiet at rest, there on the chosen tab and under the
 * pointer (styles.css). A press on it opens the menu and is nothing else —
 * not a click on the tab, not the start of a drag of the window.
 */
export function AccountChevron(props: { workspace: string; tab: RootTab; label: string }) {
  if (!iaAvailable.value) return null;
  return (
    <button
      class="ai-chevron"
      title="IA e conta desta aba"
      aria-label={`Escolher a IA da aba ${props.label}`}
      aria-haspopup="menu"
      data-ia-ws={props.workspace}
      data-ia-tab={props.tab.root.id}
      onMouseDown={(e) => e.stopPropagation()}
      onDblClick={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        openAccountMenu(e.currentTarget as HTMLElement, props.workspace, props.tab, props.label);
      }}
    >
      <icon.ChevronDown />
    </button>
  );
}
