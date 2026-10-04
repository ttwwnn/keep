//! A lixeira de worktrees de ponta a ponta: repositórios git de verdade em
//! pasta temporária, transcritos do Claude e rollouts do Codex sintéticos
//! com horários em volta do nascimento real de cada worktree, e um keepd de
//! mentira (o próprio teste) quando precisa da lista. Nada aqui toca no
//! keepd, no `~/.claude` nem no estado de verdade: cada teste tem o seu
//! mundo (`Ambiente::de_teste`), sem variável de ambiente nenhuma.
//!
//! Portado do `teste_worktrees.py` do kit-mac: os nomes `tNN_…` são os de
//! lá. Os processos que fazem de shell, de Claude, de Codex e de órfão são o
//! próprio executável de teste (`papel`), para valer igual nos três sistemas.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keep_ia::processos;
use keep_ia::worktrees::abas::Contexto;
use keep_ia::worktrees::dono::Decisao;
use keep_ia::worktrees::prazo::Prazo;
use keep_ia::worktrees::sessoes::SessaoViva;
use keep_ia::worktrees::texto::{agora, iso, ts};
use keep_ia::worktrees::{
    self, AbaListada, Ambiente, Listagem, Mantida, analisa_alvo, estado, listar, pastas, repos, varredura,
};
use keep_proto::net::Listener;
use keep_proto::{ClientMsg, DaemonInfo, ServerMsg, TabInfo, WorkspaceInfo};
use serde_json::{Value, json};

// ---------------------------------------------------------------- processos de teste

/// Os papéis que o próprio executável de teste faz quando chamado com
/// `KEEP_WT_TESTE_PAPEL`: `dorme` (um processo qualquer: o Claude, o Codex,
/// um órfão) e `concha` (um shell que obedece a ordens num arquivo).
#[test]
#[ignore]
fn papel() {
    match std::env::var("KEEP_WT_TESTE_PAPEL").as_deref() {
        Ok("dorme") => std::thread::sleep(Duration::from_secs(90)),
        Ok("concha") => concha(),
        Ok("keepd-antigo") => keepd_antigo(),
        _ => {}
    }
}

/// O shell de mentira: lê `KEEP_WT_TESTE_ORDENS` a cada 20 ms. `cd <pasta>`,
/// `sobe <arg0> <arquivo do pid> [args…]` (um filho que dorme), `mata` (o
/// filho). Responde criando `<ordens>.feito`.
fn concha() {
    let ordens = PathBuf::from(std::env::var("KEEP_WT_TESTE_ORDENS").unwrap());
    let feito = PathBuf::from(format!("{}.feito", ordens.display()));
    let mut filho: Option<Child> = None;
    let fim = Instant::now() + Duration::from_secs(120);
    while Instant::now() < fim {
        if let Ok(txt) = std::fs::read_to_string(&ordens) {
            std::fs::remove_file(&ordens).ok();
            for linha in txt.lines() {
                let (cmd, resto) = linha.split_once(' ').unwrap_or((linha, ""));
                match cmd {
                    "cd" => {
                        std::env::set_current_dir(resto).ok();
                    }
                    "mata" | "sai" => {
                        if let Some(mut f) = filho.take() {
                            f.kill().ok();
                            f.wait().ok();
                        }
                        if cmd == "sai" {
                            std::fs::write(&feito, "").ok();
                            return;
                        }
                    }
                    "sobe" => {
                        let p: Vec<&str> = resto.split(' ').collect();
                        let f = sobe_processo("dorme", None, Some(p[0]), &p[2..], &[]);
                        std::fs::write(p[1], f.id().to_string()).ok();
                        filho = Some(f);
                    }
                    _ => {}
                }
            }
            std::fs::write(&feito, "").ok();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if let Some(mut f) = filho {
        f.kill().ok();
    }
}

/// Um keepd de antes do `List3`, num processo só dele: os filhos dele são só
/// o shell da aba (`KEEP_WT_TESTE_PASTA`), e a lista é a de
/// `KEEP_WT_TESTE_ABAS` (`[{ws, id, title, cwd}]`), relida a cada pergunta.
/// O `List3` ele não conhece: fecha a conexão.
fn keepd_antigo() {
    let var = |n: &str| std::env::var(n).unwrap();
    let concha = sobe_processo(
        "concha",
        Some(Path::new(&var("KEEP_WT_TESTE_PASTA"))),
        None,
        &[],
        &[("KEEP_WT_TESTE_ORDENS", var("KEEP_WT_TESTE_ORDENS"))],
    );
    std::fs::write(var("KEEP_WT_TESTE_PID"), concha.id().to_string()).unwrap();
    let abas = PathBuf::from(var("KEEP_WT_TESTE_ABAS"));
    let l = Listener::bind(var("KEEP_WT_TESTE_SOCKET")).unwrap();
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(90));
        std::process::exit(0);
    });
    for conexao in l.incoming() {
        let Ok(mut s) = conexao else { break };
        if let Ok(Some(ClientMsg::List2)) = ClientMsg::read(&mut s) {
            let lista: Vec<Value> = serde_json::from_slice(&std::fs::read(&abas).unwrap_or_default()).unwrap_or_default();
            let mut ws: Vec<WorkspaceInfo> = Vec::new();
            for a in lista {
                let nome = a["ws"].as_str().unwrap_or("W").to_string();
                let info = TabInfo {
                    id: a["id"].as_u64().unwrap_or(1) as u32,
                    title: a["title"].as_str().unwrap_or("").into(),
                    cwd: a["cwd"].as_str().unwrap_or("").into(),
                    ..TabInfo::default()
                };
                match ws.iter_mut().find(|w| w.name == nome) {
                    Some(w) => w.tabs.push(info),
                    None => ws.push(WorkspaceInfo { name: nome, tabs: vec![info] }),
                }
            }
            ServerMsg::Workspaces2(ws).write(&mut s).ok();
        }
    }
}

/// O executável de teste com um nome (`arg0`): no unix pelo `argv[0]`; no
/// Windows por uma cópia dele com esse nome.
fn executavel(arg0: Option<&str>) -> (PathBuf, Option<String>) {
    let exe = std::env::current_exe().unwrap();
    match arg0 {
        None => (exe, None),
        Some(nome) if cfg!(windows) => {
            // Uma cópia por nome, feita uma vez: duas threads copiando o mesmo
            // arquivo dariam a uma delas um executável pela metade.
            static COPIAS: Mutex<()> = Mutex::new(());
            let _vez = COPIAS.lock().unwrap_or_else(|e| e.into_inner());
            let pasta = std::env::temp_dir().join(format!("keep-wt-exe-{}", std::process::id()));
            std::fs::create_dir_all(&pasta).unwrap();
            let copia = pasta.join(format!("{nome}.exe"));
            if !copia.exists() {
                let tmp = pasta.join(format!("{nome}.copiando"));
                std::fs::copy(&exe, &tmp).expect("copiar o executável de teste");
                std::fs::rename(&tmp, &copia).expect("dar o nome à cópia");
            }
            (copia, None)
        }
        Some(nome) => (exe, Some(nome.to_string())),
    }
}

fn sobe_processo(papel: &str, cwd: Option<&Path>, arg0: Option<&str>, extra: &[&str], vars: &[(&str, String)]) -> Child {
    let (exe, nome) = executavel(arg0);
    let mut c = Command::new(exe);
    #[cfg(unix)]
    if let Some(n) = nome {
        use std::os::unix::process::CommandExt;
        c.arg0(n);
    }
    #[cfg(not(unix))]
    let _ = nome;
    c.args(["--exact", "papel", "--ignored", "-q", "--test-threads=1"])
        .args(extra)
        .env("KEEP_WT_TESTE_PAPEL", papel)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in vars {
        c.env(k, v);
    }
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    c.spawn().expect("subir o processo de teste")
}

/// Um processo que só dorme (com a pasta atual em `cwd`).
fn dorme(cwd: Option<&Path>) -> Child {
    sobe_processo("dorme", cwd, None, &[], &[])
}

fn mata(mut c: Child) {
    c.kill().ok();
    c.wait().ok();
}

