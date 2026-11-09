// "Is this a project you trust?": reading the question off the screen, and the
// keys that answer it. The screens are the ones the macOS app's tests use
// (tools/status-test, tools/trust-test.sh) and the numbered one newer versions draw.
import { describe, expect, it } from "vitest";
import { TrustAnswerer, TrustWatch, trustKey, trustMarker, trustPointer, type TrustIO, type TrustPane } from "./trust";

const OLD = ` Accessing workspace:

 /Users/ana/projetos/novo

 Quick safety check: Is this a project you created or one you trust? (Like your own code, a well-known open source project, or work from your
 team). If not, take a moment to review what's in this folder first.

 ❯ No, exit
   Yes, I trust this folder

 Enter to confirm · Esc to cancel
`;
const OLD_ON_YES = OLD.replace("❯ No, exit", "  No, exit").replace("  Yes, I trust", "❯ Yes, I trust");
const FOOTER = "\n Enter to confirm · Esc to cancel\n";
const NUMBERED_ON_YES = "Do you trust the files in this folder?\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n" + FOOTER;
const NUMBERED_ON_NO = "Do you trust the files in this folder?\n   1. Yes, I trust this folder\n ❯ 2. No, exit\n" + FOOTER;

describe("the question on the screen", () => {
  it("is read where the pointer is", () => {
    expect(trustPointer(OLD)).toEqual({ on: "no", yes: "below" });
    expect(trustPointer(OLD_ON_YES)).toEqual({ on: "yes" });
    expect(trustPointer(NUMBERED_ON_YES)).toEqual({ on: "yes" });
    expect(trustPointer(NUMBERED_ON_NO)).toEqual({ on: "no", yes: "above" });
  });

  it("is not there when the words are only quoted, or there is no pointer", () => {
    const screen = "─".repeat(20) + "\n❯ \n" + "─".repeat(20);
    expect(trustPointer("ele disse: Yes, I trust this folder e No, exit\n" + screen)).toBeUndefined();
    expect(trustPointer(screen)).toBeUndefined();
    expect(trustPointer("")).toBeUndefined();
    expect(trustPointer("Yes, I trust this folder\nNo, exit\n")).toBeUndefined();
  });

  it("is not the question when printed inside a conversation, or without its header and footer", () => {
    const rule = "─".repeat(40);
    // The same lines, over the prompt box every conversation draws.
    expect(trustPointer(`● Li o arquivo:\n${OLD}\n${rule}\n❯ \n${rule}\n  ⏵⏵ bypass permissions on\n`)).toBeUndefined();
    expect(trustPointer(" ❯ No, exit\n   Yes, I trust this folder\n")).toBeUndefined();
    const crowded = Array.from({ length: 30 }, (_, i) => `linha ${i}`).join("\n") + "\n" + OLD;
    expect(trustPointer(crowded)).toBeUndefined();
  });
});

describe("the key that answers it", () => {
  it("moves the pointer toward the yes, and presses Enter only on it", () => {
    expect(trustKey({ on: "no", yes: "below" })).toBe("\x1b[B");
    expect(trustKey({ on: "no", yes: "above" })).toBe("\x1b[A");
    expect(trustKey({ on: "yes" })).toBe("\r");
  });
});

describe("each second's answer", () => {
  const showingNo = { key: "a", pointer: { on: "no", yes: "below" } } as const;
  const showingYes = { key: "a", pointer: { on: "yes" } } as const;

  it("walks the pointer down and then confirms, never pressing Enter on the no", () => {
    const answerer = new TrustAnswerer();
    expect(answerer.step([showingNo], 0)).toEqual([{ key: "a", keys: "\x1b[B" }]);
    // Next look: drawn with the pointer moved.
    expect(answerer.step([showingYes], 1000)).toEqual([{ key: "a", keys: "\r" }]);
  });

  it("waits for a key the question has not taken yet, and sends Enter once", () => {
    const answerer = new TrustAnswerer();
    answerer.step([showingNo], 0);
    // Still on the no a moment later: not again.
    expect(answerer.step([showingNo], 1000)).toEqual([]);
    // A long moment later it is.
    expect(answerer.step([showingNo], 1600)).toEqual([{ key: "a", keys: "\x1b[B" }]);
    expect(answerer.step([showingYes], 2200)).toEqual([{ key: "a", keys: "\r" }]);
    expect(answerer.step([showingYes], 5000)).toEqual([]);
    expect(answerer.step([showingYes], 12300)).toEqual([{ key: "a", keys: "\r" }]);
  });

  it("answers every pane showing it, each on its own clock, and forgets a pane that stopped", () => {
    const answerer = new TrustAnswerer();
    const other = { key: "b", pointer: { on: "yes" } } as const;
    expect(answerer.step([showingNo, other], 0)).toEqual([
      { key: "a", keys: "\x1b[B" },
      { key: "b", keys: "\r" },
    ]);
    expect(answerer.step([other], 100)).toEqual([]);
    // "a" came back: it is a new question.
    expect(answerer.step([showingNo, other], 200)).toEqual([{ key: "a", keys: "\x1b[B" }]);
  });
});

