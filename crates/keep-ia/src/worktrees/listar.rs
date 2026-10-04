//! `listar`: as worktrees que vão para a Lixeira com as abas que fecham, as
//! que ficam e por quê. Responde SEMPRE antes do prazo: o que não deu tempo
//! de verificar vira aviso e não vai para a Lixeira.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use super::abas::{self, Alvo, Contexto};
use super::ambiente::Ambiente;
use super::dono::{Decisao, decidir};
use super::estado::{self, DetalhesGit, Estado};
use super::git::git;
use super::indice;
use super::pastas::{canon, dentro, e_pasta, outra_maquina, pai};
use super::prazo::{Estourou, Prazo};
use super::repos::{self, Worktree, chave};
use super::sessoes::SessaoViva;
use super::texto::agora;
use super::varredura::{self, Varredura, concha_por_mapa};
use super::{ALVO_RAPIDO, GIT_VALIDADE, ItemDaLixeira, Listagem, MARGEM, MARGEM_DURA, Mantida, ProcessoDentro};

fn trava<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// As sessões vivas do mundo (ou as dadas pelo teste).
pub fn sessoes_vivas(amb: &Ambiente) -> Vec<SessaoViva> {
    match &amb.ganchos.vivas {
        Some(v) => v.clone(),
        None => super::sessoes::vivas_em(&amb.sessoes),
    }
}

/// O caminho já está na Lixeira.
pub fn na_lixeira(amb: &Ambiente, cam: &str) -> bool {
    if let Some(l) = &amb.lixeira {
        let l = l.to_string_lossy();
        let real = if e_pasta(&l) { canon(&l) } else { l.to_string() };
        if dentro(cam, &real) || dentro(cam, &l) {
            return true;
        }
    }
    cfg!(windows) && cam.split(['\\', '/']).any(|c| c.eq_ignore_ascii_case("$recycle.bin"))
}

/// Ramo, HEAD, alterações RASTREADAS (`status -uno`: não anda pelas pastas)
/// e commits só nesta HEAD (fora de ramo e de remoto).
pub fn git_rastreado(amb: &Ambiente, cam: &str, prazo: &Prazo) -> Result<DetalhesGit, Falha> {
    prazo.checa()?;
    let s = git(
        &amb.git,
        &["-C", cam, "status", "--porcelain=v2", "--branch", "--untracked-files=no"],
        prazo.restante(),
    )?;
    if !s.ok() {
        return Err(Falha::Erro(format!("git status falhou em {cam}: {}", corta(s.erro.trim(), 200))));
    }
    let (mut oid, mut head, mut alt) = (None, None, 0u64);
    for linha in s.saida.lines() {
        if let Some(v) = linha.strip_prefix("# branch.oid ") {
            oid = Some(v.trim().to_string());
        } else if let Some(v) = linha.strip_prefix("# branch.head ") {
            head = Some(v.trim().to_string());
        } else if !linha.is_empty() && !linha.starts_with('#') {
            alt += 1;
        }
    }
    let r = git(&amb.git, &["-C", cam, "rev-list", "--count", "HEAD", "--not", "--branches", "--remotes"], prazo.restante())?;
    let so_aqui = r.saida.trim().parse::<u64>().ok().filter(|_| r.ok());
    Ok(DetalhesGit {
        ramo: head.filter(|h| h != "(detached)"),
        head: oid.filter(|o| o != "(initial)").map(|o| o.chars().take(9).collect()),
        rastreados: alt,
        commits_so_aqui: so_aqui,
        submodulos: std::path::Path::new(cam).join(".gitmodules").exists(),
        ..DetalhesGit::default()
    })
}

/// Os não rastreados como o `git status` os conta (pasta nova inteira = 1).
/// É a parte lenta do status: anda por toda pasta não ignorada.
pub fn git_nao_rastreados(amb: &Ambiente, cam: &str, prazo: &Prazo) -> Result<u64, Falha> {
    prazo.checa()?;
    let s = git(
        &amb.git,
        &["-C", cam, "ls-files", "-z", "--others", "--exclude-standard", "--directory", "--no-empty-directory"],
        prazo.restante(),
    )?;
    if !s.ok() {
        return Err(Falha::Erro(format!("git ls-files falhou em {cam}: {}", corta(s.erro.trim(), 200))));
    }
    Ok(s.saida.bytes().filter(|&b| b == 0).count() as u64)
}

