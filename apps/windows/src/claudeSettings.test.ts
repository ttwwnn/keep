// Claude Code's settings file as the bar touches it: the status line set up
// on opening — added, nothing else changed — and the defaults a `/model <id>`
// saved put back.
import { describe, expect, it } from "vitest";
import {
  canonical,
  defaultsOf,
  parseSettings,
  planStatusLine,
  restoreDefaults,
  statusLineCommand,
} from "./claudeSettings";

const SCRIPT = "C:\\Users\\Ana Lú\\AppData\\Local\\Keep\\keep-statusline.ps1";
const COMMAND = 'powershell -NoProfile -ExecutionPolicy Bypass -File "C:/Users/Ana Lú/AppData/Local/Keep/keep-statusline.ps1"';

/** The text written, when the plan is to write. */
function written(text: string | null, script = SCRIPT): string {
  const plan = planStatusLine(text, script);
  if (plan.kind !== "write") throw new Error(`não gravou: ${JSON.stringify(plan)}`);
  expect(plan.command).toBe(statusLineCommand(script));
  return plan.text;
}

describe("the status line command", () => {
  it("runs the script with Windows PowerShell, the path quoted and in forward slashes", () => {
    expect(statusLineCommand(SCRIPT)).toBe(COMMAND);
  });
});

describe("setting the status line up", () => {
  it("adds it before the closing brace, every other byte as it was", () => {
    const before = '{\n  "theme": "dark",\n  "permissions": {\n    "allow": [\n      "Bash(git status)"\n    ]\n  }\n}\n';
    const after = written(before);
    expect(after).toBe(
      '{\n  "theme": "dark",\n  "permissions": {\n    "allow": [\n      "Bash(git status)"\n    ]\n  },\n' +
        '  "statusLine": {\n    "type": "command",\n    "command": ' +
        JSON.stringify(COMMAND) +
        "\n  }\n}\n",
    );
    // What was there, byte for byte, up to where the key goes.
    expect(after.startsWith(before.slice(0, before.lastIndexOf("}")).trimEnd())).toBe(true);
    expect(parseSettings(after)?.statusLine).toEqual({ type: "command", command: COMMAND });
  });

  it("in the file's own indentation and line ends", () => {
    const before = '{\r\n    "theme": "light"\r\n}';
    expect(written(before)).toBe(
      '{\r\n    "theme": "light",\r\n    "statusLine": {\r\n        "type": "command",\r\n        "command": ' +
        JSON.stringify(COMMAND) +
        "\r\n    }\r\n}",
    );
  });

  it("an object with nothing in it gets it without a comma", () => {
    expect(parseSettings(written("{\n}\n"))).toEqual({ statusLine: { type: "command", command: COMMAND } });
    expect(written("{}")).toBe(`{"statusLine":${JSON.stringify({ type: "command", command: COMMAND })}}`);
  });

  it("a file all on one line keeps it on one line", () => {
    expect(written('{"theme":"dark"}\n')).toBe(`{"theme":"dark","statusLine":${JSON.stringify({ type: "command", command: COMMAND })}}\n`);
  });

  it("a byte order mark stays where it was", () => {
    const after = written('\uFEFF{\n  "theme": "dark"\n}\n');
    expect(after.startsWith('\uFEFF{\n  "theme": "dark",\n  "statusLine"')).toBe(true);
    expect(parseSettings(after)?.theme).toBe("dark");
  });

  it("no file, or an empty one: a file with only it", () => {
    const expected = { statusLine: { type: "command", command: COMMAND } };
    for (const text of [null, "", "  \n"]) {
      const after = written(text);
      expect(JSON.parse(after)).toEqual(expected);
      expect(after).toBe(`${JSON.stringify(expected, null, 2)}\n`);
    }
  });

  it("set to the script already: nothing to do", () => {
    const text = JSON.stringify({ statusLine: { type: "command", command: COMMAND }, theme: "dark" });
    expect(planStatusLine(text, SCRIPT)).toEqual({ kind: "ours" });
  });

  it("another one: left alone, and what it runs said", () => {
    const text = JSON.stringify({ statusLine: { type: "command", command: "bash ~/.claude/minha-linha.sh" } });
    expect(planStatusLine(text, SCRIPT)).toEqual({ kind: "other", command: "bash ~/.claude/minha-linha.sh" });
    // The script at another place is another one too: the person set it up.
    const elsewhere = JSON.stringify({ statusLine: { type: "command", command: statusLineCommand("C:\\velho\\keep-statusline.ps1") } });
    expect(planStatusLine(elsewhere, SCRIPT).kind).toBe("other");
    // Whatever it holds.
    expect(planStatusLine('{"statusLine": null}', SCRIPT)).toEqual({ kind: "other", command: "null" });
    expect(planStatusLine('{"statusLine": {"type": "static"}}', SCRIPT)).toEqual({ kind: "other", command: '{"type":"static"}' });
  });

  it("a file that is not a JSON object is not touched", () => {
    for (const text of ['{"theme": "dark",}', "// comentário\n{}", "[]", '"x"', "{", "null"]) {
      expect(planStatusLine(text, SCRIPT).kind).toBe("unreadable");
    }
  });

  it("a path a shell would read as something else is not risked", () => {
    for (const script of ["C:\\Users\\a$b\\k.ps1", "C:\\Users\\a`b\\k.ps1", "C:\\Users\\100%\\k.ps1"]) {
      const plan = planStatusLine("{}", script);
      expect(plan.kind).toBe("unreadable");
    }
    // An apostrophe, brackets, spaces and accents are only a path.
    expect(planStatusLine("{}", "C:\\Users\\O'Brien (Trabalho) José\\k.ps1").kind).toBe("write");
  });
});

