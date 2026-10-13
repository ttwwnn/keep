// The bar under a conversation: what the status file and the screens say, and
// what a click may type. A port of the macOS rules (ClaudeStatus.swift) plus
// the Codex reading.
import { describe, expect, it } from "vitest";
import {
  CANNOT_TYPE_NOTE,
  CLAUDE_EFFORTS,
  CODEX_EFFORTS,
  PICKING_NOTE,
  agentsLabel,
  claudePromptIsEmpty,
  claudeStatus,
  codexEffortFor,
  codexModelNamed,
  codexPromptIsEmpty,
  codexStatus,
  countAgents,
  effortChoices,
  effortLabel,
  menuNote,
  modelChoices,
  parseCodexModels,
  parseStatusFile,
  readCodexModel,
  sameStatus,
  shortModelName,
  type BarAction,
  type Choice,
  type CodexModel,
} from "./aiStatus";

const RULE = "─".repeat(40);

/** A Claude Code screen: conversation, the prompt box with `prompt` in it, then the footer lines. */
const claudeScreen = (prompt: string, ...footer: string[]) =>
  ["● Pronto.", "", RULE, prompt, RULE, ...footer].join("\n");

describe("the status file", () => {
  const file = (model: object, effort?: object) => JSON.stringify({ session_id: "s", model, ...(effort ? { effort } : {}) });

  it("reads the model and the effort the status line was given", () => {
    const got = parseStatusFile(
      file({ id: "claude-opus-5-5[1m]", display_name: "Opus 5.5 (1M context)" }, { level: "xhigh" }),
    );
    expect(got).toEqual({ modelId: "claude-opus-5-5[1m]", modelName: "Opus 5.5 (1M)", effort: "xhigh" });
  });

  it("a model without an effort setting has none", () => {
    expect(parseStatusFile(file({ id: "claude-haiku-5-5", display_name: "Haiku 5.5" }))?.effort).toBeNull();
    expect(parseStatusFile(file({ id: "m", display_name: "M" }, { level: "" }))?.effort).toBeNull();
    expect(parseStatusFile(file({ id: "m", display_name: "M" }, { level: 3 }))?.effort).toBeNull();
  });

  it("the id stands in for a missing name, and the name for a missing id", () => {
    expect(parseStatusFile(file({ id: "claude-x" }))).toEqual({ modelId: "claude-x", modelName: "claude-x", effort: null });
    expect(parseStatusFile(file({ display_name: "X" }))).toEqual({ modelId: "", modelName: "X", effort: null });
  });

  it("says nothing for what is not a model", () => {
    expect(parseStatusFile("")).toBeNull();
    expect(parseStatusFile("{\"model\":")).toBeNull();
    expect(parseStatusFile("[]")).toBeNull();
    expect(parseStatusFile("null")).toBeNull();
    expect(parseStatusFile("{}")).toBeNull();
    expect(parseStatusFile(file({}))).toBeNull();
    expect(parseStatusFile(JSON.stringify({ model: "opus" }))).toBeNull();
  });

  it("a byte order mark, as PowerShell 5 writes, does not hide it", () => {
    expect(parseStatusFile("\uFEFF" + file({ id: "a", display_name: "A" }))?.modelId).toBe("a");
  });

  it("shortens the context in the name", () => {
    expect(shortModelName("Opus 5.5 (1M context)")).toBe("Opus 5.5 (1M)");
    expect(shortModelName("Sonnet 5.5")).toBe("Sonnet 5.5");
  });
});