/// Espera `cond` por até `seg` segundos.
fn espera(seg: f64, mut cond: impl FnMut() -> bool) -> bool {
    let fim = Instant::now() + Duration::from_secs_f64(seg);
    while Instant::now() < fim {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cond()
}

/// Um shell de mentira com um "programa" dentro.
struct Concha {
    /// Quando o shell é filho deste processo (e não de um keepd de mentira).
    filho: Option<Child>,
    pid: u32,
    ordens: PathBuf,
    agente: Option<u32>,
}

impl Concha {
    fn nova(m: &Mundo, cwd: &Path) -> Concha {
        let ordens = m.base.join(format!("ordens-{}", uuid()));
        let filho = sobe_processo("concha", Some(cwd), None, &[], &[("KEEP_WT_TESTE_ORDENS", texto(&ordens))]);
        Concha { pid: filho.id(), filho: Some(filho), ordens, agente: None }
    }

    fn pid(&self) -> u32 {
        self.pid
    }

    fn manda(&self, txt: &str) {
        let feito = PathBuf::from(format!("{}.feito", self.ordens.display()));
        std::fs::remove_file(&feito).ok();
        let tmp = PathBuf::from(format!("{}.tmp", self.ordens.display()));
        std::fs::write(&tmp, txt).unwrap();
        std::fs::rename(&tmp, &self.ordens).unwrap();
        assert!(espera(20.0, || feito.exists()), "a concha não respondeu a {txt:?}");
    }

    /// Sobe o programa (`claude`, `codex`) e espera o processo dele.
    fn sobe(&mut self, arg0: &str, args: &[&str]) -> u32 {
        let pidf = self.ordens.with_extension(format!("pid-{}", uuid()));
        let mut linha = format!("sobe {arg0} {}", pidf.display());
        for a in args {
            linha.push(' ');
            linha.push_str(a);
        }
        self.manda(&linha);
        assert!(espera(10.0, || std::fs::read_to_string(&pidf).is_ok_and(|s| !s.is_empty())));
        let pid: u32 = std::fs::read_to_string(&pidf).unwrap().trim().parse().unwrap();
        assert!(espera(10.0, || processos::programa(pid).as_deref() == Some(arg0)
            || processos::um(pid).is_some_and(|p| p.nome.eq_ignore_ascii_case(arg0))));
        self.agente = Some(pid);
        pid
    }

    fn mata_agente(&mut self) {
        self.manda("mata");
        if let Some(p) = self.agente.take() {
            assert!(espera(10.0, || !processos::vivo(p)), "o agente não morreu");
        }
    }

    fn entra(&self, pasta: &str) {
        self.manda(&format!("cd {pasta}"));
        assert!(espera(10.0, || {
            processos::cwd(self.pid()).is_some_and(|c| pastas::dentro(&pastas::canon(&c.to_string_lossy()), pasta))
        }));
    }

    /// Mata o programa e o shell.
    fn fim(self) {
        if processos::vivo(self.pid()) {
            self.manda("sai");
        }
        if let Some(f) = self.filho {
            mata(f);
        }
    }
}

// ---------------------------------------------------------------- o mundo

fn uuid() -> String {
    use std::hash::BuildHasher;
    static N: AtomicU64 = AtomicU64::new(0);
    let s = std::collections::hash_map::RandomState::new();
    let a = s.hash_one((N.fetch_add(1, Ordering::Relaxed), std::process::id(), Instant::now()));
    let b = s.hash_one((a, "keep"));
    let h = format!("{a:016x}{b:016x}");
    format!("{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

fn texto(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn sh(args: &[&str], cwd: Option<&Path>) -> String {
    let mut c = Command::new(worktrees::git::padrao());
    c.args(args).env("LC_ALL", "C");
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let o = c.output().expect("git");
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// `git -C repo …`
fn g(repo: &str, args: &[&str]) -> String {
    let mut a = vec!["-C", repo];
    a.extend_from_slice(args);
    sh(&a, None)
}

/// O nascimento do registro de uma worktree, como o `listar` o conta.
fn nascimento_do_registro(cam: &str) -> f64 {
    let gitdir = std::fs::read_to_string(Path::new(cam).join(".git")).unwrap();
    let adm = gitdir.trim().strip_prefix("gitdir:").unwrap().trim().to_string();
    repos::nascimento_do_registro_em(&adm).unwrap()
}

struct Mundo {
    _dir: tempfile::TempDir,
    base: PathBuf,
    amb: Ambiente,
    raiz: String,
    proj: PathBuf,
    sess: PathBuf,
    lixeira: PathBuf,
    codex: PathBuf,
    repo: String,
}

impl Mundo {
    fn novo() -> Mundo {
        let dir = tempfile::tempdir().unwrap();
        let base = PathBuf::from(pastas::canon(&texto(dir.path())));
        let amb = Ambiente::de_teste(&base, base.join("k.sock"));
        for d in ["projetos", "projects", "sessions", "estado", "Trash", "app", "codex"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let mut m = Mundo {
            raiz: texto(&base.join("projetos")),
            proj: base.join("projects"),
            sess: base.join("sessions"),
            lixeira: base.join("Trash"),
            codex: base.join("codex"),
            repo: String::new(),
            amb,
            base,
            _dir: dir,
        };
        m.repo = m.novo_repo("repo", false);
        m
    }

    fn novo_repo(&self, nome: &str, gitmodules: bool) -> String {
        let r = Path::new(&self.raiz).join(nome);
        std::fs::create_dir_all(&r).unwrap();
        let r = texto(&r);
        g(&r, &["init", "-q", "-b", "main"]);
        g(&r, &["config", "user.email", "t@t"]);
        g(&r, &["config", "user.name", "t"]);
        g(&r, &["config", "core.autocrlf", "false"]);
        std::fs::write(Path::new(&r).join("a.txt"), "a\n").unwrap();
        g(&r, &["add", "a.txt"]);
        if gitmodules {
            std::fs::write(Path::new(&r).join(".gitmodules"), "[submodule \"sub\"]\n\tpath = sub\n\turl = ../sub\n").unwrap();
            g(&r, &["add", ".gitmodules"]);
        }
        g(&r, &["commit", "-q", "-m", "inicial"]);
        pastas::canon(&r)
    }

    /// Uma worktree nova: (caminho, nascimento).
    fn wt(&self, nome: &str) -> (String, f64) {
        self.wt_em(nome, None, None, None)
    }

    fn wt_em(&self, nome: &str, repo: Option<&str>, ramo: Option<&str>, onde: Option<&Path>) -> (String, f64) {
        let repo = repo.unwrap_or(&self.repo);
        let cam = onde.map(texto).unwrap_or_else(|| texto(&Path::new(&self.raiz).join(nome)));
        let mut a = vec!["worktree", "add", "-q"];
        match ramo {
            Some(r) => a.extend(["-b", r]),
            None => a.push("--detach"),
        }
        a.extend([cam.as_str(), "HEAD"]);
        g(repo, &a);
        let cam = pastas::canon(&cam);
        let t = nascimento_do_registro(&cam);
        (cam, t)
    }

    fn com_vivas(&self, vivas: Vec<SessaoViva>) -> Ambiente {
        let mut amb = self.amb.clone();
        amb.ganchos.vivas = Some(vivas);
        amb
    }

    fn indexar(&self, agora: f64) -> worktrees::Indexacao {
        self.indexar_com(agora, Vec::new())
    }

    fn indexar_com(&self, agora: f64, vivas: Vec<SessaoViva>) -> worktrees::Indexacao {
        worktrees::indexar_em(&self.com_vivas(vivas), Some(agora))
    }

    /// O `indexar` com as sessões vivas lidas de `sessions/` (as dos
    /// processos de teste): é por elas que o histórico do shell se grava.
    fn indexar_de_verdade(&self, agora: f64) -> worktrees::Indexacao {
        worktrees::indexar_em(&self.amb, Some(agora))
    }

    fn registro(&self) -> BTreeMap<String, Decisao> {
        estado::carrega(&self.amb).map(|e| e.nascimentos).unwrap_or_default()
    }

    fn decisao_de(&self, cam: &str) -> Decisao {
        let ds: Vec<Decisao> = self.registro().into_values().filter(|d| d.caminho.as_deref() == Some(cam)).collect();
        assert_eq!(ds.len(), 1, "esperava 1 decisão para {cam}: {ds:?}");
        ds.into_iter().next().unwrap()
    }

    /// `listar` com o vínculo aba → sessões já dado (o vínculo tem testes
    /// próprios, t22…).
    fn listar_com(&self, sessoes: &[&str], o: Op) -> (Listagem, f64) {
        let fechando: HashSet<u32> = o.fechando.iter().copied().collect();
        let ctx = Contexto {
            conchas_fechando: fechando.clone(),
            conchas_keepd: fechando.iter().chain(o.outras.iter()).copied().collect(),
            rotulos: o.rotulos.iter().map(|(p, a, w)| (*p, (a.to_string(), w.to_string()))).collect(),
            extras: Vec::new(),
            daemon: None,
        };
        let exato = o.vinculo == "exato";
        let aba = AbaListada {
            alvo: "W:1".into(),
            agente: "claude".into(),
            vinculo: o.vinculo.into(),
            sessoes: if exato { sessoes.iter().map(|s| s.to_string()).collect() } else { Vec::new() },
            pids: Vec::new(),
        };
        let mut amb = self.com_vivas(o.vivas);
        if let Some(g) = o.git {
            amb.git = g;
        }
        amb.ganchos.decidir_lento = o.decidir_lento;
        amb.ganchos.vinculo = Some(Arc::new(move |_: &[worktrees::Alvo]| (vec![aba.clone()], ctx.clone())));
        let p = match o.rapido {
            Some(r) => Prazo::com_alvo(o.prazo, r),
            None => Prazo::de(o.prazo),
        };
        let r = Mutex::new(Listagem::nova());
        let t0 = Instant::now();
        listar::listar(&amb, &[analisa_alvo("W:1").unwrap()], &p, &r).ok();
        let dt = t0.elapsed().as_secs_f64();
        (r.into_inner().unwrap(), dt)
    }

    fn listar(&self, sessoes: &[&str]) -> Listagem {
        self.listar_com(sessoes, Op::default()).0
    }
}

struct Op {
    vivas: Vec<SessaoViva>,
    fechando: Vec<u32>,
    outras: Vec<u32>,
    rotulos: Vec<(u32, &'static str, &'static str)>,
    prazo: f64,
    vinculo: &'static str,
    rapido: Option<f64>,
    git: Option<PathBuf>,
    decidir_lento: bool,
}

impl Default for Op {
    fn default() -> Op {
        Op {
            vivas: Vec::new(),
            fechando: Vec::new(),
            outras: Vec::new(),
            rotulos: Vec::new(),
            prazo: 20.0,
            vinculo: "exato",
            rapido: None,
            git: None,
            decidir_lento: false,
        }
    }
}

fn viva(sid: &str) -> SessaoViva {
    viva_em(sid, std::process::id())
}

fn viva_em(sid: &str, pid: u32) -> SessaoViva {
    SessaoViva { pid, id: sid.into(), kind: Some("interactive".into()), cwd: String::new(), job: None, parked: None }
}

fn caminhos(r: &Listagem) -> Vec<String> {
    let mut v: Vec<String> = r.lixeira.iter().map(|x| x.caminho.clone()).collect();
    v.sort();
    v
}

fn mantida(caminho: &str, motivo: &str) -> Mantida {
    Mantida { caminho: caminho.into(), motivo: motivo.into() }
}

// ---------------------------------------------------------------- transcritos e rollouts

/// Um transcrito do Claude Code.
struct Conversa {
    sid: String,
    arq: PathBuf,
    cwd: String,
    k: u32,
}

#[derive(Default)]
struct Ch<'a> {
    res: Option<&'a str>,
    nome: Option<&'a str>,
    bg: bool,
    cwd: Option<&'a str>,
    entrada: Option<Value>,
}

impl Conversa {
    fn nova(m: &Mundo) -> Conversa {
        Conversa::com(m, None, None)
    }

    fn com(m: &Mundo, sid: Option<&str>, cwd: Option<&str>) -> Conversa {
        let sid = sid.map(str::to_string).unwrap_or_else(uuid);
        let d = m.proj.join("-p");
        std::fs::create_dir_all(&d).unwrap();
        Conversa { arq: d.join(format!("{sid}.jsonl")), sid, cwd: cwd.map(str::to_string).unwrap_or(m.raiz.clone()), k: 0 }
    }

    fn linha(&self, o: Value) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.arq).unwrap();
        writeln!(f, "{}", serde_json::to_string(&o).unwrap()).unwrap();
    }

    fn chamada(&mut self, t0: f64, t1: Option<f64>, cmd: &str) -> String {
        self.chamada_x(t0, t1, cmd, Ch::default())
    }

    fn chamada_x(&mut self, t0: f64, t1: Option<f64>, cmd: &str, x: Ch) -> String {
        self.k += 1;
        let tid = format!("toolu_{}_{}", &self.sid[..8], self.k);
        let cwd = x.cwd.map(str::to_string).unwrap_or(self.cwd.clone());
        let inp = x.entrada.unwrap_or_else(|| json!({"command": cmd, "description": "passo"}));
        self.linha(json!({"type": "assistant", "timestamp": iso(t0), "cwd": cwd, "sessionId": self.sid,
            "message": {"role": "assistant", "content": [{"type": "tool_use", "id": tid,
                "name": x.nome.unwrap_or("Bash"), "input": inp}]}}));
        if let Some(t1) = t1 {
            let (txt, tur) = if x.bg {
                let dir = self.arq.parent().unwrap().display().to_string();
                (
                    format!("Command running in background with ID: b{0}. Output is being written to: {dir}/b{0}.output", self.k),
                    json!({"backgroundTaskId": format!("b{}", self.k)}),
                )
            } else {
                let res = x.res.unwrap_or("ok");
                (res.to_string(), json!({"stdout": res, "stderr": ""}))
            };
            self.linha(json!({"type": "user", "timestamp": iso(t1), "cwd": cwd, "sessionId": self.sid,
                "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": tid, "content": txt}]},
                "toolUseResult": tur}));
        }
        tid
    }

    fn notificacao(&self, t: f64, tid: &str) {
        self.linha(json!({"type": "queue-operation", "operation": "enqueue", "timestamp": iso(t), "sessionId": self.sid,
            "content": format!("<task-notification>\n<task-id>x</task-id>\n<tool-use-id>{tid}</tool-use-id>\n<status>completed</status>\n</task-notification>")}));
    }

    fn usuario(&self, t: f64, texto: &str) {
        self.linha(json!({"type": "user", "timestamp": iso(t), "cwd": self.cwd, "sessionId": self.sid,
            "message": {"role": "user", "content": texto}}));
    }

    fn texto(&self, t: f64, cwd: Option<&str>) {
        self.linha(json!({"type": "assistant", "timestamp": iso(t), "cwd": cwd.unwrap_or(&self.cwd),
            "sessionId": self.sid, "message": {"role": "assistant", "content": [{"type": "text", "text": "feito"}]}}));
    }

    fn titulo(&self, tit: &str) {
        self.linha(json!({"type": "ai-title", "aiTitle": tit, "sessionId": self.sid}));
    }
}

fn data_local(t: f64, formato: &str) -> String {
    chrono::DateTime::from_timestamp(t as i64, 0).unwrap().with_timezone(&chrono::Local).format(formato).to_string()
}

/// Um rollout do Codex no formato do 0.157.0: linhas `{timestamp, type,
/// payload}`; `session_meta`; `function_call` e `function_call_output` com o
/// envelope do unified exec.
struct Rollout {
    tid: String,
    arq: PathBuf,
    zst: bool,
    k: u32,
    linhas: Vec<String>,
}

impl Rollout {
    fn novo(m: &Mundo, t0: f64) -> Rollout {
        Rollout::com(m, t0, None, None, false)
    }

    fn com(m: &Mundo, t0: f64, tid: Option<&str>, pai: Option<&str>, zst: bool) -> Rollout {
        let tid = tid.map(str::to_string).unwrap_or_else(uuid);
        let d = m.codex.join("sessions").join(data_local(t0, "%Y")).join(data_local(t0, "%m")).join(data_local(t0, "%d"));
        std::fs::create_dir_all(&d).unwrap();
        let arq = d.join(format!("rollout-{}-{tid}.jsonl", data_local(t0, "%Y-%m-%dT%H-%M-%S")));
        let fonte = match pai {
            Some(p) => json!({"subagent": {"thread_spawn": {"parent_thread_id": p, "depth": 1}}}),
            None => json!("cli"),
        };
        let mut r = Rollout { tid: tid.clone(), arq, zst, k: 0, linhas: Vec::new() };
        r.linha(t0, "session_meta", json!({"id": tid, "timestamp": iso(t0), "cwd": m.raiz,
            "originator": "codex_cli_rs", "cli_version": "0.157.0", "source": fonte, "model_provider": "openai"}));
        r
    }

