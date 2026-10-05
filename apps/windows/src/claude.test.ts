// The cases of tools/claude-activity-test and tools/tab-colours-test, made up
// screens in Claude Code's and Codex's real shape, against the TypeScript port.
import { describe, expect, it } from "vitest";
import {
  ATTENTION_INK,
  BREATH_PERIOD,
  BREATHES_FOR,
  activityColor,
  attentionFill,
  attentionGlow,
  attentionTitleInk,
  breathColor,
  breathPhase,
  declaredMode,
  isAtWork,
  lastBreath,
  modeColor,
  plainTitle,
  programOf,
  busyMark,
  readActivity,
  readCodexActivity,
  selectionEdge,
  type ClaudeActivity,
  type ClaudeMode,
  type Rgb,
} from "./claude";

const rule = "─".repeat(60);

/** A screen whose turn has ended: its last status line, the prompt box, the footer under it. */
function screen(status: string, footer: string, prompt = "❯ ", above: string[] = []): string {
  return ["⏺ Pronto, terminei a parte que dava para fazer agora.", "", ...above, status, "", rule, prompt, rule, "  " + footer].join(
    "\n",
  );
}

const mode = "⏵⏵ bypass permissions on";
const tail = "← for agents · ↓ to manage";

describe("Claude Code's turn", () => {
  for (const count of [
    "5 shells, 1 monitor",
    "1 monitor",
    "2 monitors",
    "1 shell",
    "5 shells",
    "3 background tasks",
    "1 background task",
    "1 local agent",
    "2 local agents",
    "1 background dynamic workflow",
    "1 MCP task",
    "2 shells, 3 monitors",
  ]) {
    it(`“· ${count} ·” under the box: waiting, workflow's colour`, () => {
      const s = screen(`✻ Cooked for 5m 25s · done 12:06 AM · ${count} still running`, `${mode} · ${count} · ${tail}`);
      expect(readActivity(s)).toBe("waitingForWorkflow");
    });
  }

  for (const count of [
    "1 Artifact comment monitor",
    "2 Artifact comment monitors",
    "2 teams",
    "✻ 1 cloud session",
    "dreaming",
    "auto-mode scan",
  ]) {
    it(`“· ${count} ·”: the turn is over`, () => {
      expect(readActivity(screen("✻ Baked for 23s", `${mode} · ${count} · ${tail}`))).toBe("done");
    });
  }

  it("“… 1 monitor still running” on the last line alone: over", () => {
    const stale = screen("✻ Cooked for 5m 25s · done 12:06 AM · 5 shells, 1 monitor still running", `${mode} · ${tail}`);
    expect(readActivity(stale)).toBe("done");
  });

  it("some other count under the box: over", () => {
    expect(readActivity(screen("✻ Baked for 2s", `${mode} · 2 files changed · ${tail}`))).toBe("done");
  });

  it("esc to interrupt: working", () => {
    expect(readActivity(screen("✶ Roosting… (16m)", `${mode} · esc to interrupt · ${tail}`))).toBe("working");
  });

  it("a dynamic workflow listed running: waiting", () => {
    expect(readActivity(screen("✻ Waiting for 1 dynamic workflow to finish", `${mode}\n  ◯ review ▰▰▱ 1/3`))).toBe(
      "waitingForWorkflow",
    );
  });

  it("“Waiting for” with nothing listed: over", () => {
    expect(readActivity(screen("✻ Waiting for 1 dynamic workflow to finish", mode))).toBe("done");
  });

  it("a plain ended turn: over", () => {
    expect(readActivity(screen("✻ Baked for 23s", mode))).toBe("done");
  });

  it("a dialog: waiting for you", () => {
    expect(readActivity("Deseja continuar?\n❯ 1. Sim\n  2. Não\n\nEsc to cancel")).toBe("waitingForYou");
  });

  it("a plan's approval without Esc to cancel: waiting for you by the pointer", () => {
    expect(readActivity("Plano pronto.\n❯ 1. Yes, auto-accept edits\n  2. No")).toBe("waitingForYou");
  });

  it("an agent's conversation says nothing of the main turn", () => {
    expect(readActivity(screen("✻ Baked for 1s", mode, "❯ Message @general-purpose…"))).toBeUndefined();
  });

  it("not Claude Code at all: nothing to say", () => {
    expect(readActivity("PS C:\\Users\\x> dir\n\n    Directory: C:\\Users\\x")).toBeUndefined();
  });
});

