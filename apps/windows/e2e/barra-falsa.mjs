// Stand-ins for Claude Code and Codex in a tab of the end-to-end run
// (run.ps1, src/e2e.ts): what the bar under a conversation reads and drives,
// on a real Windows, without anybody's account.
//
//   node barra-falsa.mjs claude   poses as Claude Code 2.1.296: its prompt box
//                                 with three agents under it, `/model`'s list
//                                 and `/effort`'s rule (each takes "s" for this
//                                 session only), the question before a switch
//                                 of models, and `/model <id>` saving the
//                                 default in settings.json. It runs the status
//                                 line command of settings.json, as Claude Code
//                                 does — through Git Bash where there is one —
//                                 with its model and effort on stdin, when it
//                                 starts and after each change.
//   node barra-falsa.mjs codex    poses as Codex 0.162.1: its prompt and the
//                                 footer that says the model and the effort,
//                                 and its `/model` picker (a number key
//                                 chooses at once; Max and Ultra under "More
//                                 reasoning…").
//
// The screens are the ones tools/status-bar-test.sh and tools/codex-bar-test.sh
// draw on macOS. Each says, on its first line, the keys of the last change it
// was walked through (`e2e: /model<CR>23`) for the check to read; every byte
// it reads also goes to KEEP_E2E_BARRA\<program>-teclas.txt, and each run of
// the status line to claude-statusline.txt.

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const program = process.argv[2];
const work = process.env.KEEP_E2E_BARRA || process.cwd();
const out = process.stdout;

const ESC = "\x1b";
/** Bytes as written down: Enter and Esc spelled out. */
const spelled = (bytes) => bytes.replace(/\r/g, "<CR>").replace(/\x1b/g, "<ESC>");
/** A change's keys as said on screen: an arrow in either of its forms said as `<ESC>[B`. */
const walked = (bytes) => spelled(bytes.replace(/\x1bO([ABCD])/g, "\x1b[$1"));

function note(file, line) {
  try {
    fs.appendFileSync(path.join(work, file), `${line}\n`);
  } catch {
    // A note that cannot be written loses nothing the check needs.
  }
}

/** The keys in what was read: an arrow (`ESC [ C`, or `ESC O C`) is one, written down as `ESC [ C`. */
function keys(bytes) {
  const found = [];
  for (let i = 0; i < bytes.length; ) {
    const arrow = /^\x1b[[O]([ABCD])/.exec(bytes.slice(i));
    if (arrow) {
      found.push(`${ESC}[${arrow[1]}`);
      i += arrow[0].length;
    } else {
      found.push(bytes[i]);
      i += 1;
    }
  }
  return found;
}

function columns() {
  return Math.max(40, (out.columns || 100) - 2);
}

function show(lines, title) {
  out.write(`${title ? `\x1b]2;${title}\x07` : ""}\x1b[2J\x1b[H${lines.join("\r\n")}`);
}

// ---------------------------------------------------------------- Claude Code

const LEVELS = ["low", "medium", "high", "xhigh", "max"];
const LIST = [
  ["Default (recommended)", null],
  ["Opus 5.5", ["claude-opus-5-5", "Opus 5.5"]],
  ["Fable 5.1", ["claude-fable-5-1", "Fable 5.1"]],
  ["Sonnet 5.5", ["claude-sonnet-5-5", "Sonnet 5.5"]],
  ["Haiku 5.5", ["claude-haiku-5-5", "Haiku 5.5"]],
];
const NAMES = { "claude-opus-5-5[1m]": "Opus 5.5 (1M context)" };

function claudeHome() {
  const set = (v) => (v && v.trim() ? v : null);
  return set(process.env.CLAUDE_CONFIG_DIR) ?? path.join(set(process.env.USERPROFILE) ?? process.env.HOME ?? ".", ".claude");
}

function readSettings() {
  try {
    return JSON.parse(fs.readFileSync(path.join(claudeHome(), "settings.json"), "utf8").replace(/^﻿/, ""));
  } catch {
    return {};
  }
}

/** What `/model <id>` and Enter in a picker do: the default for new sessions, saved. */
function saveDefault(key, value) {
  const settings = readSettings();
  settings[key] = value;
  fs.mkdirSync(claudeHome(), { recursive: true });
  fs.writeFileSync(path.join(claudeHome(), "settings.json"), `${JSON.stringify(settings, null, 2)}\n`);
  note("claude-statusline.txt", `${new Date().toISOString()} default saved: ${key}=${JSON.stringify(value)}`);
}

/** Git Bash, which Claude Code runs its hooks and status line through on Windows; null where there is none. */
function gitBash() {
  if (process.platform !== "win32") return "/bin/sh";
  const candidates = [
    process.env.CLAUDE_CODE_GIT_BASH_PATH,
    process.env.ProgramW6432 && path.join(process.env.ProgramW6432, "Git", "bin", "bash.exe"),
    process.env.ProgramFiles && path.join(process.env.ProgramFiles, "Git", "bin", "bash.exe"),
  ];
  return candidates.find((p) => p && fs.existsSync(p)) ?? null;
}

