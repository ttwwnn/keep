//! Que processos estão por trás de cada aba: o shell dela e o processo da
//! frente (o que segura o terminal agora — o shell, no prompt).
//!
//! O daemon novo diz isso na lista (`List3`): vínculo `exato`. O antigo não
//! diz, e o daemon sobrevive a cada atualização do app; para ele, o
//! vínculo é deduzido dos processos (`provavel`): serve para LER a conta e a
//! conversa de uma aba, nunca para digitar nela.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::daemon::Retrato;
use crate::processos;
use crate::tela::{Programa, programa};
use crate::worktrees::texto::titulo_limpo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vinculo {
    pub shell: u32,
    /// O processo da frente; o próprio shell no prompt.
    pub frente: u32,
    pub exato: bool,
}

/// O vínculo de cada aba viva, por (workspace, aba).
pub fn vincular(r: &Retrato) -> HashMap<(String, u32), Vinculo> {
    if r.exato {
        return r
            .abas
            .iter()
            .filter(|a| !a.info.finished && a.info.pid != 0)
            .map(|a| {
                let shell = if a.info.shell_pid != 0 { a.info.shell_pid } else { a.info.pid };
                ((a.ws.clone(), a.info.id), Vinculo { shell, frente: a.info.pid, exato: true })
            })
            .collect();
    }
    provavel(r)
}

/// O vínculo deduzido para um daemon sem `List3`: só os pares únicos.
///
/// Os shells das abas são os filhos do keepd. Cada um se compara com cada
/// aba pelo que os dois lados dizem — o tamanho do terminal, o programa da
/// frente e a pasta dele — e, antes, pelo título que o Claude põe na aba (o
/// da conversa que roda naquele shell). Fica só o par em que cada lado
/// aceita o outro e só ele; o que sobra fica sem vínculo. É o casamento do
/// kit (`kit_keep.vincular`) sem o carimbo do tty, que lá servia para
/// promover o par a exato — e aqui nada deduzido é exato.
fn provavel(r: &Retrato) -> HashMap<(String, u32), Vinculo> {
    let lista = processos::todos();
    let keepd = if r.daemon.pid != 0 {
        r.daemon.pid
    } else {
        // Sem o pid na resposta: só se houver um keepd só.
        let mut k = lista.iter().filter(|p| p.nome.eq_ignore_ascii_case("keepd"));
        match (k.next(), k.next()) {
            (Some(p), None) => p.pid,
            _ => return HashMap::new(),
        }
    };
    // Um filho sem pasta legível acabou de morrer (ou nem é do usuário): não
    // é shell de aba, e contado como "pode ser qualquer uma" tiraria o par de
    // quem é.
    let conchas: Vec<Concha> = lista
        .iter()
        .filter(|p| p.ppid == keepd && p.pid != keepd)
        .map(|p| Concha::de(p, &lista))
        .filter(|c| c.pasta.is_some())
        .collect();
    if conchas.is_empty() {
        return HashMap::new();
    }
    if conchas.iter().any(|c| c.pede_titulos) {
        let vivas = crate::worktrees::sessoes::vivas_em(&crate::worktrees::sessoes::pasta_padrao());
        let projetos = crate::caminhos::claude().join("projects");
        let mut conchas = conchas;
        for c in &mut conchas {
            let debaixo = processos::descendentes_em(&lista, c.pid);
            for s in vivas.iter().filter(|s| debaixo.contains(&s.pid)) {
                if let Some(tr) = crate::worktrees::sessoes::transcrito_de(&s.id, &projetos) {
                    c.titulos.extend(crate::worktrees::sessoes::titulos_do_transcrito(&tr));
                }
            }
        }
        return casa(r, &conchas);
    }
    casa(r, &conchas)
}

fn casa(r: &Retrato, conchas: &[Concha]) -> HashMap<(String, u32), Vinculo> {
    let abas: Vec<&crate::daemon::Aba> = r.abas.iter().filter(|a| !a.info.finished).collect();
    let mut pares: HashMap<usize, usize> = HashMap::new();
    for modo in [Modo::Titulo, Modo::Base] {
        casamento_unico(conchas, &abas, modo, &mut pares);
    }
    pares
        .into_iter()
        .map(|(c, a)| {
            let (c, a) = (&conchas[c], abas[a]);
            ((a.ws.clone(), a.info.id), Vinculo { shell: c.pid, frente: c.frente, exato: false })
        })
        .collect()
}

/// Um shell filho do keepd e o que dá para comparar dele com uma aba.
struct Concha {
    pid: u32,
    frente: u32,
    colunas: u16,
    linhas: u16,
    /// O processo da frente, pelo nome do sistema e pela linha de comando.
    programas: Vec<Programa>,
    pasta: Option<String>,
    /// Os títulos das conversas do Claude que rodam neste shell.
    titulos: BTreeSet<String>,
    pede_titulos: bool,
}

