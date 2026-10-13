// The pickers of Claude Code and Codex: reading them off the screens they
// drew (Claude Code 2.1.296 and Codex 0.162.1, 12/10 — the same ones the
// macOS tests read), and walking them by keys against made-up programs that
// draw them the same way.
import { describe, expect, it } from "vitest";
import {
  CLAUDE_PICKER,
  chooseClaudeEffort,
  chooseClaudeModel,
  chooseCodex,
  claudeAsksToSwitch,
  claudeEffortSteps,
  claudeModelItems,
  claudeModelSteps,
  claudePickerLabel,
  codexChoiceNumber,
  codexLevelLabel,
  CODEX_PICKER,
  type CodexModel,
  type PickerIO,
} from "./pickers";

const ESC = "\x1b";
const UP = "\x1b[A";
const DOWN = "\x1b[B";
const RIGHT = "\x1b[C";
const LEFT = "\x1b[D";

// ---------------------------------------------------------------- the screens as they were

const CLAUDE_MODELS_SCREEN = [
  "   Select model",
  "   Switch between Claude models. Your pick becomes the default for new sessions. For other/previous model names,",
  "   specify with --model.",
  "     1.  Default (recommended)  Fable 5.1",
  "     2.  Opus 5.5               For complex work and everyday tasks",
  "     3.  Fable 5.1              For your toughest challenges",
  "   ❯ 4.  Sonnet 5.5 ✔           Most efficient for simpler tasks",
  "     5.  Haiku 5.5              Fastest for quick answers",
  "     6.  Haiku 4.5              Fastest for quick answers",
  "   ↓ 10. Opus 4.8               Best for everyday, complex tasks",
  "   ○ Low effort ←/→ to adjust",
  "   Enter to set as default · s to use this session only · Esc to cancel",
].join("\n");

const CLAUDE_EFFORT_SCREEN = [
  "   Effort",
  "                             Faster                             Smarter",
  "                             ─▲────────────────────────────────────────      Ultracode  off",
  "                             low     medium     high     xhigh      max      Tab to toggle",
  "   ←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel",
].join("\n");

describe("Claude Code's /model list (2.1.296)", () => {
  it("reads the items and the pointed one", () => {
    const items = claudeModelItems(CLAUDE_MODELS_SCREEN);
    expect(items.map((i) => i.label)).toEqual([
      "Default (recommended)",
      "Opus 5.5",
      "Fable 5.1",
      "Sonnet 5.5",
      "Haiku 5.5",
      "Haiku 4.5",
      "Opus 4.8",
    ]);
    expect(items.find((i) => i.pointed)?.label).toBe("Sonnet 5.5");
  });

  it("counts the arrows from the pointer", () => {
    expect(claudeModelSteps("Opus 5.5", CLAUDE_MODELS_SCREEN)).toBe(-2);
    expect(claudeModelSteps("Haiku 5.5", CLAUDE_MODELS_SCREEN)).toBe(1);
    expect(claudeModelSteps("Sonnet 5.5", CLAUDE_MODELS_SCREEN)).toBe(0);
    // Its own item, not the default's.
    expect(claudeModelSteps("Fable 5.1", CLAUDE_MODELS_SCREEN)).toBe(-1);
  });

  it("nothing to count without the item or the pointer", () => {
    expect(claudeModelSteps("Opus 5.5 (1M)", CLAUDE_MODELS_SCREEN)).toBeNull();
    expect(claudeModelSteps("Opus 5.5", CLAUDE_MODELS_SCREEN.replace("❯", " "))).toBeNull();
    expect(claudeModelSteps("Opus 5.5", "")).toBeNull();
  });

  it("a numbered line of the conversation above the list is not an item", () => {
    const screen = ["❯ 1. Opus 5.5 is the one I want", "● ok", CLAUDE_MODELS_SCREEN].join("\n");
    expect(claudeModelSteps("Opus 5.5", screen)).toBe(-2);
  });

  it("the items the bar's models are", () => {
    expect(claudePickerLabel("claude-fable-5-1")).toBe("Fable 5.1");
    expect(claudePickerLabel("claude-opus-5-5")).toBe("Opus 5.5");
    expect(claudePickerLabel("claude-sonnet-5-5")).toBe("Sonnet 5.5");
    expect(claudePickerLabel("claude-haiku-5-5")).toBe("Haiku 5.5");
    // The 1M-context Opus has none: its "Opus 5.5" is the 200k one.
    expect(claudePickerLabel("claude-opus-5-5[1m]")).toBeNull();
  });
});

