//! A varredura: as worktrees que a conversa de uma aba criou e que ficaram
//! para trás porque a aba fechou sem o diálogo — o shell saiu (`exit`,
//! Ctrl-D), o workspace acabou por outro caminho, o computador reiniciou e
//! a aba não voltou. A regra é a da pessoa ("sempre que uma aba é fechada no
//! keep a sua pasta wt deve ser deletada") e o destino é o do diálogo: a
//! Lixeira, com os commits guardados em `refs/keep-lixeira`. Aqui o
//! consentimento é a própria regra, sem pergunta; por isso tudo é mais
//! desconfiado que no diálogo, e na dúvida a worktree fica.
//!
//! Órfã = decidida (dona, não provisória), e a conversa dona:
//! - rodou numa aba do Keep que fechou (`encerradas`, gravado pelo
//!   `indexar`): conversa do Terminal, de um editor ou de um `claude -p`
//!   agendado nunca passou por uma aba e nunca é varrida;
//! - não tem aba aberta (nem rodando num shell, nem no histórico de um), nem
//!   ela nem a conversa em que ela seguiu (`continuedInSessionId`, ao ir
//!   para segundo plano);
//! - não é devida pela restauração das abas (`keep restauro`): enquanto ela
//!   não acaba neste keepd, toda conversa dos retratos guardados conta como
//!   viva; depois, a de cada aba que ela recriou e segue aberta — a conversa
//!   que não voltou deixa a aba só com o shell, e aba aberta está em uso;
//! - não roda em lugar nenhum: nenhuma sessão do Claude viva com o id,
//!   nenhum Codex com a thread aberta, nenhum processo com o id na linha de
//!   comando (o cartão de retomada que espera o ENTER);
//! - não mexe no transcrito (ou no rollout) há `CARENCIA`.
//!
//! O keepd tem de dizer os processos das abas (`List3`) e estar de pé há
//! `CARENCIA`: depois de um reinício as abas voltam aos poucos. A worktree
//! em si: nem principal, nem travada, nem com submódulos, nem já na
//! Lixeira, sem processo nenhum com a pasta atual dentro e sem outra
//! conversa viva trabalhando nela. Só depois de órfã por `CARENCIA`
//! seguidos ela vai (`orfas` no estado guarda desde quando).
//!
//! Registro cuja pasta já sumiu (`prunable`) de dona órfã: sai só aquele
//! registro (`esquecer`), com a HEAD guardada — nunca `git worktree prune`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::abas::{abas_vivas, threads_do_codex};
use super::ambiente::Ambiente;
use super::codex;
use super::dono::Decisao;
use super::estado;
use super::git::git;
use super::listar::{detalhes_git, em_uso, na_lixeira, sessoes_vivas};
use super::pastas::{absoluto, e_pasta, fora_do_disco, mesmo, mtime, normaliza, outra_maquina, pai};
use super::prazo::{Estourou, Prazo};
use super::repos::{self, Worktree, chave};
use super::sessoes;
use super::texto::{agora, iso};
use super::varredura;
use super::{CARENCIA, Conclusao, ItemDaLixeira, LOG_LIXEIRA, Mantida, Orfas};
use crate::tela::{Programa, programa};

/// O tempo de cada git daqui.
const PRAZO_GIT: f64 = 20.0;

fn min(s: f64) -> i64 {
    (s.max(0.0) / 60.0).floor() as i64
}

/// O repositório guarda submódulos: o registro da worktree leva os
/// repositórios deles, e sair com ele os apagaria.
fn com_submodulos(w: &Worktree) -> bool {
    Path::new(&w.repo).join(".gitmodules").exists()
        || Path::new(&w.caminho).join(".gitmodules").exists()
        || Path::new(&w.adm).join("modules").exists()
}