describe("agents on the Claude Code screen", () => {
  it("counts the lines it lists under the prompt", () => {
    const screen = claudeScreen("❯", "  ◯ general-purpose  Auditar", "  ◯ Explore  Mapear", "  ⏵⏵ accept edits on");
    expect(countAgents(screen)).toBe(2);
  });

  it("counts them from the folded line", () => {
    expect(countAgents(claudeScreen("❯", "  3 local agents · ↓ to manage"))).toBe(3);
    expect(countAgents(claudeScreen("❯", "  1 local agent"))).toBe(1);
    expect(countAgents(claudeScreen("❯", "  2 shells · 4 local agents"))).toBe(4);
  });

  it("the largest count of the folded lines", () => {
    expect(countAgents(claudeScreen("❯", "  1 local agent", "  3 local agents", "  2 local agents"))).toBe(3);
  });

  it("the larger of the two when both appear", () => {
    expect(countAgents(claudeScreen("❯", "  ◯ a", "  2 local agents"))).toBe(2);
    expect(countAgents(claudeScreen("❯", "  ◯ a", "  ◯ b", "  ◯ c", "  2 local agents"))).toBe(3);
  });

  it("none under a quiet prompt, nor above the box", () => {
    expect(countAgents(claudeScreen("❯", "  ⏵⏵ accept edits on"))).toBe(0);
    expect(countAgents(["  ◯ above the box", RULE, "❯", RULE, "  ? for shortcuts"].join("\n"))).toBe(0);
  });

  it("none where there is no prompt box", () => {
    expect(countAgents("PS C:\\> ")).toBe(0);
    expect(countAgents("")).toBe(0);
  });

  it("the last box is the one that counts", () => {
    const screen = [RULE, "❯", RULE, "  ◯ old", "● more", RULE, "❯", RULE, "  ? for shortcuts"].join("\n");
    expect(countAgents(screen)).toBe(0);
  });
});

describe("Claude Code's prompt", () => {
  it("is empty with only the pointer", () => {
    expect(claudePromptIsEmpty(claudeScreen("❯"))).toBe(true);
    expect(claudePromptIsEmpty(claudeScreen("  ❯  "))).toBe(true);
    expect(claudePromptIsEmpty(claudeScreen(">"))).toBe(true);
  });

  it("is not with a draft in it", () => {
    expect(claudePromptIsEmpty(claudeScreen("❯ git status"))).toBe(false);
  });

  it("is not where there is no box, or a dialog is in front", () => {
    expect(claudePromptIsEmpty("PS C:\\> ")).toBe(false);
    expect(claudePromptIsEmpty([RULE, "❯ 1. Yes", "  2. No", RULE].join("\n"))).toBe(false);
    expect(claudePromptIsEmpty(RULE)).toBe(false);
  });
});

describe("the status of a Claude Code pane", () => {
  const file = { modelId: "claude-opus-5-5", modelName: "Opus 5.5", effort: "high" };

  it("is none until the status line has said", () => {
    expect(claudeStatus(null, claudeScreen("❯"), "done")).toBeNull();
  });

  it("takes the model from the file and the agents from the screen", () => {
    const status = claudeStatus(file, claudeScreen("❯", "  ◯ one"), "done");
    expect(status).toEqual({
      program: "claude",
      modelId: "claude-opus-5-5",
      modelName: "Opus 5.5",
      effort: "high",
      agents: 1,
      canType: true,
    });
  });

  it("can be typed into only with the turn over and the prompt empty", () => {
    expect(claudeStatus(file, claudeScreen("❯"), "done")?.canType).toBe(true);
    expect(claudeStatus(file, claudeScreen("❯"), "working")?.canType).toBe(false);
    expect(claudeStatus(file, claudeScreen("❯"), "waitingForYou")?.canType).toBe(false);
    expect(claudeStatus(file, claudeScreen("❯"), "waitingForWorkflow")?.canType).toBe(false);
    expect(claudeStatus(file, claudeScreen("❯"), undefined)?.canType).toBe(false);
    expect(claudeStatus(file, claudeScreen("❯ half a thought"), "done")?.canType).toBe(false);
  });
});

