// The bar under a Claude Code or Codex conversation: the model and the effort
// it runs with — both change with a click — and how many agents it has going.
// A port of ClaudeStatusBar.swift in the macOS app.
//
// A change walks the program's own picker by keys (pickers.ts): Claude Code's
// `/model` list and `/effort` rule, for this conversation only, and Codex's
// `/model`, its models and levels from its cache. It types into the
// conversation, so the menus only turn it on when the turn is over and the
// prompt is empty (aiStatus.ts): typing into a draft, a dialog or a running
// turn would spoil it.

import {
  agentsLabel,
  effortChoices,
  effortLabel,
  menuNote,
  modelChoices,
  type AiStatus,
  type Choice,
} from "../aiStatus";
import { viewKey } from "../terminals";
import { codexModels, focusPane, picking, runChoice } from "../store";
import { openMenuAt, type MenuItem } from "./common";

/** Rows a menu takes, to hang it above the bar rather than over it. */
const ROW = 28;
const MENU_PADDING = 12;

function menuItems(
  status: AiStatus,
  choices: Choice[],
  busy: boolean,
  run: (choice: Choice) => void,
): (MenuItem | "separator")[] {
  const note = menuNote(status, busy);
  const items: (MenuItem | "separator")[] = [];
  if (note) items.push({ label: note, disabled: true, action: () => {} }, "separator");
  for (const choice of choices) {
    items.push({
      label: choice.label,
      mark: choice.current ? "on" : null,
      disabled: busy || !status.canType,
      action: () => run(choice),
    });
  }
  return items;
}

/**
 * A press on a chip leaves the keyboard where it was, in the terminal, and
 * does not reach the window's handler that closes a menu: the click that
 * follows opens the menu again.
 */
function keepFocus(event: MouseEvent): void {
  event.preventDefault();
  event.stopPropagation();
}

export function AiStatusBar(props: { workspace: string; root: number; pane: number; status: AiStatus }) {
  const { workspace, root, pane, status } = props;
  const busy = picking.value.has(viewKey(workspace, pane));
  const open = (event: MouseEvent, choices: Choice[]) => {
    event.stopPropagation();
    focusPane(workspace, root, pane);
    // While a picker is being walked, the menu says so and offers nothing.
    const items = menuItems(status, choices, busy, (choice) => void runChoice(workspace, pane, choice));
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const height = items.length * ROW + MENU_PADDING;
    openMenuAt(rect.left, Math.max(6, rect.top - height - 4), items, true);
  };
  const models = codexModels.value;
  return (
    <div class="ai-status-bar" data-program={status.program} data-bar-pane={pane} data-picking={busy ? "1" : undefined}>
      <button
        class="ai-chip"
        data-chip="model"
        title={`Modelo: ${status.modelId}. Clique para trocar.`}
        onMouseDown={keepFocus}
        onClick={(e) => open(e, modelChoices(status, models))}
      >
        {status.modelName} ▾
      </button>
      {status.effort !== null && (
        <>
          <span class="ai-dot">·</span>
          <button
            class="ai-chip"
            data-chip="effort"
            title="Esforço de raciocínio. Clique para trocar."
            onMouseDown={keepFocus}
            onClick={(e) => open(e, effortChoices(status, models))}
          >
            {effortLabel(status.effort)} ▾
          </button>
        </>
      )}
      {status.agents !== null && (
        <>
          <span class="ai-dot">·</span>
          <span class="ai-agents" data-chip="agents">
            {agentsLabel(status.agents)}
          </span>
        </>
      )}
    </div>
  );
}