describe("Claude Code's mode", () => {
  const box = (footer: string) => ["algo", rule, "❯ ", rule, "  " + footer].join("\n");
  const cases: [string, ClaudeMode | null][] = [
    ["⏸ plan mode on (shift+tab to cycle)", "plan"],
    ["⏵⏵ accept edits on (shift+tab to cycle)", "acceptEdits"],
    ["⏵⏵ bypass permissions on (shift+tab to cycle)", "bypass"],
    ["⏵⏵ don't ask on (shift+tab to cycle)", "bypass"],
    ["⏵⏵ auto mode on", "auto"],
    ["? for shortcuts", null],
  ];
  for (const [footer, expected] of cases) {
    it(`“${footer}”`, () => expect(declaredMode(box(footer))).toBe(expected));
  }
  it("a dialog in front says nothing", () => {
    expect(declaredMode("Deseja continuar?\n" + rule + "\nPergunta\n" + rule)).toBeUndefined();
  });
  it("no footer under the box is manual", () => {
    expect(declaredMode(["x", rule, "❯ ", rule].join("\n"))).toBeNull();
  });
});

describe("Codex's turn", () => {
  for (const status of ["Working", "• Working (12s • esc to interrupt)", "Working…", "Workflow"]) {
    it(`Codex final ${status}: workflow's colour`, () => {
      expect(readCodexActivity(`Previous message\n${status}\n\n» \n100% context left`)).toBe("waitingForWorkflow");
    });
  }
  it("Codex old Working does not colour a later completed message", () => {
    expect(readCodexActivity("• Working (12s)\nDone with the task.\n» \n100% context left")).toBe("done");
  });
  it("a sentence beginning Working is not the running status", () => {
    expect(readCodexActivity("Working tree clean\n» \n100% context left")).toBe("done");
  });
  it("Codex redraw without prompt keeps known state", () => {
    expect(readCodexActivity("Working")).toBeUndefined();
  });
  it("Codex's UI tip below Working keeps the workflow colour", () => {
    expect(
      readCodexActivity(
        "Working (12s • esc to interrupt)\n  └ Tip: Use /theme to choose a theme.\n› Ask Codex to do anything\nGPT-6-Astra max",
      ),
    ).toBe("waitingForWorkflow");
  });
  it("an old tip and Working do not override a completed reply", () => {
    expect(
      readCodexActivity("Working (12s • esc to interrupt)\n  └ Tip: Use /theme.\nDone with the task.\n› \nGPT-6-Astra max"),
    ).toBe("done");
  });

  for (const dialog of [
    "Escolha uma opção\n» 1. Continuar\n  2. Parar\nenter to submit · esc to interrupt",
    "Would you like to run this command?\n› 1. Yes\n  2. No\nenter continue · esc back",
    "Question 1/2\nDigite a resposta\nEnter to submit · Esc to cancel",
    "• Deseja continuar?\n» ",
  ]) {
    it(`Codex question waits for the user: ${JSON.stringify(dialog.slice(0, 24))}`, () => {
      expect(readCodexActivity(dialog)).toBe("waitingForYou");
    });
  }
  for (const completed of [
    "• Deseja continuar?\n• Concluído.\n» ",
    "Old dialog\n› 1. Yes\nEnter to submit\n• Cancelled.\n» ",
    "• Exemplo:\n```sh\necho ?\n```\n» ",
    "• Pronto.\n» Minha pergunta?",
  ]) {
    it(`Codex old dialog or typed question is not pending: ${JSON.stringify(completed.slice(0, 24))}`, () => {
      expect(readCodexActivity(completed)).toBe("done");
    });
  }
  it("scrolled history does not change the current activity", () => {
    expect(readCodexActivity("Jump to bottom\nOld question\n› 1. Yes\nenter continue")).toBeUndefined();
  });

  const queuedHeading = "• Messages to be submitted after next tool call (press esc to interrupt and send immediately)";
  for (const queued of [
    "  ↳ Minha pergunta?",
    "  ↳ Primeira pergunta?\n  ↳ Outra pergunta\n    em duas linhas?",
    "  ↳ Uma pergunta com lista:\n    • Este item está correto?",
  ]) {
    it(`queued user questions keep Working blue: ${JSON.stringify(queued.slice(0, 20))}`, () => {
      expect(
        readCodexActivity(`• Resposta anterior.\nWorking (12s • esc to interrupt)\n${queuedHeading}\n${queued}\n› \nGPT-6`),
      ).toBe("waitingForWorkflow");
    });
  }
  it("queue without a visible status still means working", () => {
    expect(readCodexActivity(`${queuedHeading}\n  ↳ Minha pergunta?\n› `)).toBe("working");
  });
  for (const sent of [
    "› Minha pergunta?",
    "› Minha pergunta\n  em duas linhas?",
    "› 1. Minha pergunta numerada?",
    "› Confira esta lista:\n  • Este item está correto?",
  ]) {
    it(`a newer user message clears the previous assistant question: ${JSON.stringify(sent.slice(0, 20))}`, () => {
      expect(readCodexActivity(`• Deseja continuar?\n${sent}\n› `)).toBe("done");
    });
  }
  it("queued user marker is never an assistant question", () => {
    expect(readCodexActivity("• Resposta anterior.\n  ↳ Minha pergunta?\n› ")).not.toBe("waitingForYou");
  });
  it("a new assistant question after the user's answer still waits", () => {
    expect(readCodexActivity("• Deseja continuar?\n› Sim\n• Qual opção você prefere?\n› ")).toBe("waitingForYou");
  });
  it("an old queue does not override a later assistant answer", () => {
    expect(readCodexActivity(`${queuedHeading}\n  ↳ Minha pergunta?\n• Concluído.\n› `)).toBe("done");
  });
});

