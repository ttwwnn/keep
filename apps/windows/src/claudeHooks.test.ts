// The account hook in Claude Code's settings: added after what the person has,
// every other byte as it was, and never twice.
import { describe, expect, it } from "vitest";
import { ACCOUNT_HOOK_ARGS, accountHookCommand, planAccountHook, planHook } from "./claudeHooks";
import { parseSettings } from "./claudeSettings";

const KEEP = "C:\\Users\\Ana Lú\\AppData\\Local\\Keep\\bin\\keep.exe";
const COMMAND = `"C:/Users/Ana Lú/AppData/Local/Keep/bin/keep.exe" ${ACCOUNT_HOOK_ARGS}`;
const OURS = { type: "command", command: COMMAND, timeout: 10 };
const GROUP = { hooks: [OURS] };

/** The text written, when the plan is to write. */
function written(text: string | null, keep = KEEP): string {
  const plan = planAccountHook(text, keep);
  if (plan.kind !== "write") throw new Error(`não gravou: ${JSON.stringify(plan)}`);
  return plan.text;
}

/** What the file asks Claude Code to run at an event, group by group. */
const at = (text: string, event = "UserPromptSubmit") => (parseSettings(text)?.hooks as Record<string, unknown> | undefined)?.[event];

describe("the command", () => {
  it("is the keep with its path quoted and in forward slashes", () => {
    expect(accountHookCommand(KEEP)).toBe(COMMAND);
    expect(ACCOUNT_HOOK_ARGS).toBe("ia conta-da-aba --gancho");
  });
});

describe("no file, or an empty one", () => {
  it("is a file with only the hook", () => {
    for (const text of [null, "", "  \n", "\uFEFF"]) {
      const after = written(text);
      expect(JSON.parse(after)).toEqual({ hooks: { UserPromptSubmit: [GROUP] } });
      expect(after).toBe(`${JSON.stringify({ hooks: { UserPromptSubmit: [GROUP] } }, null, 2)}\n`);
    }
  });
});

describe("a file without hooks", () => {
  const before = '{\n  "theme": "dark",\n  "permissions": {\n    "allow": [\n      "Bash(git status)"\n    ]\n  }\n}\n';

  it("gets them before the closing brace, every other byte as it was", () => {
    const after = written(before);
    expect(after.startsWith(before.slice(0, before.lastIndexOf("}")).trimEnd() + ",")).toBe(true);
    expect(at(after)).toEqual([GROUP]);
    expect(parseSettings(after)?.theme).toBe("dark");
    expect(after).toBe(
      before.slice(0, before.lastIndexOf("}")).trimEnd() +
        `,\n  "hooks": ${JSON.stringify({ UserPromptSubmit: [GROUP] }, null, 2).replace(/\n/g, "\n  ")}\n}\n`,
    );
  });

  it("in the file's own indentation and line ends", () => {
    const crlf = written('{\r\n    "theme": "light"\r\n}');
    expect(crlf.startsWith('{\r\n    "theme": "light",\r\n    "hooks": {\r\n        "UserPromptSubmit": [')).toBe(true);
    expect(crlf.includes("\n") && !/[^\r]\n/.test(crlf)).toBe(true);
    expect(at(crlf)).toEqual([GROUP]);
    const tabs = written('{\n\t"theme": "light"\n}\n');
    expect(tabs.startsWith('{\n\t"theme": "light",\n\t"hooks": {\n\t\t"UserPromptSubmit": [')).toBe(true);
    expect(at(tabs)).toEqual([GROUP]);
  });

  it("an object with nothing in it, on one line or on two", () => {
    expect(JSON.parse(written("{}"))).toEqual({ hooks: { UserPromptSubmit: [GROUP] } });
    expect(written("{}")).toBe(`{"hooks":${JSON.stringify({ UserPromptSubmit: [GROUP] })}}`);
    expect(JSON.parse(written("{\n}\n"))).toEqual({ hooks: { UserPromptSubmit: [GROUP] } });
  });

  it("a file all on one line keeps it on one line", () => {
    expect(written('{"theme":"dark"}\n')).toBe(`{"theme":"dark","hooks":${JSON.stringify({ UserPromptSubmit: [GROUP] })}}\n`);
  });

  it("a byte order mark stays where it was", () => {
    const after = written('\uFEFF{\n  "theme": "dark"\n}\n');
    expect(after.startsWith('\uFEFF{\n  "theme": "dark",\n  "hooks"')).toBe(true);
    expect(at(after)).toEqual([GROUP]);
  });
});

