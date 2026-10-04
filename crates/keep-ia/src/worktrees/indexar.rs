//! `indexar` (o app chama a cada minuto): o registro de nascimentos → dono,
//! o histórico aba → conversas, o índice dos transcritos e o git de cada
//! worktree candidata (no máximo `GIT_ORCAMENTO` segundos por rodada).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use super::abas::{atualizar_historico, nascimento_do_socket};
use super::ambiente::Ambiente;
use super::dono::decidir;
use super::estado::{self, DetalhesGit, Estado, adota_historico_do_kit};
use super::indice;
use super::listar::{detalhes_git, sessoes_vivas};
use super::pastas::e_pasta;
use super::prazo::Prazo;
use super::repos::{self, Worktree, chave};
use super::texto::{agora, iso};
use super::{GIT_IDADE, GIT_ORCAMENTO, Indexacao};

/// Refaz o git de cada worktree candidata (dona decidida, existe,
/// destravada) contado há mais de `GIT_IDADE`, as mais velhas primeiro, no
/// máximo `orcamento` segundos (o git que estoura fica para a próxima).
pub fn atualizar_git(amb: &Ambiente, est: &mut Estado, wts: &[Worktree], agora: f64, orcamento: f64) -> usize {
    let atuais: HashSet<String> = wts.iter().map(chave).collect();
    est.git.retain(|k, _| atuais.contains(k));
    let em = |est: &Estado, w: &Worktree| est.git.get(&chave(w)).map(|g| g.em).unwrap_or(0.0);
    let mut cands: Vec<&Worktree> = wts
        .iter()
        .filter(|w| w.existe && w.travada.is_none())
        .filter(|w| est.nascimentos.get(&chave(w)).is_some_and(|d| d.estado == "dono"))
        .filter(|w| agora - em(est, w) >= GIT_IDADE)
        .collect();
    if cands.is_empty() {
        return 0;
    }
    cands.sort_by(|a, b| em(est, a).total_cmp(&em(est, b)));
    let prazo = Prazo::de(orcamento);
    let fila: Arc<Mutex<VecDeque<(String, String)>>> =
        Arc::new(Mutex::new(cands.iter().map(|w| (chave(w), w.caminho.clone())).collect()));
    let feitos: Arc<Mutex<BTreeMap<String, DetalhesGit>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let trabalhadores: Vec<_> = (0..cands.len().min(4))
        .map(|_| {
            let (fila, feitos, amb) = (fila.clone(), feitos.clone(), amb.clone());
            std::thread::spawn(move || {
                loop {
                    let Some((k, cam)) = fila.lock().unwrap_or_else(|e| e.into_inner()).pop_front() else { return };
                    if let Ok(mut d) = detalhes_git(&amb, &cam, &prazo) {
                        d.em = agora;
                        feitos.lock().unwrap_or_else(|e| e.into_inner()).insert(k, d);
                    }
                }
            })
        })
        .collect();
    for t in trabalhadores {
        t.join().ok();
    }
    let feitos = std::mem::take(&mut *feitos.lock().unwrap_or_else(|e| e.into_inner()));
    let n = feitos.len();
    est.git.extend(feitos);
    n
}