/// Uma pasta que sumiu pode estar só num disco desmontado agora: aí o
/// registro dela fica.
pub fn pode_esquecer(cam: &str) -> bool {
    if outra_maquina(cam) || fora_do_disco(cam) {
        return false;
    }
    if cfg!(target_os = "macos") {
        return !(cam == "/Volumes" || cam.starts_with("/Volumes/"));
    }
    if cfg!(windows) {
        let raiz: std::path::PathBuf = Path::new(cam)
            .components()
            .take_while(|c| !matches!(c, std::path::Component::Normal(_)))
            .collect();
        return !raiz.as_os_str().is_empty() && raiz.is_dir();
    }
    !["/media/", "/mnt/", "/run/media/"].iter().any(|m| cam.starts_with(m))
}

fn e_codex(pid: u32, nome: &str) -> bool {
    programa(nome) == Programa::Codex
        || crate::processos::programa(pid).is_some_and(|p| programa(&p) == Programa::Codex)
}

/// Quando a conversa mexeu pela última vez no transcrito (Claude) ou no
/// rollout (Codex).
fn mexeu_em(amb: &Ambiente, id: &str, codex: bool, rollouts: &mut Option<HashMap<String, f64>>) -> Option<f64> {
    let claude = || {
        sessoes::transcrito_de(id, &amb.projetos).and_then(|p| std::fs::metadata(p).ok()).map(|m| mtime(&m))
    };
    if !codex {
        return claude();
    }
    let r = rollouts.get_or_insert_with(|| {
        let mut m: HashMap<String, f64> = HashMap::new();
        for (p, tid) in codex::rollouts(amb) {
            if let Some(t) = std::fs::metadata(&p).ok().map(|m| mtime(&m)) {
                let e = m.entry(tid).or_insert(t);
                *e = e.max(t);
            }
        }
        m
    });
    r.get(id).copied().or_else(claude)
}

/// As conversas de `donas` que rodam agora em algum lugar: a sessão do
/// Claude publicada, a thread aberta num Codex, o id na linha de comando de
/// qualquer processo (`--resume <id>`, `resume <id>`, o cartão do
/// cc-window que espera o ENTER).
fn rodando(amb: &Ambiente, donas: &HashSet<String>, vivas: &[sessoes::SessaoViva]) -> HashSet<String> {
    let mut saida: HashSet<String> = vivas.iter().filter(|s| donas.contains(&s.id)).map(|s| s.id.clone()).collect();
    if donas.is_empty() {
        return saida;
    }
    let eu = std::process::id();
    for p in crate::processos::todos() {
        if p.pid == eu {
            continue;
        }
        if e_codex(p.pid, &p.nome) {
            saida.extend(threads_do_codex(amb, p.pid).into_iter().filter(|t| donas.contains(t)));
        }
        if let Some(a) = crate::processos::argv(p.pid) {
            for id in donas {
                if a.iter().any(|x| x.contains(id.as_str())) {
                    saida.insert(id.clone());
                }
            }
        }
    }
    saida
}

fn le_json(p: &Path) -> Option<serde_json::Value> {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()).filter(|v| v.is_object())
}

/// O keepd de um arquivo da restauração é o que responde agora (pelo pid e
/// pelo nascimento do `List3`).
fn e_o_keepd(k: &serde_json::Value, daemon: &keep_proto::DaemonInfo) -> bool {
    k["pid"].as_u64() == Some(u64::from(daemon.pid)) && k["inicio_ms"].as_u64().unwrap_or(0) == daemon.started_ms
}

