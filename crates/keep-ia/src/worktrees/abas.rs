//! As abas que fecham: que shell está por trás de cada uma (o daemon diz,
//! com `List3`), que conversa roda nele, e quais já rodaram (o histórico que
//! o `indexar` grava).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use keep_proto::TabInfo;
use serde_json::Value;

use super::AbaListada;
use super::ambiente::Ambiente;
use super::codex;
use super::estado::{Estado, Historia, chave_keepd, historico_da_concha, nascimento_do_processo};
use super::prazo::Prazo;
use super::sessoes::SessaoViva;
use super::texto::{e_uuid, titulo_limpo};
use crate::processos::{self, Processo};
use crate::tela::{Programa, programa};

/// Um alvo do `listar`: `<workspace>:<aba>[,<painel>…]` — uma aba e os
/// painéis dela, que fecham juntos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alvo {
    pub texto: String,
    pub ws: String,
    pub ids: Vec<u32>,
}

/// Lê um alvo da linha de comando.
pub fn analisa_alvo(s: &str) -> Result<Alvo, String> {
    let Some((ws, resto)) = s.rsplit_once(':').filter(|(ws, _)| !ws.is_empty()) else {
        return Err(format!("ALVO sem workspace: {s:?} (use <workspace>:<aba>[,<painel>...])"));
    };
    let mut ids = Vec::new();
    for x in resto.split(',') {
        let x = x.trim();
        let id: u32 = x
            .parse()
            .ok()
            .filter(|_| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| format!("ALVO com id de aba inválido: {s:?}"))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(Alvo { texto: s.to_string(), ws: ws.to_string(), ids })
}

/// O que o `listar` precisa saber das abas além da resposta.
#[derive(Clone, Debug, Default)]
pub struct Contexto {
    /// Os shells das abas que fecham: o que roda neles morre junto.
    pub conchas_fechando: HashSet<u32>,
    /// Todos os shells do keepd.
    pub conchas_keepd: HashSet<u32>,
    /// O nome de cada shell no diálogo: (aba, workspace).
    pub rotulos: HashMap<u32, (String, String)>,
    /// As pastas das abas que fecham (para achar os repositórios).
    pub extras: Vec<String>,
    /// O daemon, como chave do histórico (só o que diz os processos).
    pub daemon: Option<String>,
}

/// O que roda num shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parte {
    pub concha: u32,
    /// O processo do agente (o Claude ou o Codex), se há um.
    pub pid: Option<u32>,
    /// `claude`, `codex` ou `shell`.
    pub agente: &'static str,
    /// As conversas que rodam nele agora.
    pub ids: Vec<String>,
}

fn e_programa(pid: u32, lista: &[Processo], qual: &Programa) -> bool {
    processos::programa(pid).is_some_and(|p| &programa(&p) == qual)
        || lista.iter().find(|p| p.pid == pid).is_some_and(|p| &programa(&p.nome) == qual)
}

fn profundidade(pid: u32, concha: u32, lista: &[Processo]) -> usize {
    let mut p = pid;
    for n in 0..64 {
        if p == concha {
            return n;
        }
        match lista.iter().find(|x| x.pid == p) {
            Some(x) if x.ppid > 1 => p = x.ppid,
            _ => break,
        }
    }
    usize::MAX
}

