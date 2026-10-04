// Stand-ins for the AI layer in the end-to-end run (run.ps1), so the footer,
// the tabs' menu and the worktrees in the close question are exercised on a
// real Windows without anybody's account:
//
//   node ia-falso.mjs casa <dir>        a made-up home: two Claude logins (the
//                                       global one and a login folder of the
//                                       Keep's) and a GPT one
//   node ia-falso.mjs servidor <file>   the services the core asks — owners and
//                                       usage — on 127.0.0.1; the port goes in <file>
//   node ia-falso.mjs keep <args…>      `keep` for the app (KEEP_IA_BIN, through
//                                       keep-falso.cmd): `ia contas|uso|ordem` go
//                                       to the real core (KEEP_E2E_REAL_KEEP);
//                                       the rest is answered as docs/ia.md says,
//                                       from the state in KEEP_E2E_FAKE_STATE,
//                                       every call written to <state>.chamadas.log
//
// The tokens are made up and the services answer only on this machine.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";

const [mode, ...rest] = process.argv.slice(2);
const DAY = 24 * 3600 * 1000;

function write(file, content) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, typeof content === "string" ? content : JSON.stringify(content, null, 2));
}

function home(dir) {
  const until = Date.now() + DAY;
  const login = (token) => ({
    claudeAiOauth: {
      accessToken: token,
      refreshToken: `r-${token}`,
      expiresAt: until,
      rateLimitTier: "default_claude_max_20x",
      subscriptionType: "max",
    },
  });
  write(path.join(dir, ".claude", ".credentials.json"), login("tok-ana"));
  const own = path.join(dir, ".claude", "contas", "fixas", "k-e2e0001");
  write(path.join(own, ".credentials.json"), login("tok-bia"));
  write(path.join(own, "keep.json"), { versao: 1, apelido: "trabalho", email: "bia@exemplo.com", uuid: "u-2", criadaEm: Date.now() });
  const part = (o) => Buffer.from(JSON.stringify(o)).toString("base64url");
  const claims = {
    exp: Math.floor(until / 1000),
    "https://api.openai.com/profile": { email: "ana@exemplo.com" },
    "https://api.openai.com/auth": { chatgpt_account_id: "acct-1", chatgpt_plan_type: "pro" },
  };
  const token = `${part({ alg: "none" })}.${part(claims)}.x`;
  write(path.join(dir, ".codex", "auth.json"), { tokens: { access_token: token, id_token: token, account_id: "acct-1" } });
}

function serve(portFile) {
  const owners = { "tok-ana": ["u-1", "ana@exemplo.com"], "tok-bia": ["u-2", "bia@exemplo.com"] };
  const iso = (ms) => new Date(Date.now() + ms).toISOString();
  const usage = {
    "tok-ana": {
      five_hour: { utilization: 42, resets_at: iso(2 * 3600_000 + 13 * 60_000) },
      seven_day: { utilization: 63, resets_at: iso(3 * DAY) },
      limits: [{ kind: "weekly_scoped", percent: 18, resets_at: iso(3 * DAY), scope: { model: { display_name: "Fable" } } }],
    },
    "tok-bia": {
      five_hour: { utilization: 96, resets_at: iso(40 * 60_000) },
      seven_day: { utilization: 100, resets_at: iso(DAY + 5 * 3600_000) },
    },
  };
  const server = http.createServer((request, response) => {
    const token = (request.headers.authorization ?? "").replace(/^Bearer\s+/i, "");
    const send = (status, body) => {
      response.writeHead(status, { "Content-Type": "application/json" });
      response.end(body === undefined ? "" : JSON.stringify(body));
    };
    const route = (request.url ?? "").split("?")[0];
    if (route.endsWith("/profile")) {
      const owner = owners[token];
      return owner ? send(200, { account: { uuid: owner[0], email_address: owner[1] } }) : send(401);
    }
    if (route.endsWith("/claude")) return usage[token] ? send(200, usage[token]) : send(401);
    if (route.endsWith("/codex")) {
      if (request.headers["chatgpt-account-id"] !== "acct-1") return send(403);
      const now = Math.floor(Date.now() / 1000);
      return send(200, {
        rate_limit: {
          limit_reached: false,
          primary_window: { used_percent: 30, limit_window_seconds: 18000, reset_at: now + 3 * 3600 },
          secondary_window: { used_percent: 12, limit_window_seconds: 604800, reset_at: now + 5 * 86400 },
        },
      });
    }
    send(404);
  });
  server.listen(0, "127.0.0.1", () => write(portFile, String(server.address().port)));
}

// ------------------------------------------------------------------ the stand-in core

/** The birth of the made-up worktree, past what a JavaScript number holds: it must come back exact. */
const BORN = "1791000000123456789";

function options(args) {
  const out = {};
  for (const a of args) {
    if (!a.startsWith("--")) continue;
    const [name, ...value] = a.slice(2).split("=");
    out[name] = value.join("=");
  }
  return out;
}