describe("Claude Code's /effort rule (2.1.296)", () => {
  it("on low: four right to max, none to low", () => {
    expect(claudeEffortSteps("max", CLAUDE_EFFORT_SCREEN)).toBe(4);
    expect(claudeEffortSteps("low", CLAUDE_EFFORT_SCREEN)).toBe(0);
  });

  it("on high: one left to medium, two right to max", () => {
    const onHigh = CLAUDE_EFFORT_SCREEN.replace("─▲──────────────────", "───────────────────▲");
    expect(claudeEffortSteps("medium", onHigh)).toBe(-1);
    expect(claudeEffortSteps("max", onHigh)).toBe(2);
  });

  it("nothing to count without the marker or the level", () => {
    expect(claudeEffortSteps("ultra", CLAUDE_EFFORT_SCREEN)).toBeNull();
    expect(claudeEffortSteps("low", CLAUDE_EFFORT_SCREEN.replace("▲", "─"))).toBeNull();
    expect(claudeEffortSteps("low", "▲")).toBeNull();
  });

  it("the switch question", () => {
    expect(claudeAsksToSwitch("Switch model?\n  the full history gets re-read\n ❯ 1. Yes, switch to Sonnet 5.5\n   2. No")).toBe(true);
    expect(claudeAsksToSwitch("Switch model?\n   1. Yes, switch to Sonnet 5.5\n ❯ 2. No")).toBe(false);
    expect(claudeAsksToSwitch(CLAUDE_MODELS_SCREEN)).toBe(false);
  });
});

// Codex 0.162.1's pickers, as tools/codex-bar-test.sh's stand-in draws them.
const CODEX_MODELS_SCREEN = [
  "  >_ OpenAI Codex (v0.162.1)",
  "     ~/x",
  "",
  "  Select Model and Effort",
  "",
  "  1. GPT-6.1-Sol",
  "› 2. GPT-6-Astra (current)",
  "  3. GPT-6-Sol",
  "",
  "  enter select · esc back",
].join("\n");

const CODEX_LEVELS_SCREEN = [
  "  Select Reasoning Level for GPT-6-Astra",
  "",
  "  1. Low",
  "  2. Medium",
  "  3. High",
  "  4. Extra high",
  "  5. More reasoning…",
  "",
  "  enter select · esc back",
].join("\n");

const CODEX_ADVANCED_SCREEN = [
  "  Advanced Reasoning",
  "  ⚠ Consumes usage limits faster",
  "",
  "  1. Max",
  "  2. Ultra",
  "",
  "  enter default · s session · esc back",
].join("\n");

describe("Codex's /model picker (0.162.1)", () => {
  it("finds a model's number by its name, the pointer and a note after it", () => {
    expect(codexChoiceNumber("GPT-6-Astra", CODEX_MODELS_SCREEN, CODEX_PICKER.modelsTitle)).toBe(2);
    expect(codexChoiceNumber("GPT-6.1-Sol", CODEX_MODELS_SCREEN, CODEX_PICKER.modelsTitle)).toBe(1);
    // GPT-6-Sol is not GPT-6.1-Sol.
    expect(codexChoiceNumber("GPT-6-Sol", CODEX_MODELS_SCREEN, CODEX_PICKER.modelsTitle)).toBe(3);
    expect(codexChoiceNumber("gpt-6-sol", CODEX_MODELS_SCREEN, CODEX_PICKER.modelsTitle)).toBe(3);
    expect(codexChoiceNumber("GPT-6", CODEX_MODELS_SCREEN, CODEX_PICKER.modelsTitle)).toBeNull();
  });

  it("a level's number: High is not Extra high, and More reasoning ends in its ellipsis", () => {
    expect(codexChoiceNumber("High", CODEX_LEVELS_SCREEN, CODEX_PICKER.levelsTitle)).toBe(3);
    expect(codexChoiceNumber("Extra high", CODEX_LEVELS_SCREEN, CODEX_PICKER.levelsTitle)).toBe(4);
    expect(codexChoiceNumber("More reasoning", CODEX_LEVELS_SCREEN, CODEX_PICKER.levelsTitle)).toBe(5);
    expect(codexChoiceNumber("Max", CODEX_LEVELS_SCREEN, CODEX_PICKER.levelsTitle)).toBeNull();
    expect(codexChoiceNumber("Ultra", CODEX_ADVANCED_SCREEN, CODEX_PICKER.advancedTitle)).toBe(2);
  });

  it("only under its title: a numbered line of the conversation above is not a choice", () => {
    const screen = ["• 1. High hopes for this one", CODEX_LEVELS_SCREEN].join("\n");
    expect(codexChoiceNumber("High", screen, CODEX_PICKER.levelsTitle)).toBe(3);
  });

  it("how its levels are called", () => {
    expect(["low", "medium", "high", "xhigh", "max", "ultra"].map(codexLevelLabel)).toEqual([
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Max",
      "Ultra",
    ]);
  });
});