/// A conversa (ou as conversas) que roda no shell `concha`, cujo processo
/// da frente é `frente`.
pub fn identifica(amb: &Ambiente, concha: u32, frente: u32, lista: &[Processo], vivas: &[SessaoViva]) -> Parte {
    let mut debaixo: HashSet<u32> = processos::descendentes_em(lista, concha).into_iter().collect();
    if frente != concha && frente != 0 {
        debaixo.insert(frente);
    }
    let mut parte = Parte { concha, pid: None, agente: "shell", ids: Vec::new() };
    // O Claude: pela sessão que ele publica.
    let mut claude: Vec<&SessaoViva> = vivas
        .iter()
        .filter(|s| debaixo.contains(&s.pid) && matches!(s.kind.as_deref(), None | Some("interactive")))
        .collect();
    claude.sort_by_key(|s| (s.pid != frente, profundidade(s.pid, concha, lista), s.pid));
    if let Some(s) = claude.first() {
        // A estacionada vale pela sessão de segundo plano, que é o que a aba
        // mostra (a mesma regra do kit).
        let id = s
            .parked
            .as_ref()
            .and_then(|job| vivas.iter().find(|b| b.kind.as_deref() == Some("bg") && b.job.as_ref() == Some(job)))
            .map(|b| b.id.clone())
            .unwrap_or_else(|| s.id.clone());
        parte.agente = "claude";
        parte.pid = Some(s.pid);
        parte.ids.push(id);
        return parte;
    }
    // O Codex: o processo de cima (os auxiliares dele também se chamam codex).
    let mut cx: Vec<u32> = debaixo
        .iter()
        .copied()
        .filter(|&p| e_programa(p, lista, &Programa::Codex))
        .filter(|&p| {
            let pai = lista.iter().find(|x| x.pid == p).map(|x| x.ppid).unwrap_or(0);
            !(pai != 0 && pai != concha && e_programa(pai, lista, &Programa::Codex))
        })
        .collect();
    cx.sort_by_key(|&p| (p != frente, profundidade(p, concha, lista), p));
    if let Some(&p) = cx.first() {
        parte.agente = "codex";
        parte.pid = Some(p);
        parte.ids = threads_do_codex(amb, p);
        return parte;
    }
    // O Claude sem sessão publicada (versão antiga, ou ainda abrindo).
    let mut cl: Vec<u32> = debaixo.iter().copied().filter(|&p| e_programa(p, lista, &Programa::Claude)).collect();
    cl.sort_by_key(|&p| (p != frente, profundidade(p, concha, lista), p));
    if let Some(&p) = cl.first() {
        parte.agente = "claude";
        parte.pid = Some(p);
    }
    parte
}

/// As conversas de um processo do Codex: os locks de thread que ele segura
/// abertos (`<CODEX_HOME>/thread-writer-locks/<id>.lock`; desde o 0.159 eles
/// são do daemon do Codex, não da tela) e o `resume <id>` da linha de
/// comando. Uma por subagente também vale: a prova deles conta pela de cima.
pub fn threads_do_codex(amb: &Ambiente, pid: u32) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let home = processos::variavel(pid, "CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| amb.codex.clone());
    let locks = std::fs::canonicalize(home.join("thread-writer-locks")).unwrap_or(home.join("thread-writer-locks"));
    for a in processos::arquivos_abertos(pid) {
        let real = std::fs::canonicalize(&a).unwrap_or(a);
        if real.parent() != Some(locks.as_path()) {
            continue;
        }
        let nome = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(id) = nome.strip_suffix(".lock").filter(|n| !n.is_empty() && !n.starts_with('.')) {
            if !ids.iter().any(|x| x == id) {
                ids.push(id.to_string());
            }
        }
    }
    if let Some(a) = processos::argv(pid) {
        for par in a.windows(2) {
            if par[0] == "resume" && e_uuid(&par[1]) && !ids.contains(&par[1]) {
                ids.push(par[1].clone());
            }
        }
    }
    ids
}