/// As conversas que a restauração das abas (`keep restauro`, o estado em
/// `restaurado.json`, `retrato.json` e `anterior.json`) ainda deve, ou cuja
/// aba ela recriou e segue aberta. Enquanto ela não acaba neste keepd
/// (`feito` falso), toda conversa dos retratos de outro keepd: depois de um
/// reinício as abas voltam primeiro só com o shell e as conversas sobem uma
/// a uma, sem processo nenhum que cite o id até lá. Depois que acaba, a de
/// cada aba recriada que o keepd ainda tem: a conversa que não voltou
/// (transcrito sumido, falha ao subir) deixa a aba aberta só com o shell.
fn devidas_pela_restauracao(amb: &Ambiente, retrato: &crate::daemon::Retrato) -> HashSet<String> {
    let mut saida = HashSet::new();
    let Some(pasta) = amb.restauro.as_ref() else { return saida };
    let Some(reg) = le_json(&pasta.join("restaurado.json")).filter(|r| e_o_keepd(&r["keepd"], &retrato.daemon)) else {
        return saida;
    };
    let retratos: Vec<serde_json::Value> =
        ["retrato.json", "anterior.json"].iter().filter_map(|n| le_json(&pasta.join(n))).collect();
    let conversa = |a: &serde_json::Value| a["ia"]["conversa"].as_str().filter(|c| !c.is_empty()).map(str::to_string);
    if reg["feito"].as_bool() != Some(true) {
        for r in retratos.iter().filter(|r| !e_o_keepd(&r["keepd"], &retrato.daemon)) {
            saida.extend(r["abas"].as_array().into_iter().flatten().filter_map(conversa));
        }
        return saida;
    }
    let origem = &reg["origem"];
    let Some(de_origem) = retratos.iter().find(|r| {
        r["keepd"]["pid"] == origem["pid"] && r["keepd"]["inicio_ms"] == origem["inicio_ms"] && !origem.is_null()
    }) else {
        return saida;
    };
    for c in reg["criadas"].as_array().into_iter().flatten() {
        let (Some(ws), Some(de), Some(aba)) = (c["workspace"].as_str(), c["origem"].as_u64(), c["aba"].as_u64()) else {
            continue;
        };
        if retrato.aba(ws, aba as u32).is_none() {
            continue;
        }
        let guardada = de_origem["abas"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["workspace"].as_str() == Some(ws) && a["aba"].as_u64() == Some(de));
        saida.extend(guardada.and_then(conversa));
    }
    saida
}

/// Os ids em que a conversa seguiu: ao ir para segundo plano o Claude
/// continua a conversa noutro id e grava `continuedInSessionId` no fim do
/// transcrito (a restauração retoma esse, e não o de antes). Até 5 passos.
fn continuacoes(amb: &Ambiente, id: &str) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let re = regex::Regex::new(r#""continuedInSessionId"\s*:\s*"([0-9a-f-]{36})""#).expect("regex válida");
    let mut saida: Vec<String> = Vec::new();
    let mut atual = id.to_string();
    for _ in 0..5 {
        let Some(tr) = sessoes::transcrito_de(&atual, &amb.projetos) else { break };
        let Ok(mut f) = std::fs::File::open(&tr) else { break };
        let tamanho = f.metadata().map(|m| m.len()).unwrap_or(0);
        let ini = tamanho.saturating_sub(512 * 1024);
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(ini)).is_err() || f.read_to_end(&mut buf).is_err() {
            break;
        }
        let cauda = String::from_utf8_lossy(&buf);
        let Some(proximo) = re.captures_iter(&cauda).last().map(|m| m[1].to_string()) else { break };
        if proximo == id || saida.contains(&proximo) {
            break;
        }
        saida.push(proximo.clone());
        atual = proximo;
    }
    saida
}

/// Uma órfã que passou por tudo, à espera da carência ou pronta.
struct Pronta<'a> {
    w: &'a Worktree,
    d: Decisao,
}