describe("a file with hooks of the person's", () => {
  const stop = '    "Stop": [\n      {\n        "hooks": [\n          {\n            "type": "command",\n            "command": "python3 ~/.claude/hooks/fim.py",\n            "timeout": 10\n          }\n        ]\n      }\n    ]\n';
  const other = `{\n  "hooks": {\n${stop}  },\n  "theme": "dark"\n}\n`;

  it("another event: added inside hooks, after theirs, theirs untouched", () => {
    const after = written(other);
    expect(after).toBe(
      `{\n  "hooks": {\n${stop.trimEnd()},\n    "UserPromptSubmit": ${JSON.stringify([GROUP], null, 2).replace(/\n/g, "\n    ")}\n  },\n  "theme": "dark"\n}\n`,
    );
    expect((parseSettings(after)?.hooks as Record<string, unknown>).Stop).toEqual((parseSettings(other)?.hooks as Record<string, unknown>).Stop);
    expect(at(after)).toEqual([GROUP]);
    expect(parseSettings(after)?.theme).toBe("dark");
  });

  it("the same event: a group of its own after theirs, theirs untouched", () => {
    const jev = { hooks: [{ type: "command", command: "python3 ~/.claude/skills/jev/ganchos.py modelo", timeout: 20 }] };
    const before = `{\n  "hooks": {\n    "UserPromptSubmit": [\n      {\n        "hooks": [\n          {\n            "type": "command",\n            "command": "python3 ~/.claude/skills/jev/ganchos.py modelo",\n            "timeout": 20\n          }\n        ]\n      }\n    ],\n${stop}  },\n  "language": "portugues"\n}\n`;
    const after = written(before);
    expect(at(after)).toEqual([jev, GROUP]);
    // The person's file as it was, with the group right after their last one.
    const group = JSON.stringify(GROUP, null, 2).replace(/\n/g, "\n      ");
    expect(after).toBe(before.replace("        ]\n      }\n    ],\n", `        ]\n      },\n      ${group}\n    ],\n`));
    expect(parseSettings(after)?.language).toBe("portugues");
    expect((parseSettings(after)?.hooks as Record<string, unknown>).Stop).toEqual((parseSettings(before)?.hooks as Record<string, unknown>).Stop);
  });

  it("an empty list or an empty hooks object takes it too", () => {
    expect(at(written('{\n  "hooks": {\n    "UserPromptSubmit": []\n  }\n}\n'))).toEqual([GROUP]);
    expect(at(written('{\n  "hooks": {}\n}\n'))).toEqual([GROUP]);
    expect(at(written('{"hooks":{"UserPromptSubmit":[]}}'))).toEqual([GROUP]);
    expect(written('{"hooks":{"Stop":[]}}')).toBe(`{"hooks":{"Stop":[],"UserPromptSubmit":${JSON.stringify([GROUP])}}}`);
    expect(written('{"hooks":{"UserPromptSubmit":[{"hooks":[]}]}}')).toBe(`{"hooks":{"UserPromptSubmit":[{"hooks":[]},${JSON.stringify(GROUP)}]}}`);
  });

  it("the author's own file: his hooks stay and the account hook comes after them", () => {
    const before = JSON.stringify(
      {
        hooks: {
          PreCompact: [{ matcher: "manual", hooks: [{ type: "command", command: "/bin/bash guard.sh", timeout: 20 }] }],
          UserPromptSubmit: [{ hooks: [{ type: "command", command: "python3 jev.py modelo", timeout: 20 }] }],
          Stop: [{ hooks: [{ type: "command", command: "python3 fim.py", timeout: 10 }] }],
        },
        statusLine: { type: "command", command: "keep ia statusline" },
        effortLevel: "xhigh",
      },
      null,
      2,
    );
    const after = written(`${before}\n`);
    const hooks = parseSettings(after)?.hooks as Record<string, unknown[]>;
    expect(hooks.UserPromptSubmit).toHaveLength(2);
    expect(hooks.UserPromptSubmit[1]).toEqual(GROUP);
    expect(hooks.PreCompact).toEqual((JSON.parse(before).hooks as Record<string, unknown[]>).PreCompact);
    expect(hooks.Stop).toEqual((JSON.parse(before).hooks as Record<string, unknown[]>).Stop);
    expect(parseSettings(after)?.statusLine).toEqual({ type: "command", command: "keep ia statusline" });
  });
});