impl Concha {
    fn de(p: &processos::Processo, lista: &[processos::Processo]) -> Concha {
        // O terminal só diz algo do shell se for o dele: um filho do keepd sem
        // pty própria herdou o de outro, cuja frente nem é da árvore dele.
        let debaixo = processos::descendentes_em(lista, p.pid);
        let terminal = processos::terminal(p.pid)
            .filter(|t| t.frente.is_some_and(|f| f == p.pid || debaixo.contains(&f)));
        let frente = match terminal.and_then(|t| t.frente) {
            Some(f) if processos::vivo(f) => f,
            _ if cfg!(windows) => mais_novo(p, lista),
            _ => p.pid,
        };
        let mut programas = Vec::new();
        let nomes = [
            lista.iter().find(|q| q.pid == frente).map(|q| q.nome.clone()),
            processos::programa(frente),
        ];
        for n in nomes.into_iter().flatten() {
            let prog = programa(&n);
            if prog != Programa::Desconhecido && !programas.contains(&prog) {
                programas.push(prog);
            }
        }
        let pasta = processos::cwd(frente).or_else(|| processos::cwd(p.pid)).map(|d| d.to_string_lossy().into_owned());
        Concha {
            pid: p.pid,
            frente,
            colunas: terminal.map(|t| t.colunas).unwrap_or(0),
            linhas: terminal.map(|t| t.linhas).unwrap_or(0),
            pede_titulos: programas.contains(&Programa::Claude),
            programas,
            pasta,
            titulos: BTreeSet::new(),
        }
    }
}