function claude() {
  let model = ["claude-opus-5-5[1m]", NAMES["claude-opus-5-5[1m]"]];
  let effort = "xhigh";
  let screen = "box";
  let typed = "";
  let pointer = 1;
  let marker = LEVELS.indexOf(effort);
  let walk = "";
  let last = "";

  // The status line: one run at a time, the last state always the last run.
  let running = false;
  let again = false;
  function statusLine() {
    if (running) {
      again = true;
      return;
    }
    const line = readSettings().statusLine;
    if (!line || line.type !== "command" || typeof line.command !== "string") {
      note("claude-statusline.txt", `${new Date().toISOString()} no statusLine in ${claudeHome()}`);
      return;
    }
    running = true;
    const started = Date.now();
    const shell = gitBash() ?? true;
    const payload = {
      session_id: "keep-e2e-claude",
      transcript_path: "",
      cwd: process.cwd(),
      model: { id: model[0], display_name: model[1] },
      workspace: { current_dir: process.cwd(), project_dir: process.cwd() },
      version: "2.1.296",
      output_style: { name: "default" },
      effort: { level: effort },
    };
    let stderr = "";
    let stdout = "";
    const child = spawn(line.command, [], { shell, stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    const done = (code) => {
      note(
        "claude-statusline.txt",
        `${new Date().toISOString()} ran through ${shell === true ? "cmd" : shell} in ${Date.now() - started} ms: ` +
          `exit ${code}; ${model[0]} ${effort}; stdout ${JSON.stringify(stdout.trim())}; stderr ${JSON.stringify(stderr.trim())}`,
      );
      running = false;
      if (again) {
        again = false;
        statusLine();
      }
    };
    child.on("error", (e) => {
      stderr += String(e);
      done("error");
    });
    child.on("close", (code) => done(code));
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(payload));
  }

  function draw() {
    const head = [`  e2e: ${last}`];
    if (screen === "models") {
      show([
        ...head,
        "   Select model",
        ...LIST.map(([label], i) => `${i === pointer ? "   ❯ " : "     "}${i + 1}.  ${label}`),
        "   Enter to set as default · s to use this session only · Esc to cancel",
      ]);
    } else if (screen === "effort") {
      const at = [29, 37, 48, 57, 68];
      const rule = [...Array(29).fill(" "), ...Array(42).fill("─")];
      rule[at[marker] + 1] = "▲";
      show([
        ...head,
        "   Effort",
        rule.join(""),
        `${" ".repeat(29)}low     medium     high     xhigh      max`,
        "   ←/→ to adjust · Enter to confirm · s for this session only · Esc to cancel",
      ]);
    } else if (screen === "switch") {
      show([...head, "   Switch model?", "   the full history gets re-read", ` ❯ 1. Yes, switch to ${LIST[pointer][0]}`, "   2. No"]);
    } else {
      const rule = "─".repeat(columns());
      show(
        [
          ...head,
          "",
          rule,
          `❯ ${typed}`,
          rule,
          "  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents",
          "",
          "  ⏺ main",
          "  ◯ general-purpose  Reading files",
          "  ◯ Explore  Scanning",
          "  ◯ Plan  Thinking",
        ],
        "✳ Claude Code (e2e)",
      );
    }
  }

  /** Back at the prompt: the change walked through is the one to say. */
  function finish() {
    screen = "box";
    typed = "";
    last = walk;
    walk = "";
  }

  function press(key) {
    if (key.startsWith(`${ESC}[`)) {
      if (screen === "models") pointer = Math.max(0, Math.min(LIST.length - 1, pointer + (key.endsWith("B") ? 1 : key.endsWith("A") ? -1 : 0)));
      else if (screen === "effort") marker = Math.max(0, Math.min(4, marker + (key.endsWith("C") ? 1 : key.endsWith("D") ? -1 : 0)));
      return;
    }
    if (key === ESC) return finish();
    if (screen === "box") {
      if (key !== "\r") {
        typed += key;
        return;
      }
      const command = typed;
      typed = "";
      if (command === "/model") {
        screen = "models";
        const at = LIST.findIndex(([, m]) => m && m[0] === model[0]);
        pointer = at > 0 ? at : 1;
      } else if (command === "/effort") {
        screen = "effort";
        marker = Math.max(0, LEVELS.indexOf(effort));
      } else if (command.startsWith("/model ")) {
        const id = command.slice(7).trim();
        model = [id, NAMES[id] ?? id];
        saveDefault("model", id);
        finish();
        statusLine();
      } else if (command.startsWith("/effort ")) {
        effort = command.slice(8).trim();
        saveDefault("effortLevel", effort);
        finish();
        statusLine();
      } else {
        finish();
      }
    } else if (screen === "models") {
      if ((key === "s" || key === "\r") && LIST[pointer][1]) {
        if (key === "\r") {
          model = LIST[pointer][1];
          saveDefault("model", model[0]);
          finish();
          statusLine();
        } else {
          // A conversation with a cache: Claude Code asks first.
          screen = "switch";
        }
      }
    } else if (screen === "switch") {
      if (key === "\r" || key === "1") {
        model = LIST[pointer][1];
        finish();
        statusLine();
      } else if (key === "2") {
        finish();
      }
    } else if (screen === "effort") {
      if (key === "s" || key === "\r") {
        effort = LEVELS[marker];
        if (key === "\r") saveDefault("effortLevel", effort);
        finish();
        statusLine();
      }
    }
  }

  return { draw, press, start: statusLine, record: (bytes) => (walk += walked(bytes)) };
}

// ---------------------------------------------------------------- Codex

const CODEX_MODELS = ["GPT-6.1-Sol", "GPT-6-Astra", "GPT-6-Sol"];
const CODEX_LEVELS = ["low", "medium", "high", "xhigh"];

function codex() {
  let model = "GPT-6-Astra";
  let effort = "max";
  let screen = "idle";
  let typed = "";
  let picked = "";
  let walk = "";
  let last = "";
  const said = [];

  function draw() {
    const head = ["  >_ OpenAI Codex (v0.162.1)", "     ~/x", `  e2e: ${last}`, ""];
    if (screen === "models") {
      show([
        ...head,
        "  Select Model and Effort",
        "",
        ...CODEX_MODELS.map((m, i) => `${m === model ? "› " : "  "}${i + 1}. ${m}${m === model ? " (current)" : ""}`),
        "",
        "  enter select · esc back",
      ]);
    } else if (screen === "levels") {
      show([
        ...head,
        `  Select Reasoning Level for ${picked}`,
        "",
        ...["Low", "Medium", "High", "Extra high", "More reasoning…"].map((l, i) => `  ${i + 1}. ${l}`),
        "",
        "  enter select · esc back",
      ]);
    } else if (screen === "advanced") {
      show([...head, "  Advanced Reasoning", "  ⚠ Consumes usage limits faster", "", "  1. Max", "  2. Ultra", "", "  enter default · s session · esc back"]);
    } else {
      show(
        [
          ...head,
          ...said.slice(-3).flatMap((l) => [l, ""]),
          `› ${typed || "Ask Codex to do anything"}`,
          "",
          `  ${model} ${effort} · ~/x`,
          "  ← for agents · ? for shortcuts",
        ],
        "codex (e2e)",
      );
    }
  }

  function finish() {
    screen = "idle";
    typed = "";
    last = walk;
    walk = "";
  }

  function change(m, e) {
    model = m;
    effort = e;
    said.push(`• Model changed to ${m.toLowerCase()} ${e}`);
    finish();
  }

  function press(key) {
    if (key === ESC || key.startsWith(`${ESC}[`)) {
      if (key === ESC) finish();
      return;
    }
    if (screen === "idle") {
      if (key !== "\r") {
        typed += key;
        return;
      }
      if (typed === "/model") {
        screen = "models";
        typed = "";
      } else {
        finish();
      }
    } else if (screen === "models" && /^[1-3]$/.test(key)) {
      picked = CODEX_MODELS[Number(key) - 1];
      screen = "levels";
    } else if (screen === "levels" && /^[1-4]$/.test(key)) {
      change(picked, CODEX_LEVELS[Number(key) - 1]);
    } else if (screen === "levels" && key === "5") {
      screen = "advanced";
    } else if (screen === "advanced" && /^[12]$/.test(key)) {
      change(picked, key === "1" ? "max" : "ultra");
    }
  }

  return { draw, press, start: () => {}, record: (bytes) => (walk += walked(bytes)) };
}

// ---------------------------------------------------------------- the terminal

const made = program === "claude" ? claude() : program === "codex" ? codex() : null;
if (!made) {
  console.error("uso: node barra-falsa.mjs claude|codex");
  process.exit(2);
}
const pid = process.pid;
note(`${program}-teclas.txt`, `--- ${new Date().toISOString()} pid ${pid}`);
if (process.stdin.isTTY) process.stdin.setRawMode(true);
process.stdin.setEncoding("utf8");
process.stdin.on("data", (bytes) => {
  note(`${program}-teclas.txt`, spelled(bytes));
  // Ctrl+C leaves, as in the programs posed as.
  if (bytes.includes("\x03")) process.exit(0);
  made.record(bytes);
  for (const key of keys(bytes)) made.press(key);
  made.draw();
});
process.stdin.on("end", () => process.exit(0));
out.on("resize", () => made.draw());
made.draw();
made.start();