describe("Codex's model and effort", () => {
  const screen = (prompt: string, ...under: string[]) => ["• Pronto.", "", prompt, "", ...under].join("\n");

  it("from the footer under the prompt", () => {
    expect(readCodexModel(screen("› ", "  gpt-6.1-sol high · 100% context left"))).toEqual({
      model: "gpt-6.1-sol",
      effort: "high",
    });
    expect(readCodexModel(screen("›", "gpt-5.1-codex-max xhigh · 80% left · ~/proj"))).toEqual({
      model: "gpt-5.1-codex-max",
      effort: "xhigh",
    });
    expect(readCodexModel(screen("›", "  gpt-5.2-codex medium fast · 70% context left"))).toEqual({
      model: "gpt-5.2-codex",
      effort: "medium",
    });
  });

  it("a footer with nothing after the effort", () => {
    expect(readCodexModel(screen("›", "gpt-5-codex high"))).toEqual({ model: "gpt-5-codex", effort: "high" });
  });

  it("the footer's effort is lowercased, and an effortless model has none", () => {
    expect(readCodexModel(screen("›", "gpt-5-codex HIGH · 9% context left"))?.effort).toBe("high");
    expect(readCodexModel(screen("›", "gpt-4.1 · 100% context left"))).toEqual({ model: "gpt-4.1", effort: null });
  });

  it("not from the shortcuts line, nor from words that are not a model", () => {
    expect(readCodexModel(screen("›", "  ? for shortcuts                      100% context left"))).toBeNull();
    expect(readCodexModel(screen("›", "  Context high · 100% left"))).toBeNull();
    expect(readCodexModel(screen("›", "  ctrl+c to quit"))).toBeNull();
  });

  it("the footer wins over the header, which may be out of date", () => {
    const text = [
      "╭──────────────────────────────────────────╮",
      "│ >_ OpenAI Codex (v0.50.0)                │",
      "│ model:     gpt-5-codex low   /model to change │",
      "╰──────────────────────────────────────────╯",
      "› ",
      "  gpt-6.1-sol high · 100% context left",
    ].join("\n");
    expect(readCodexModel(text)).toEqual({ model: "gpt-6.1-sol", effort: "high" });
  });

  it("from the header box when the footer does not carry it", () => {
    const text = [
      "╭──────────────────────────────────────────╮",
      "│ >_ OpenAI Codex (v0.50.0)                │",
      "│                                          │",
      "│ model:     gpt-5-codex high   /model to change │",
      "│ directory: ~/proj                        │",
      "╰──────────────────────────────────────────╯",
      "",
      "› Explain this codebase",
      "",
      "  ? for shortcuts                100% context left",
    ].join("\n");
    expect(readCodexModel(text)).toEqual({ model: "gpt-5-codex", effort: "high" });
  });

  it("the last change in the conversation beats the header", () => {
    const text = [
      "│ model:     gpt-5-codex high   /model to change │",
      "› /model",
      "• Model changed to gpt-5.1-codex-max medium",
      "",
      "› ",
      "  100% context left",
    ].join("\n");
    expect(readCodexModel(text)).toEqual({ model: "gpt-5.1-codex-max", effort: "medium" });
    expect(readCodexModel("• Model changed to o3 with high\n› ")).toEqual({ model: "o3", effort: "high" });
    expect(readCodexModel("• Model changed to o3\n› ")).toEqual({ model: "o3", effort: null });
  });

  it("a user's own words in the conversation are not the model", () => {
    expect(readCodexModel("› model: gpt-9-x high\n› ")).toBeNull();
    expect(readCodexModel("PS C:\\> ")).toBeNull();
    expect(readCodexModel("")).toBeNull();
  });
});

describe("Codex's prompt", () => {
  it("is empty when bare or showing its hint", () => {
    expect(codexPromptIsEmpty("• ok\n›\n  100% context left")).toBe(true);
    expect(codexPromptIsEmpty("› Ask Codex to do anything")).toBe(true);
    expect(codexPromptIsEmpty("› Explain this codebase")).toBe(true);
    expect(codexPromptIsEmpty("› Find and fix a bug in @filename")).toBe(true);
    expect(codexPromptIsEmpty("» Use /skills to list available skills")).toBe(true);
  });

  it("is not with a draft, or where there is no prompt, or a selector in front", () => {
    expect(codexPromptIsEmpty("› git status")).toBe(false);
    expect(codexPromptIsEmpty("› Explain this codebase to me")).toBe(false);
    expect(codexPromptIsEmpty("PS C:\\> ")).toBe(false);
    expect(codexPromptIsEmpty("› 1. gpt-5.1-codex (current)\n  2. gpt-5.1-codex-max")).toBe(false);
  });

  it("the last prompt is the one that counts", () => {
    expect(codexPromptIsEmpty("› old draft\n• reply\n›")).toBe(true);
    expect(codexPromptIsEmpty("›\n• reply\n› new draft")).toBe(false);
  });
});