    fn linha(&mut self, t: f64, tipo: &str, payload: Value) {
        self.linhas.push(serde_json::to_string(&json!({"timestamp": iso(t), "type": tipo, "payload": payload})).unwrap());
        if self.zst {
            let tudo = self.linhas.join("\n") + "\n";
            let z = ruzstd::encoding::compress_to_vec(tudo.as_bytes(), ruzstd::encoding::CompressionLevel::Fastest);
            std::fs::write(format!("{}.zst", self.arq.display()), z).unwrap();
        } else {
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.arq).unwrap();
            writeln!(f, "{}", self.linhas.last().unwrap()).unwrap();
        }
    }

    fn chamada(&mut self, t: f64, cmd: &str) -> String {
        self.chamada_x(t, cmd, "exec_command", None)
    }

    fn chamada_x(&mut self, t: f64, cmd: &str, nome: &str, sessao: Option<u32>) -> String {
        self.k += 1;
        let cid = format!("call_{}_{}", &self.tid[..8], self.k);
        let args = if nome == "write_stdin" {
            json!({"session_id": sessao, "chars": cmd})
        } else {
            json!({"cmd": cmd, "workdir": "/tmp"})
        };
        self.linha(t, "response_item", json!({"type": "function_call", "name": nome,
            "arguments": serde_json::to_string(&args).unwrap(), "call_id": cid}));
        cid
    }

    fn saida(&mut self, t: f64, cid: &str, texto: &str, wall: f64, vivo: Option<u32>) {
        let estado = match vivo {
            Some(s) => format!("Process running with session ID {s}"),
            None => "Process exited with code 0".into(),
        };
        let out = format!("Chunk ID: 1a2b3c\nWall time: {wall:.4} seconds\n{estado}\nOriginal token count: 7\nOutput:\n{texto}");
        self.linha(t, "response_item", json!({"type": "function_call_output", "call_id": cid, "output": out}));
    }
}

// ---------------------------------------------------------------- dono

#[test]
fn t01_dono_a_pelo_nome() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-alfa");
    let (mut c1, mut c2) = (Conversa::nova(&m), Conversa::nova(&m));
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add -q {w} HEAD"));
    c2.chamada(t - 0.5, Some(t + 0.5), "ls -la"); // outra sessão ocupada no mesmo instante, sem prova
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()) && d.nivel.as_deref() == Some("A") && !d.provisorio, "{d:?}");
    let r = m.listar(&[&c1.sid]);
    assert_eq!(caminhos(&r), vec![w.clone()], "{r:?}");
    let x = serde_json::to_value(&r.lixeira[0]).unwrap();
    let campos: HashSet<&str> = x.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        campos,
        HashSet::from(["caminho", "repo", "id", "ramo", "head", "alteracoes", "commits_so_aqui", "sessao", "submodulos", "processos", "nasceu_ns"]),
        "os campos do contrato"
    );
    let x = &r.lixeira[0];
    assert!(x.repo == m.repo && x.id == "wt-alfa" && x.ramo.is_none() && x.sessao == c1.sid, "{x:?}");
    let head: String = g(&w, &["rev-parse", "HEAD"]).trim().chars().take(9).collect();
    assert!(x.head.as_deref() == Some(head.as_str()) && x.alteracoes == 0 && x.processos.is_empty(), "{x:?}");
    let r2 = m.listar(&[&c2.sid]);
    assert!(r2.lixeira.is_empty() && r2.avisos.is_empty(), "a outra aba não leva nada: {r2:?}");
}

#[test]
fn t02_dono_b_nome_em_molde_no_laco() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-lote-b");
    let mut c1 = Conversa::nova(&m);
    c1.chamada_x(t - 0.4, Some(t + 0.4), r#"for g in a b; do git worktree add -q "$R/wt-lote-$g" HEAD >/dev/null; done"#,
        Ch { res: Some(""), ..Ch::default() });
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()) && d.nivel.as_deref() == Some("B"), "{d:?}");
}

#[test]
fn t03_laco_que_imprime_o_caminho_vira_a() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-lote-c");
    let mut c1 = Conversa::nova(&m);
    let res = format!("ok {w} pronto\n");
    c1.chamada_x(t - 0.4, Some(t + 0.4), r#"for g in c d; do prepara.sh "$R/wt-lote-$g"; done"#,
        Ch { res: Some(&res), ..Ch::default() });
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()) && d.nivel.as_deref() == Some("A"), "{d:?}");
}

#[test]
fn t04_dois_com_prova_e_ambigua() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-disputa");
    let (mut c1, mut c2) = (Conversa::nova(&m), Conversa::nova(&m));
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    c2.chamada(t - 1.0, Some(t + 1.0), "git worktree list | grep wt-disputa");
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "ambigua" && d.dono.is_none(), "{d:?}");
    let r = m.listar(&[&c1.sid]);
    assert!(r.lixeira.is_empty(), "{r:?}");
    assert_eq!(r.avisos, vec![format!("possivelmente desta aba, não verificada: {w}")]);
}

#[test]
fn t05_so_indicio_e_indecidida() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-anonima");
    let (mut c1, c2) = (Conversa::nova(&m), Conversa::nova(&m));
    c1.chamada_x(t - 0.3, Some(t + 0.3), r#"git worktree add "$(mktemp -d)/x" HEAD"#, Ch { res: Some(""), ..Ch::default() });
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "indecidida" && d.niveis == BTreeMap::from([(c1.sid.clone(), "C".to_string())]), "{d:?}");
    let r = m.listar(&[&c1.sid]);
    assert!(r.lixeira.is_empty() && r.avisos == vec![format!("possivelmente desta aba, não verificada: {w}")], "{r:?}");
    assert!(m.listar(&[&c2.sid]).avisos.is_empty(), "aba sem indício não recebe aviso");
}

#[test]
fn t06_ninguem_rodando_e_sem_dono() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-solta");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 60.0, Some(t - 50.0), &format!("git worktree add {w} HEAD")); // citou, mas acabou antes
    m.indexar(t + 60.0);
    assert_eq!(m.decisao_de(&w).estado, "sem-dono");
    let r = m.listar(&[&c1.sid]);
    assert!(r.lixeira.is_empty() && r.mantidas.is_empty() && r.avisos.is_empty(), "nada a mover nem avisar: {r:?}");
}

#[test]
fn t07_id_diferente_do_nome_worktree_movida() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-original");
    let mut c1 = Conversa::nova(&m);
    c1.chamada_x(t - 0.3, Some(t + 0.3), "W=$SP/wt-original; git worktree add -q $W HEAD", Ch { res: Some(""), ..Ch::default() });
    let novo = Path::new(&m.raiz).join("sub").join("respostas-2609");
    std::fs::create_dir_all(novo.parent().unwrap()).unwrap();
    g(&m.repo, &["worktree", "move", &w, &texto(&novo)]);
    let novo = pastas::canon(&texto(&novo));
    m.indexar(t + 60.0);
    let d = m.decisao_de(&novo);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()) && d.id.as_deref() == Some("wt-original"), "{d:?}");
    let r = m.listar(&[&c1.sid]);
    assert!(caminhos(&r) == vec![novo.clone()] && r.lixeira[0].id == "wt-original", "{r:?}");
}

#[test]
fn t08_fronteira_de_palavra() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-f-e");
    let (mut c1, mut c2) = (Conversa::nova(&m), Conversa::nova(&m));
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {}/wt-f-e2 HEAD", m.raiz)); // outra worktree (e2)
    c2.chamada(t - 0.3, Some(t + 0.3), &format!("ls {}/wt-f-e/", m.raiz));
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c2.sid.as_str()), "{d:?}");
    assert_eq!(d.niveis.get(&c1.sid).map(String::as_str), Some("C"), "{d:?}");
}

#[test]
fn t09_chamada_de_outra_maquina_nao_conta() {
    // A pasta de uma chamada feita em outro computador (o transcrito veio de lá).
    let de_fora = if cfg!(target_os = "macos") {
        "/home/ttwwnn/Projetos/777leads"
    } else if cfg!(windows) {
        "/home/ttwwnn/Projetos/777leads"
    } else {
        "/Users/ttwwnn/Projetos/777leads"
    };
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-linux");
    let (mut c1, mut c2) = (Conversa::com(&m, None, Some(de_fora)), Conversa::nova(&m));
    c1.chamada(t - 0.3, Some(t + 0.3), "git worktree add ../wt-linux HEAD");
    c2.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c2.sid.as_str()) && !d.niveis.contains_key(&c1.sid), "{d:?}");
}

#[test]
fn t10_sessao_viva_chamada_sem_resultado_vale_ate_agora() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-lenta");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 5.0, None, &format!("sleep 5; git worktree add {w} HEAD")); // ainda rodando
    m.indexar(t + 60.0);
    assert_eq!(m.decisao_de(&w).estado, "sem-dono", "sessão morta: a janela fecha na última linha");
    std::fs::remove_file(estado::arquivo(&m.amb)).unwrap();
    m.indexar_com(t + 60.0, vec![viva(&c1.sid)]);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()) && d.provisorio, "{d:?}");
}

#[test]
fn t10b_arquivo_de_sessao_viva_nao_e_podado_pelo_mtime() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-lenta2");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 5.0, None, &format!("sleep 5; git worktree add {w} HEAD"));
    // Nada escrito depois do nascimento: o comando ainda roda.
    let f = std::fs::File::options().write(true).open(&c1.arq).unwrap();
    let antes = std::time::UNIX_EPOCH + Duration::from_secs_f64(t - 4.0);
    f.set_times(std::fs::FileTimes::new().set_modified(antes).set_accessed(antes)).unwrap();
    let comum = repos::comum_de(&m.repo).unwrap();
    let ws: Vec<repos::Worktree> = repos::worktrees_do_repo(&comum).into_iter().filter(|x| x.caminho == w).collect();
    let refs: Vec<&repos::Worktree> = ws.iter().collect();
    let vivas = HashSet::from([c1.sid.clone()]);
    let ds = worktrees::dono::decidir(&m.amb, &refs, None, &vivas, t + 60.0, &Prazo::sem(), None).unwrap();
    let d = &ds[&repos::chave(&ws[0])];
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()), "varredura fria com a sessão viva: {d:?}");
}

#[test]
fn t10c_chamada_abandonada_nao_fica_aberta_para_sempre() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-depois");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 3.0 * 3600.0, None, "git worktree list  # wt-depois"); // abandonada horas antes
    c1.texto(t + 120.0, None); // a conversa seguiu
    m.indexar_com(t + 180.0, vec![viva(&c1.sid)]);
    let d = m.decisao_de(&w);
    assert!(d.estado == "sem-dono" && !d.provisorio, "{d:?}");
}

#[test]
fn t26_segundo_plano_vale_ate_a_notificacao() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-fundo");
    let mut c1 = Conversa::nova(&m);
    let tid = c1.chamada_x(t - 10.0, Some(t - 9.9), &format!("sleep 10; git worktree add {w} HEAD"), Ch { bg: true, ..Ch::default() });
    c1.texto(t - 9.0, None);
    c1.notificacao(t + 5.0, &tid);
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()), "{d:?}");
}

#[test]
fn t27_mensagem_do_usuario_interrompe_a_pendente() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-interrompida");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 20.0, None, &format!("git worktree add {w} HEAD"));
    c1.usuario(t - 10.0, "pare"); // Esc + mensagem: a chamada acabou aí
    c1.texto(t + 30.0, None);
    m.indexar(t + 60.0);
    assert_eq!(m.decisao_de(&w).estado, "sem-dono");
}

// ---------------------------------------------------------------- Codex

#[test]
fn t28_codex_e_dono() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-do-codex");
    let mut r = Rollout::novo(&m, t - 60.0);
    let cid = r.chamada(t - 0.3, &format!("git worktree add -q {w} HEAD"));
    r.saida(t + 0.3, &cid, "", 0.6, None);
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(r.tid.as_str()) && d.nivel.as_deref() == Some("A"), "{d:?}");
    assert_eq!(d.codex, Some(vec![r.tid.clone()]));
    let l = m.listar(&[&r.tid]);
    assert!(caminhos(&l) == vec![w.clone()] && l.lixeira[0].sessao == r.tid && l.avisos.is_empty(), "{l:?}");
}

