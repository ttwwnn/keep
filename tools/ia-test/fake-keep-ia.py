#!/usr/bin/env python3
"""A stand-in for `keep ia`, the core the app asks about AI accounts, for the
app's tests.

Called as the app calls the `keep` inside it — `ia` first, then the command
(the `ia` may also be left out) — it answers as the contract has it
(docs/ia.md): `--json`, one object printed, "versao": 1, exit 0 for ok and 1
for not. Out of a home and a directory of the test's own, never the real ones:

  ordem --json
  ordem mover <key> cima|baixo --json
      the order in <home>/.claude/contas/.ordem, with every account the home
      holds that it does not list yet put at its end, the key moved one
      place, written back whole (a new file, as the core writes it)
  entrar claude|gpt --ws=<W> --json
      a new tab in W, opened with the client named in FAKE_IA_KEEP, and the
      login the core would have made there: a slot "nova" in the vault, or a
      folder "nova" in .codex-contas, put at the end of the order
  trocar --ws=<W> --aba=<N> --para=<key> [--interromper] --json
      the tab's account written into $FAKE_IA_DIR/abas.json
  abas --json
      the "ia" list of $FAKE_IA_DIR/abas.json, said to be about the daemon
      at KEEP_SOCKET: its process and start, asked of it with the third list
      (or FAKE_IA_INICIO_MS, when set, and process 0)
  uso [--agora|--cache|--conta=<key>] --json
      the real core's answer when FAKE_IA_CORE names a `keep` (it reads the
      same made-up home, and the stand-in services the test points it at);
      else $FAKE_IA_DIR/uso.json, or no accounts at all
  jev [--agora|--cache] --json
      the real core's answer when FAKE_IA_CORE names a `keep`; else
      $FAKE_IA_DIR/jev.json, or `"jev": null` (no key of the Jev's here)
  sincronizar --json
      a round that moved nothing

The commands that change something (ordem mover, entrar, trocar) are logged,
one JSON line each, to $FAKE_IA_DIR/calls.jsonl: their arguments (without
the `ia`, and whether it came: "grupo"), the socket they were told to use
and the home (KEEP_IA_HOME); the ones that only read (ordem, uso, abas,
sincronizar), to $FAKE_IA_DIR/reads.jsonl — the app asks those on its own,
every few seconds, and a test counting what was asked of it counts the first.
Files in $FAKE_IA_DIR change how it answers:
  lento     seconds to wait before answering anything that changes something
  falha     `ordem mover` says no
  ocupada   `trocar` without --interromper says the tab is busy
  precisa-login
            `trocar` opens a tab in the workspace, as the core does for an
            account with no login of its own for tabs yet, and says so: the
            motivo "precisa-login", with "aba_login" and "ws_login"
  erro      `trocar` says no, with this file's text as the reason

The home is KEEP_IA_HOME — the one the app hands its `keep` when it reads a
made-up home — or else KEEP_AI_USAGE_HOME.
"""

import base64
import json
import os
import re
import socket
import struct
import subprocess
import sys
import time

DIR = os.environ.get("FAKE_IA_DIR") or os.path.dirname(os.path.abspath(__file__))
HOME = os.environ.get("KEEP_IA_HOME") or os.environ.get("KEEP_AI_USAGE_HOME") or "/nonexistent"
VAULT = os.path.join(HOME, ".claude", "contas")
ORDER = os.path.join(VAULT, ".ordem")
TABS = os.path.join(DIR, "abas.json")
KEY = re.compile(r"^(claude|gpt):[^\s/:]+$")
READS = ("uso", "jev", "abas", "sincronizar")


def flag(name):
    return os.path.join(DIR, name)


def log(argv, group):
    reads = (bool(argv) and argv[0] in READS) or argv == ["ordem", "--json"]
    with open(flag("reads.jsonl" if reads else "calls.jsonl"), "a", encoding="utf-8") as out:
        out.write(json.dumps({"argv": argv, "grupo": group, "socket": os.environ.get("KEEP_SOCKET"),
                              "casa": os.environ.get("KEEP_IA_HOME")}) + "\n")


def answer(body, ok=True):
    body = dict(body)
    body.setdefault("versao", 1)
    body["ok"] = ok
    print(json.dumps(body, ensure_ascii=False))
    sys.exit(0 if ok else 1)