describe("the status of a Codex pane", () => {
  const screen = "• Pronto.\n\n›\n\n  gpt-6.1-sol high · 100% context left";

  it("carries no agent count", () => {
    expect(codexStatus(screen, "done")).toEqual({
      program: "codex",
      modelId: "gpt-6.1-sol",
      modelName: "gpt-6.1-sol",
      effort: "high",
      agents: null,
      canType: true,
    });
  });

  it("is typed into with the turn over and the prompt empty only", () => {
    expect(codexStatus(screen, "working")?.canType).toBe(false);
    expect(codexStatus(screen, "waitingForYou")?.canType).toBe(false);
    expect(codexStatus(screen, undefined)?.canType).toBe(false);
    expect(codexStatus(screen.replace("›", "› git status"), "done")?.canType).toBe(false);
  });

  it("keeps what was read when the screen no longer says it", () => {
    const kept = { model: "gpt-5-codex", effort: "low" };
    expect(codexStatus("• long reply\n›", "done", kept)?.modelId).toBe("gpt-5-codex");
    expect(codexStatus(screen, "done", kept)?.modelId).toBe("gpt-6.1-sol");
    expect(codexStatus("• long reply\n›", "done")).toBeNull();
  });
});

/** What each choice does, in a form a test can compare. */
const actions = (choices: Choice[]) => choices.map((c) => c.action);
const marked = (choices: Choice[]) => choices.filter((c) => c.current).map((c) => c.label);

/** Codex's cache as Codex 0.162.1 keeps it, cut to what is read (12/10). */
const CODEX_CACHE = JSON.stringify({
  fetched_at: "2026-10-12T23:46:00Z",
  client_version: "0.162.1",
  models: [
    {
      slug: "gpt-6-sol",
      display_name: "GPT-6-Sol",
      visibility: "list",
      priority: 3,
      default_reasoning_level: "medium",
      supported_reasoning_levels: [{ effort: "low" }, { effort: "medium" }, { effort: "high" }, { effort: "xhigh" }, { effort: "max" }, { effort: "ultra" }],
    },
    {
      slug: "gpt-reserve",
      display_name: "GPT-Reserve",
      visibility: "hide",
      priority: 0,
      supported_reasoning_levels: [{ effort: "low" }],
    },
    {
      slug: "gpt-6.1-sol",
      display_name: "GPT-6.1-Sol",
      visibility: "list",
      priority: 1,
      default_reasoning_level: "low",
      supported_reasoning_levels: [{ effort: "low" }, { effort: "medium" }, { effort: "high" }, { effort: "xhigh" }],
    },
    {
      slug: "gpt-6-astra",
      display_name: "GPT-6-Astra",
      visibility: "list",
      priority: 2,
      default_reasoning_level: "medium",
      supported_reasoning_levels: [{ effort: "low" }, { effort: "medium" }, { effort: "high" }, { effort: "xhigh" }, { effort: "max" }, { effort: "ultra" }],
    },
  ],
});

