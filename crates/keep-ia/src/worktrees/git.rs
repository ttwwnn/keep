//! O git que este módulo roda: com prazo, sem travas opcionais nem perguntas,
//! e anotado — o `git -C <worktree>` põe a pasta atual dele lá dentro, e a
//! varredura de processos não pode tomá-lo por alguém usando a worktree.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::prazo::Estourou;

/// Os git que este processo subiu: pid → quando acabou (`None`: rodando).
/// Quem acabou fica um minuto: a varredura pode tê-lo visto vivo, com a
/// pasta atual na worktree, e só consultar esta lista depois.
static MEUS: Mutex<Option<HashMap<u32, Option<Instant>>>> = Mutex::new(None);
const GUARDA: Duration = Duration::from_secs(60);

fn com_meus<T>(f: impl FnOnce(&mut HashMap<u32, Option<Instant>>) -> T) -> T {
    let mut g = MEUS.lock().unwrap_or_else(|e| e.into_inner());
    let mapa = g.get_or_insert_with(HashMap::new);
    mapa.retain(|_, fim| fim.is_none_or(|t| t.elapsed() < GUARDA));
    f(mapa)
}

/// Os pids dos git deste processo (vivos e os que acabaram há pouco).
pub fn meus() -> HashSet<u32> {
    com_meus(|m| m.keys().copied().collect())
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
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .env("LC_ALL", "C")
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
            m.insert(f.id(), None);
        }
        f
    });
    let mut filho = match filho {
        Ok(f) => f,
        Err(e) => return Ok(Saida { codigo: 127, saida: String::new(), erro: e.to_string() }),
    };
    let pid = filho.id();
    let acabou = || com_meus(|m| m.insert(pid, Some(Instant::now())));
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