#[test]
fn t29_codex_no_nascimento_tira_o_dono_claude() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-disputada-cx");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add -q {w} HEAD"));
    let mut r = Rollout::novo(&m, t - 60.0);
    // A linha da chamada só gravada no fim (T+5): o "Wall time" puxa o início para T-3,1.
    let cid = r.chamada(t + 5.0, &format!("git worktree add -q {w} HEAD && sleep 8"));
    r.saida(t + 5.1, &cid, "", 8.2, None);
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "ambigua", "{d:?}");
    assert_eq!(d.niveis, BTreeMap::from([(c1.sid.clone(), "A".into()), (r.tid.clone(), "A".into())]));
    let l = m.listar(&[&c1.sid]);
    assert!(l.lixeira.is_empty() && l.avisos == vec![format!("possivelmente desta aba, não verificada: {w}")], "{l:?}");
}

#[test]
fn t29b_codex_longe_do_nascimento_nao_atrapalha() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-so-claude");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add -q {w} HEAD"));
    let mut r = Rollout::novo(&m, t - 600.0);
    let cid = r.chamada(t - 300.0, &format!("git worktree list  # {w}")); // cita, mas 5 min antes
    r.saida(t - 299.5, &cid, "", 0.5, None);
    let cid = r.chamada(t - 0.5, "ls -la"); // rodando no nascimento, sem citar
    r.saida(t + 0.5, &cid, "", 1.0, None);
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(c1.sid.as_str()) && d.codex.is_none(), "{d:?}");
}

#[test]
fn t29c_codex_linha_so_no_fim_wall_time_sem_dois_pontos() {
    // O code mode escreve "Wall time 8.2 seconds" (sem dois-pontos); com a
    // linha da chamada gravada só no fim, é ele que põe o nascimento dentro.
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-codex-fim");
    let mut r = Rollout::novo(&m, t - 60.0);
    r.k += 1;
    let cid = "call_fim_1";
    let celula = format!(r#"text(await tools.exec_command({{cmd:"git worktree add -q {w} HEAD && sleep 8"}}));"#);
    r.linha(t + 5.0, "response_item", json!({"type": "custom_tool_call", "name": "exec", "call_id": cid,
        "status": "completed", "input": celula}));
    r.linha(t + 5.1, "response_item", json!({"type": "custom_tool_call_output", "call_id": cid,
        "output": [{"type": "input_text", "text": "Script completed\nWall time 8.2 seconds\nOutput:\n"}]}));
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(r.tid.as_str()) && d.nivel.as_deref() == Some("A"), "{d:?}");
}

#[test]
fn t30_codex_processo_vivo_vale_ate_sair() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-exec-longo");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add -q {w} HEAD"));
    let mut r = Rollout::novo(&m, t - 120.0);
    let cid = r.chamada(t - 60.0, "./prepara-tudo.sh"); // não cita o nome
    r.saida(t - 50.0, &cid, "", 10.0, Some(7)); // o processo continua (sessão 7)
    let c2 = r.chamada_x(t + 30.0, "", "write_stdin", Some(7));
    r.saida(t + 40.0, &c2, &format!("pronto: {w}\n"), 10.0, None); // acaba e imprime o caminho
    m.indexar(t + 90.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "ambigua" && d.niveis.get(&r.tid).map(String::as_str) == Some("A"), "{d:?}");
}

#[test]
fn t31_codex_subagente_conta_pela_conversa_de_cima() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-do-sub");
    let pai = Rollout::novo(&m, t - 100.0);
    let mut sub = Rollout::com(&m, t - 50.0, None, Some(&pai.tid), false);
    let cid = sub.chamada(t - 0.3, &format!("git worktree add {w} HEAD"));
    sub.saida(t + 0.3, &cid, "", 0.6, None);
    m.indexar(t + 60.0);
    let d = m.decisao_de(&w);
    assert!(d.estado == "dono" && d.dono.as_deref() == Some(pai.tid.as_str()), "{d:?}");
    assert_eq!(d.niveis, BTreeMap::from([(pai.tid.clone(), "A".to_string())]));
    let l = m.listar(&[&pai.tid]);
    assert!(caminhos(&l) == vec![w] && l.avisos.is_empty(), "{l:?}");
}

#[test]
fn t32_codex_comprimido_e_ilegivel() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-zst");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    let mut r = Rollout::com(&m, t - 60.0, None, None, true); // o Codex comprime os antigos
    let cid = r.chamada(t - 0.2, "git worktree list | grep wt-zst");
    r.saida(t + 0.2, &cid, "", 0.4, None);
    m.indexar(t + 60.0);
    assert_eq!(m.decisao_de(&w).estado, "ambigua", "o .jsonl.zst é lido: {:?}", m.decisao_de(&w));
    let (w2, t2) = m.wt("wt-zst2");
    let mut c2 = Conversa::nova(&m);
    c2.chamada(t2 - 0.3, Some(t2 + 0.3), &format!("git worktree add {w2} HEAD"));
    let ruim = r.arq.parent().unwrap().join(format!("rollout-2026-09-26T10-00-00-{}.jsonl.zst", uuid()));
    std::fs::write(ruim, b"isto nao e zstd").unwrap(); // ilegível, escrito depois do nascimento
    m.indexar(t2 + 60.0);
    let d2 = m.decisao_de(&w2);
    assert!(d2.estado == "ambigua" && d2.niveis.get(&c2.sid).map(String::as_str) == Some("A"), "ilegível é dúvida: {d2:?}");
}

#[test]
fn t32b_codex_ilegivel_sozinho_nao_e_dono() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-so-ilegivel");
    let ile = uuid();
    let d = m.codex.join("sessions").join(data_local(t, "%Y")).join(data_local(t, "%m")).join(data_local(t, "%d"));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join(format!("rollout-2026-09-26T10-00-01-{ile}.jsonl.zst")), b"tambem nao e zstd").unwrap();
    m.indexar(t + 60.0);
    let dd = m.decisao_de(&w);
    assert!(dd.estado == "indecidida" && dd.dono.is_none(), "ilegível sozinho não é dono: {dd:?}");
    assert!(m.listar(&[&ile]).lixeira.is_empty());
}

#[test]
fn t11a_decisao_e_imutavel() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-fixa");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    assert_eq!(m.decisao_de(&w).dono.as_deref(), Some(c1.sid.as_str()), "1ª decisão");
    std::fs::remove_file(&c1.arq).unwrap(); // transcrito apagado (a limpeza de 30 dias do Claude)
    let r = m.indexar(t + 120.0);
    assert!(m.decisao_de(&w).dono.as_deref() == Some(c1.sid.as_str()) && r.decididas_agora == 0, "a decisão mudou: {:?}", m.decisao_de(&w));
}

#[test]
fn t11b_nome_reaproveitado_e_outra_worktree() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-reuso");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    g(&m.repo, &["worktree", "remove", &w]);
    std::thread::sleep(Duration::from_secs_f64(4.7)); // fora da folga (2 s) da chamada que criou a 1ª
    let (w2, t2) = m.wt("wt-reuso");
    assert!(w2 == w && t2 != t, "mesmo caminho, outro nascimento");
    let mut c2 = Conversa::nova(&m);
    c2.chamada(t2 - 0.3, Some(t2 + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t2 + 60.0);
    let d = m.decisao_de(&w);
    assert_eq!(d.dono.as_deref(), Some(c2.sid.as_str()), "o nome reaproveitado herdou o dono antigo: {d:?}");
}

// ---------------------------------------------------------------- filtros do listar

#[test]
fn t12_travada_fica() {
    let m = Mundo::novo();
    let (w, t) = m.wt("fin-perm");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    g(&m.repo, &["worktree", "lock", "--reason", "kit-mac: permanente", &w]);
    m.indexar(t + 60.0);
    let r = m.listar(&[&c1.sid]);
    assert!(r.lixeira.is_empty() && r.mantidas == vec![mantida(&w, "travada: kit-mac: permanente")], "{r:?}");
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), None);
    assert!(!p.ok && p.motivo.as_deref().is_some_and(|x| x.starts_with("travada")), "{p:?}");
}

#[test]
fn t13_principal_nunca() {
    let m = Mundo::novo();
    let p = worktrees::preparar_em(&m.amb, Path::new(&m.repo), None);
    assert!(!p.ok && p.motivo.as_deref().is_some_and(|x| x.contains("principal")), "{p:?}");
    let (w, _) = m.wt("wt-sub");
    std::fs::create_dir_all(Path::new(&w).join("pasta")).unwrap();
    let p = worktrees::preparar_em(&m.amb, &Path::new(&w).join("pasta"), None);
    assert!(!p.ok && p.motivo.as_deref().is_some_and(|x| x.contains("raiz")), "subpasta não é a worktree: {p:?}");
    assert_eq!(g(&m.repo, &["for-each-ref", "refs/keep-lixeira"]), "", "recusa não grava ref");
}

#[test]
fn t14_em_uso_por_processo_de_outra_aba() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-compartilhada");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let outra = dorme(Some(Path::new(&w))); // o shell de outra aba, com a pasta atual dentro
    let o = outra.id();
    assert!(espera(10.0, || processos::cwd(o).is_some()));
    let (r, _) = m.listar_com(&[&c1.sid], Op { outras: vec![o], rotulos: vec![(o, "Relatório", "Brokerfy")], ..Op::default() });
    mata(outra);
    assert!(r.lixeira.is_empty() && r.mantidas == vec![mantida(&w, "em uso pela aba “Relatório” (Brokerfy)")], "{r:?}");
    let minha = dorme(Some(Path::new(&w))); // processo da PRÓPRIA aba: morre com ela
    let p = minha.id();
    assert!(espera(10.0, || processos::cwd(p).is_some()));
    let (r, _) = m.listar_com(&[&c1.sid], Op { fechando: vec![p], ..Op::default() });
    mata(minha);
    assert!(caminhos(&r) == vec![w] && r.lixeira[0].processos.is_empty(), "{r:?}");
}

/// Um git lento que entra na worktree (`-C`), avisa (`dentro`), espera e
/// roda; avisa de novo (`acabou`). Só no unix (é um script).
#[cfg(unix)]
fn git_que_entra(m: &Mundo) -> (PathBuf, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let (dentro, acabou, lento) = (m.base.join("git-dentro"), m.base.join("git-acabou"), m.base.join("git-lento"));
    std::fs::write(
        &lento,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do [ \"$prox\" = 1 ] && cd \"$a\"; prox=0; [ \"$a\" = -C ] && prox=1; done\n\
             touch \"{}\"; sleep 0.4\n\"{}\" \"$@\"; r=$?\ntouch \"{}\"\nexit $r\n",
            dentro.display(),
            worktrees::git::padrao().display(),
            acabou.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&lento, std::fs::Permissions::from_mode(0o755)).unwrap();
    (lento, dentro, acabou)
}

#[cfg(unix)]
#[test]
fn t14b_git_do_proprio_listar_nao_conta_como_em_uso() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-git-lento");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let (lento, dentro, acabou) = git_que_entra(&m);
    // A varredura com um git do próprio processo rodando dentro da worktree.
    let w2 = w.clone();
    let lento2 = lento.clone();
    let th = std::thread::spawn(move || worktrees::git::git(&lento2, &["-C", &w2, "status"], 20.0));
    assert!(espera(6.0, || dentro.exists()), "o git lento não entrou");
    let v = varredura::processos();
    th.join().unwrap().unwrap();
    assert!(v.procs.iter().all(|p| !pastas::dentro(&p.cwd, &w)), "o git do processo apareceu: {:?}", v.procs);
    assert!(acabou.exists(), "o git lento não rodou");
    let eu = std::process::id();
    let (r, _) = m.listar_com(&[&c1.sid], Op { outras: vec![eu], rotulos: vec![(eu, "Keep", "pc")], ..Op::default() });
    assert!(caminhos(&r) == vec![w] && r.lixeira[0].processos.is_empty() && r.mantidas.is_empty(), "{r:?}");
}