describe("Codex's model cache", () => {
  it("lists the models it shows, in its order, with their names and levels", () => {
    const models = parseCodexModels(CODEX_CACHE);
    expect(models.map((m) => m.slug)).toEqual(["gpt-6.1-sol", "gpt-6-astra", "gpt-6-sol"]);
    expect(models[1]).toEqual({
      slug: "gpt-6-astra",
      name: "GPT-6-Astra",
      efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
      defaultEffort: "medium",
    });
  });

  it("takes levels written as words too, and a model without a name goes by its slug", () => {
    const text = JSON.stringify({
      models: [{ slug: "o9", visibility: "list", supported_reasoning_levels: ["low", { effort: "high" }, 7, { effort: "" }] }],
    });
    expect(parseCodexModels(text)).toEqual([{ slug: "o9", name: "o9", efforts: ["low", "high"], defaultEffort: null }]);
  });

  it("models without a priority come last, in the order written", () => {
    const text = JSON.stringify({
      models: [
        { slug: "b", visibility: "list" },
        { slug: "a", visibility: "list", priority: 5 },
        { slug: "c", visibility: "list" },
      ],
    });
    expect(parseCodexModels(text).map((m) => m.slug)).toEqual(["a", "b", "c"]);
  });

  it("nothing from what is not a cache", () => {
    expect(parseCodexModels("")).toEqual([]);
    expect(parseCodexModels("{")).toEqual([]);
    expect(parseCodexModels("[]")).toEqual([]);
    expect(parseCodexModels(JSON.stringify({ models: {} }))).toEqual([]);
    expect(parseCodexModels(JSON.stringify({ models: [null, 3, { slug: 5 }, { slug: "x" }] }))).toEqual([]);
    expect(parseCodexModels("\uFEFF" + CODEX_CACHE)).toHaveLength(3);
  });

  it("a name on screen finds its model by the slug or the display name, in any case", () => {
    const models = parseCodexModels(CODEX_CACHE);
    expect(codexModelNamed("GPT-6-Astra", models)?.slug).toBe("gpt-6-astra");
    expect(codexModelNamed("gpt-6-astra", models)?.slug).toBe("gpt-6-astra");
    expect(codexModelNamed("GPT-6.1-SOL", models)?.slug).toBe("gpt-6.1-sol");
    expect(codexModelNamed("gpt-reserve", models)).toBeNull();
    expect(codexModelNamed("gpt-9", models)).toBeNull();
  });

  it("gives a Codex pane its slug and its name", () => {
    const models = parseCodexModels(CODEX_CACHE);
    const screen = "• ok\n› Ask Codex to do anything\n\n  GPT-6-Astra max · ~/x";
    expect(codexStatus(screen, "done", null, models)).toMatchObject({
      modelId: "gpt-6-astra",
      modelName: "GPT-6-Astra",
      effort: "max",
      canType: true,
    });
    // Without the cache: what the screen says, the id in lower case.
    expect(codexStatus(screen, "done")).toMatchObject({ modelId: "gpt-6-astra", modelName: "GPT-6-Astra" });
  });
});

