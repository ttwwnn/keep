#!/usr/bin/env python3
# `keep worktrees` falso: comportamento dirigido por arquivos em $FALSO_DIR.
# Chamado como o app chama o `keep` de dentro dele: `worktrees` primeiro.
# O registro guarda os argumentos sem ele, e se ele veio ("grupo").
import json, os, sys, time
d = os.environ["FALSO_DIR"]
args = sys.argv[1:]
grupo = args[0] if args[:1] == ["worktrees"] else None
if grupo:
    args = args[1:]
log = open(os.path.join(d, "chamadas.log"), "a")
def vivo(pid):
    try: os.kill(int(pid), 0); return True
    except Exception: return False
fp = os.environ.get("FALSO_PID")
log.write(json.dumps({"argv": args, "grupo": grupo, "socket": os.environ.get("KEEP_SOCKET"),
                      "pid_vivo": vivo(fp) if fp else None}) + "\n"); log.close()
cmd = args[0]
def resp(name, default):
    p = os.path.join(d, name)
    return json.load(open(p)) if os.path.exists(p) else default
if cmd == "listar":
    time.sleep(float(resp("demora-listar.json", 0)))
    print(json.dumps(resp("listar.json", {"versao": 1})))
elif cmd == "preparar":
    caminho = args[1]
    n = os.path.join(d, "preparar-" + os.path.basename(caminho) + ".json")
    seq = json.load(open(n)) if os.path.exists(n) else [{"versao":1,"ok":True}]
    r = seq.pop(0) if len(seq) > 1 else seq[0]
    json.dump(seq, open(n, "w"))
    # "_saida": a recusa dita com a saída 1 do contrato, em vez da 0
    saida = r.pop("_saida", 0)
    print(json.dumps(r))
    sys.exit(saida)
elif cmd == "concluir":
    print(json.dumps({"versao":1,"ok":os.path.exists(args[1]) is False, "motivo": "ainda existe" if os.path.exists(args[1]) else None}))
elif cmd == "indexar":
    print(json.dumps({"versao": 1, "novas": 0}))
else:
    sys.exit(3)