#[cfg(unix)]
#[test]
fn t14c_git_que_acaba_durante_a_varredura_nao_vira_orfao() {
    let m = Mundo::novo();
    let (w, _) = m.wt("wt-git-acaba");
    let (lento, dentro, acabou) = git_que_entra(&m);
    let (w2, lento2) = (w.clone(), lento.clone());
    let th = std::thread::spawn(move || worktrees::git::git(&lento2, &["-C", &w2, "status"], 20.0));
    assert!(espera(6.0, || dentro.exists()), "o git lento não entrou");
    // A varredura vê o git vivo, com a pasta atual na worktree...
    let crua = varredura::crua();
    assert!(crua.procs.iter().any(|p| pastas::dentro(&p.cwd, &w)), "a varredura não viu o git dentro");
    // ...e ele acaba antes de a lista dos git ser consultada.
    assert!(espera(6.0, || acabou.exists()));
    th.join().unwrap().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let v = varredura::sem_os_meus(crua);
    assert!(v.procs.iter().all(|p| !pastas::dentro(&p.cwd, &w)), "o git que acabou virou órfão: {:?}", v.procs);
}

#[test]
fn t15_processo_orfao_vai_na_lista_e_barra_o_preparar() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-orfa");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let orfao = dorme(Some(Path::new(&w)));
    let o = orfao.id();
    assert!(espera(10.0, || processos::cwd(o).is_some()));
    let nome = processos::um(o).unwrap().nome;
    let r = m.listar(&[&c1.sid]);
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), None);
    mata(orfao);
    assert_eq!(caminhos(&r), vec![w.clone()], "{r:?}");
    assert_eq!(r.lixeira[0].processos, vec![worktrees::ProcessoDentro { pid: o, nome: nome.clone() }]);
    assert!(!p.ok && p.processos == Some(vec![worktrees::ProcessoDentro { pid: o, nome }]), "{p:?}");
    assert!(espera(10.0, || !processos::vivo(o)));
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), None);
    assert!(p.ok, "sem o processo, prepara: {p:?}");
}

#[test]
fn t16_outra_sessao_viva_trabalhando_dentro() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-visitada");
    let (mut c1, c2) = (Conversa::nova(&m), Conversa::nova(&m));
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    c2.texto(t + 30.0, Some(&texto(&Path::new(&w).join("src")))); // outra conversa viva lá dentro
    m.indexar(t + 60.0);
    let (r, _) = m.listar_com(&[&c1.sid], Op { vivas: vec![viva(&c2.sid)], ..Op::default() });
    let motivo = format!("em uso pela conversa {} (fora do Keep)", &c2.sid[..8]);
    assert!(r.lixeira.is_empty() && r.mantidas == vec![mantida(&w, &motivo)], "{r:?}");
}

#[test]
fn t17_conversa_da_aba_viva_em_outra_aba() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-resumida");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let outra = dorme(None); // a conversa foi retomada (/resume) em outra aba
    let o = outra.id();
    let (r, _) = m.listar_com(
        &[&c1.sid],
        Op { vivas: vec![viva_em(&c1.sid, o)], outras: vec![o], rotulos: vec![(o, "Outra", "pc")], ..Op::default() },
    );
    mata(outra);
    assert!(r.lixeira.is_empty() && r.mantidas == vec![mantida(&w, "em uso pela aba “Outra” (pc)")], "{r:?}");
}

#[test]
fn t23_ja_na_lixeira_e_inexistente() {
    let m = Mundo::novo();
    let (w1, t1) = m.wt_em("wt-no-lixo", None, None, Some(&m.lixeira.join("wt-no-lixo")));
    let (w2, t2) = m.wt("wt-sumiu");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t1.min(t2) - 0.3, Some(t1.max(t2) + 0.3), &format!("git worktree add {w1} HEAD; git worktree add {w2} HEAD"));
    m.indexar(t1.max(t2) + 60.0);
    std::fs::rename(&w2, m.base.join("fora-wt-sumiu")).unwrap();
    let r = m.listar(&[&c1.sid]);
    let mut esperadas = vec![mantida(&w1, "já está na Lixeira"), mantida(&w2, "caminho inexistente")];
    esperadas.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    assert!(r.lixeira.is_empty() && r.mantidas == esperadas, "{r:?}");
}

// ---------------------------------------------------------------- preparar / concluir

#[test]
fn t18_submodulos_mantem_o_registro() {
    let m = Mundo::novo();
    let repo = m.novo_repo("comsub", true);
    let (w, t) = m.wt_em("wt-sm", Some(&repo), Some("ramo-sm"), None);
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let r = m.listar(&[&c1.sid]);
    assert!(caminhos(&r) == vec![w.clone()] && r.lixeira[0].submodulos, "{r:?}");
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), None);
    assert!(p.ok, "{p:?}");
    let destino = m.lixeira.join("wt-sm");
    std::fs::rename(&w, &destino).unwrap();
    let c = worktrees::concluir_em(&m.amb, Path::new(&w), &destino);
    let adm = Path::new(&repo).join(".git").join("worktrees").join("wt-sm");
    assert!(c.ok && c.registro_removido == Some(false) && c.nota.as_deref().is_some_and(|n| n.contains("submódulos")) && adm.is_dir(), "{c:?}");
}

#[test]
fn t19_preparar_e_concluir() {
    let m = Mundo::novo();
    let (w, _) = m.wt_em("wt-ciclo", None, Some("ramo-ciclo"), None);
    let mut f = std::fs::OpenOptions::new().append(true).open(Path::new(&w).join("a.txt")).unwrap();
    f.write_all("mudança\n".as_bytes()).unwrap();
    drop(f);
    std::fs::write(Path::new(&w).join("novo.txt"), "não rastreado\n").unwrap();
    let head = g(&w, &["rev-parse", "HEAD"]).trim().to_string();
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), None);
    assert!(p.ok && p.id.as_deref() == Some("wt-ciclo") && p.repo.as_deref() == Some(m.repo.as_str()), "{p:?}");
    assert_eq!(p.ramo, Some(Some("ramo-ciclo".into())));
    let r = p.r#ref.clone().unwrap();
    assert!(r == "refs/keep-lixeira/wt-ciclo" && g(&m.repo, &["rev-parse", &r]).trim() == head, "{p:?}");
    assert_eq!(p.ref_sujo, Some(Some("refs/keep-lixeira/wt-ciclo-sujo".into())));
    assert!(g(&m.repo, &["show", "refs/keep-lixeira/wt-ciclo-sujo:a.txt"]).contains("mudança"), "o sujo guarda a alteração rastreada");
    assert_eq!(g(&w, &["status", "--porcelain"]).matches('\n').count(), 2, "o preparar não mexe na árvore");
    let destino = m.lixeira.join("wt-ciclo");
    let c = worktrees::concluir_em(&m.amb, Path::new(&w), &destino);
    assert!(!c.ok && c.motivo.as_deref().is_some_and(|x| x.contains("ainda existe")), "concluir com a pasta no lugar: {c:?}");
    std::fs::rename(&w, &destino).unwrap(); // o que a Lixeira faz
    let c = worktrees::concluir_em(&m.amb, Path::new(&w), &destino);
    assert!(c.ok && c.registro_removido == Some(true) && c.r#ref == Some(Some(r.clone())), "{c:?}");
    assert_eq!(c.ramo, Some(Some("ramo-ciclo".into())));
    assert!(!g(&m.repo, &["worktree", "list", "--porcelain"]).contains(&w.replace('\\', "/")), "registro removido");
    g(&m.repo, &["branch", "-D", "ramo-ciclo"]); // o ramo ficou livre
    assert!(destino.join("novo.txt").exists(), "a cópia na lixeira fica intacta");
    let log = std::fs::read_to_string(m.amb.estado.join("lixeira.log")).unwrap();
    let linhas: Vec<Value> = log.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(linhas.len(), 1);
    let l = &linhas[0];
    assert!(l["registro_removido"] == json!(true) && l["destino"] == json!(texto(&destino)), "{l}");
    for k in ["ts", "caminho", "destino", "repo", "ramo", "head", "ref", "ref_sujo", "registro_removido"] {
        assert!(l.get(k).is_some(), "falta {k}: {l}");
    }
    let (w2, _) = m.wt("wt-ciclo"); // o mesmo nome de novo: a ref antiga fica
    let p2 = worktrees::preparar_em(&m.amb, Path::new(&w2), None);
    let r2 = p2.r#ref.clone().unwrap_or_default();
    assert!(p2.ok && r2 != r && r2.starts_with("refs/keep-lixeira/wt-ciclo-"), "{p2:?}");
    assert_eq!(g(&m.repo, &["rev-parse", &r]).trim(), head, "a ref da 1ª continua");
}

#[test]
fn t19b_preparar_confere_o_nascimento_listado() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-renasce");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add -q {w} HEAD"));
    m.indexar(t + 60.0);
    let r = m.listar(&[&c1.sid]);
    let n = r.lixeira[0].nasceu_ns;
    assert!(n > 0, "o listar diz o nascimento: {r:?}");
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), Some(n + 1000));
    assert!(!p.ok && p.motivo.as_deref().is_some_and(|x| x.contains("não é a que foi listada")), "outro nascimento: {p:?}");
    let p = worktrees::preparar_em(&m.amb, Path::new(&w), Some(n));
    assert!(p.ok, "o nascimento listado: {p:?}");
}

#[test]
fn t37_outra_sessao_entrou_na_worktree() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-entrada");
    let (mut c1, c2, c3) = (Conversa::nova(&m), Conversa::nova(&m), Conversa::nova(&m));
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    // c2 entrou na worktree (EnterWorktree) e continua lá, com a pasta atual
    // fora; 600 KB depois (vários blocos de leitura, linhas cortadas na divisa).
    c2.linha(json!({"type": "worktree-state", "sessionId": c2.sid, "worktreeSession": {"worktreePath": w}}));
    for i in 0..1500 {
        c2.texto(t + 1.0 + f64::from(i) * 0.01, Some(&m.raiz));
        c2.linha(json!({"type": "user", "sessionId": c2.sid, "message": {"role": "user", "content": "x".repeat(400)}}));
    }
    // c3 entrou e saiu (worktreePath vazio no fim).
    c3.linha(json!({"type": "worktree-state", "sessionId": c3.sid, "worktreeSession": {"worktreePath": w}}));
    c3.texto(t + 2.0, Some(&m.raiz));
    c3.linha(json!({"type": "worktree-state", "sessionId": c3.sid, "worktreeSession": null}));
    m.indexar(t + 60.0);
    use worktrees::sessoes::estado_final;
    assert_eq!(estado_final(&c2.sid, &m.proj), (Some(m.raiz.clone()), Some(w.clone())));
    assert_eq!(estado_final(&c3.sid, &m.proj), (Some(m.raiz.clone()), None));
    let (r, _) = m.listar_com(&[&c1.sid], Op { vivas: vec![viva(&c3.sid)], ..Op::default() });
    assert_eq!(caminhos(&r), vec![w.clone()], "quem saiu não segura: {r:?}");
    let (r, _) = m.listar_com(&[&c1.sid], Op { vivas: vec![viva(&c2.sid)], ..Op::default() });
    let motivo = format!("em uso pela conversa {} (fora do Keep)", &c2.sid[..8]);
    assert!(r.lixeira.is_empty() && r.mantidas == vec![mantida(&w, &motivo)], "{r:?}");
}

#[test]
fn t38_processos_e_a_cadeia_ate_o_shell() {
    let m = Mundo::novo();
    let d = m.base.join("pasta-do-filho");
    std::fs::create_dir_all(&d).unwrap();
    let d = pastas::canon(&texto(&d));
    let mut c = Concha::nova(&m, Path::new(&d));
    let filho = c.sobe("dorminhoco", &[]);
    let v = varredura::crua();
    let aqui: HashSet<(u32, u32)> =
        v.procs.iter().filter(|p| pastas::dentro(&pastas::canon(&p.cwd), &d)).map(|p| (p.pid, p.ppid)).collect();
    let (concha, eu) = (c.pid(), std::process::id());
    // Uma cadeia com um processo fora do mapa (de root no meio, ex. sudo): o
    // pai dele é perguntado a ele.
    let mut sem = v.mapa.clone();
    sem.remove(&concha);
    let r = varredura::concha_por_mapa(filho, &HashSet::from([eu]), &mut sem, &v.inicios);
    c.fim();
    assert_eq!(aqui, HashSet::from([(concha, eu), (filho, concha)]), "{:?}", v.procs);
    assert_eq!(r, Some(eu), "a cadeia com buraco não chegou no shell");
}