// ---------------------------------------------------------------- walking them

/** A program in a pane: what a key does to it, and its screen. */
interface Program {
  press(key: string): void;
  draw(): string;
}

/**
 * A pane over `program`: a key reaches it `lag` looks later (it takes a
 * moment to draw), every key is written down, and waiting takes no time.
 * `run` walks a picker and lets the program have every key sent before the
 * test looks at it.
 */
function pane(program: Program, lag = 0) {
  const sent: string[] = [];
  const said: string[] = [];
  const queue: { key: string; due: number }[] = [];
  let looks = 0;
  const io: PickerIO = {
    key: async (key) => {
      sent.push(key);
      queue.push({ key, due: looks + lag });
    },
    screen: async () => {
      looks++;
      while (queue.length > 0 && queue[0].due < looks) program.press(queue.shift()!.key);
      return program.draw();
    },
    sleep: async () => {},
    log: (line) => void said.push(line),
  };
  const run = async (walk: (io: PickerIO) => Promise<string | null>) => {
    const result = await walk(io);
    for (const { key } of queue.splice(0)) program.press(key);
    return result;
  };
  return { run, sent, said };
}

/** Bytes a program reads, keys at a time: an arrow's three bytes are one key. */
function keysOf(bytes: string): string[] {
  const keys: string[] = [];
  for (let i = 0; i < bytes.length; ) {
    const arrow = /^\x1b\[[ABCD]/.exec(bytes.slice(i));
    const key = arrow ? arrow[0] : bytes[i];
    keys.push(key);
    i += key.length;
  }
  return keys;
}

const RULE = "─".repeat(60);
const LEVELS = ["low", "medium", "high", "xhigh", "max"];
const MODELS = ["Default (recommended)", "Opus 5.5", "Fable 5.1", "Sonnet 5.5", "Haiku 5.5"];

/** Claude Code with its two pickers, as tools/status-bar-test.sh's stand-in has them. */
class FakeClaude implements Program {
  screen: "box" | "models" | "effort" | "switch" = "box";
  typed = "";
  pointer = 1;
  marker: number;
  model = "Opus 5.5 (1M)";
  /** Arrows it lets go by, as one pressed the moment a picker opens. */
  drops = 0;
  /** Whether it asks before switching models: a conversation with a cache. */
  asks = false;
  /** Whether `/model` opens anything. */
  opens = true;

  constructor(public effort = "xhigh") {
    this.marker = LEVELS.indexOf(effort);
  }

  press(bytes: string): void {
    for (const key of keysOf(bytes)) this.one(key);
  }

  private one(key: string): void {
    if (key.startsWith("\x1b[")) {
      if (this.drops > 0) {
        this.drops--;
        return;
      }
      if (this.screen === "models") this.pointer = Math.max(0, Math.min(MODELS.length - 1, this.pointer + (key === DOWN ? 1 : key === UP ? -1 : 0)));
      if (this.screen === "effort") this.marker = Math.max(0, Math.min(4, this.marker + (key === RIGHT ? 1 : key === LEFT ? -1 : 0)));
      return;
    }
    if (key === ESC) {
      this.screen = "box";
      this.typed = "";
    } else if (this.screen === "box") {
      if (key !== "\r") this.typed += key;
      else {
        if (this.typed === "/model" && this.opens) this.screen = "models";
        else if (this.typed === "/effort" && this.opens) this.screen = "effort";
        this.typed = "";
      }
    } else if (this.screen === "models" && key === "s" && this.pointer > 0) {
      if (this.asks) this.screen = "switch";
      else this.switchModel();
    } else if (this.screen === "switch" && key === "\r") {
      this.switchModel();
    } else if (this.screen === "effort" && key === "s") {
      this.effort = LEVELS[this.marker];
      this.screen = "box";
    }
  }

  private switchModel(): void {
    this.model = MODELS[this.pointer];
    this.screen = "box";
  }

  draw(): string {
    switch (this.screen) {
      case "models":
        return [
          "   Select model",
          ...MODELS.map((label, i) => `${i === this.pointer ? "   ❯ " : "     "}${i + 1}.  ${label}`),
          "   Enter to set as default · s to use this session only · Esc to cancel",
        ].join("\n");
      case "effort": {
        const columns = [29, 37, 48, 57, 68];
        const line = [...Array(29).fill(" "), ...Array(42).fill("─")];
        line[columns[this.marker] + 1] = "▲";
        return [
          "   Effort",
          line.join(""),
          `${" ".repeat(29)}low     medium     high     xhigh      max`,
          "   ←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel",
        ].join("\n");
      }
      case "switch":
        return ["   Switch model?", "   the full history gets re-read", ` ❯ 1. Yes, switch to ${MODELS[this.pointer]}`, "   2. No"].join("\n");
      default:
        return ["", RULE, `❯ ${this.typed}`, RULE, "  ⏵⏵ bypass permissions on"].join("\n");
    }
  }
}

describe("walking Claude Code's /effort rule", () => {
  it("one right to max, then s: this session only", async () => {
    const claude = new FakeClaude("xhigh");
    const { run, sent, said } = pane(claude);
    expect(await run((io) => chooseClaudeEffort(io, "max"))).toBeNull();
    expect(sent).toEqual(["/effort", "\r", RIGHT, "s"]);
    expect(claude.effort).toBe("max");
    expect(claude.screen).toBe("box");
    expect(said).toEqual(["picker: effort max, this session only"]);
  });

  it("left as far as it takes", async () => {
    const claude = new FakeClaude("xhigh");
    const { run, sent } = pane(claude, 2);
    expect(await run((io) => chooseClaudeEffort(io, "low"))).toBeNull();
    expect(sent).toEqual(["/effort", "\r", LEFT, LEFT, LEFT, "s"]);
    expect(claude.effort).toBe("low");
  });

  it("already there: only s", async () => {
    const claude = new FakeClaude("high");
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeEffort(io, "high"))).toBeNull();
    expect(sent).toEqual(["/effort", "\r", "s"]);
  });

  it("a key lost as the rule opens is pressed again, not counted", async () => {
    const claude = new FakeClaude("low");
    claude.drops = 1;
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeEffort(io, "medium"))).toBeNull();
    expect(sent).toEqual(["/effort", "\r", RIGHT, RIGHT, "s"]);
    expect(claude.effort).toBe("medium");
  });

  it("no rule: backs out with Esc", async () => {
    const claude = new FakeClaude("low");
    claude.opens = false;
    const { run, sent, said } = pane(claude);
    expect(await run((io) => chooseClaudeEffort(io, "max"))).toBe("no effort picker");
    expect(sent).toEqual(["/effort", "\r", ESC]);
    expect(said).toEqual(["picker: no effort picker; backing out"]);
  });

  it("a level the rule does not have: backs out", async () => {
    const claude = new FakeClaude("low");
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeEffort(io, "ultra"))).toBe("ultra not on the rule");
    expect(sent).toEqual(["/effort", "\r", ESC]);
    expect(claude.screen).toBe("box");
    expect(claude.effort).toBe("low");
  });
});