/// Os nomes que a pessoa deu às abas no app do macOS (`names.json`), se
/// valem para este daemon (o app os guarda pelo nascimento do socket).
pub fn nomes_do_app(amb: &Ambiente, nascimento_do_socket: Option<f64>) -> HashMap<(String, u32), String> {
    let mut saida = HashMap::new();
    let (Some(app), Some(nasceu)) = (&amb.app, nascimento_do_socket) else { return saida };
    let Some(v) = std::fs::read(app.join("names.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return saida;
    };
    if v.get("daemonStart").and_then(Value::as_f64).is_none_or(|d| (d - nasceu).abs() > 0.01) {
        return saida;
    }
    for (k, nome) in v.get("tabs").and_then(Value::as_object).into_iter().flatten() {
        let Some((ws, tab)) = k.rsplit_once('\u{1f}') else { continue };
        let (Ok(tab), Some(nome)) = (tab.parse::<u32>(), nome.as_str().map(str::trim)) else { continue };
        if !ws.is_empty() && !nome.is_empty() {
            saida.insert((ws.to_string(), tab), nome.to_string());
        }
    }
    saida
}

/// Quando o socket do daemon nasceu (unix): é por ele que o app e o kit
/// guardavam o que valia só para um daemon.
pub fn nascimento_do_socket(amb: &Ambiente) -> Option<f64> {
    if cfg!(windows) {
        return None;
    }
    let m = std::fs::metadata(&amb.socket).ok()?;
    Some(super::pastas::nascimento(&m))
}

fn raiz_da_aba(t: &TabInfo) -> u32 {
    if t.split_of != keep_proto::TAB_ANY { t.split_of } else { t.id }
}

fn rotulo_da_aba(ws: &str, tab: u32, abas: &HashMap<(String, u32), TabInfo>, nomes: &HashMap<(String, u32), String>) -> String {
    let info = abas.get(&(ws.to_string(), tab));
    let raiz = info.map(raiz_da_aba).unwrap_or(tab);
    if let Some(n) = nomes.get(&(ws.to_string(), raiz)).or_else(|| nomes.get(&(ws.to_string(), tab))) {
        return n.clone();
    }
    let titulo = |t: Option<&TabInfo>| t.map(|t| titulo_limpo(&t.title)).filter(|s| !s.is_empty());
    titulo(info)
        .or_else(|| titulo(abas.get(&(ws.to_string(), raiz))))
        .unwrap_or_else(|| format!("tab {tab}"))
}

enum Estado_ {
    Inexistente,
    Ambiguo,
    Achada { parte: Parte, exato: bool },
}

/// Os alvos viram as entradas de `abas` e o contexto do `listar`.
pub fn resolver(
    amb: &Ambiente,
    alvos: &[Alvo],
    prazo: &Prazo,
    avisos: &mut Vec<String>,
    est: Option<&Estado>,
    vivas: &[SessaoViva],
) -> (Vec<AbaListada>, Contexto) {
    if let Some(dado) = &amb.ganchos.vinculo {
        return dado(alvos);
    }
    let mut ctx = Contexto::default();
    let espera = prazo.restante().clamp(0.3, 2.0);
    let r = match crate::daemon::listar_em(&amb.socket, Some(Duration::from_secs_f64(espera))) {
        Ok(r) => r,
        Err(e) => {
            avisos.push(format!("o Keep (keepd) não respondeu: nada será movido ({e})"));
            let saida = alvos.iter().map(|a| AbaListada::nenhum(&a.texto)).collect();
            return (saida, ctx);
        }
    };
    let vinc = crate::vinculo::vincular(&r);
    let abas: HashMap<(String, u32), TabInfo> =
        r.abas.iter().filter(|a| !a.info.finished).map(|a| ((a.ws.clone(), a.info.id), a.info.clone())).collect();
    if r.exato {
        ctx.daemon = Some(chave_keepd(&r.daemon));
    }
    let nomes = nomes_do_app(amb, nascimento_do_socket(amb));
    let lista = processos::todos();
    for ((ws, id), v) in &vinc {
        ctx.conchas_keepd.insert(v.shell);
        ctx.rotulos.insert(v.shell, (rotulo_da_aba(ws, *id, &abas, &nomes), ws.clone()));
    }
    let mut partes: HashMap<(String, u32), Estado_> = HashMap::new();
    for alvo in alvos {
        for &tab in &alvo.ids {
            let k = (alvo.ws.clone(), tab);
            if partes.contains_key(&k) {
                continue;
            }
            let Some(info) = abas.get(&k) else {
                partes.insert(k, Estado_::Inexistente);
                continue;
            };
            ctx.extras.push(info.cwd.clone());
            let e = match vinc.get(&k) {
                None => Estado_::Ambiguo,
                Some(v) => {
                    Estado_::Achada { parte: identifica(amb, v.shell, v.frente, &lista, vivas), exato: v.exato }
                }
            };
            partes.insert(k, e);
        }
    }
    if !r.exato && !alvos.is_empty() {
        avisos.push(
            "o keepd em execução é anterior a esta versão do Keep: as worktrees das abas só vão para a \
             lixeira depois que ele reiniciar"
                .into(),
        );
    }
    let legiveis = std::cell::OnceCell::new();
    let mut saida = Vec::new();
    for alvo in alvos {
        let rot = rotulo_da_aba(&alvo.ws, alvo.ids[0], &abas, &nomes);
        let (mut sess, mut pids, mut agentes, mut codex_ids) = (Vec::<String>::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut ambiguo, mut provavel, mut some) = (false, false, false);
        for &tab in &alvo.ids {
            match &partes[&(alvo.ws.clone(), tab)] {
                Estado_::Ambiguo => ambiguo = true,
                Estado_::Inexistente => some = true,
                Estado_::Achada { parte, exato } => {
                    if !*exato {
                        provavel = true;
                    }
                    ctx.conchas_fechando.insert(parte.concha);
                    let historia = historico_da_concha(est, ctx.daemon.as_deref(), parte.concha, false);
                    for i in parte.ids.iter().chain(historia.iter()) {
                        if !sess.contains(i) {
                            sess.push(i.clone());
                        }
                    }
                    if parte.agente == "codex" {
                        codex_ids.extend(parte.ids.iter().cloned());
                    }
                    codex_ids.extend(historico_da_concha(est, ctx.daemon.as_deref(), parte.concha, true));
                    if let Some(p) = parte.pid {
                        pids.push(p);
                    }
                    pids.push(parte.concha);
                    agentes.push(parte.agente);
                    ctx.rotulos.insert(parte.concha, (rot.clone(), alvo.ws.clone()));
                }
            }
        }
        let agente = agentes
            .iter()
            .copied()
            .find(|a| *a == "claude" || *a == "codex")
            .unwrap_or(if agentes.is_empty() { "nenhum" } else { "shell" });
        let vinculo = if ambiguo {
            "ambiguo"
        } else if provavel {
            "provavel"
        } else if !sess.is_empty() || (agente == "shell" && !agentes.is_empty()) {
            "exato"
        } else {
            "nenhum"
        };
        match vinculo {
            "exato" => {
                if !codex_ids.is_empty() {
                    let leg = legiveis.get_or_init(|| codex::threads_legiveis(amb));
                    if codex_ids.iter().any(|t| !leg.contains(t)) {
                        // Conversa do Codex sem rollout legível agora: o que ela já
                        // provou ter criado segue decidido; o que não estava decidido
                        // não tem como ser atribuído, e a aba fica sabendo.
                        avisos.push(format!(
                            "conversa do Codex da aba “{rot}” sem registro legível: podem faltar worktrees dela nesta lista"
                        ));
                    }
                }
            }
            "nenhum" if some && agentes.is_empty() => {
                avisos.push(format!("a aba “{rot}” ({}) não existe mais no Keep", alvo.ws));
            }
            _ => avisos.push(format!("não identifiquei a conversa da aba “{rot}”")),
        }
        saida.push(AbaListada {
            alvo: alvo.texto.clone(),
            agente: agente.into(),
            vinculo: vinculo.into(),
            sessoes: if vinculo == "exato" { sess } else { Vec::new() },
            pids,
        });
    }
    (saida, ctx)
}

/// O `indexar` acumula as conversas de cada shell do daemon que está
/// rodando (só o que diz os processos): `{pid do shell: {ids, nasceu}}`. Os
/// de outro daemon e os shells que morreram saem. Devolve quantas conversas
/// novas entraram.
pub fn atualizar_historico(amb: &Ambiente, est: &mut Estado, r: &crate::daemon::Retrato, vivas: &[SessaoViva]) -> usize {
    if !r.exato {
        return 0;
    }
    let k = chave_keepd(&r.daemon);
    est.historico.retain(|x, _| *x == k);
    let lista = processos::todos();
    let vinc = crate::vinculo::vincular(r);
    let cur = est.historico.entry(k).or_default();
    let mut novos = 0;
    for v in vinc.values().filter(|v| v.exato) {
        let p = identifica(amb, v.shell, v.frente, &lista, vivas);
        if p.ids.is_empty() {
            continue;
        }
        let Some(nasceu) = nascimento_do_processo(v.shell) else { continue };
        let ent = cur.entry(v.shell.to_string()).or_default();
        if (ent.nasceu - nasceu).abs() > 0.01 {
            *ent = Historia { ids: Vec::new(), nasceu, codex: Vec::new() };
        }
        for id in &p.ids {
            if !ent.ids.contains(id) {
                ent.ids.push(id.clone());
                novos += 1;
            }
            if p.agente == "codex" && !ent.codex.contains(id) {
                ent.codex.push(id.clone());
            }
        }
    }
    cur.retain(|pid, h| {
        pid.parse::<u32>().ok().and_then(nascimento_do_processo).is_some_and(|n| (h.nasceu - n).abs() <= 0.01)
    });
    novos
}
