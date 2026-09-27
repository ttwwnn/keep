#!/usr/bin/env python3
# Ajudante falso: comportamento dirigido por arquivos em $FALSO_DIR.
import json, os, sys, time
d = os.environ["FALSO_DIR"]
log = open(os.path.join(d, "chamadas.log"), "a")
def vivo(pid):
    try: os.kill(int(pid), 0); return True
    except Exception: return False
fp = os.environ.get("FALSO_PID")
log.write(json.dumps({"argv": sys.argv[1:], "socket": os.environ.get("KEEP_SOCKET"), "pid_vivo": vivo(fp) if fp else None}) + "\n"); log.close()
cmd = sys.argv[1]
def resp(name, default):
    p = os.path.join(d, name)
    return json.load(open(p)) if os.path.exists(p) else default
if cmd == "listar":
    time.sleep(float(resp("demora-listar.json", 0)))
    print(json.dumps(resp("listar.json", {"versao": 1})))
elif cmd == "preparar":
    caminho = sys.argv[2]
    n = os.path.join(d, "preparar-" + os.path.basename(caminho) + ".json")
    seq = json.load(open(n)) if os.path.exists(n) else [{"versao":1,"ok":True}]
    r = seq.pop(0) if len(seq) > 1 else seq[0]
    json.dump(seq, open(n, "w"))
    print(json.dumps(r))
elif cmd == "concluir":
    print(json.dumps({"versao":1,"ok":os.path.exists(sys.argv[2]) is False, "motivo": "ainda existe" if os.path.exists(sys.argv[2]) else None}))
else:
    sys.exit(3)