describe("walking Claude Code's /model list", () => {
  it("down to Sonnet 5.5, then s: this session only", async () => {
    const claude = new FakeClaude();
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeModel(io, "Sonnet 5.5"))).toBeNull();
    expect(sent).toEqual(["/model", "\r", DOWN, DOWN, "s"]);
    expect(claude.model).toBe("Sonnet 5.5");
  });

  it("up to an item above the pointer", async () => {
    const claude = new FakeClaude();
    claude.pointer = 4;
    const { run, sent } = pane(claude, 1);
    expect(await run((io) => chooseClaudeModel(io, "Opus 5.5"))).toBeNull();
    expect(sent).toEqual(["/model", "\r", UP, UP, UP, "s"]);
    expect(claude.model).toBe("Opus 5.5");
  });

  it("the question of a conversation with a cache is answered yes", async () => {
    const claude = new FakeClaude();
    claude.asks = true;
    const { run, sent, said } = pane(claude);
    expect(await run((io) => chooseClaudeModel(io, "Haiku 5.5"))).toBeNull();
    expect(sent).toEqual(["/model", "\r", DOWN, DOWN, DOWN, "s", "\r"]);
    expect(claude.model).toBe("Haiku 5.5");
    expect(said).toEqual(["picker: model Haiku 5.5, this session only (switch confirmed)"]);
  });

  it("the question is waited for when it takes a moment to come", async () => {
    const claude = new FakeClaude();
    claude.asks = true;
    const { run, sent } = pane(claude, 3);
    expect(await run((io) => chooseClaudeModel(io, "Fable 5.1"))).toBeNull();
    expect(sent).toEqual(["/model", "\r", DOWN, "s", "\r"]);
    expect(claude.model).toBe("Fable 5.1");
  });

  it("keys lost as the list opens are pressed again", async () => {
    const claude = new FakeClaude();
    claude.drops = 2;
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeModel(io, "Sonnet 5.5"))).toBeNull();
    expect(sent.filter((k) => k === DOWN)).toHaveLength(4);
    expect(claude.model).toBe("Sonnet 5.5");
  });

  it("an item the list does not have: backs out, nothing chosen", async () => {
    const claude = new FakeClaude();
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeModel(io, "Opus 5.5 (1M)"))).toBe("Opus 5.5 (1M) not in the picker");
    expect(sent).toEqual(["/model", "\r", ESC]);
    expect(claude.model).toBe("Opus 5.5 (1M)");
    expect(claude.screen).toBe("box");
  });

  it("no list: backs out", async () => {
    const claude = new FakeClaude();
    claude.opens = false;
    const { run, sent } = pane(claude);
    expect(await run((io) => chooseClaudeModel(io, "Sonnet 5.5"))).toBe("no model picker");
    expect(sent).toEqual(["/model", "\r", ESC]);
  });

  it("a frame half drawn is not read: the list is read once it holds still", async () => {
    const claude = new FakeClaude();
    let halves = 1;
    const { run, sent } = pane({
      press: (k) => claude.press(k),
      draw: () => {
        const full = claude.draw();
        // The first look at the open list catches its title and its foot only.
        if (claude.screen === "models" && halves-- > 0) {
          return full
            .split("\n")
            .filter((l) => !/\d\./.test(l))
            .join("\n");
        }
        return full;
      },
    });
    expect(await run((io) => chooseClaudeModel(io, "Haiku 5.5"))).toBeNull();
    expect(sent).toEqual(["/model", "\r", DOWN, DOWN, DOWN, "s"]);
    expect(claude.model).toBe("Haiku 5.5");
  });

  it(`reads the list by its foot (${CLAUDE_PICKER.modelSessionOnly})`, () => {
    expect(new FakeClaude().draw()).not.toContain(CLAUDE_PICKER.modelSessionOnly);
  });
});