/// O que o diálogo mostra de uma worktree (a conta do `git status`).
pub fn detalhes_git(amb: &Ambiente, cam: &str, prazo: &Prazo) -> Result<DetalhesGit, Falha> {
    let mut g = git_rastreado(amb, cam, prazo)?;
    g.nao_rastreados = git_nao_rastreados(amb, cam, prazo)?;
    g.alteracoes = g.rastreados + g.nao_rastreados;
    Ok(g)
}

fn corta(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Um git que não respondeu.
#[derive(Clone, Debug)]
pub enum Falha {
    Estourou,
    Erro(String),
}

impl From<Estourou> for Falha {
    fn from(_: Estourou) -> Falha {
        Falha::Estourou
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Tipo {
    Rastreado,
    NaoRastreados,
}

#[derive(Default)]
struct Placar {
    rastreado: HashMap<String, Result<DetalhesGit, Falha>>,
    nao: HashMap<String, Result<u64, Falha>>,
}

/// Os git das worktrees que podem ir, rodando enquanto o resto se decide.
struct Pedidos {
    placar: Arc<(Mutex<Placar>, Condvar)>,
    cancelados: Arc<Mutex<HashSet<String>>>,
}

impl Pedidos {
    fn sobe(amb: &Ambiente, caminhos: &[String], prazo: Prazo) -> Pedidos {
        let fila: VecDeque<(String, Tipo)> =
            caminhos.iter().flat_map(|c| [(c.clone(), Tipo::Rastreado), (c.clone(), Tipo::NaoRastreados)]).collect();
        let n = fila.len().min(8);
        let fila = Arc::new(Mutex::new(fila));
        let placar = Arc::new((Mutex::new(Placar::default()), Condvar::new()));
        let cancelados = Arc::new(Mutex::new(HashSet::new()));
        for _ in 0..n {
            let (fila, placar, cancelados, amb) = (fila.clone(), placar.clone(), cancelados.clone(), amb.clone());
            std::thread::spawn(move || {
                loop {
                    let Some((cam, tipo)) = trava(&fila).pop_front() else { return };
                    if trava(&cancelados).contains(&cam) {
                        continue;
                    }
                    let (m, cv) = &*placar;
                    match tipo {
                        Tipo::Rastreado => {
                            let r = git_rastreado(&amb, &cam, &prazo);
                            trava(m).rastreado.insert(cam, r);
                        }
                        Tipo::NaoRastreados => {
                            let r = git_nao_rastreados(&amb, &cam, &prazo);
                            trava(m).nao.insert(cam, r);
                        }
                    }
                    cv.notify_all();
                }
            });
        }
        Pedidos { placar, cancelados }
    }

    fn cancela(&self, cam: &str) {
        trava(&self.cancelados).insert(cam.to_string());
    }

    /// Espera os dois git de cada caminho, no máximo `seg`.
    fn espera(&self, caminhos: &[String], seg: f64) {
        let fim = Instant::now() + Duration::from_secs_f64(seg.clamp(0.0, 1e6));
        let (m, cv) = &*self.placar;
        let mut g = trava(m);
        loop {
            if caminhos.iter().all(|c| g.rastreado.contains_key(c) && g.nao.contains_key(c)) {
                return;
            }
            let agora = Instant::now();
            if agora >= fim {
                return;
            }
            g = cv.wait_timeout(g, fim - agora).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
        }
    }
}

/// Uma worktree candidata e o que já se sabe dela.
struct Candidata<'a> {
    w: &'a Worktree,
    d: Decisao,
    procs: Vec<ProcessoDentro>,
}

fn rotulo_de_concha(ctx: &Contexto, concha: u32) -> String {
    match ctx.rotulos.get(&concha) {
        Some((aba, ws)) => format!("em uso pela aba “{aba}” ({ws})"),
        None => "em uso por outra aba do Keep".into(),
    }
}

fn fora_do_keep(id: &str) -> String {
    format!("em uso pela conversa {} (fora do Keep)", id.chars().take(8).collect::<String>())
}

/// Separa o que outra aba viva usa (→ mantidas) e anexa os processos
/// órfãos com a pasta atual dentro.
fn em_uso<'a>(
    amb: &Ambiente,
    ficam: Vec<(&'a Worktree, Decisao)>,
    sess_aba: &HashSet<String>,
    vivas: &[SessaoViva],
    ctx: &Contexto,
    mantidas: &mut Vec<Mantida>,
    varredura: &Varredura,
) -> Vec<Candidata<'a>> {
    let (keepd, fechando) = (&ctx.conchas_keepd, &ctx.conchas_fechando);
    let mut mapa = varredura.mapa.clone();
    // A conversa dona ainda viva em OUTRA aba (/resume noutro lugar): a
    // worktree continua em uso lá.
    let mut sessao_fora: HashMap<String, String> = HashMap::new();
    for s in vivas {
        let c = concha_por_mapa(s.pid, keepd, &mut mapa);
        if sess_aba.contains(&s.id) && !c.is_some_and(|c| fechando.contains(&c)) {
            let m = c.map(|c| rotulo_de_concha(ctx, c)).unwrap_or_else(|| fora_do_keep(&s.id));
            sessao_fora.insert(s.id.clone(), m);
        }
    }
    // Outras sessões vivas trabalhando dentro (a última pasta do transcrito
    // ou a worktree em que entraram).
    let mut outras: Vec<(Option<String>, Option<String>, String)> = Vec::new();
    for s in vivas {
        if sess_aba.contains(&s.id) {
            continue;
        }
        let c = concha_por_mapa(s.pid, keepd, &mut mapa);
        if c.is_some_and(|c| fechando.contains(&c)) {
            continue;
        }
        let (cwd, wt) = super::sessoes::estado_final(&s.id, &amb.projetos);
        let m = c.map(|c| rotulo_de_concha(ctx, c)).unwrap_or_else(|| fora_do_keep(&s.id));
        outras.push((cwd, wt, m));
    }
    let mut saida = Vec::new();
    for (w, d) in ficam {
        let cam = &w.caminho;
        if let Some(m) = d.dono.as_ref().and_then(|x| sessao_fora.get(x)) {
            mantidas.push(Mantida { caminho: cam.clone(), motivo: m.clone() });
            continue;
        }
        let mut motivo = None;
        let mut orfaos = Vec::new();
        for p in &varredura.procs {
            if !dentro(&p.cwd, cam) {
                continue;
            }
            match concha_por_mapa(p.pid, keepd, &mut mapa) {
                None => orfaos.push(ProcessoDentro { pid: p.pid, nome: p.nome.clone() }),
                Some(c) if !fechando.contains(&c) => {
                    motivo = Some(rotulo_de_concha(ctx, c));
                    break;
                }
                Some(_) => {}
            }
        }
        if motivo.is_none() {
            let dentro_de = |p: &Option<String>| {
                p.as_deref().is_some_and(|p| {
                    !outra_maquina(p) && dentro(&if e_pasta(p) { canon(p) } else { p.to_string() }, cam)
                })
            };
            motivo = outras.iter().find(|(cwd, wt, _)| dentro_de(cwd) || dentro_de(wt)).map(|(_, _, m)| m.clone());
        }
        match motivo {
            Some(m) => mantidas.push(Mantida { caminho: cam.clone(), motivo: m }),
            None => {
                orfaos.sort_by_key(|p| p.pid);
                saida.push(Candidata { w, d, procs: orfaos });
            }
        }
    }
    saida
}

/// O `listar` de verdade: escreve em `r` à medida que sabe (quem chamou
/// responde com o que houver quando o prazo acabar).
pub fn listar(amb: &Ambiente, alvos: &[Alvo], prazo: &Prazo, r: &Mutex<Listagem>) -> Result<(), Estourou> {
    let est = estado::carrega(amb);
    let vivas = sessoes_vivas(amb);
    let mut avisos = Vec::new();
    let (abas, ctx) = abas::resolver(amb, alvos, prazo, &mut avisos, est.as_ref(), &vivas);
    {
        let mut g = trava(r);
        g.abas = abas.clone();
        g.avisos.append(&mut avisos);
    }
    let aviso = |m: String| trava(r).avisos.push(m);
    let sess_aba: HashSet<String> =
        abas.iter().filter(|a| a.vinculo == "exato").flat_map(|a| a.sessoes.iter().cloned()).collect();
    if sess_aba.is_empty() {
        return Ok(());
    }
    let est = est.unwrap_or_else(|| {
        aviso("o registro das worktrees ainda não existe (o indexar não rodou)".into());
        Estado::vazio()
    });
    prazo.checa()?;
    let instante = agora();
    let ids_vivos: HashSet<String> = vivas.iter().map(|s| s.id.clone()).collect();
    let mut extras = ctx.extras.clone();
    extras.extend(vivas.iter().filter(|s| sess_aba.contains(&s.id)).map(|s| s.cwd.clone()));
    let mut comuns = repos::descobrir(amb, &extras);
    comuns.extend(est.repos.iter().filter(|c| e_pasta(c)).cloned());
    let wts = repos::todas(&comuns);
    let mut decisoes: BTreeMap<String, Decisao> = BTreeMap::new();
    let mut pend: Vec<&Worktree> = Vec::new();
    for w in &wts {
        match est.nascimentos.get(&chave(w)) {
            Some(d) if !d.provisorio => {
                decisoes.insert(chave(w), d.clone());
            }
            _ => pend.push(w),
        }
    }
    if !pend.is_empty() {
        let novas = (|| {
            let mut ind = indice::carrega(amb);
            let ind = match ind.criado {
                None => None,
                Some(_) => {
                    indice::atualizar(amb, &mut ind, instante, &ids_vivos, prazo)?;
                    Some(ind)
                }
            };
            decidir(amb, &pend, ind.as_ref(), &ids_vivos, instante, prazo, Some(&sess_aba))
        })();
        if let Ok(novas) = novas {
            decisoes.extend(novas);
        }
        for w in &pend {
            if decisoes.contains_key(&chave(w)) {
                continue;
            }
            // Provisória que não deu para refazer: se apontava esta aba, vira
            // aviso (nada se move por ela).
            let antes = est.nascimentos.get(&chave(w));
            let desta = antes.is_none_or(|d| {
                d.dono.as_ref().is_some_and(|x| sess_aba.contains(x)) || d.niveis.keys().any(|s| sess_aba.contains(s))
            });
            if desta {
                aviso(format!("não deu tempo de verificar a worktree nova: {}", w.caminho));
            }
        }
    }
    let mut cands: Vec<(&Worktree, Decisao)> = Vec::new();
    for w in &wts {
        let Some(d) = decisoes.get(&chave(w)) else { continue };
        if d.estado == "dono" && d.dono.as_ref().is_some_and(|x| sess_aba.contains(x)) {
            cands.push((w, d.clone()));
        } else if (d.estado == "ambigua" || d.estado == "indecidida") && d.niveis.keys().any(|s| sess_aba.contains(s)) {
            aviso(format!("possivelmente desta aba, não verificada: {}", w.caminho));
        }
    }
    if cands.is_empty() {
        return Ok(());
    }
    let mut mantidas: Vec<Mantida> = Vec::new();
    let mut ficam = Vec::new();
    for (w, d) in cands {
        let cam = &w.caminho;
        let motivo = if *cam == w.repo || *cam == pai(&w.comum) {
            Some("worktree principal".to_string())
        } else if !w.existe {
            Some("caminho inexistente".into())
        } else if na_lixeira(amb, cam) {
            Some("já está na Lixeira".into())
        } else {
            w.travada.as_ref().map(|t| if t.is_empty() { "travada".to_string() } else { format!("travada: {t}") })
        };
        match motivo {
            Some(m) => mantidas.push(Mantida { caminho: cam.clone(), motivo: m }),
            None => ficam.push((w, d)),
        }
    }
    let mut lixeira = Vec::new();
    if !ficam.is_empty() {
        prazo.checa()?;
        // Os processos são varridos ANTES de subir o git; o git roda enquanto
        // o `em_uso` lê as conversas; a parte lenta (não rastreados) à parte.
        let varredura = varredura::processos();
        let caminhos: Vec<String> = ficam.iter().map(|(w, _)| w.caminho.clone()).collect();
        let pedidos = Pedidos::sobe(amb, &caminhos, *prazo);
        let ficam = em_uso(amb, ficam, &sess_aba, &vivas, &ctx, &mut mantidas, &varredura);
        let vao: HashSet<&str> = ficam.iter().map(|c| c.w.caminho.as_str()).collect();
        for c in &caminhos {
            if !vao.contains(c.as_str()) {
                pedidos.cancela(c);
            }
        }
        let mut falta: BTreeMap<String, Candidata> = ficam.into_iter().map(|c| (c.w.caminho.clone(), c)).collect();
        let item = |c: &Candidata, g: &DetalhesGit, n: u64| ItemDaLixeira {
            caminho: c.w.caminho.clone(),
            repo: c.w.repo.clone(),
            id: c.w.id.clone(),
            ramo: g.ramo.clone(),
            head: g.head.clone(),
            alteracoes: g.rastreados + n,
            commits_so_aqui: g.commits_so_aqui,
            sessao: c.d.dono.clone().unwrap_or_default(),
            submodulos: g.submodulos,
            processos: c.procs.clone(),
            nasceu_ns: c.w.nasceu_ns,
        };
        // Tira de `falta` o que já tem resposta: o git ao vivo pronto; ou,
        // passado o alvo rápido, o registro do `indexar` no lugar do que ainda
        // roda. `final`: o prazo acabou.
        let mut colhe = |final_: bool, falta: &mut BTreeMap<String, Candidata>| {
            let placar = trava(&pedidos.placar.0);
            let mut prontos = Vec::new();
            for (cam, c) in falta.iter() {
                let g = placar.rastreado.get(cam);
                let n = placar.nao.get(cam);
                let erro = [g.and_then(|r| r.as_ref().err()), n.and_then(|r| r.as_ref().err())]
                    .into_iter()
                    .flatten()
                    .find_map(|f| match f {
                        Falha::Erro(e) => Some(e.clone()),
                        Falha::Estourou => None,
                    });
                if let Some(e) = erro {
                    aviso(format!("não consegui ler o git de {cam} ({e}): fica"));
                    prontos.push(cam.clone());
                    continue;
                }
                let g = g.and_then(|r| r.as_ref().ok());
                let n = n.and_then(|r| r.as_ref().ok()).copied();
                if let (Some(g), Some(n)) = (g, n) {
                    lixeira.push(item(c, g, n));
                    prontos.push(cam.clone());
                    continue;
                }
                let reg = est.git.get(&chave(c.w));
                let idade = reg.map(|r| agora() - r.em);
                if let (Some(reg), Some(idade)) = (reg, idade.filter(|&i| i <= GIT_VALIDADE)) {
                    // O ramo vem do HEAD da worktree (ao vivo).
                    let g = g.cloned().unwrap_or_else(|| DetalhesGit { ramo: c.w.ramo.clone(), ..reg.clone() });
                    lixeira.push(item(c, &g, n.unwrap_or(reg.nao_rastreados)));
                    aviso(format!(
                        "alterações de {cam} aproximadas (contadas há {} s; o git demorou)",
                        idade.max(0.0) as i64
                    ));
                    prontos.push(cam.clone());
                } else if final_ {
                    aviso(format!("não deu tempo de verificar: {cam}"));
                    prontos.push(cam.clone());
                }
            }
            drop(placar);
            for p in prontos {
                falta.remove(&p);
            }
        };
        let faltam = |f: &BTreeMap<String, Candidata>| f.keys().cloned().collect::<Vec<_>>();
        pedidos.espera(&faltam(&falta), prazo.ate_o_alvo().max(0.0));
        colhe(false, &mut falta);
        if !falta.is_empty() {
            pedidos.espera(&faltam(&falta), prazo.restante().max(0.01));
            colhe(true, &mut falta);
        }
        for c in falta.keys() {
            pedidos.cancela(c);
        }
    }
    lixeira.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    mantidas.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    let mut g = trava(r);
    g.lixeira = lixeira;
    g.mantidas = mantidas;
    Ok(())
}

/// `listar` com o prazo duro: a resposta sai antes de `prazo` (contado de
/// `decorrido` segundos atrás) mesmo que o trabalho fique preso — num `stat`
/// que trava, num processo que não responde. O que não deu tempo vira aviso
/// e nada se move por ele.
pub fn com_prazo(amb: &Ambiente, alvos: Vec<Alvo>, prazo: f64, decorrido: f64) -> Listagem {
    if alvos.is_empty() {
        return Listagem::nova();
    }
    let base = Instant::now().checked_sub(Duration::from_secs_f64(decorrido.max(0.0))).unwrap_or_else(Instant::now);
    let p = Prazo::com_alvo((prazo - MARGEM - decorrido).max(0.05), (ALVO_RAPIDO - decorrido).max(0.0));
    let r = Arc::new(Mutex::new(Listagem::nova()));
    let (tx, rx) = mpsc::channel();
    let (amb2, r2) = (amb.clone(), r.clone());
    let estourou = format!("prazo de {prazo:.1} s estourado: nada será movido");
    let estourou2 = estourou.clone();
    std::thread::spawn(move || {
        let resultado = std::panic::catch_unwind(AssertUnwindSafe(|| listar(&amb2, &alvos, &p, &r2)));
        match resultado {
            Ok(Ok(())) => {}
            Ok(Err(Estourou)) => {
                let mut g = trava(&r2);
                g.lixeira.clear();
                g.avisos.push(estourou2);
            }
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                estado::registra(&amb2, &format!("listar: falhou: {msg}"));
                let mut g = trava(&r2);
                g.lixeira.clear();
                g.avisos.push(format!("erro na lixeira de worktrees ({msg}): nada será movido"));
            }
        }
        tx.send(()).ok();
    });
    let espera = (prazo - MARGEM_DURA - base.elapsed().as_secs_f64()).max(0.0);
    match rx.recv_timeout(Duration::from_secs_f64(espera)) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => trava(&r).clone(),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let g = trava(&r);
            let mut avisos = g.avisos.clone();
            avisos.push(estourou);
            Listagem { abas: g.abas.clone(), avisos, ..Listagem::nova() }
        }
    }
}