describe("never twice", () => {
  const files = [
    null,
    "{}",
    '{\n  "theme": "dark"\n}\n',
    '{"hooks":{"Stop":[]}}',
    '{\r\n  "hooks": {\r\n    "UserPromptSubmit": [\r\n      {\r\n        "hooks": [{ "type": "command", "command": "x" }]\r\n      }\r\n    ]\r\n  }\r\n}\r\n',
  ];

  it("asked again of what it wrote, it has nothing to do", () => {
    for (const text of files) {
      const once = written(text);
      expect(planAccountHook(once, KEEP)).toEqual({ kind: "ours" });
    }
  });

  it("a hook of the same kind from another place is the person's, and left alone", () => {
    const manual = '{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"/Applications/Keep.app/Contents/Resources/keep ia conta-da-aba --gancho"}]}]}}';
    expect(planAccountHook(manual, KEEP)).toEqual({
      kind: "present",
      command: "/Applications/Keep.app/Contents/Resources/keep ia conta-da-aba --gancho",
    });
    const moved = JSON.stringify({ hooks: { UserPromptSubmit: [{ hooks: [{ type: "command", command: `"C:/velho/keep.exe" ${ACCOUNT_HOOK_ARGS}` }] }] } });
    expect(planAccountHook(moved, KEEP).kind).toBe("present");
  });

  it("the same command on another event is not the same hook", () => {
    const stop = JSON.stringify({ hooks: { Stop: [{ hooks: [{ type: "command", command: COMMAND }] }] } });
    expect(planAccountHook(stop, KEEP).kind).toBe("write");
  });
});

describe("what is not touched", () => {
  it("a file that is not a JSON object", () => {
    for (const text of ['{"theme": "dark",}', "// comentário\n{}", "[]", '"x"', "{", "null"]) {
      expect(planAccountHook(text, KEEP).kind).toBe("unreadable");
    }
  });

  it("hooks of a shape this does not know", () => {
    expect(planAccountHook('{"hooks": []}', KEEP).kind).toBe("unreadable");
    expect(planAccountHook('{"hooks": "x"}', KEEP).kind).toBe("unreadable");
    expect(planAccountHook('{"hooks": {"UserPromptSubmit": {}}}', KEEP).kind).toBe("unreadable");
    expect(planAccountHook('{"hooks": {"UserPromptSubmit": "x"}}', KEEP).kind).toBe("unreadable");
  });

  it("a path a shell would read as something else", () => {
    for (const keep of ["C:\\Users\\a$b\\keep.exe", "C:\\Users\\a`b\\keep.exe", "C:\\Users\\100%\\keep.exe", 'C:\\a"b\\keep.exe']) {
      expect(planAccountHook("{}", keep).kind).toBe("unreadable");
    }
    // Spaces, brackets, apostrophes and accents are only a path.
    expect(planAccountHook("{}", "C:\\Users\\O'Brien (Trabalho) José\\keep.exe").kind).toBe("write");
  });
});

describe("the same shape in Codex's hooks.json", () => {
  it("takes a hook of its own the same way, the person's PreToolUse hook with its matcher untouched", () => {
    const before =
      '{\n  "hooks": {\n    "UserPromptSubmit": [\n      {\n        "hooks": [\n          {\n            "type": "command",\n            "command": "python3 ganchos.py modelo codex",\n            "timeout": 20,\n            "statusMessage": "Jev: escolhendo modelo e esforço"\n          }\n        ]\n      }\n    ],\n    "PreToolUse": [\n      {\n        "matcher": "^request_user_input$",\n        "hooks": []\n      }\n    ]\n  }\n}\n';
    const plan = planHook(before, { event: "UserPromptSubmit", command: "keep-codex-hook", timeout: 20 }, "keep-codex-hook");
    if (plan.kind !== "write") throw new Error(JSON.stringify(plan));
    const hooks = parseSettings(plan.text)?.hooks as Record<string, Array<Record<string, unknown>>>;
    expect(hooks.UserPromptSubmit).toHaveLength(2);
    expect(hooks.PreToolUse[0].matcher).toBe("^request_user_input$");
  });
});
