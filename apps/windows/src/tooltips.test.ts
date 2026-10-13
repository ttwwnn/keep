// The workspace header's tooltip: what runs, how many tabs, and the real name
// of one that was renamed.
import { describe, expect, it } from "vitest";
import { workspaceState, workspaceTooltip } from "./tooltips";

describe("the state of a workspace", () => {
  it("is empty without tabs, running over attached over idle", () => {
    expect(workspaceState([])).toBe("empty");
    expect(workspaceState([{ busy: false, clients: 0 }])).toBe("idle");
    expect(workspaceState([{ busy: false, clients: 1 }])).toBe("attached");
    expect(workspaceState([{ busy: false, clients: 1 }, { busy: true, clients: 0 }])).toBe("running");
  });
});

describe("the tooltip of a workspace", () => {
  const base = { name: "clinica", label: "clinica", tabs: 3, state: "attached" as const, running: [] };
  it("counts the tabs and says the state when nothing runs", () => {
    expect(workspaceTooltip(base)).toBe("3 abas · anexado");
    expect(workspaceTooltip({ ...base, tabs: 1, state: "idle" })).toBe("1 aba · ocioso");
    expect(workspaceTooltip({ ...base, tabs: 0, state: "empty" })).toBe("0 abas · vazio");
  });
  it("lists what runs, one to a line", () => {
    expect(workspaceTooltip({ ...base, running: ["claude"] })).toBe("1 rodando\n· claude");
    expect(workspaceTooltip({ ...base, running: ["claude", "npm run dev"] })).toBe("2 rodando\n· claude\n· npm run dev");
  });
  it("leads with the name it answers to when it was renamed", () => {
    expect(workspaceTooltip({ ...base, label: "Clínica" })).toBe("clinica\n3 abas · anexado");
    expect(workspaceTooltip({ ...base, label: "Clínica", running: ["claude"] })).toBe("clinica\n1 rodando\n· claude");
  });
});