// ---------------------------------------------------------------- git

#[cfg(unix)]
fn git_devagar(m: &Mundo, seg: f64, so: &[&str]) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = m.base.join(format!("git-devagar-{}", uuid()));
    let cond = if so.is_empty() {
        "true".to_string()
    } else {
        so.iter().map(|x| format!("[ \"$a\" = {x} ]")).collect::<Vec<_>>().join(" || ")
    };
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do if {cond}; then sleep {seg}; break; fi; done\nexec \"{}\" \"$@\"\n",
            worktrees::git::padrao().display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// 1 rastreado alterado, 1 novo no índice, 1 arquivo e 1 pasta não
/// rastreados, pasta e arquivo ignorados.
fn suja(w: &str) {
    let p = |n: &str| Path::new(w).join(n);
    std::fs::write(p(".gitignore"), "*.log\ncache/\n").unwrap();
    g(w, &["add", ".gitignore"]);
    g(w, &["commit", "-q", "-m", "ignora"]);
    let mut f = std::fs::OpenOptions::new().append(true).open(p("a.txt")).unwrap();
    f.write_all(b"mudou\n").unwrap();
    std::fs::write(p("novo.txt"), "n\n").unwrap();
    g(w, &["add", "novo.txt"]);
    std::fs::write(p("solto.txt"), "s\n").unwrap();
    std::fs::create_dir_all(p("pasta-nova/sub")).unwrap();
    for n in ["pasta-nova/x", "pasta-nova/sub/y"] {
        std::fs::write(p(n), "z\n").unwrap();
    }
    std::fs::create_dir_all(p("cache/d")).unwrap();
    std::fs::create_dir_all(p("so-ignorados")).unwrap();
    for n in ["cache/d/c1", "so-ignorados/a.log", "raiz.log"] {
        std::fs::write(p(n), "i\n").unwrap();
    }
}

fn chave_de(m: &Mundo, w: &str) -> String {
    let c = repos::comum_de(&m.repo).unwrap();
    repos::chave(&repos::worktrees_do_repo(&c).into_iter().find(|x| x.caminho == w).unwrap())
}

#[test]
fn t35_alteracoes_como_o_git_status() {
    let m = Mundo::novo();
    let (w, t) = m.wt_em("wt-suja", None, Some("ramo-sujo"), None);
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    suja(&w);
    m.indexar(t + 60.0);
    let status: Vec<String> = g(&w, &["status", "--porcelain=v2"]).lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
    assert_eq!(status.len(), 4, "o cenário mudou: {status:?}");
    let r = m.listar(&[&c1.sid]);
    let x = &r.lixeira[0];
    assert!(x.alteracoes == 4 && x.ramo.as_deref() == Some("ramo-sujo") && x.commits_so_aqui == Some(0), "{x:?}");
    let est = estado::carrega(&m.amb).unwrap();
    let gi = &est.git[&chave_de(&m, &w)];
    assert!(gi.alteracoes == 4 && gi.nao_rastreados == 2 && gi.rastreados == 2, "{gi:?}");
}

#[cfg(unix)]
#[test]
fn t36_git_lento_usa_o_registro_do_indexar() {
    let m = Mundo::novo();
    let (w, t) = m.wt_em("wt-lenta-git", None, Some("ramo-lento"), None);
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    suja(&w);
    m.indexar(t + 60.0); // grava o registro do git (4 alterações)
    std::fs::write(Path::new(&w).join("outro-solto.txt"), "depois do indexar\n").unwrap(); // ao vivo, 5
    let lento = git_devagar(&m, 2.5, &["ls-files"]); // os não rastreados demoram
    let (r, dt) = m.listar_com(&[&c1.sid], Op { prazo: 10.0, rapido: Some(0.4), git: Some(lento), ..Op::default() });
    assert!(dt < 1.5, "esperou o git lento: {dt:.2} s");
    assert_eq!(caminhos(&r), vec![w.clone()], "{r:?}");
    let x = &r.lixeira[0];
    assert!(x.alteracoes == 4 && x.ramo.as_deref() == Some("ramo-lento") && x.commits_so_aqui == Some(0), "{x:?}");
    assert!(r.avisos.len() == 1 && r.avisos[0].starts_with(&format!("alterações de {w} aproximadas (contadas há ")), "{:?}", r.avisos);
    // Registro velho demais: espera o git ao vivo.
    let mut est = estado::carrega(&m.amb).unwrap();
    for v in est.git.values_mut() {
        v.em = agora() - worktrees::GIT_VALIDADE - 5.0;
    }
    estado::grava(&m.amb, &mut est).unwrap();
    let lento = git_devagar(&m, 0.8, &["ls-files"]);
    let (r, _) = m.listar_com(&[&c1.sid], Op { prazo: 10.0, rapido: Some(0.2), git: Some(lento), ..Op::default() });
    assert!(caminhos(&r) == vec![w] && r.lixeira[0].alteracoes == 5 && r.avisos.is_empty(), "{r:?}");
}

// ---------------------------------------------------------------- prazo

#[test]
fn t20_prazo_cooperativo_vira_aviso() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-nova");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    let (r, dt) = m.listar_com(&[&c1.sid], Op { prazo: 0.4, decidir_lento: true, ..Op::default() });
    assert!(dt < 1.5, "demorou {dt:.2} s");
    assert!(r.lixeira.is_empty() && r.avisos.contains(&format!("não deu tempo de verificar a worktree nova: {w}")), "{r:?}");
    #[cfg(unix)]
    {
        m.indexar(t + 60.0);
        let mut est = estado::carrega(&m.amb).unwrap();
        est.git.clear(); // sem o registro do indexar: nada aproximado
        estado::grava(&m.amb, &mut est).unwrap();
        let lento = git_devagar(&m, 1.0, &["status", "ls-files"]);
        let (r, _) = m.listar_com(&[&c1.sid], Op { prazo: 0.4, git: Some(lento), ..Op::default() });
        assert!(r.lixeira.is_empty() && r.avisos == vec![format!("não deu tempo de verificar: {w}")], "{r:?}");
    }
}

#[test]
fn t21_prazo_duro_json_sai_antes() {
    let m = Mundo::novo();
    let mut amb = m.amb.clone();
    // O vínculo trava (um stat preso, um processo que não responde).
    amb.ganchos.vinculo = Some(Arc::new(|_: &[worktrees::Alvo]| {
        std::thread::sleep(Duration::from_secs(30));
        (Vec::new(), Contexto::default())
    }));
    let t0 = Instant::now();
    let r = worktrees::listar_em(&amb, vec![analisa_alvo("W:1").unwrap()], 1.0, 0.0);
    let dt = t0.elapsed().as_secs_f64();
    assert!(dt < 1.0, "o JSON saiu depois do prazo: {dt:.3} s");
    assert!(r.lixeira.is_empty() && r.avisos.iter().any(|a| a.contains("prazo de 1.0 s estourado")), "{r:?}");
}

// ---------------------------------------------------------------- o vínculo de verdade (keepd de mentira)

/// Um keepd de mentira neste processo: responde a lista com as abas dadas
/// (com `List3`, ou como o daemon antigo, sem ele).
struct Keepd {
    workspaces: Arc<Mutex<Vec<WorkspaceInfo>>>,
}

impl Keepd {
    fn sobe(sock: &Path, novo: bool) -> Keepd {
        let workspaces: Arc<Mutex<Vec<WorkspaceInfo>>> = Arc::new(Mutex::new(Vec::new()));
        let l = Listener::bind(sock).expect("o socket do keepd de mentira");
        let ws = workspaces.clone();
        std::thread::spawn(move || {
            while let Ok((mut s, _)) = l.accept() {
                let lista = ws.lock().unwrap().clone();
                match ClientMsg::read(&mut s) {
                    Ok(Some(ClientMsg::List3)) if novo => {
                        let d = DaemonInfo { pid: std::process::id(), started_ms: 1_791_000_000_000 };
                        ServerMsg::Workspaces3(d, lista).write(&mut s).ok();
                    }
                    Ok(Some(ClientMsg::List2)) => {
                        ServerMsg::Workspaces2(lista).write(&mut s).ok();
                    }
                    _ => {}
                }
            }
        });
        Keepd { workspaces }
    }

    fn aba(&self, ws: &str, info: TabInfo) {
        let mut g = self.workspaces.lock().unwrap();
        match g.iter_mut().find(|w| w.name == ws) {
            Some(w) => {
                w.tabs.retain(|t| t.id != info.id);
                w.tabs.push(info);
            }
            None => g.push(WorkspaceInfo { name: ws.into(), tabs: vec![info] }),
        }
    }
}

fn aba(id: u32, titulo: &str, concha: u32, frente: u32) -> TabInfo {
    TabInfo { id, title: titulo.into(), cols: 80, rows: 24, shell_pid: concha, pid: frente, ..TabInfo::default() }
}

/// A sessão que o Claude Code publica para o processo `pid`.
fn sessao_claude(m: &Mundo, pid: u32, sid: &str) {
    let nasceu = processos::um(pid).unwrap().inicio_ms;
    let lstart = chrono::DateTime::from_timestamp((nasceu / 1000) as i64, 0).unwrap().format("%a %b %e %H:%M:%S %Y").to_string();
    let v = json!({"pid": pid, "sessionId": sid, "procStart": lstart, "kind": "interactive", "cwd": m.raiz});
    std::fs::write(m.sess.join(format!("{pid}.json")), serde_json::to_vec(&v).unwrap()).unwrap();
}

/// Uma aba com o Claude dentro: (shell, processo do Claude, conversa).
fn aba_com_claude(m: &Mundo, k: &Keepd, id: u32, titulo: &str) -> (Concha, u32, Conversa) {
    let mut c = Concha::nova(m, Path::new(&m.raiz));
    let agente = c.sobe("claude", &[]);
    let conversa = Conversa::nova(m);
    conversa.titulo(&keep_ia::worktrees::texto::titulo_limpo(titulo));
    sessao_claude(m, agente, &conversa.sid);
    k.aba("W", aba(id, titulo, c.pid(), agente));
    (c, agente, conversa)
}

/// O `listar` com o resolvedor de verdade (o keepd de mentira do mundo).
fn listar_de_verdade(m: &Mundo, alvos: &[&str]) -> Listagem {
    let alvos: Vec<_> = alvos.iter().map(|a| analisa_alvo(a).unwrap()).collect();
    let r = Mutex::new(Listagem::nova());
    listar::listar(&m.amb, &alvos, &Prazo::de(20.0), &r).ok();
    r.into_inner().unwrap()
}

#[test]
fn t22a_vinculo_exato_pela_lista() {
    let m = Mundo::novo();
    let k = Keepd::sobe(&m.amb.socket, true);
    let (c, agente, mut conversa) = aba_com_claude(&m, &k, 1, "✳ Minha conversa");
    let (w, t) = m.wt("wt-da-aba");
    conversa.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let r = listar_de_verdade(&m, &["W:1", "W:9"]);
    let concha = c.pid();
    c.fim();
    assert_eq!(
        r.abas[0],
        AbaListada { alvo: "W:1".into(), agente: "claude".into(), vinculo: "exato".into(), sessoes: vec![conversa.sid.clone()], pids: vec![agente, concha] }
    );
    assert!(r.abas[1].vinculo == "nenhum" && r.abas[1].sessoes.is_empty(), "{:?}", r.abas);
    assert_eq!(r.avisos, vec!["a aba “tab 9” (W) não existe mais no Keep".to_string()]);
    assert_eq!(caminhos(&r), vec![w]);
}