const label = (key) => (key === "claude:ordem" ? "Claude (ordem de prioridade)" : `${key.startsWith("gpt:") ? "GPT" : "Claude"} · ${key.split(":")[1]}`);

function answer(args, state) {
  const [area, command, ...more] = args;
  const opts = options(args);
  if (area === "ia" && command === "abas") {
    const ia = Object.entries(state.abas ?? {}).map(([where, tab]) => {
      const at = where.lastIndexOf(":");
      return { workspace: where.slice(0, at), aba: Number(where.slice(at + 1)), vinculo: "exato", pid: 0, conversa: null, ...tab };
    });
    return [0, { ok: true, keepd: { pid: 0, inicioMs: 0 }, exato: true, ia }];
  }
  if (area === "ia" && command === "trocar") {
    const where = `${opts.ws}:${opts.aba}`;
    state.ocupada ??= {};
    if (state.ocupada[where] && !("interromper" in opts)) {
      return [1, { ok: false, motivo: "ocupada", detalhe: "A aba está trabalhando." }];
    }
    delete state.ocupada[where];
    state.abas ??= {};
    const key = opts.para;
    state.abas[where] = {
      agente: key.startsWith("gpt:") ? "codex" : "claude",
      conta: key,
      atual: key === "claude:ordem" ? "claude:ana" : key,
    };
    return [0, { ok: true, feito: `retomou ${label(key)}` }];
  }
  if (area === "ia" && command === "entrar") {
    return [0, { ok: true, ws: opts.ws, aba: state.abaLogin ?? 1 }];
  }
  if (area === "ia" && command === "sincronizar") {
    return [0, { ok: true, estado: "feito", alvo: null, alteradas: [], pendentes: [] }];
  }
  if (area === "worktrees" && command === "listar") {
    const targets = more.filter((a) => !a.startsWith("--") && a.includes(":"));
    const wanted = state.worktreeAlvo;
    const mine =
      wanted &&
      state.caminhoWorktree &&
      targets.some((t) => {
        const at = t.lastIndexOf(":");
        return t.slice(0, at) === wanted.workspace && t.slice(at + 1).split(",").map(Number).includes(wanted.aba);
      });
    const body = {
      ok: true,
      abas: targets.map((alvo) => ({ alvo, pids: [] })),
      lixeira: mine
        ? [{ caminho: state.caminhoWorktree, ramo: "e2e-wt", head: "abc1234", alteracoes: 1, commits_so_aqui: 1, processos: [], nasceu_ns: "@BORN@" }]
        : [],
      mantidas: [],
      avisos: [],
    };
    return [0, body];
  }
  if (area === "worktrees" && command === "preparar") {
    const at = args.indexOf("--nasceu");
    const born = at >= 0 ? args[at + 1] : "";
    if (born !== BORN) return [0, { ok: false, motivo: `nascimento diferente do listado: ${born}` }];
    state.preparado = more[0];
    return [0, { ok: true }];
  }
  if (area === "worktrees" && command === "concluir") {
    state.concluido = { caminho: more[0], destino: more[1] };
    return [0, { ok: true }];
  }
  if (area === "worktrees" && command === "indexar") return [0, { ok: true }];
  return [2, { ok: false, motivo: "uso", detalhe: `keep (falso): pedido desconhecido: ${args.join(" ")}` }];
}

function core(args) {
  const stateFile = process.env.KEEP_E2E_FAKE_STATE;
  if (stateFile) fs.appendFileSync(`${stateFile}.chamadas.log`, `${args.join(" ")}\n`);
  const real = process.env.KEEP_E2E_REAL_KEEP;
  if (args[0] === "ia" && ["contas", "uso", "ordem"].includes(args[1]) && real && fs.existsSync(real)) {
    const run = spawnSync(real, args, { stdio: ["ignore", "pipe", "pipe"], env: process.env, windowsHide: true });
    process.stdout.write(run.stdout ?? "");
    if (stateFile && run.status !== 0) fs.appendFileSync(`${stateFile}.chamadas.log`, `  -> ${run.status}: ${run.stderr ?? ""}\n`);
    process.exit(run.status ?? 1);
  }
  let state = {};
  try {
    state = JSON.parse(fs.readFileSync(stateFile, "utf8"));
  } catch {
    state = {};
  }
  const [code, body] = answer(args, state);
  if (stateFile) fs.writeFileSync(stateFile, JSON.stringify(state, null, 2));
  process.stdout.write(JSON.stringify({ versao: 1, ...body }).replace('"@BORN@"', BORN) + "\n");
  process.exit(code);
}

if (mode === "casa") home(rest[0]);
else if (mode === "servidor") serve(rest[0]);
else if (mode === "keep") core(rest);
else {
  console.error("uso: node ia-falso.mjs casa <dir> | servidor <arquivo> | keep <args…>");
  process.exit(2);
}