/// A varredura: o que vai para a Lixeira agora, os registros sem pasta que
/// saem, as órfãs na carência e as que ficam, com o motivo. Grava no estado
/// desde quando cada uma é órfã (`so_ler`: não grava nada). `agora` (testes):
/// o instante em que ela finge rodar.
pub fn orfas(amb: &Ambiente, agora_dado: Option<f64>, so_ler: bool) -> Orfas {
    let mut r = Orfas { versao: crate::VERSAO, ..Orfas::default() };
    let _trava = if so_ler {
        None
    } else {
        match estado::trava(amb) {
            Some(t) => Some(t),
            None => {
                r.ocupado = true;
                return r;
            }
        }
    };
    let agora = agora_dado.unwrap_or_else(agora);
    let Some(mut est) = estado::carrega(amb) else {
        r.avisos.push("o registro das worktrees ainda não existe (o indexar não rodou)".into());
        return r;
    };
    estado::recanoniza(&mut est);
    let retrato = match crate::daemon::listar_em(&amb.socket, Some(Duration::from_secs(2))) {
        Ok(x) => x,
        Err(e) => {
            r.avisos.push(format!("o Keep (keepd) não respondeu: nada será movido ({e})"));
            return r;
        }
    };
    if !retrato.exato {
        r.avisos.push(
            "o keepd em execução é anterior a esta versão do Keep: nada é varrido até ele reiniciar".into(),
        );
        return r;
    }
    let de_pe = if retrato.daemon.started_ms > 0 { agora - retrato.daemon.started_ms as f64 / 1000.0 } else { 0.0 };
    if de_pe < CARENCIA {
        r.avisos.push(format!(
            "o keepd subiu há {} min: a varredura espera {} min, o tempo de as abas voltarem depois de um reinício",
            min(de_pe),
            min(CARENCIA)
        ));
        return r;
    }
    let vivas = sessoes_vivas(amb);
    let (ctx, mut com_aba) = abas_vivas(amb, &retrato, Some(&est), &vivas);
    com_aba.extend(devidas_pela_restauracao(amb, &retrato));
    let mut extras = ctx.extras.clone();
    extras.extend(vivas.iter().map(|s| s.cwd.clone()));
    let mut comuns = repos::descobrir(amb, &extras);
    comuns.extend(est.repos.iter().filter(|c| e_pasta(c)).cloned());
    let wts = repos::todas(&comuns);

    // As chaves que deixam de ser órfãs nesta rodada (a carência recomeça).
    let mut fora: HashSet<String> = HashSet::new();
    let mut mantidas: Vec<Mantida> = Vec::new();
    let fica = |w: &Worktree, k: String, motivo: String, mantidas: &mut Vec<Mantida>, fora: &mut HashSet<String>| {
        mantidas.push(Mantida { caminho: w.caminho.clone(), motivo });
        fora.insert(k);
    };

    // A dona decidida (e a conversa em que ela seguiu), que teve aba e não
    // tem mais.
    let mut cadeias: HashMap<String, Vec<String>> = HashMap::new();
    let mut cands: Vec<(&Worktree, Decisao)> = Vec::new();
    for w in &wts {
        let k = chave(w);
        let Some(d) = est.nascimentos.get(&k) else { continue };
        let dona = match (d.estado.as_str(), d.dono.as_ref(), d.provisorio) {
            ("dono", Some(x), false) => x.clone(),
            _ => {
                fora.insert(k);
                continue;
            }
        };
        let cadeia = cadeias
            .entry(dona.clone())
            .or_insert_with(|| std::iter::once(dona.clone()).chain(continuacoes(amb, &dona)).collect());
        if cadeia.iter().any(|x| com_aba.contains(x)) {
            fora.insert(k);
            continue;
        }
        if !cadeia.iter().any(|x| est.encerradas.contains_key(x)) {
            fica(w, k, "a conversa dona não foi vista fechar numa aba do Keep".into(), &mut mantidas, &mut fora);
            continue;
        }
        cands.push((w, d.clone()));
    }

    let donas: HashSet<String> = cands
        .iter()
        .filter_map(|(_, d)| d.dono.as_ref())
        .flat_map(|x| cadeias.get(x).cloned().unwrap_or_default())
        .collect();
    let rodam = rodando(amb, &donas, &vivas);
    let mut rollouts: Option<HashMap<String, f64>> = None;
    let mut existem: Vec<(&Worktree, Decisao)> = Vec::new();
    let mut sem_pasta: Vec<Pronta> = Vec::new();
    for (w, d) in cands {
        let k = chave(w);
        let dona = d.dono.clone().unwrap_or_default();
        if cadeias.get(&dona).is_some_and(|c| c.iter().any(|x| rodam.contains(x))) {
            fica(w, k, "a conversa dona ainda roda, fora de uma aba aberta".into(), &mut mantidas, &mut fora);
            continue;
        }
        let do_codex = est.encerradas.get(&dona).is_some_and(|e| e.codex)
            || d.codex.as_ref().is_some_and(|c| c.contains(&dona));
        // A última vez que a conversa (ou a continuação dela) escreveu.
        let ultima = cadeias
            .get(&dona)
            .into_iter()
            .flatten()
            .filter_map(|x| mexeu_em(amb, x, do_codex && *x == dona, &mut rollouts))
            .reduce(f64::max);
        match ultima {
            None => {
                fica(w, k, "não achei o transcrito da conversa dona".into(), &mut mantidas, &mut fora);
                continue;
            }
            Some(t) if agora - t < CARENCIA => {
                let m = format!("a conversa dona mexeu no transcrito há {} min", min(agora - t));
                fica(w, k, m, &mut mantidas, &mut fora);
                continue;
            }
            Some(_) => {}
        }
        let cam = &w.caminho;
        let motivo = if *cam == w.repo || *cam == pai(&w.comum) {
            Some("worktree principal".to_string())
        } else if let Some(t) = &w.travada {
            Some(if t.is_empty() { "travada".to_string() } else { format!("travada: {t}") })
        } else if w.existe && na_lixeira(amb, cam) {
            Some("já está na Lixeira".into())
        } else if com_submodulos(w) {
            Some("repositório com submódulos: o registro levaria os repositórios deles".into())
        } else if !w.existe && !pode_esquecer(cam) {
            Some("a pasta sumiu, mas pode estar num disco desmontado".into())
        } else {
            None
        };
        if let Some(m) = motivo {
            fica(w, k, m, &mut mantidas, &mut fora);
            continue;
        }
        if w.existe {
            existem.push((w, d));
        } else {
            sem_pasta.push(Pronta { w, d });
        }
    }

    // Em uso: um processo com a pasta atual dentro, ou outra conversa viva
    // trabalhando nela (a última pasta dela, a worktree em que entrou).
    let mut prontas: Vec<Pronta> = Vec::new();
    if !existem.is_empty() {
        let varredura = varredura::processos();
        let sess: HashSet<String> = existem.iter().filter_map(|(_, d)| d.dono.clone()).collect();
        let caminhos: HashMap<String, String> = existem.iter().map(|(w, _)| (w.caminho.clone(), chave(w))).collect();
        let mut em_uso_m: Vec<Mantida> = Vec::new();
        let livres = em_uso(amb, existem, &sess, &vivas, &ctx, &mut em_uso_m, &varredura);
        for m in em_uso_m {
            if let Some(k) = caminhos.get(&m.caminho) {
                fora.insert(k.clone());
            }
            mantidas.push(m);
        }
        for c in livres {
            if !c.procs.is_empty() {
                let nomes: Vec<String> = c.procs.iter().map(|p| format!("{} ({})", p.nome, p.pid)).collect();
                let m = format!("processo com a pasta atual dentro: {}", nomes.join(", "));
                fica(c.w, chave(c.w), m, &mut mantidas, &mut fora);
                continue;
            }
            prontas.push(Pronta { w: c.w, d: c.d });
        }
    }

    // A carência: órfã há `CARENCIA` seguidos.
    let mut vao: Vec<&Pronta> = Vec::new();
    let mut vao_sem_pasta: Vec<&Pronta> = Vec::new();
    for (lista, sem) in [(&prontas, false), (&sem_pasta, true)] {
        for p in lista.iter() {
            let k = chave(p.w);
            let desde = *est.orfas.entry(k).or_insert(agora);
            if agora - desde < CARENCIA {
                r.aguardando.push(Mantida {
                    caminho: p.w.caminho.clone(),
                    motivo: format!(
                        "aba fechada há {} min (órfã desde {}): vai para a Lixeira depois de {} min",
                        min(agora - desde),
                        iso(desde),
                        min(CARENCIA)
                    ),
                });
            } else if sem {
                vao_sem_pasta.push(p);
            } else {
                vao.push(p);
            }
        }
    }
    for k in &fora {
        est.orfas.remove(k);
    }
    let atuais: HashSet<String> = wts.iter().map(chave).collect();
    est.orfas.retain(|k, _| atuais.contains(k));

    let prazo = Prazo::de(PRAZO_GIT * 2.0);
    for p in vao {
        match detalhes_git(amb, &p.w.caminho, &prazo) {
            Ok(g) => r.lixeira.push(ItemDaLixeira {
                caminho: p.w.caminho.clone(),
                repo: p.w.repo.clone(),
                id: p.w.id.clone(),
                ramo: g.ramo.clone(),
                head: g.head.clone(),
                alteracoes: g.rastreados + g.nao_rastreados,
                commits_so_aqui: g.commits_so_aqui,
                sessao: p.d.dono.clone().unwrap_or_default(),
                submodulos: g.submodulos,
                processos: Vec::new(),
                nasceu_ns: p.w.nasceu_ns,
            }),
            Err(_) => mantidas.push(Mantida {
                caminho: p.w.caminho.clone(),
                motivo: "não consegui ler o git dela: fica para a próxima rodada".into(),
            }),
        }
    }
    for p in vao_sem_pasta {
        r.registros.push(ItemDaLixeira {
            caminho: p.w.caminho.clone(),
            repo: p.w.repo.clone(),
            id: p.w.id.clone(),
            ramo: p.w.ramo.clone(),
            sessao: p.d.dono.clone().unwrap_or_default(),
            nasceu_ns: p.w.nasceu_ns,
            ..ItemDaLixeira::default()
        });
    }
    r.lixeira.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    r.registros.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    r.aguardando.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    mantidas.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    r.mantidas = mantidas;
    if !so_ler {
        if let Err(e) = estado::grava(amb, &mut est) {
            estado::registra(amb, &format!("orfas: não gravei o estado ({e})"));
        }
        for i in r.lixeira.iter().chain(r.registros.iter()) {
            estado::registra(amb, &format!("worktree {} ({}): órfã da conversa {}, vai para a Lixeira", i.caminho, i.id, i.sessao));
        }
    }
    r
}

