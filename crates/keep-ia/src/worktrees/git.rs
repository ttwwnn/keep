//! O git que este módulo roda: com prazo, sem travas opcionais nem perguntas,
//! e anotado — o `git -C <worktree>` põe a pasta atual dele lá dentro, e a
//! varredura de processos não pode tomá-lo por alguém usando a worktree.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::prazo::Estourou;

/// Um git que este processo subiu: o pid, quando nasceu e quando acabou
/// (ms unix; `None`: rodando). O pid sozinho não basta: o Windows dá logo a
/// um processo novo o pid de um que acabou, e um shell de outra aba que
/// pegasse o pid de um destes sumiria da varredura.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meu {
    pub pid: u32,
    /// 0 quando não se soube.
    pub inicio_ms: u64,
    pub fim_ms: Option<u64>,
}

/// Os git deste processo. Quem acabou fica um minuto: a varredura pode tê-lo
/// visto vivo, com a pasta atual na worktree, e só consultar esta lista
/// depois.
static MEUS: Mutex<Vec<Meu>> = Mutex::new(Vec::new());
const GUARDA_MS: u64 = 60_000;

fn agora_ms() -> u64 {
    (super::texto::agora() * 1000.0) as u64
}

fn com_meus<T>(f: impl FnOnce(&mut Vec<Meu>) -> T) -> T {
    let mut g = MEUS.lock().unwrap_or_else(|e| e.into_inner());
    let agora = agora_ms();
    g.retain(|m| m.fim_ms.is_none_or(|fim| agora.saturating_sub(fim) < GUARDA_MS));
    f(&mut g)
}

/// Os git deste processo (os vivos e os que acabaram há pouco).
pub fn meus() -> Vec<Meu> {
    com_meus(|m| m.clone())
}

/// Para os testes: um git deste processo com esse pid e essas horas.
#[doc(hidden)]
pub fn anota_para_teste(pid: u32, inicio_ms: u64, fim_ms: Option<u64>) {
    com_meus(|m| m.push(Meu { pid, inicio_ms, fim_ms }));
}

#[doc(hidden)]
pub fn esquece_para_teste(pid: u32) {
    com_meus(|m| m.retain(|x| x.pid != pid));
}

/// O que o git respondeu.
#[derive(Clone, Debug, Default)]
pub struct Saida {
    pub codigo: i32,
    pub saida: String,
    pub erro: String,
}

impl Saida {
    pub fn ok(&self) -> bool {
        self.codigo == 0
    }
}

/// Roda `git <args>` e espera no máximo `prazo` segundos (passado disso, o
/// git morre e a resposta é `Estourou`). Git que nem sobe responde 127.
pub fn git(bin: &Path, args: &[&str], prazo: f64) -> Result<Saida, Estourou> {
    git_com(bin, args, &[], prazo)
}