def refuse(motivo, detalhe):
    answer({"motivo": motivo, "detalhe": detalhe}, ok=False)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def write_atomically(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    temporary = path + ".tmp-%d" % os.getpid()
    with open(temporary, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(temporary, path)


def slot_name(path):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read().strip() or None
    except OSError:
        return None


# ------------------------------------------------------------------ accounts


def accounts_in_todays_order():
    """Every account's key, Claude first — the preferred one, then by name,
    one key per login — then GPT: the principal, then the extras by name."""
    active = slot_name(os.path.join(VAULT, ".ativa"))
    preferred = slot_name(os.path.join(VAULT, ".preferida"))
    by_owner = {}
    try:
        names = sorted(os.listdir(VAULT))
    except OSError:
        names = []
    for name in names:
        if not name.endswith(".json") or name.startswith("."):
            continue
        slot = read_json(os.path.join(VAULT, name)) or {}
        alias = slot.get("apelido") or name[: -len(".json")]
        owner = slot.get("accountUuid") or slot.get("email") or alias
        by_owner.setdefault(owner, []).append(alias)
    claude = []
    for aliases in by_owner.values():
        # One key per login: the one the tabs know it by.
        shown = next((a for a in aliases if a == active), None) or next(
            (a for a in aliases if a == preferred), None) or sorted(aliases)[0]
        claude.append((0 if preferred in aliases else 1, shown))
    keys = ["claude:" + alias for _, alias in sorted(claude)]
    if os.path.exists(os.path.join(HOME, ".codex", "auth.json")):
        keys.append("gpt:principal")
    extras = os.path.join(HOME, ".codex-contas")
    try:
        for name in sorted(os.listdir(extras)):
            if not name.startswith(".") and os.path.exists(os.path.join(extras, name, "auth.json")):
                keys.append("gpt:" + name)
    except OSError:
        pass
    return keys


def full_order():
    listed = []
    try:
        with open(ORDER, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and line not in listed:
                    listed.append(line)
    except OSError:
        pass
    return listed + [key for key in accounts_in_todays_order() if key not in listed]


def jwt(claims):
    part = base64.urlsafe_b64encode(json.dumps(claims).encode()).decode().rstrip("=")
    return "h." + part + ".s"


def daemon_info():
    """The daemon's process and start, as its third list says them: the
    frame's first twelve bytes are its pid and its start in unix ms."""
    if os.environ.get("FAKE_IA_INICIO_MS"):
        return 0, int(os.environ["FAKE_IA_INICIO_MS"])
    path = os.environ.get("KEEP_SOCKET")
    if not path:
        return None
    try:
        c = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        c.settimeout(3)
        c.connect(path)
        c.sendall(bytes([0x0E]) + struct.pack(">I", 0))
        head = b""
        while len(head) < 5:
            part = c.recv(5 - len(head))
            if not part:
                return None
            head += part
        body = b""
        while len(body) < 12:
            part = c.recv(12 - len(body))
            if not part:
                return None
            body += part
        c.close()
        if head[0] != 0x9C:
            return None
        return struct.unpack(">IQ", body)
    except OSError:
        return None


# ------------------------------------------------------------------ commands


def ordem(argv):
    if argv[1:2] == ["mover"]:
        if len(argv) < 4 or not KEY.match(argv[2]) or argv[3] not in ("cima", "baixo"):
            sys.exit(2)
        if os.path.exists(flag("falha")):
            refuse("erro", "falha simulada do keep ia")
        order = full_order()
        key = argv[2]
        if key not in order:
            order.append(key)
        at = order.index(key)
        to = at - 1 if argv[3] == "cima" else at + 1
        if 0 <= to < len(order):
            order[at], order[to] = order[to], order[at]
            write_atomically(ORDER, "\n".join(order) + "\n")
        answer({"ordem": order})
    answer({"ordem": full_order()})


def option(argv, name):
    for item in argv:
        if item.startswith("--" + name + "="):
            return item.split("=", 1)[1]
    return None


def entrar(argv):
    service = argv[1] if len(argv) > 1 else ""
    workspace = option(argv, "ws")
    if service not in ("claude", "gpt") or not workspace:
        sys.exit(2)
    keep = os.environ.get("FAKE_IA_KEEP")
    if not keep:
        refuse("erro", "FAKE_IA_KEEP não foi dado")
    made = subprocess.run([keep, "new", workspace], capture_output=True, text=True)
    found = re.search(r"opened tab (\d+)", made.stdout)
    if not found:
        refuse("erro", "não abri a aba: " + (made.stderr or made.stdout).strip())
    now = time.time()
    if service == "gpt":
        folder = os.path.join(HOME, ".codex-contas", "nova")
        os.makedirs(folder, exist_ok=True)
        claims = {"exp": int(now + 86400),
                  "https://api.openai.com/profile": {"email": "nova@exemplo.com"},
                  "https://api.openai.com/auth": {"chatgpt_plan_type": "plus",
                                                  "chatgpt_account_id": "ACC-NOVA"}}
        body = {"auth_mode": "chatgpt",
                "tokens": {"access_token": jwt(claims), "account_id": "ACC-NOVA",
                           "refresh_token": "never-used"}}
        write_atomically(os.path.join(folder, "auth.json"), json.dumps(body))
        new_key = "gpt:nova"
    else:
        body = {"apelido": "nova", "email": "nova@exemplo.com", "accountUuid": "U-NOVA",
                "credenciais": {"claudeAiOauth": {
                    "accessToken": "tok-nova", "refreshToken": "never-used",
                    "expiresAt": int((now + 5 * 3600) * 1000),
                    "subscriptionType": "max", "rateLimitTier": "default_claude_max_20x"}}}
        write_atomically(os.path.join(VAULT, "nova.json"), json.dumps(body))
        new_key = "claude:nova"
    if os.path.exists(ORDER):
        order = full_order()
        if new_key not in order:
            order.append(new_key)
        write_atomically(ORDER, "\n".join(order) + "\n")
    answer({"ws": workspace, "aba": int(found.group(1))})


def trocar(argv):
    workspace, tab, key = option(argv, "ws"), option(argv, "aba"), option(argv, "para")
    if not workspace or not tab or not tab.isdigit() or not key or not KEY.match(key):
        sys.exit(2)
    if os.path.exists(flag("ocupada")) and "--interromper" not in argv:
        refuse("ocupada", "A aba está no meio de uma resposta.")
    if os.path.exists(flag("precisa-login")):
        keep = os.environ.get("FAKE_IA_KEEP")
        made = subprocess.run([keep, "new", workspace], capture_output=True, text=True) if keep else None
        found = re.search(r"opened tab (\d+)", made.stdout) if made else None
        if not found:
            refuse("erro", "não abri a aba do login")
        answer({"motivo": "precisa-login", "aba_login": int(found.group(1)), "ws_login": workspace,
                "detalhe": "A conta ainda não tem o login próprio das abas fixas. Abri a aba do login: "
                           "aprove lá e esta aba passa para a conta sozinha."}, ok=False)
    if os.path.exists(flag("erro")):
        with open(flag("erro"), encoding="utf-8") as f:
            refuse("erro", f.read().strip() or "erro simulado")
    tabs = read_json(TABS)
    if isinstance(tabs, dict):
        entries = [e for e in tabs.get("ia") or []
                   if not (e.get("workspace") == workspace and e.get("aba") == int(tab))]
        entries.append({"workspace": workspace, "aba": int(tab),
                        "agente": "codex" if key.startswith("gpt:") else "claude",
                        "conta": key, "atual": None if key == "claude:ordem" else key,
                        "vinculo": "exato"})
        tabs["ia"] = entries
        write_atomically(TABS, json.dumps(tabs))
    answer({"feito": "aba %s/%s em %s" % (workspace, tab, key)})


def abas(argv):
    tabs = read_json(TABS) or {}
    info = daemon_info()
    if info is None:
        refuse("erro", "o daemon não respondeu à terceira lista")
    pid, start = info
    answer({"keepd": {"pid": pid, "inicioMs": start}, "exato": True, "ia": tabs.get("ia") or []})


def uso(argv):
    core = os.environ.get("FAKE_IA_CORE")
    if core:
        os.execv(core, [core, "ia"] + argv + ["--json"])
    saved = read_json(flag("uso.json"))
    if isinstance(saved, dict):
        answer(saved)
    answer({"linhas": [], "ordem": [], "gerenteExterno": False, "medidoEm": time.time()})


def jev(argv):
    core = os.environ.get("FAKE_IA_CORE")
    if core:
        os.execv(core, [core, "ia"] + argv + ["--json"])
    saved = read_json(flag("jev.json"))
    if isinstance(saved, dict):
        answer(saved)
    answer({"jev": None, "medidoEm": time.time()})


def sincronizar(argv):
    answer({"estado": "feito", "alvo": None, "alteradas": [], "pendentes": []})


def main():
    argv = sys.argv[1:]
    group = argv[0] if argv[:1] == ["ia"] else None
    if group:
        argv = argv[1:]
    log(argv, group)
    if "--json" not in argv or not argv:
        sys.exit(2)
    argv = [item for item in argv if item != "--json"]
    if argv[0] not in READS:
        try:
            with open(flag("lento"), encoding="utf-8") as f:
                time.sleep(float(f.read().strip() or "0"))
        except (OSError, ValueError):
            pass
    commands = {"ordem": ordem, "entrar": entrar, "trocar": trocar,
                "abas": abas, "uso": uso, "jev": jev, "sincronizar": sincronizar}
    if argv[0] not in commands:
        sys.exit(2)
    commands[argv[0]](argv)


if __name__ == "__main__":
    main()