const CODEX_MODELS = ["GPT-6.1-Sol", "GPT-6-Astra", "GPT-6-Sol"];
const CODEX_LEVELS = ["low", "medium", "high", "xhigh"];

/** Codex 0.162.1 with its picker, as tools/codex-bar-test.sh's stand-in has it. */
class FakeCodex implements Program {
  screen: "idle" | "models" | "levels" | "advanced" = "idle";
  typed = "";
  picked = "";
  model = "GPT-6-Astra";
  effort = "max";

  press(bytes: string): void {
    for (const ch of bytes) {
      if (ch === ESC) {
        this.screen = "idle";
        this.typed = "";
      } else if (this.screen === "idle") {
        if (ch !== "\r") this.typed += ch;
        else {
          if (this.typed === "/model") this.screen = "models";
          this.typed = "";
        }
      } else if (this.screen === "models" && /[1-3]/.test(ch)) {
        this.picked = CODEX_MODELS[Number(ch) - 1];
        this.screen = "levels";
      } else if (this.screen === "levels" && /[1-4]/.test(ch)) {
        this.change(CODEX_LEVELS[Number(ch) - 1]);
      } else if (this.screen === "levels" && ch === "5") {
        this.screen = "advanced";
      } else if (this.screen === "advanced" && /[12]/.test(ch)) {
        this.change(ch === "1" ? "max" : "ultra");
      }
    }
  }

  private change(effort: string): void {
    this.model = this.picked;
    this.effort = effort;
    this.screen = "idle";
  }