/// `git`, com variáveis a mais no ambiente dele (`GIT_INDEX_FILE`…).
pub fn git_com(bin: &Path, args: &[&str], vars: &[(&str, &str)], prazo: f64) -> Result<Saida, Estourou> {
    let mut cmd = Command::new(bin);
    // Nada do `.git/config` nem dos hooks de uma pasta alheia roda daqui: `core.fsmonitor` é um comando que o
    // `status` e o `add` executam, e a pasta pode ter vindo num zip ou ter sido mexida por uma IA presa.
    let sem_hooks = if cfg!(windows) { "core.hooksPath=NUL" } else { "core.hooksPath=/dev/null" };
    cmd.args(["-c", "core.fsmonitor=false", "-c", sem_hooks]);
    cmd.args(args).envs(vars.iter().copied());
    cmd.env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    // A trava cobre o nascimento e a anotação: quem varre os processos lê a
    // lista DEPOIS de varrer, sob a mesma trava; todo git que viu vivo já
    // está nela.
    let filho = com_meus(|m| {
        let f = cmd.spawn();
        if let Ok(f) = &f {
            // Enquanto este processo segura o filho, o pid é dele: a hora em
            // que nasceu é a do git.
            let inicio_ms = crate::processos::inicio_ms(f.id()).unwrap_or(0);
            m.push(Meu { pid: f.id(), inicio_ms, fim_ms: None });
        }
        f
    });
    let mut filho = match filho {
        Ok(f) => f,
        Err(e) => return Ok(Saida { codigo: 127, saida: String::new(), erro: e.to_string() }),
    };
    let pid = filho.id();
    let acabou = || {
        com_meus(|m| {
            if let Some(x) = m.iter_mut().rev().find(|x| x.pid == pid && x.fim_ms.is_none()) {
                x.fim_ms = Some(agora_ms());
            }
        })
    };
    let le = |r: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        if let Some(mut r) = r {
            std::thread::spawn(move || {
                let mut b = Vec::new();
                r.read_to_end(&mut b).ok();
                tx.send(b).ok();
            });
        }
        rx
    };
    let saida = le(filho.stdout.take().map(|r| Box::new(r) as Box<dyn Read + Send>));
    let erro = le(filho.stderr.take().map(|r| Box::new(r) as Box<dyn Read + Send>));
    let fim = Instant::now() + Duration::from_secs_f64(prazo.clamp(0.05, 1e6));
    let mut passo = Duration::from_millis(1);
    let status = loop {
        match filho.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(_) => break None,
        }
        if Instant::now() >= fim {
            filho.kill().ok();
            filho.wait().ok();
            acabou();
            return Err(Estourou);
        }
        std::thread::sleep(passo);
        passo = (passo * 2).min(Duration::from_millis(10));
    };
    acabou();
    // Um neto que herdou a saída pode segurá-la aberta: não se espera por ele.
    let espera = Duration::from_millis(500);
    let texto = |rx: mpsc::Receiver<Vec<u8>>| String::from_utf8_lossy(&rx.recv_timeout(espera).unwrap_or_default()).into_owned();
    Ok(Saida {
        codigo: status.and_then(|s| s.code()).unwrap_or(-1),
        saida: texto(saida),
        erro: texto(erro),
    })
}

/// O git deste sistema: o do Homebrew no macOS se houver, senão o do PATH.
pub fn padrao() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    for p in ["/opt/homebrew/bin/git", "/usr/local/bin/git", "/usr/bin/git"] {
        if Path::new(p).is_file() {
            return p.into();
        }
    }
    "git".into()
}

#[cfg(all(test, unix))]
mod testes {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn o_git_do_keep_nao_roda_o_fsmonitor_nem_os_hooks_da_pasta() {
        let Ok(git_bin) = which_git() else { return };
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("r");
        std::fs::create_dir_all(&repo).unwrap();
        let rodou = d.path().join("rodou");
        let sh = |args: &[&str]| {
            let s = Command::new(&git_bin).args(args).current_dir(&repo).output().unwrap();
            assert!(s.status.success(), "{args:?}: {}", String::from_utf8_lossy(&s.stderr));
        };
        sh(&["init", "-q"]);
        // O que uma pasta alheia (um zip, uma IA presa) pode trazer: um comando no config e um hook.
        sh(&["config", "core.fsmonitor", &format!("touch {}", rodou.display())]);
        let hook = repo.join(".git/hooks/reference-transaction");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch {}\n", rodou.display())).unwrap();
        std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let cam = repo.to_string_lossy().into_owned();
        let s = git(&git_bin, &["-C", &cam, "status", "--porcelain=v2", "--branch"], 20.0).unwrap();
        assert_eq!(s.codigo, 0, "{}", s.erro);
        let s = git(&git_bin, &["-C", &cam, "update-ref", "refs/keep/teste", "HEAD"], 20.0).unwrap();
        let _ = s;
        assert!(!rodou.exists(), "o git do Keep executou código da pasta");
        // O mesmo git, sem a proteção, o executaria (o teste prova o que protege).
        let s = Command::new(&git_bin).args(["-C", &cam, "status", "--porcelain=v2"]).output().unwrap();
        assert!(s.status.success());
        assert!(rodou.exists(), "o fsmonitor de exemplo não rodou nem sem a proteção: o teste não prova nada");
    }

    fn which_git() -> Result<PathBuf, ()> {
        ["/usr/bin/git", "/opt/homebrew/bin/git", "/usr/local/bin/git"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or(())
    }
}
