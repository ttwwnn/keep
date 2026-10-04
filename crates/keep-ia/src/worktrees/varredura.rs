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
/// os git que ele subiu), o pai e o nascimento de cada um.
#[derive(Clone, Debug, Default)]
pub struct Varredura {
    pub procs: Vec<ComPasta>,
    pub mapa: HashMap<u32, u32>,
    /// ms unix (só os que o sistema disse).
    pub inicios: HashMap<u32, u64>,
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
    let inicios = lista.iter().filter(|p| p.inicio_ms > 0).map(|p| (p.pid, p.inicio_ms)).collect();
    let procs = lista
        .iter()
        .filter_map(|p| {
            let cwd = processos::cwd(p.pid)?;
            Some(ComPasta { pid: p.pid, ppid: p.ppid, nome: nome_legivel(p), cwd: cwd.to_string_lossy().into_owned() })
        })
        .collect();
    Varredura { procs, mapa, inicios }
}

/// Tira desta varredura este processo e os git que ele subiu (e os filhos
/// deles): o `git -C <worktree>` que o `listar` roda em paralelo tem a pasta
/// atual na worktree e não conta como uso. A lista dos git é lida DEPOIS da
/// varredura: todo git que ela viu vivo já está na lista.
pub fn sem_os_meus(mut v: Varredura) -> Varredura {
    let eu = std::process::id();
    let meus = super::git::meus();
    let mut mapa = v.mapa.clone();
    let inicios = v.inicios.clone();
    v.procs.retain(|x| x.pid != eu && !e_dos_meus(x.pid, &meus, &inicios, &mut mapa));
    v.mapa = mapa;
    v
}

/// O processo é um git deste processo, ou descende de um: pelo pid E pela
/// hora em que nasceu. Um pid de git que acabou, que o Windows já deu a
/// outro processo, não é um git: é outro. De um git que já acabou só é
/// filho quem nasceu enquanto ele rodava.
fn e_dos_meus(pid: u32, meus: &[super::git::Meu], inicios: &HashMap<u32, u64>, mapa: &mut HashMap<u32, u32>) -> bool {
    let mut p = pid;
    // O nascimento do descendente mais novo já visto na subida.
    let mut nasceu_filho: Option<u64> = None;
    for _ in 0..128 {
        let nasceu = inicios.get(&p).copied();
        if pai_velho_demais(nasceu, nasceu_filho) {
            return false;
        }
        for m in meus.iter().filter(|m| m.pid == p) {
            let e = match (nasceu, m.inicio_ms) {
                (Some(n), i) if i > 0 => n.abs_diff(i) <= 1,
                // Sem a hora do git: pelo pid, como antes.
                (Some(_), _) => true,
                // O git (ou quem tinha o pid) já não existe: o filho é dele se
                // nasceu enquanto o git rodava.
                (None, i) => nasceu_filho
                    .is_none_or(|f| f + 1 >= i && m.fim_ms.is_none_or(|fim| f <= fim + 1)),
            };
            if e {
                return true;
            }
        }
        let pai = pai_de(p, mapa);
        if pai <= 1 || pai == p {
            return false;
        }
        nasceu_filho = nasceu.or(nasceu_filho);
        p = pai;
    }
    false
}

/// O "pai" nasceu depois do filho: o pid é de outro processo, que o pegou
/// quando o pai de verdade acabou (no Windows o pai de um processo é só um
/// número, que sobrevive a ele).
fn pai_velho_demais(nasceu_pai: Option<u64>, nasceu_filho: Option<u64>) -> bool {
    matches!((nasceu_pai, nasceu_filho), (Some(p), Some(f)) if p > f)
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

/// O shell (de `conchas`) de que `pid` descende, ou ele mesmo, se é um. A
/// subida para num pai que nasceu depois do filho: esse pid já é de outro.
pub fn concha_por_mapa(
    pid: u32,
    conchas: &HashSet<u32>,
    mapa: &mut HashMap<u32, u32>,
    inicios: &HashMap<u32, u64>,
) -> Option<u32> {
    let mut p = pid;
    let mut nasceu_filho: Option<u64> = None;
    for _ in 0..128 {
        let nasceu = inicios.get(&p).copied();
        if pai_velho_demais(nasceu, nasceu_filho) {
            return None;
        }
        if conchas.contains(&p) {
            return Some(p);
        }
        let pai = pai_de(p, mapa);
        if pai <= 1 || pai == p {
            return None;
        }
        nasceu_filho = nasceu.or(nasceu_filho);
        p = pai;
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::worktrees::git::Meu;

    #[test]
    fn um_pid_de_git_que_outro_processo_pegou_nao_e_git() {
        // 50 nasceu às 5000 com o pid de um git deste processo que rodou das
        // 1000 às 2000; 60, filho dele, nasceu às 6000.
        let mut mapa = HashMap::from([(50, 10), (60, 50), (10, 1)]);
        let inicios = HashMap::from([(50, 5000), (60, 6000), (10, 100)]);
        let git = Meu { pid: 50, inicio_ms: 1000, fim_ms: Some(2000) };
        assert!(!e_dos_meus(50, &[git], &inicios, &mut mapa));
        assert!(!e_dos_meus(60, &[git], &inicios, &mut mapa));
        // O próprio git (o mesmo pid e a mesma hora), e um filho dele.
        let git = Meu { pid: 50, inicio_ms: 5000, fim_ms: None };
        assert!(e_dos_meus(50, &[git], &inicios, &mut mapa));
        assert!(e_dos_meus(60, &[git], &inicios, &mut mapa));
        // Um filho de um git que já acabou (fora da lista do sistema): só se
        // nasceu enquanto o git rodava.
        let mut mapa = HashMap::from([(70, 40), (80, 40)]);
        let inicios = HashMap::from([(70, 1500), (80, 9000)]);
        let git = Meu { pid: 40, inicio_ms: 1000, fim_ms: Some(2000) };
        assert!(e_dos_meus(70, &[git], &inicios, &mut mapa));
        assert!(!e_dos_meus(80, &[git], &inicios, &mut mapa));
    }

    #[test]
    fn a_subida_para_num_pai_mais_novo_que_o_filho() {
        // 30 tem como pai o pid 20, que hoje é um shell que nasceu depois dele.
        let conchas = HashSet::from([20]);
        let mut mapa = HashMap::from([(30, 20), (20, 1)]);
        assert_eq!(concha_por_mapa(30, &conchas, &mut mapa, &HashMap::from([(30, 1000), (20, 3000)])), None);
        assert_eq!(concha_por_mapa(30, &conchas, &mut mapa, &HashMap::from([(30, 3000), (20, 1000)])), Some(20));
        // Sem o nascimento de um dos dois, vale o pai que o sistema diz.
        assert_eq!(concha_por_mapa(30, &conchas, &mut mapa, &HashMap::from([(30, 1000)])), Some(20));
        assert_eq!(concha_por_mapa(20, &conchas, &mut mapa, &HashMap::new()), Some(20));
    }
}