describe("what runs in a tab", () => {
  it("the daemon's name wins", () => {
    expect(programOf("claude", "")).toBe("claude");
    expect(programOf("codex", "whatever")).toBe("codex");
    expect(programOf("pwsh", "PS C:\\>")).toBe("pwsh");
  });
  it("a version number behind Claude Code's marks is Claude Code", () => {
    expect(programOf("2.1.289", "✳ Revisar o PR")).toBe("claude");
    expect(programOf("2.1.289", "algo")).toBe("2.1.289");
  });
  it("node running Claude Code is Claude Code", () => {
    expect(programOf("node", "✳ Claude Code")).toBe("claude");
    expect(programOf("node", "Claude Code")).toBe("claude");
    expect(programOf("node", "server")).toBe("node");
    // A shell named for a tab whose Claude Code has a child in front.
    expect(programOf("pwsh.exe", "✳ Revisar o PR")).toBe("claude");
    expect(programOf("zsh", "◐ Revisar o PR")).toBe("claude");
    expect(programOf("pwsh", "PS C:\\>")).toBe("pwsh");
  });
  it("no name but the marks: a guess", () => {
    expect(programOf("", "◐ Trabalhando")).toBe("claude");
    expect(programOf("", "bash")).toBe("");
  });
  it("titles lose the spinner marks", () => {
    expect(plainTitle("✳ Revisar o PR", "aba 1")).toBe("Revisar o PR");
    expect(plainTitle("◐◑ ", "aba 1")).toBe("aba 1");
    expect(plainTitle("vim", "aba 1")).toBe("vim");
  });
  it("at work while a turn runs or waits on handed-off work", () => {
    expect(isAtWork("working", false)).toBe(true);
    expect(isAtWork("waitingForWorkflow", false)).toBe(true);
    expect(isAtWork("done", true)).toBe(false);
    expect(isAtWork("waitingForYou", true)).toBe(false);
    expect(isAtWork(undefined, true)).toBe(true);
  });
});

// --------------------------------------------------------------- colours

function linear(c: number): number {
  c /= 255;
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}
function luminance(c: Rgb): number {
  return 0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2]);
}
/** WCAG's contrast ratio, 1 to 21. */
function contrast(a: Rgb, b: Rgb): number {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
/** Distance in OKLab: 0 is the same colour, 0.02 about the least an eye tells apart. */
function distance(a: Rgb, b: Rgb): number {
  const lab = (c: Rgb) => {
    const [r, g, bl] = [linear(c[0]), linear(c[1]), linear(c[2])];
    const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * bl);
    const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * bl);
    const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * bl);
    return [
      0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
      1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
      0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ];
  };
  const p = lab(a);
  const q = lab(b);
  return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]);
}