/// O filho mais novo do shell, fora os hospedeiros do console (o "processo
/// da frente" do Windows, como o keepd o lê); o próprio shell se não houver.
fn mais_novo(shell: &processos::Processo, lista: &[processos::Processo]) -> u32 {
    lista
        .iter()
        .filter(|q| q.ppid == shell.pid && q.pid != shell.pid)
        .filter(|q| !matches!(q.nome.to_ascii_lowercase().as_str(), "conhost" | "openconsole"))
        .filter(|q| shell.inicio_ms == 0 || q.inicio_ms >= shell.inicio_ms)
        .max_by_key(|q| (q.inicio_ms, q.pid))
        .map(|q| q.pid)
        .unwrap_or(shell.pid)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modo {
    /// A identidade: o título da aba é o de uma conversa deste shell.
    Titulo,
    /// Só não pode contradizer: tamanho, programa e pasta.
    Base,
}

fn mesma_pasta(a: &str, b: &str) -> bool {
    let limpa = |s: &str| {
        let s = s.trim_end_matches(['/', '\\']);
        if cfg!(windows) { s.replace('/', "\\").to_lowercase() } else { s.to_string() }
    };
    limpa(a) == limpa(b)
}

fn compativel(c: &Concha, a: &crate::daemon::Aba, modo: Modo) -> bool {
    let t = &a.info;
    if t.finished {
        return false;
    }
    if c.colunas != 0 && t.cols != 0 && (c.colunas, c.linhas) != (t.cols, t.rows) {
        return false;
    }
    let comando = programa(&t.command);
    if comando != Programa::Desconhecido && !c.programas.is_empty() && !c.programas.contains(&comando) {
        return false;
    }
    if !t.cwd.is_empty() && c.pasta.as_deref().is_some_and(|p| !mesma_pasta(&t.cwd, p)) {
        return false;
    }
    match modo {
        Modo::Titulo => !c.titulos.is_empty() && c.titulos.contains(&titulo_limpo(&t.title)),
        Modo::Base => true,
    }
}

/// Pares (shell, aba) em que cada lado só aceita o outro: acha um, tira os
/// dois e repete.
fn casamento_unico(
    conchas: &[Concha],
    abas: &[&crate::daemon::Aba],
    modo: Modo,
    pares: &mut HashMap<usize, usize>,
) {
    let mut livres_c: Vec<usize> = (0..conchas.len()).filter(|c| !pares.contains_key(c)).collect();
    let usadas: HashSet<usize> = pares.values().copied().collect();
    let mut livres_a: Vec<usize> = (0..abas.len()).filter(|a| !usadas.contains(a)).collect();
    loop {
        let compat: HashMap<usize, Vec<usize>> = livres_c
            .iter()
            .map(|&c| (c, livres_a.iter().copied().filter(|&a| compativel(&conchas[c], abas[a], modo)).collect()))
            .collect();
        let achado = livres_c.iter().copied().find_map(|c| {
            let [a] = compat[&c][..] else { return None };
            let disputada = livres_c.iter().any(|&c2| c2 != c && compat[&c2].contains(&a));
            (!disputada).then_some((c, a))
        });
        let Some((c, a)) = achado else { return };
        pares.insert(c, a);
        livres_c.retain(|&x| x != c);
        livres_a.retain(|&x| x != a);
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::daemon::Aba;
    use keep_proto::{DaemonInfo, TabInfo};
    use std::process::{Command, Stdio};
    use std::time::Duration;

    /// Faz de keepd nos testes: o próprio executável de teste, que abre um
    /// "shell" (outra cópia dele, dormindo) em cada pasta pedida.
    #[test]
    #[ignore]
    fn keepd_de_mentira() {
        if std::env::var_os("KEEP_IA_TESTE_SHELL").is_some() {
            std::thread::sleep(Duration::from_secs(15));
            return;
        }
        let Ok(pastas) = std::env::var("KEEP_IA_TESTE_KEEPD") else { return };
        let mut filhos: Vec<_> = pastas
            .split('|')
            .map(|p| {
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "vinculo::testes::keepd_de_mentira", "--ignored", "-q", "--test-threads=1"])
                    .env_remove("KEEP_IA_TESTE_KEEPD")
                    .env("KEEP_IA_TESTE_SHELL", "1")
                    .current_dir(p)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap()
            })
            .collect();
        std::thread::sleep(Duration::from_secs(15));
        for f in &mut filhos {
            f.kill().ok();
            f.wait().ok();
        }
    }

    /// Um keepd de mentira com um shell em cada pasta; devolve o processo e
    /// os shells (pid, pasta), quando todos já dizem a pasta.
    fn keepd(pastas: &[&std::path::Path]) -> (std::process::Child, Vec<u32>) {
        let junto = pastas.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("|");
        let k = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "vinculo::testes::keepd_de_mentira", "--ignored", "-q", "--test-threads=1"])
            .env("KEEP_IA_TESTE_KEEPD", junto)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = k.id();
        for _ in 0..200 {
            let filhos: Vec<u32> = processos::todos().into_iter().filter(|p| p.ppid == pid).map(|p| p.pid).collect();
            if filhos.len() == pastas.len() && filhos.iter().all(|&f| processos::cwd(f).is_some()) {
                return (k, filhos);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("os shells de mentira não subiram");
    }

    fn aba(ws: &str, id: u32, cwd: &std::path::Path) -> Aba {
        let info = TabInfo { id, cwd: cwd.to_string_lossy().into_owned(), ..TabInfo::default() };
        Aba { ws: ws.into(), info }
    }

    fn mesma(x: &std::path::Path, y: &std::path::Path) -> bool {
        std::fs::canonicalize(x).unwrap() == std::fs::canonicalize(y).unwrap()
    }

    #[test]
    fn sem_list3_so_os_pares_unicos_e_nunca_exatos() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (d.path().join("a"), d.path().join("b"), d.path().join("c"));
        for p in [&a, &b, &c] {
            std::fs::create_dir(p).unwrap();
        }
        let (mut k, shells) = keepd(&[&a, &b]);
        // A pasta como o keepd a diria: a que o processo da frente diz.
        let pasta_de = |s: u32| processos::cwd(s).unwrap();
        let sa = *shells.iter().find(|&&s| mesma(&pasta_de(s), &a)).unwrap();
        let sb = *shells.iter().find(|&&s| mesma(&pasta_de(s), &b)).unwrap();
        let r = Retrato {
            daemon: DaemonInfo { pid: k.id(), started_ms: 0 },
            abas: vec![aba("W", 1, &pasta_de(sa)), aba("W", 2, &pasta_de(sb)), aba("W", 3, &c)],
            exato: false,
        };
        let v = vincular(&r);
        k.kill().ok();
        k.wait().ok();
        assert_eq!(v.get(&("W".into(), 1)), Some(&Vinculo { shell: sa, frente: sa, exato: false }), "{v:?}");
        assert_eq!(v.get(&("W".into(), 2)), Some(&Vinculo { shell: sb, frente: sb, exato: false }), "{v:?}");
        assert_eq!(v.len(), 2, "a aba sem shell fica sem vínculo: {v:?}");
    }

    #[test]
    fn dois_shells_que_servem_na_mesma_aba_ficam_sem_vinculo() {
        let d = tempfile::tempdir().unwrap();
        let (mut k, shells) = keepd(&[d.path(), d.path()]);
        let a = processos::cwd(shells[0]).unwrap();
        let r = Retrato { daemon: DaemonInfo { pid: k.id(), started_ms: 0 }, abas: vec![aba("W", 1, &a)], exato: false };
        let v = vincular(&r);
        k.kill().ok();
        k.wait().ok();
        assert!(v.is_empty(), "{v:?}");
    }
}