/// Uma rodada do `indexar`. `agora` (testes): o instante em que ela finge
/// rodar.
pub fn indexar(amb: &Ambiente, agora_dado: Option<f64>) -> Indexacao {
    let t0 = Instant::now();
    let agora = agora_dado.unwrap_or_else(agora);
    let mut est = estado::carrega(amb).unwrap_or_else(Estado::vazio);
    let vivas = sessoes_vivas(amb);
    let ids_vivos: HashSet<String> = vivas.iter().map(|s| s.id.clone()).collect();
    let mut extras: Vec<String> = vivas.iter().map(|s| s.cwd.clone()).collect();
    let mut novos_hist = 0;
    if let Ok(r) = crate::daemon::listar_em(&amb.socket, Some(Duration::from_secs(2))) {
        if r.exato {
            adota_historico_do_kit(&mut est, &r.daemon, nascimento_do_socket(amb));
            novos_hist = atualizar_historico(amb, &mut est, &r, &vivas);
        }
        // O que veio do kit e não é do daemon que respondeu não vale mais.
        est.historico_do_kit = None;
        extras.extend(r.abas.iter().map(|a| a.info.cwd.clone()));
    }
    let mut comuns = repos::descobrir(amb, &extras);
    comuns.extend(est.repos.iter().filter(|c| e_pasta(c)).cloned());
    let wts = repos::todas(&comuns);
    let t_ind = Instant::now();
    let mut ind = indice::carrega(amb);
    let lidos = indice::atualizar(amb, &mut ind, agora, &ids_vivos, &Prazo::sem()).unwrap_or(0);
    let ms_indice = t_ind.elapsed().as_millis() as u64;
    let mut pend: Vec<&Worktree> = Vec::new();
    for w in &wts {
        match est.nascimentos.get_mut(&chave(w)) {
            Some(d) if !d.provisorio => {
                // `git worktree move`: a mesma worktree, num caminho novo.
                if d.caminho.as_deref() != Some(w.caminho.as_str()) {
                    d.caminho = Some(w.caminho.clone());
                }
            }
            _ => pend.push(w),
        }
    }
    let t_dec = Instant::now();
    let decisoes = if pend.is_empty() {
        BTreeMap::new()
    } else {
        decidir(amb, &pend, Some(&ind), &ids_vivos, agora, &Prazo::sem(), None).unwrap_or_default()
    };
    let ms_decidir = t_dec.elapsed().as_millis() as u64;
    let mut novas = 0;
    for w in &pend {
        let Some(mut d) = decisoes.get(&chave(w)).cloned() else { continue };
        d.comum = Some(w.comum.clone());
        d.repo = Some(w.repo.clone());
        d.id = Some(w.id.clone());
        d.caminho = Some(w.caminho.clone());
        d.nasceu = Some(iso(w.nasceu));
        d.decidido_em = Some(iso(agora));
        let antes = est.nascimentos.get(&chave(w));
        if antes.is_none_or(|a| a.dono != d.dono || a.estado != d.estado) {
            let de = d.dono.as_ref().map(|x| format!(" de {x} ({})", d.nivel.as_deref().unwrap_or("?"))).unwrap_or_default();
            let prov = if d.provisorio { " [provisória]" } else { "" };
            estado::registra(amb, &format!("worktree {} ({}): {}{de}{prov}", w.caminho, w.id, d.estado));
        }
        est.nascimentos.insert(chave(w), d);
        novas += 1;
    }
    let atuais: HashSet<String> = wts.iter().map(chave).collect();
    // Worktree removida (ou nome reaproveitado: outro nascimento).
    est.nascimentos.retain(|k, _| atuais.contains(k));
    est.repos = comuns.iter().cloned().collect();
    let t_git = Instant::now();
    let git_feitos = atualizar_git(amb, &mut est, &wts, agora, GIT_ORCAMENTO);
    let ms_git = t_git.elapsed().as_millis() as u64;
    if let Err(e) = estado::grava(amb, &mut est) {
        estado::registra(amb, &format!("indexar: não gravei o estado ({e})"));
    }
    if let Err(e) = indice::grava(amb, &ind) {
        estado::registra(amb, &format!("indexar: não gravei o índice ({e})"));
    }
    let mut donos: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in est.nascimentos.values() {
        let k = d.dono.clone().unwrap_or_else(|| format!("({})", d.estado));
        donos.entry(k).or_default().push(d.caminho.clone().unwrap_or_default());
    }
    donos.values_mut().for_each(|v| v.sort());
    let por_estado: BTreeMap<String, usize> = ["dono", "ambigua", "indecidida", "sem-dono"]
        .iter()
        .map(|e| (e.to_string(), est.nascimentos.values().filter(|d| d.estado == *e).count()))
        .collect();
    let historico: BTreeMap<String, HashMap<String, Vec<String>>> = est
        .historico
        .iter()
        .map(|(k, cur)| (k.clone(), cur.iter().map(|(p, h)| (p.clone(), h.ids.clone())).collect()))
        .collect();
    Indexacao {
        versao: crate::VERSAO,
        ocupado: false,
        worktrees: wts.len(),
        repos: comuns.len(),
        decididas_agora: novas,
        provisorias: est.nascimentos.values().filter(|d| d.provisorio).count(),
        por_estado,
        donos,
        historico: serde_json::to_value(historico).unwrap_or_default(),
        novas_no_historico: novos_hist,
        indice: json!({
            "arquivos": ind.arquivos.len(),
            "bytes_lidos": lidos,
            "ms": ms_indice,
            "chamadas": ind.arquivos.values().map(|e| e.ab.len() + e.fe.len()).sum::<usize>(),
        }),
        ms_decidir,
        git: json!({"atualizadas": git_feitos, "registradas": est.git.len(), "ms": ms_git}),
        ms: t0.elapsed().as_millis() as u64,
    }
}
