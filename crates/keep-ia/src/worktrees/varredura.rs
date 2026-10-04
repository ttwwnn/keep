//! Quem está trabalhando dentro de cada worktree: os processos com a pasta
//! atual lá, e de que aba cada um é (subindo pelos pais até um shell do
//! keepd).

use std::collections::{HashMap, HashSet};

use crate::processos;
use crate::tela::{Programa, programa};

/// Um processo com a pasta atual legível.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComPasta {
    pub pid: u32,
    pub ppid: u32,
    pub nome: String,
    pub cwd: String,
}

/// Uma passada pelos processos: os com pasta legível (fora este processo e
/// os git que ele subiu) e o pai de cada um.
#[derive(Clone, Debug, Default)]
pub struct Varredura {
    pub procs: Vec<ComPasta>,
    pub mapa: HashMap<u32, u32>,
}

/// O nome que o diálogo mostra: o do sistema, menos o do Claude nativo, que
/// o sistema dá pela versão (`2.1.289`).
fn nome_legivel(p: &processos::Processo) -> String {
    if programa(&p.nome) == Programa::Claude && p.nome != "claude" {
        return "claude".into();
    }
    p.nome.clone()
}

/// Os processos sem filtro nenhum (os testes param aqui no meio).
pub fn crua() -> Varredura {
    let lista = processos::todos();
    let mapa = lista.iter().map(|p| (p.pid, p.ppid)).collect();
    let procs = lista
        .iter()
        .filter_map(|p| {
            let cwd = processos::cwd(p.pid)?;
            Some(ComPasta { pid: p.pid, ppid: p.ppid, nome: nome_legivel(p), cwd: cwd.to_string_lossy().into_owned() })
        })
        .collect();
    Varredura { procs, mapa }
}

/// Tira desta varredura este processo e os git que ele subiu (e os filhos
/// deles): o `git -C <worktree>` que o `listar` roda em paralelo tem a pasta
/// atual na worktree e não conta como uso. A lista dos git é lida DEPOIS da
/// varredura: todo git que ela viu vivo já está na lista.
pub fn sem_os_meus(mut v: Varredura) -> Varredura {
    let eu = std::process::id();
    let meus = super::git::meus();
    let mut mapa = v.mapa.clone();
    v.procs.retain(|x| x.pid != eu && !descende_de(x.pid, &meus, &mut mapa));
    v.mapa = mapa;
    v
}

/// `crua` e `sem_os_meus`.
pub fn processos() -> Varredura {
    sem_os_meus(crua())
}

/// O pai pelo mapa; quem ficou de fora dele (processo de outro usuário no
/// meio da cadeia, ex. `sudo`), perguntado a ele.
pub fn pai_de(pid: u32, mapa: &mut HashMap<u32, u32>) -> u32 {
    if let Some(&p) = mapa.get(&pid) {
        return p;
    }
    let p = processos::pai(pid).unwrap_or(0);
    mapa.insert(pid, p);
    p
}

pub fn descende_de(pid: u32, ancestrais: &HashSet<u32>, mapa: &mut HashMap<u32, u32>) -> bool {
    let mut p = pid;
    for _ in 0..128 {
        if ancestrais.contains(&p) {
            return true;
        }
        p = pai_de(p, mapa);
        if p <= 1 {
            return false;
        }
    }
    false
}

/// O shell (de `conchas`) de que `pid` descende, ou ele mesmo, se é um.
pub fn concha_por_mapa(pid: u32, conchas: &HashSet<u32>, mapa: &mut HashMap<u32, u32>) -> Option<u32> {
    let mut p = pid;
    for _ in 0..128 {
        if conchas.contains(&p) {
            return Some(p);
        }
        p = pai_de(p, mapa);
        if p <= 1 {
            return None;
        }
    }
    None
}