describe("the defaults /model <id> saves", () => {
  const before = '{\n  "theme": "dark",\n  "model": "claude-fable-5-1",\n  "effortLevel": "high"\n}\n';

  it("are the model, the effort and the model settings, absent ones left out", () => {
    expect(defaultsOf(before)).toBe('{"effortLevel":"high","model":"claude-fable-5-1"}');
    expect(defaultsOf('{"modelSettings": {"b": 1, "a": [2]}, "x": 1}')).toBe('{"modelSettings":{"a":[2],"b":1}}');
    expect(defaultsOf(null)).toBe("{}");
    expect(defaultsOf("não é JSON")).toBe("{}");
  });

  it("are put back as they were, the rest as the file has it now", () => {
    const saved = defaultsOf(before);
    // Claude Code wrote the model, and somebody added a key meanwhile.
    const now = JSON.stringify(
      { theme: "dark", model: "claude-opus-5-5[1m]", effortLevel: "high", modelSettings: { x: 1 }, novo: true },
      null,
      2,
    );
    const back = restoreDefaults(`${now}\n`, saved);
    expect(back).not.toBeNull();
    expect(JSON.parse(back!)).toEqual({ theme: "dark", model: "claude-fable-5-1", effortLevel: "high", novo: true });
    expect(back!.endsWith("}\n")).toBe(true);
    // The keys stay where they were.
    expect(Object.keys(JSON.parse(back!))).toEqual(["theme", "model", "effortLevel", "novo"]);
  });

  it("a key that was not there goes; one that went comes back", () => {
    const back = restoreDefaults('{"model": "claude-opus-5-5[1m]", "theme": "dark"}', defaultsOf('{"theme": "dark", "effortLevel": "low"}'));
    expect(JSON.parse(back!)).toEqual({ theme: "dark", effortLevel: "low" });
  });

  it("nothing to put back when nothing changed, or there is no file to put it in", () => {
    expect(restoreDefaults(before, defaultsOf(before))).toBeNull();
    // Another key changed: not theirs to undo.
    expect(restoreDefaults(before.replace("dark", "light"), defaultsOf(before))).toBeNull();
    expect(restoreDefaults(null, defaultsOf(before))).toBeNull();
    expect(restoreDefaults("não é JSON", defaultsOf(before))).toBeNull();
  });

  it("the file's line ends and byte order mark are kept", () => {
    const back = restoreDefaults('\uFEFF{\r\n  "model": "x"\r\n}\r\n', "{}");
    expect(back).toBe("\uFEFF{}\r\n");
    const crlf = restoreDefaults('{\r\n  "model": "x",\r\n  "theme": "dark"\r\n}', "{}");
    expect(crlf).toBe('{\r\n  "theme": "dark"\r\n}');
  });

  it("two equal values read the same whatever their keys' order", () => {
    expect(canonical({ b: 1, a: { d: [1, { f: 1, e: 2 }], c: null } })).toBe('{"a":{"c":null,"d":[1,{"e":2,"f":1}]},"b":1}');
  });
});