#[test]
fn t22b_daemon_sem_list3_nao_move_nada() {
    // O daemon antigo não diz os processos das abas: o shell é deduzido (os
    // filhos do keepd, pela pasta), e a conversa que roda nele também — mas
    // nada se move por um vínculo deduzido.
    let m = Mundo::novo();
    let (ordens, pidf, abas) = (m.base.join("ordens-keepd"), m.base.join("concha.pid"), m.base.join("abas.json"));
    std::fs::write(&abas, "[]").unwrap();
    let keepd = sobe_processo(
        "keepd-antigo",
        None,
        None,
        &[],
        &[
            ("KEEP_WT_TESTE_SOCKET", texto(&m.amb.socket)),
            ("KEEP_WT_TESTE_ABAS", texto(&abas)),
            ("KEEP_WT_TESTE_ORDENS", texto(&ordens)),
            ("KEEP_WT_TESTE_PID", texto(&pidf)),
            ("KEEP_WT_TESTE_PASTA", m.raiz.clone()),
        ],
    );
    assert!(espera(20.0, || std::fs::read_to_string(&pidf).is_ok_and(|s| !s.is_empty())), "o keepd antigo não subiu");
    let pid: u32 = std::fs::read_to_string(&pidf).unwrap().trim().parse().unwrap();
    let mut c = Concha { filho: None, pid, ordens, agente: None };
    let agente = c.sobe("claude", &[]);
    let mut conversa = Conversa::nova(&m);
    sessao_claude(&m, agente, &conversa.sid);
    let pasta = texto(&processos::cwd(c.pid()).unwrap());
    std::fs::write(&abas, json!([{"ws": "W", "id": 1, "title": "✳ Outra coisa", "cwd": pasta}]).to_string()).unwrap();
    assert!(espera(20.0, || keep_proto::net::Stream::connect(&m.amb.socket).is_ok()), "o keepd antigo não atende");
    let (w, t) = m.wt("wt-talvez");
    conversa.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let r = listar_de_verdade(&m, &["W:1"]);
    let concha = c.pid();
    c.fim();
    mata(keepd);
    let a = &r.abas[0];
    assert!(a.vinculo == "provavel" && a.agente == "claude" && a.sessoes.is_empty(), "{:?}", r.abas);
    assert_eq!(a.pids, vec![agente, concha]);
    assert!(r.lixeira.is_empty(), "{r:?}");
    assert!(r.avisos.contains(&"não identifiquei a conversa da aba “Outra coisa”".to_string()), "{:?}", r.avisos);
    assert!(r.avisos.iter().any(|a| a.contains("anterior a esta versão do Keep")), "{:?}", r.avisos);
}

#[test]
fn t22c_historico_do_shell_entra_nas_sessoes() {
    let m = Mundo::novo();
    let k = Keepd::sobe(&m.amb.socket, true);
    let (c, agente, _nova) = aba_com_claude(&m, &k, 1, "✳ Minha conversa");
    // A conversa de antes do /clear, no mesmo shell.
    let mut antiga = Conversa::nova(&m);
    sessao_claude(&m, agente, &antiga.sid);
    let (w, t) = m.wt("wt-de-antes");
    antiga.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar_de_verdade(t + 60.0); // grava o histórico: shell → [antiga]
    let est = estado::carrega(&m.amb).unwrap();
    let chave = format!("{}:1791000000000", std::process::id());
    let h = &est.historico[&chave][&c.pid().to_string()];
    assert_eq!(h.ids, vec![antiga.sid.clone()], "{:?}", est.historico);
    // /clear: a sessão do processo agora é outra conversa.
    let nova = Conversa::nova(&m);
    sessao_claude(&m, agente, &nova.sid);
    let r = listar_de_verdade(&m, &["W:1"]);
    assert_eq!(r.abas[0].sessoes, vec![nova.sid.clone(), antiga.sid.clone()], "{:?}", r.abas);
    assert!(caminhos(&r) == vec![w.clone()] && r.lixeira[0].sessao == antiga.sid, "{r:?}");
    let concha = c.pid();
    c.fim();
    assert!(espera(10.0, || !processos::vivo(concha)));
    m.indexar_de_verdade(agora() + 1.0); // shell morto (aba fechada): sai do histórico
    let est = estado::carrega(&m.amb).unwrap();
    assert!(est.historico.get(&chave).is_some_and(|h| h.is_empty()), "{:?}", est.historico);
}

#[test]
fn t33_aba_do_codex_move_as_dele() {
    let m = Mundo::novo();
    let k = Keepd::sobe(&m.amb.socket, true);
    let id = uuid();
    let mut c = Concha::nova(&m, Path::new(&m.raiz));
    let cx = c.sobe("codex", &["resume", &id]);
    k.aba("W", aba(1, "gpt1", c.pid(), cx));
    let (w, t) = m.wt("wt-do-gpt");
    let mut r = Rollout::com(&m, t - 60.0, Some(&id), None, false);
    let cid = r.chamada(t - 0.3, &format!("git worktree add -q {w} HEAD"));
    r.saida(t + 0.3, &cid, "", 0.6, None);
    m.indexar_de_verdade(t + 60.0);
    let l = listar_de_verdade(&m, &["W:1"]);
    assert!(l.abas[0].agente == "codex" && l.abas[0].vinculo == "exato" && l.abas[0].sessoes == vec![id.clone()], "{:?}", l.abas);
    assert!(caminhos(&l) == vec![w.clone()] && l.lixeira[0].sessao == id && l.avisos.is_empty(), "{l:?}");
    // O mesmo shell já teve uma conversa do Claude: a worktree dela vai junto.
    c.mata_agente();
    let antes = {
        let a = c.sobe("claude", &[]);
        let mut antes = Conversa::nova(&m);
        sessao_claude(&m, a, &antes.sid);
        k.aba("W", aba(1, "gpt1", c.pid(), a));
        let (w2, t2) = m.wt("wt-do-claude-antes");
        antes.chamada(t2 - 0.3, Some(t2 + 0.3), &format!("git worktree add -q {w2} HEAD"));
        m.indexar_de_verdade(t2 + 60.0);
        (antes.sid.clone(), w2)
    };
    c.mata_agente();
    let cx = c.sobe("codex", &["resume", &id]);
    k.aba("W", aba(1, "gpt1", c.pid(), cx));
    let l = listar_de_verdade(&m, &["W:1"]);
    assert_eq!(l.abas[0].sessoes, vec![id.clone(), antes.0.clone()], "{:?}", l.abas);
    let mut esperado = vec![w.clone(), antes.1.clone()];
    esperado.sort();
    assert!(caminhos(&l) == esperado && l.avisos.is_empty(), "{l:?}");
    // O rollout some: o que ele já provou segue na lista, e a aba fica sabendo.
    std::fs::remove_file(&r.arq).unwrap();
    let l = listar_de_verdade(&m, &["W:1"]);
    let aviso = "conversa do Codex da aba “gpt1” sem registro legível: podem faltar worktrees dela nesta lista".to_string();
    assert!(caminhos(&l) == esperado && l.avisos == vec![aviso.clone()], "{l:?}");
    // E uma worktree nascida sem registro do Codex para provar: não vai.
    let (w3, t3) = m.wt("wt-gpt-sem-registro");
    m.indexar_de_verdade(t3 + 60.0);
    let l = listar_de_verdade(&m, &["W:1"]);
    c.fim();
    assert!(!caminhos(&l).contains(&w3) && l.avisos.first() == Some(&aviso), "{l:?}");
}

#[test]
fn t34_alvo_com_paineis() {
    let m = Mundo::novo();
    let k = Keepd::sobe(&m.amb.socket, true);
    let (p1, a1, mut c1) = aba_com_claude(&m, &k, 1, "✳ Painel um");
    let (p5, a5, mut c5) = aba_com_claude(&m, &k, 5, "✳ Painel cinco");
    let (p7, a7, c7) = aba_com_claude(&m, &k, 7, "✳ Painel sete");
    let (p9, _a9, _c9) = aba_com_claude(&m, &k, 9, "✳ Outra aba");
    let (w1, t1) = m.wt("wt-painel-um");
    c1.chamada(t1 - 0.3, Some(t1 + 0.3), &format!("git worktree add {w1} HEAD"));
    let (w5, t5) = m.wt("wt-painel-cinco");
    c5.chamada(t5 - 0.3, Some(t5 + 0.3), &format!("git worktree add {w5} HEAD"));
    m.indexar(t1.max(t5) + 60.0);
    p7.entra(&w1); // outro painel da MESMA aba trabalhando na worktree do painel 1: morre com ela
    p9.entra(&w5); // outra aba trabalhando na worktree do painel 5: fica
    let r = listar_de_verdade(&m, &["W:1,5,7"]);
    let pids = vec![a1, p1.pid(), a5, p5.pid(), a7, p7.pid()];
    for c in [p1, p5, p7, p9] {
        c.fim();
    }
    let a = &r.abas[0];
    assert!(a.alvo == "W:1,5,7" && a.vinculo == "exato" && a.agente == "claude", "{a:?}");
    assert_eq!(a.sessoes, vec![c1.sid.clone(), c5.sid.clone(), c7.sid.clone()]);
    assert_eq!(a.pids, pids);
    assert!(caminhos(&r) == vec![w1] && r.lixeira[0].processos.is_empty(), "{r:?}");
    assert_eq!(r.mantidas, vec![mantida(&w5, "em uso pela aba “Outra aba” (W)")]);
    assert!(r.avisos.is_empty(), "{:?}", r.avisos);
}

// ---------------------------------------------------------------- Codex real

fn dados(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("worktrees_dados").join(nome)
}