fn falha(motivo: impl Into<String>) -> Conclusao {
    Conclusao { versao: crate::VERSAO, ok: false, motivo: Some(motivo.into()), ..Conclusao::default() }
}

/// O registro de uma worktree cuja pasta já sumiu sai do repositório (só
/// ele: `git worktree remove`, nunca `prune`), com a HEAD guardada em
/// `refs/keep-lixeira/<id>`; uma linha no `lixeira.log`. `nasceu`: o que a
/// varredura mostrou — outro registro no mesmo caminho é outro.
pub fn esquecer(amb: &Ambiente, caminho: &str, nasceu: Option<i64>) -> Conclusao {
    match esquecer_(amb, &normaliza(&absoluto(caminho)), nasceu) {
        Ok(c) => c,
        Err(Estourou) => falha("o git não respondeu a tempo"),
    }
}

fn esquecer_(amb: &Ambiente, cam: &str, nasceu: Option<i64>) -> Result<Conclusao, Estourou> {
    if outra_maquina(cam) {
        return Ok(falha("caminho de outra máquina"));
    }
    if std::fs::symlink_metadata(cam).is_ok() {
        return Ok(falha("a pasta existe: ela só sai pela Lixeira (preparar, mover, concluir)"));
    }
    if !pode_esquecer(cam) {
        return Ok(falha("a pasta pode estar num disco desmontado: registro mantido"));
    }
    let mut comuns = repos::descobrir(amb, &[]);
    if let Some(est) = estado::carrega(amb) {
        comuns.extend(est.repos.into_iter().filter(|c| e_pasta(c)));
    }
    let Some(w) = repos::todas(&comuns).into_iter().find(|w| !w.existe && mesmo(&w.caminho, cam)) else {
        return Ok(falha("nenhum registro de worktree aponta para esse caminho"));
    };
    if nasceu.is_some_and(|n| n != w.nasceu_ns) {
        return Ok(falha("o registro neste caminho não é o que foi listado"));
    }
    if let Some(t) = &w.travada {
        return Ok(falha(if t.is_empty() { "travada".to_string() } else { format!("travada: {t}") }));
    }
    if com_submodulos(&w) {
        return Ok(falha("repositório com submódulos: registro mantido"));
    }
    let git_dir = format!("--git-dir={}", w.comum);
    let adm = format!("--git-dir={}", w.adm);
    let s = git(&amb.git, &[&adm, "rev-parse", "--verify", "HEAD^{commit}"], PRAZO_GIT)?;
    let head = s.ok().then(|| s.saida.trim().to_string()).filter(|h| !h.is_empty());
    let mut guardada = None;
    if let Some(h) = &head {
        let base_ref = format!("refs/keep-lixeira/{}", w.id);
        let mut r = base_ref.clone();
        let mut k = 0u32;
        while git(&amb.git, &[&git_dir, "show-ref", "--verify", "--quiet", &r], PRAZO_GIT)?.ok() {
            k += 1;
            r = format!("{base_ref}-{}{}", agora() as i64, if k == 1 { String::new() } else { format!("-{k}") });
        }
        let msg = format!("keep: {cam} -> esquecida (a pasta já não existia)");
        let s = git(&amb.git, &[&git_dir, "update-ref", "--create-reflog", "-m", &msg, &r, h, ""], PRAZO_GIT)?;
        if !s.ok() {
            return Ok(falha(format!("não consegui gravar {r} ({})", s.erro.trim().chars().take(200).collect::<String>())));
        }
        guardada = Some(r);
    }
    let mut args: Vec<&str> = if Path::new(&w.repo).join(".git").is_dir() { vec!["-C", &w.repo] } else { vec![&git_dir] };
    args.extend(["worktree", "remove", &w.caminho]);
    let s = git(&amb.git, &args, PRAZO_GIT)?;
    let removido = s.ok() && !e_pasta(&w.adm);
    let nota = if removido {
        "a pasta já não existia: só o registro saiu".to_string()
    } else {
        format!("git worktree remove falhou: {}", s.erro.trim().chars().take(300).collect::<String>())
    };
    let curto = head.as_ref().map(|h| h.chars().take(9).collect::<String>());
    let linha = json!({
        "ts": iso(agora()), "caminho": w.caminho, "destino": "", "repo": w.repo, "id": w.id,
        "ramo": w.ramo, "head": curto, "ref": guardada, "ref_sujo": null,
        "registro_removido": removido, "nota": nota,
    });
    estado::acrescenta(amb, LOG_LIXEIRA, &linha).ok();
    Ok(Conclusao {
        versao: crate::VERSAO,
        ok: removido,
        caminho: Some(w.caminho.clone()),
        destino: Some(String::new()),
        repo: Some(w.repo.clone()),
        id: Some(w.id.clone()),
        ramo: Some(w.ramo.clone()),
        head: Some(curto),
        r#ref: Some(guardada),
        ref_sujo: Some(None),
        registro_removido: Some(removido),
        motivo: (!removido).then(|| nota.clone()),
        nota: Some(nota),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pasta_de_disco_desmontado_nao_se_esquece() {
        assert!(!pode_esquecer("relativo/wt"));
        if cfg!(target_os = "macos") {
            assert!(!pode_esquecer("/Volumes/Externo/projeto/.claude/worktrees/wt"));
            assert!(!pode_esquecer("/Volumes"));
            assert!(pode_esquecer("/Users/alguem/Projetos/app/.claude/worktrees/wt"));
        }
    }
}