describe("the menus", () => {
  const claude = claudeStatus(
    { modelId: "claude-opus-5-5[1m]", modelName: "Opus 5.5 (1M)", effort: "high" },
    claudeScreen("❯"),
    "done",
  )!;
  const models = parseCodexModels(CODEX_CACHE);
  const codex = codexStatus("•\n›\n  GPT-6-Astra max · ~/x", "done", null, models)!;

  it("Claude Code's efforts go through its /effort rule, the one in use marked", () => {
    const choices = effortChoices(claude);
    expect(choices.map((c) => c.label)).toEqual([...CLAUDE_EFFORTS]);
    expect(actions(choices)).toEqual(CLAUDE_EFFORTS.map((level) => ({ kind: "claudeEffort", level })));
    expect(marked(choices)).toEqual(["high"]);
  });

  it("Claude Code's models go through its /model list, but the 1M Opus by its id with the default put back", () => {
    const choices = modelChoices(claude);
    expect(choices.map((c) => c.label)).toEqual(["Fable 5.1", "Opus 5.5", "Opus 5.5 (1M)", "Sonnet 5.5", "Haiku 5.5"]);
    expect(actions(choices)).toEqual<BarAction[]>([
      { kind: "claudeModel", label: "Fable 5.1" },
      { kind: "claudeModel", label: "Opus 5.5" },
      { kind: "type", command: "/model claude-opus-5-5[1m]", keepDefaults: true },
      { kind: "claudeModel", label: "Sonnet 5.5" },
      { kind: "claudeModel", label: "Haiku 5.5" },
    ]);
    expect(marked(choices)).toEqual(["Opus 5.5 (1M)"]);
  });

  it("no effort is marked when the model has none", () => {
    const plain = { ...claude, effort: null };
    expect(effortChoices(plain).some((c) => c.current)).toBe(false);
  });

  it("Codex's efforts are the levels the model takes, each through its picker with the model kept", () => {
    const choices = effortChoices(codex, models);
    expect(choices.map((c) => c.label)).toEqual(["low", "medium", "high", "xhigh", "max", "ultra"]);
    expect(marked(choices)).toEqual(["max"]);
    const astra = codexModelNamed("gpt-6-astra", models)!;
    expect(choices[2].action).toEqual({ kind: "codex", model: astra, effort: "high" });
    const sol = codexStatus("•\n›\n  GPT-6.1-Sol low · ~/x", "done", null, models)!;
    expect(effortChoices(sol, models).map((c) => c.label)).toEqual(["low", "medium", "high", "xhigh"]);
  });

  it("Codex's efforts without the model in the cache: all of them, its name as on screen", () => {
    const unknown = codexStatus("•\n›\n  gpt-9-x high · ~/x", "done")!;
    const choices = effortChoices(unknown, models);
    expect(choices.map((c) => c.label)).toEqual([...CODEX_EFFORTS]);
    expect(choices[0].action).toEqual({
      kind: "codex",
      model: { slug: "gpt-9-x", name: "gpt-9-x", efforts: [], defaultEffort: null },
      effort: "low",
    });
  });

  it("Codex's models are the cache's, each keeping the effort when it takes it", () => {
    const choices = modelChoices(codex, models);
    expect(choices.map((c) => c.label)).toEqual(["GPT-6.1-Sol", "GPT-6-Astra", "GPT-6-Sol"]);
    expect(marked(choices)).toEqual(["GPT-6-Astra"]);
    // GPT-6.1-Sol has no max: its own default, low.
    expect(actions(choices).map((a) => (a.kind === "codex" ? `${a.model.slug} ${a.effort}` : a.kind))).toEqual([
      "gpt-6.1-sol low",
      "gpt-6-astra max",
      "gpt-6-sol max",
    ]);
  });

  it("Codex without a cache: its picker opened for the person to choose", () => {
    expect(modelChoices(codex)).toEqual([
      { label: "Escolher no seletor do Codex (/model)", current: false, action: { kind: "type", command: "/model", keepDefaults: false } },
    ]);
  });

  it("the level a model is changed to", () => {
    const model: CodexModel = { slug: "m", name: "M", efforts: ["low", "high"], defaultEffort: "low" };
    expect(codexEffortFor(model, "high")).toBe("high");
    expect(codexEffortFor(model, "max")).toBe("low");
    expect(codexEffortFor(model, null)).toBe("low");
    expect(codexEffortFor({ ...model, defaultEffort: null }, "max")).toBe("medium");
  });

  it("the note appears only when typing is off, or a pick is under way", () => {
    expect(menuNote(claude)).toBeNull();
    expect(menuNote({ ...claude, canType: false })).toBe(CANNOT_TYPE_NOTE);
    expect(CANNOT_TYPE_NOTE).toBe("Disponível com o turno terminado e o prompt vazio");
    expect(menuNote(claude, true)).toBe(PICKING_NOTE);
    expect(menuNote({ ...claude, canType: false }, true)).toBe(PICKING_NOTE);
  });

  it("words on the chips", () => {
    expect(effortLabel("max")).toBe("esforço max");
    expect(agentsLabel(0)).toBe("0 agentes");
    expect(agentsLabel(1)).toBe("1 agente");
    expect(agentsLabel(2)).toBe("2 agentes");
  });
});

describe("telling two statuses apart", () => {
  const a = claudeStatus({ modelId: "m", modelName: "M", effort: "low" }, claudeScreen("❯"), "done")!;

  it("same in every field is the same", () => {
    expect(sameStatus(a, { ...a })).toBe(true);
    expect(sameStatus(undefined, undefined)).toBe(true);
  });

  it("any field apart is a change", () => {
    expect(sameStatus(a, undefined)).toBe(false);
    expect(sameStatus(a, { ...a, effort: "high" })).toBe(false);
    expect(sameStatus(a, { ...a, agents: 2 })).toBe(false);
    expect(sameStatus(a, { ...a, canType: false })).toBe(false);
    expect(sameStatus(a, { ...a, modelId: "n" })).toBe(false);
    expect(sameStatus(a, { ...a, program: "codex" })).toBe(false);
  });
});

describe("Codex 0.162.1 as it was on screen (12/10)", () => {
  const idle = [
    "  >_ OpenAI Codex (v0.162.1)",
    "     ~/Developer/keep",
    "",
    "› Ask Codex to do anything",
    "",
    "  GPT-6-Astra max · ~/Developer/keep",
    "  ← for agents · ? for shortcuts",
  ].join("\n");
  it("reads the model and Max under the prompt", () => {
    expect(readCodexModel(idle)).toEqual({ model: "GPT-6-Astra", effort: "max" });
  });
  it("reads Ultra", () => {
    expect(readCodexModel(idle.replace("max ·", "ultra ·"))?.effort).toBe("ultra");
  });
});