/// Troca um texto dentro de todas as strings de um valor JSON.
fn troca(v: &mut Value, de: &str, para: &str) {
    match v {
        Value::String(s) => {
            if s.contains(de) {
                *s = s.replace(de, para);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| troca(x, de, para)),
        Value::Object(o) => o.values_mut().for_each(|x| troca(x, de, para)),
        _ => {}
    }
}

/// Grava um rollout de verdade com os horários deslocados de `delta`.
fn grava_rollout(m: &Mundo, linhas: &[Value], tid: &str, delta: f64, trocas: &[(&str, &str)]) {
    let t0 = ts(linhas[0]["timestamp"].as_str().unwrap()).unwrap() + delta;
    let d = m.codex.join("sessions").join(data_local(t0, "%Y")).join(data_local(t0, "%m")).join(data_local(t0, "%d"));
    std::fs::create_dir_all(&d).unwrap();
    let arq = d.join(format!("rollout-{}-{tid}.jsonl", data_local(t0, "%Y-%m-%dT%H-%M-%S")));
    let mut f = std::fs::File::create(arq).unwrap();
    for l in linhas {
        let mut l = l.clone();
        let novo = iso(ts(l["timestamp"].as_str().unwrap()).unwrap() + delta);
        l["timestamp"] = json!(novo);
        for (de, para) in trocas {
            troca(&mut l, de, para);
        }
        writeln!(f, "{}", serde_json::to_string(&l).unwrap()).unwrap();
    }
}

#[test]
fn t39_codex_real_0157() {
    // Rollouts REAIS do codex 0.157.0 (sanitizados): caminho literal, laço com
    // $w e script. Os horários são deslocados para o nascimento das worktrees
    // do teste cair onde o real caiu dentro de cada chamada.
    let d: Value = serde_json::from_slice(&std::fs::read(dados("codex-real-0.157.0.json")).unwrap()).unwrap();
    let m = Mundo::novo();
    let base = m.raiz.clone();
    for conv in d["conversas"].as_array().unwrap() {
        let linhas = conv["linhas"].as_array().unwrap();
        let tid = linhas[0]["payload"]["id"].as_str().unwrap().to_string();
        let mut criadas = Vec::new();
        for x in conv["worktrees"].as_array().unwrap() {
            let nome = x["nome"].as_str().unwrap();
            criadas.push((m.wt_em(nome, None, None, Some(&Path::new(&base).join(nome))), x["nasceu"].as_f64().unwrap()));
        }
        let delta = (criadas[0].0).1 - criadas[0].1;
        grava_rollout(&m, linhas, &tid, delta, &[("@@BASE@@", &base)]);
        m.indexar((criadas.last().unwrap().0).1 + 60.0);
        for ((w, _), _) in &criadas {
            let dd = m.decisao_de(w);
            assert!(dd.estado == "dono" && dd.dono.as_deref() == Some(tid.as_str()) && dd.nivel.as_deref() == Some("A"),
                "{w} do codex real: {dd:?}");
        }
        let mut esperado: Vec<String> = criadas.iter().map(|((w, _), _)| w.clone()).collect();
        esperado.sort();
        assert_eq!(caminhos(&m.listar(&[&tid])), esperado);
    }
}

#[test]
fn t40_codex_real_processo_longo() {
    // Rollout REAL (code mode): "sleep 25; git worktree add …; echo pronta
    // <caminho>". A célula volta em 10 s com o processo vivo, duas consultas
    // write_stdin vazias, e a worktree nasce ENTRE elas: só a janela do
    // processo vivo, até a saída com exit_code, cobre o nascimento.
    let meta: Value = serde_json::from_slice(&std::fs::read(dados("nascimentos.json")).unwrap()).unwrap();
    let meta = &meta["rollouts"]["rollout-d-processo-longo.jsonl"];
    let thread = meta["thread"].as_str().unwrap();
    let m = Mundo::novo();
    let base = m.raiz.clone();
    let x = &meta["worktrees"][0];
    let nome = x["id"].as_str().unwrap();
    let (w, t) = m.wt_em(nome, None, None, Some(&Path::new(&base).join(nome)));
    let delta = t - x["nasceu"].as_f64().unwrap();
    let linhas: Vec<Value> = std::fs::read_to_string(dados("rollout-d-processo-longo.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let codex = texto(&m.codex);
    grava_rollout(&m, &linhas, thread, delta, &[("@RAIZ@", &base), ("@CODEX@", &codex), ("@HOME@", &base)]);
    m.indexar(t + 60.0);
    let dd = m.decisao_de(&w);
    assert!(dd.estado == "dono" && dd.dono.as_deref() == Some(thread) && dd.nivel.as_deref() == Some("A"), "{dd:?}");
    assert_eq!(caminhos(&m.listar(&[thread])), vec![w], "a aba do Codex leva a worktree");
}

#[test]
fn t40b_saida_do_comando_nao_abre_a_janela() {
    // Um comando que cita a worktree e IMPRIME "Process running with session
    // ID 5" terminou (o cabeçalho diz que saiu): a janela dele fecha na saída.
    let m = Mundo::novo();
    let mut r = Rollout::novo(&m, agora() - 400.0);
    let cid = r.chamada(agora() - 300.0, "cat notas.txt  # wt-risco");
    r.saida(agora() - 299.9, &cid, "Process running with session ID 5\n", 0.1, None);
    let (w, t) = m.wt("wt-risco"); // nasce 5 min depois, criada por ninguém
    let cid2 = r.chamada(t + 10.0, "ls"); // a conversa segue viva depois
    r.saida(t + 10.1, &cid2, "", 0.1, None);
    m.indexar(t + 60.0);
    let dd = m.decisao_de(&w);
    assert!(dd.estado != "dono" && !dd.niveis.contains_key(&r.tid), "o impresso não é o envelope: {dd:?}");
}

#[test]
fn t41_worktree_criada_pelo_codex_diz_o_dono() {
    // codex --worktree: o próprio Codex cria a worktree antes do session_meta
    // e grava <comum>/worktrees/<id>/codex-thread.json {version, ownerThreadId}.
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-gerenciada");
    let tid = uuid();
    let gitdir = std::fs::read_to_string(Path::new(&w).join(".git")).unwrap();
    let adm = gitdir.trim().strip_prefix("gitdir:").unwrap().trim().to_string();
    std::fs::write(Path::new(&adm).join("codex-thread.json"), json!({"version": 1, "ownerThreadId": tid}).to_string()).unwrap();
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), "git worktree list  # wt-gerenciada"); // outra conversa citando: não muda
    m.indexar(t + 60.0);
    let dd = m.decisao_de(&w);
    assert!(dd.estado == "dono" && dd.dono.as_deref() == Some(tid.as_str()), "{dd:?}");
    assert_eq!(dd.prova.as_ref().and_then(|p| p["ferramenta"].as_str()), Some("codex:codex-thread.json"));
    assert_eq!(caminhos(&m.listar(&[&tid])), vec![w]);
    assert!(m.listar(&[&c1.sid]).lixeira.is_empty(), "só a aba do Codex a leva");
}

// ---------------------------------------------------------------- indexar e contrato

#[test]
fn t24_indexar_sem_novidade_e_barato() {
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-barata");
    Conversa::nova(&m).chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let t0 = Instant::now();
    let r = m.indexar(t + 120.0);
    let dt = t0.elapsed().as_secs_f64();
    // O git do Windows custa mais por chamada (e o indexar refaz o de cada
    // worktree depois de um minuto).
    let teto = if cfg!(windows) { 3.0 } else { 1.0 };
    assert!(r.decididas_agora == 0 && r.indice["bytes_lidos"] == json!(0) && dt < teto, "{r:?} {dt}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modo = std::fs::metadata(estado::arquivo(&m.amb)).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo, 0o600, "o estado é só do dono");
    }
}

#[test]
fn t25_linha_de_comando_sempre_json() {
    let m = Mundo::novo();
    let cli = |a: &[&str]| {
        let args: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        let (rc, s) = worktrees::executar(&m.amb, &args, 0.0);
        (rc, serde_json::from_str::<Value>(&s).unwrap_or_else(|e| panic!("não é JSON ({e}): {s}")))
    };
    let (rc, d) = cli(&["listar", "W:1"]); // sem keepd no socket
    let chaves: HashSet<&str> = d.as_object().unwrap().keys().map(String::as_str).collect();
    assert!(rc == 0 && chaves == HashSet::from(["versao", "abas", "lixeira", "mantidas", "avisos"]) && d["versao"] == json!(1), "{d}");
    assert_eq!(d["abas"], json!([{"alvo": "W:1", "agente": "nenhum", "vinculo": "nenhum", "sessoes": [], "pids": []}]));
    assert!(d["avisos"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains("não respondeu")), "{d}");
    let (rc, d) = cli(&["listar", "semdoispontos"]);
    assert!(rc == 2 && d.get("erro").is_some(), "{d}");
    let (rc, d) = cli(&["preparar"]);
    assert!(rc == 2 && d["ok"] == json!(false), "{d}");
    let falta = texto(&m.base.join("nao-existe"));
    let (rc, d) = cli(&["preparar", &falta]);
    assert_eq!((rc, d), (0, json!({"versao": 1, "ok": false, "motivo": "caminho inexistente", "processos": []})));
    let destino = texto(&m.base.join("x"));
    let (rc, d) = cli(&["concluir", &m.repo, &destino]);
    assert!(rc == 0 && d["ok"] == json!(false) && d["motivo"].as_str().unwrap().contains("ainda existe"), "{d}");
    let (rc, d) = cli(&["indexar", "--json"]);
    assert!(rc == 0 && d["versao"] == json!(1) && d.get("worktrees").is_some(), "{d}");
    let (rc, d) = cli(&["sumir"]);
    assert!(rc == 2 && d["erro"].as_str().unwrap().contains("desconhecido"), "{d}");
}

// ---------------------------------------------------------------- o que é deste porte

#[test]
fn decisoes_do_kit_valem_sem_estado_proprio() {
    // Quem usava o kit: a decisão gravada lá vale aqui antes do primeiro indexar.
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-do-kit");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    let kit = m.base.join("kit");
    let mut amb_kit = m.amb.clone();
    amb_kit.estado = kit.clone();
    worktrees::indexar_em(&amb_kit, Some(t + 60.0)); // o kit decidiu, no estado dele
    std::fs::remove_file(&c1.arq).unwrap(); // e o transcrito já não existe
    let mut m2 = m;
    m2.amb.kit = Some(kit);
    assert!(!estado::arquivo(&m2.amb).exists());
    let r = m2.listar(&[&c1.sid]);
    assert_eq!(caminhos(&r), vec![w.clone()], "{r:?}");
    assert!(r.avisos.is_empty(), "{:?}", r.avisos);
    let est = estado::carrega(&m2.amb).unwrap();
    assert!(est.trazido_do_kit.is_some() && est.nascimentos.values().any(|d| d.caminho.as_deref() == Some(w.as_str())));
}

#[test]
fn historico_do_kit_vale_para_o_daemon_que_roda() {
    // O kit viu, neste shell e sob este daemon, uma conversa que já não roda:
    // a worktree dela vai com a aba, antes mesmo do primeiro indexar daqui.
    let m = Mundo::novo();
    let k = Keepd::sobe(&m.amb.socket, true);
    let (c, _agente, _atual) = aba_com_claude(&m, &k, 1, "✳ Agora");
    let mut antiga = Conversa::nova(&m);
    let (w, t) = m.wt("wt-do-kit-antes");
    antiga.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    let kit = m.base.join("kit");
    std::fs::create_dir_all(&kit).unwrap();
    let nasceu = processos::um(c.pid()).unwrap().inicio_ms as f64 / 1000.0;
    // O kit guardava o daemon pelo nascimento do socket ("%.6f"); este diz
    // que nasceu em 1791000000000 ms.
    // O mesmo pid sob outro daemon (de antes de um reinício) não vale.
    let historico = json!({"1791000000.000000": {c.pid().to_string(): {"ids": [antiga.sid], "nasceu": nasceu}},
                           "1700000000.000000": {c.pid().to_string(): {"ids": ["de-outro-daemon"], "nasceu": nasceu}}});
    std::fs::write(kit.join("worktrees.json"),
        json!({"versao": 1, "nascimentos": {}, "historico": historico, "repos": []}).to_string()).unwrap();
    let mut m = m;
    m.amb.kit = Some(kit);
    let r = listar_de_verdade(&m, &["W:1"]);
    let s = &r.abas[0].sessoes;
    assert!(s.contains(&antiga.sid) && !s.contains(&"de-outro-daemon".to_string()), "{:?}", r.abas);
    assert_eq!(caminhos(&r), vec![w.clone()], "{r:?}");
    m.indexar_de_verdade(t + 60.0); // o indexar o adota de vez, pela chave daqui
    let est = estado::carrega(&m.amb).unwrap();
    let chave = format!("{}:1791000000000", std::process::id());
    let h = &est.historico[&chave][&c.pid().to_string()];
    c.fim();
    assert!(h.ids.contains(&antiga.sid) && !h.ids.contains(&"de-outro-daemon".to_string()), "{:?}", est.historico);
    assert!(est.historico_do_kit.is_none() && est.historico.len() == 1, "{:?}", est.historico);
}

#[test]
fn pid_de_um_git_que_acabou_nao_esconde_quem_esta_dentro() {
    // O Windows dá logo a um processo novo o pid de um que acabou, e um git
    // deste processo fica na lista dos dele por um minuto: o shell de outra
    // aba que pegou esse pid sumia da varredura, e a worktree em que ele
    // trabalha ia para a lista da Lixeira (o t34 no Windows, uma vez em três).
    let m = Mundo::novo();
    let (w, t) = m.wt("wt-pid-reaproveitado");
    let mut c1 = Conversa::nova(&m);
    c1.chamada(t - 0.3, Some(t + 0.3), &format!("git worktree add {w} HEAD"));
    m.indexar(t + 60.0);
    let outro = dorme(Some(Path::new(&w))); // o shell de outra aba, dentro da worktree
    let pid = outro.id();
    assert!(espera(10.0, || processos::cwd(pid).is_some()));
    let nasceu = processos::inicio_ms(pid).unwrap();
    // Um git deste processo que teve o mesmo pid e acabou antes de ele nascer.
    worktrees::git::anota_para_teste(pid, nasceu - 60_000, Some(nasceu - 30_000));
    let (r, _) = m.listar_com(&[&c1.sid], Op { outras: vec![pid], rotulos: vec![(pid, "Outra", "W")], ..Op::default() });
    // O mesmo processo (o pid e a hora) anotado como git deste: esse sai.
    worktrees::git::anota_para_teste(pid, nasceu, None);
    let saiu = varredura::processos().procs.iter().all(|p| p.pid != pid);
    worktrees::git::esquece_para_teste(pid);
    mata(outro);
    assert!(r.lixeira.is_empty() && r.mantidas == vec![mantida(&w, "em uso pela aba “Outra” (W)")], "{r:?}");
    assert!(saiu, "um git deste processo (o mesmo pid e a mesma hora) ficou na varredura");
}

#[test]
fn mover_para_a_lixeira_e_concluir() {
    let m = Mundo::novo();
    let (w, _) = m.wt_em("wt-vai", None, Some("ramo-vai"), None);
    let mut amb = m.amb.clone();
    amb.ganchos.lixeira_falsa = Some(m.lixeira.clone());
    let p = worktrees::preparar_em(&amb, Path::new(&w), None);
    assert!(p.ok, "{p:?}");
    let destino = worktrees::mover_para_lixeira_em(&amb, Path::new(&w)).unwrap();
    assert!(!Path::new(&w).exists() && destino.join("a.txt").exists() && destino.starts_with(&m.lixeira));
    let c = worktrees::concluir_em(&amb, Path::new(&w), &destino);
    assert!(c.ok && c.registro_removido == Some(true), "{c:?}");
    g(&m.repo, &["branch", "-D", "ramo-vai"]);
}