  draw(): string {
    const head = ["  >_ OpenAI Codex (v0.162.1)", "     ~/x", ""];
    switch (this.screen) {
      case "models":
        return [
          ...head,
          "  Select Model and Effort",
          "",
          ...CODEX_MODELS.map((m, i) => `${m === this.model ? "› " : "  "}${i + 1}. ${m}${m === this.model ? " (current)" : ""}`),
          "",
          "  enter select · esc back",
        ].join("\n");
      case "levels":
        return [...head, CODEX_LEVELS_SCREEN.replace("GPT-6-Astra", this.picked)].join("\n");
      case "advanced":
        return [...head, CODEX_ADVANCED_SCREEN].join("\n");
      default:
        return [...head, `› ${this.typed || "Ask Codex to do anything"}`, "", `  ${this.model} ${this.effort} · ~/x`].join("\n");
    }
  }
}

const model = (name: string, slug = name.toLowerCase()): CodexModel => ({
  slug,
  name,
  efforts: ["low", "medium", "high", "xhigh", "max", "ultra"],
  defaultEffort: "medium",
});

describe("walking Codex's /model picker", () => {
  it("the effort: the model's number, then the level's", async () => {
    const codex = new FakeCodex();
    const { run, sent, said } = pane(codex);
    expect(await run((io) => chooseCodex(io, model("GPT-6-Astra"), "high"))).toBeNull();
    expect(sent.join("")).toBe("/model\r23");
    expect([codex.model, codex.effort]).toEqual(["GPT-6-Astra", "high"]);
    expect(said).toEqual(["picker: gpt-6-astra high"]);
  });

  it("the model, with the effort kept", async () => {
    const codex = new FakeCodex();
    codex.effort = "high";
    const { run, sent } = pane(codex, 2);
    expect(await run((io) => chooseCodex(io, model("GPT-6-Sol"), "high"))).toBeNull();
    expect(sent.join("")).toBe("/model\r33");
    expect([codex.model, codex.effort]).toEqual(["GPT-6-Sol", "high"]);
  });

  it("Ultra and Max by way of More reasoning", async () => {
    const codex = new FakeCodex();
    const { run, sent } = pane(codex);
    expect(await run((io) => chooseCodex(io, model("GPT-6-Sol"), "ultra"))).toBeNull();
    expect(sent.join("")).toBe("/model\r352");
    expect([codex.model, codex.effort]).toEqual(["GPT-6-Sol", "ultra"]);
    sent.length = 0;
    expect(await run((io) => chooseCodex(io, model("GPT-6-Sol"), "max"))).toBeNull();
    expect(sent.join("")).toBe("/model\r351");
    expect(codex.effort).toBe("max");
  });

  it("Extra high is Extra high, not High", async () => {
    const codex = new FakeCodex();
    const { run, sent } = pane(codex);
    expect(await run((io) => chooseCodex(io, model("GPT-6.1-Sol"), "xhigh"))).toBeNull();
    expect(sent.join("")).toBe("/model\r14");
    expect([codex.model, codex.effort]).toEqual(["GPT-6.1-Sol", "xhigh"]);
  });

  it("a model the list does not have: three Esc, nothing chosen", async () => {
    const codex = new FakeCodex();
    const { run, sent, said } = pane(codex);
    expect(await run((io) => chooseCodex(io, model("GPT-9"), "high"))).toBe("GPT-9 not in the list");
    expect(sent).toEqual(["/model", "\r", ESC, ESC, ESC]);
    expect([codex.model, codex.effort, codex.screen]).toEqual(["GPT-6-Astra", "max", "idle"]);
    expect(said).toEqual(["picker: GPT-9 not in the list; backing out"]);
  });

  it("a level nowhere in the picker: backs out", async () => {
    const codex = new FakeCodex();
    const { run, sent } = pane(codex);
    expect(await run((io) => chooseCodex(io, model("GPT-6-Astra"), "minimal"))).toBe("Minimal not under More reasoning");
    expect(sent.join("")).toBe(`/model\r25${ESC}${ESC}${ESC}`);
    expect(codex.effort).toBe("max");
  });

  it("no picker: backs out", async () => {
    const quiet: Program = { press: () => {}, draw: () => "› " };
    const { run, sent } = pane(quiet);
    expect(await run((io) => chooseCodex(io, model("GPT-6-Astra"), "high"))).toBe("no model list");
    expect(sent).toEqual(["/model", "\r", ESC, ESC, ESC]);
  });
});