describe("the shell's own watcher", () => {
  it("is told by a file named for the shell", () => {
    expect(trustMarker("C:\\Users\\t", 4242, true)).toBe("C:\\Users\\t\\.local\\state\\keep\\trust\\4242");
    expect(trustMarker("/Users/t/", 77, false)).toBe("/Users/t/.local/state/keep/trust/77");
  });
});

describe("the panes looked at", () => {
  /** `refused`: the folders the core does not vouch for, and why. */
  function setup(markers: string[] = [], refused: Record<string, string> = {}) {
    const sent: string[] = [];
    const logged: string[] = [];
    const asked: string[] = [];
    let clock = 0;
    const io: TrustIO = {
      home: "/Users/t",
      windows: false,
      markerExists: async (path) => markers.includes(path),
      whyNotTrust: async (folder) => {
        asked.push(folder);
        return refused[folder];
      },
      log: (line) => void logged.push(line),
      now: () => clock,
    };
    const pane = (key: string, screen: string, shellPid = 100, folder = "/Users/t/projeto"): TrustPane => ({
      key,
      screen,
      shellPid,
      folder: async () => folder,
      send: async (keys) => void sent.push(`${key}:${JSON.stringify(keys)}`),
    });
    return { io, sent, logged, asked, pane, tick: (ms: number) => (clock += ms) };
  }

  it("leaves the question to the person where the core does not vouch for the folder", async () => {
    const hook = "/Users/t/clonado/.claude/settings.json tem `hooks`";
    const { io, sent, logged, pane } = setup([], { "/Users/t/clonado": hook });
    await new TrustWatch().look([pane("a", OLD, 100, "/Users/t/clonado"), pane("b", OLD, 200)], io);
    expect(sent).toEqual(['b:"\\u001b[B"']);
    expect(logged).toContain(`trust question in a: left to the person (${hook})`);
  });

  it("leaves it too when the daemon cannot say the folder, without asking the core", async () => {
    const { io, sent, asked, pane } = setup();
    await new TrustWatch().look([pane("a", OLD, 100, "")], io);
    expect(sent).toEqual([]);
    expect(asked).toEqual([]);
  });

  it("asks the core once for each question, and again for another folder", async () => {
    const { io, asked, pane, tick } = setup([], { "/Users/t/clonado": "tem `hooks`" });
    const watch = new TrustWatch();
    await watch.look([pane("a", OLD, 100, "/Users/t/clonado")], io);
    tick(1000);
    await watch.look([pane("a", OLD, 100, "/Users/t/clonado")], io);
    expect(asked).toEqual(["/Users/t/clonado"]);
    tick(1000);
    await watch.look([pane("a", OLD, 100, "/Users/t/outro")], io);
    expect(asked).toEqual(["/Users/t/clonado", "/Users/t/outro"]);
    // The question went away and came back: asked again.
    await watch.look([pane("a", "só uma conversa\n❯ ")], io);
    await watch.look([pane("a", OLD, 100, "/Users/t/outro")], io);
    expect(asked).toEqual(["/Users/t/clonado", "/Users/t/outro", "/Users/t/outro"]);
  });

  it("answers the pane that shows the question, and only that one", async () => {
    const { io, sent, pane, tick } = setup();
    const watch = new TrustWatch();
    await watch.look([pane("a", OLD), pane("b", "só uma conversa\n❯ ")], io);
    expect(sent).toEqual(['a:"\\u001b[B"']);
    tick(1000);
    await watch.look([pane("a", OLD_ON_YES), pane("b", "")], io);
    expect(sent).toEqual(['a:"\\u001b[B"', 'a:"\\r"']);
  });

  it("leaves a pane the shell's own watcher is answering", async () => {
    const { io, sent, pane } = setup([trustMarker("/Users/t", 100, false)]);
    await new TrustWatch().look([pane("a", OLD, 100), pane("b", OLD, 200)], io);
    expect(sent).toEqual(['b:"\\u001b[B"']);
  });

  it("says in the log what it did", async () => {
    const { io, logged, pane } = setup();
    await new TrustWatch().look([pane("ws\u00001", NUMBERED_ON_YES)], io);
    expect(logged).toEqual(["trust question in ws/1: yes"]);
  });
});