const grounds: { dark: boolean; name: string; colour: Rgb }[] = [
  { dark: true, name: "dark terminal", colour: [0x28, 0x2c, 0x34] },
  { dark: true, name: "dark sidebar", colour: [29, 32, 37] },
  { dark: false, name: "light terminal", colour: [255, 255, 255] },
  { dark: false, name: "light sidebar", colour: [244, 244, 244] },
];

describe("the colours a tab is titled in", () => {
  for (const dark of [true, false]) {
    const theme = dark ? "dark" : "light";
    it(`${theme}: selection changes the waiting title's tone`, () => {
      expect(distance(attentionTitleInk(true), attentionTitleInk(false))).toBeGreaterThanOrEqual(0.04);
    });
    it(`${theme}: selection outline is visible over the orange badge`, () => {
      expect(contrast(selectionEdge(dark), attentionFill(dark))).toBeGreaterThanOrEqual(2);
    });
    it(`${theme}: unselected waiting title remains readable`, () => {
      expect(contrast(attentionTitleInk(false), attentionFill(dark))).toBeGreaterThanOrEqual(4.5);
    });
    const modes: ClaudeMode[] = ["plan", "acceptEdits", "bypass", "auto"];
    const inks: [string, Rgb][] = modes.map((m) => [m, modeColor(m, dark)]);
    for (const activity of ["waitingForWorkflow", "done"] as ClaudeActivity[]) {
      inks.push([activity, activityColor(activity, dark)!]);
    }
    for (const ground of grounds.filter((g) => g.dark === dark)) {
      for (const [name, ink] of inks) {
        it(`${theme}: ${name} reads on the ${ground.name}`, () => {
          expect(contrast(ink, ground.colour)).toBeGreaterThanOrEqual(3);
        });
      }
      it(`${theme}: the badge stands out from the ${ground.name}`, () => {
        expect(contrast(attentionFill(dark), ground.colour)).toBeGreaterThanOrEqual(2.5);
      });
    }
    inks.forEach(([a, inkA], i) => {
      for (const [b, inkB] of inks.slice(i + 1)) {
        it(`${theme}: ${a} and ${b} are told apart`, () => {
          expect(distance(inkA, inkB)).toBeGreaterThanOrEqual(0.08);
        });
      }
    });
    for (const [end, colour] of [
      ["fill", attentionFill(dark)],
      ["glow", attentionGlow(dark)],
    ] as [string, Rgb][]) {
      it(`${theme}: the badge's ink reads at the ${end} of a breath`, () => {
        expect(contrast(ATTENTION_INK, colour)).toBeGreaterThanOrEqual(4.5);
      });
    }
    it(`${theme}: a turn waiting on you is titled in the badge's ink`, () => {
      expect(activityColor("waitingForYou", dark)).toEqual(ATTENTION_INK);
    });
    it(`${theme}: a running turn keeps the mode's colour`, () => {
      expect(activityColor("working", dark)).toBeNull();
    });
  }

  it("a breath starts on the fill and reaches the glow", () => {
    expect(breathColor(true, 0)).toEqual(attentionFill(true));
    expect(breathColor(true, 1)).toEqual(attentionGlow(true));
  });
  it("every badge is at the fill on the clock's beat, at the glow halfway", () => {
    const onBeat = 1000 * BREATH_PERIOD;
    expect(breathPhase(onBeat)).toBeLessThan(1e-9);
    expect(Math.abs(breathPhase(onBeat + BREATH_PERIOD / 2) - 1)).toBeLessThan(1e-9);
  });
  it("breathing stops within a breath of its minute, on the fill", () => {
    const since = 812_345.678;
    const last = lastBreath(since)!;
    expect(last - since).toBeGreaterThanOrEqual(BREATHES_FOR);
    expect(last - since).toBeLessThan(BREATHES_FOR + BREATH_PERIOD);
    expect(breathPhase(last)).toBeLessThan(1e-6);
  });
  it("a tab not waiting does not breathe", () => {
    expect(lastBreath(undefined)).toBeUndefined();
  });
});

describe("busyMark", () => {
  it("gives Claude Code, Codex and any other command each a mark of their own", () => {
    expect(busyMark("claude")).toEqual({ glyph: "✳", color: "var(--mark-claude)" });
    expect(busyMark("codex")).toEqual({ glyph: "◆", color: "var(--mark-codex)" });
    expect(busyMark("npm")).toEqual({ glyph: "✳", color: "var(--dot-busy)" });
  });
});
