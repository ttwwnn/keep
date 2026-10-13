//! `keep ia confiar <pid do shell>`: responde "sim" à pergunta do Claude Code
//! sobre confiar na pasta ("Quick safety check: Is this a project you created
//! or one you trust?"), na aba cujo shell é o pid dado — quem digitou `claude`
//! ali já disse sim. Pelo keepd: vale com a aba à vista ou não.
//!
//! O ponteiro começa em "No, exit": desce até "Yes, I trust this folder" e só
//! então Enter, e só depois de ver a pergunta (tecla cedo vira Esc, que é
//! "No, exit"). Sai ao responder, quando o Claude já está no prompt, ou em
//! 40 s. Enquanto roda, `~/.local/state/keep/trust/<pid>` existe e o app não
//! responde na mesma aba (duas setas passariam do ponto).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::{caminhos, daemon, trocar};

/// Onde o ponteiro da pergunta está: `Some(true)` no sim, `Some(false)` no
/// não, `None` quando a pergunta não está na tela.
pub fn ponteiro(tela: &str) -> Option<bool> {
    if !tela.contains("Yes, I trust this folder") || !tela.contains("No, exit") {
        return None;
    }
    for linha in tela.lines() {
        let t = linha.trim();
        if !t.starts_with('❯') {
            continue;
        }
        if t.contains("Yes, I trust this folder") {
            return Some(true);
        }
        if t.contains("No, exit") {
            return Some(false);
        }
    }
    None
}

pub fn marca(shell: u32) -> PathBuf {
    caminhos::casa().join(".local/state/keep/trust").join(shell.to_string())
}

fn aba_do_shell(shell: u32) -> Option<(String, u32)> {
    let r = daemon::listar().ok()?;
    r.abas
        .iter()
        .find(|a| a.info.shell_pid == shell && !a.info.finished)
        .map(|a| (a.ws.clone(), a.info.id))
}

pub fn cli(args: &[String]) -> i32 {
    let Some(shell) = args.first().and_then(|s| s.parse::<u32>().ok()) else { return 2 };
    let m = marca(shell);
    if let Some(dir) = m.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&m, b"");
    let resultado = responder(shell);
    let _ = std::fs::remove_file(&m);
    resultado
}

fn responder(shell: u32) -> i32 {
    let Some((ws, tab)) = aba_do_shell(shell) else { return 0 };
    let fim = Instant::now() + Duration::from_secs(40);
    let mut ultimo: Option<Instant> = None;
    while Instant::now() < fim {
        std::thread::sleep(Duration::from_millis(400));
        let Ok(tela) = daemon::tela(&ws, tab) else { return 0 };
        match ponteiro(&tela) {
            None => {
                if tela.contains("bypass permissions") || tela.contains("? for shortcuts") {
                    return 0;
                }
            }
            Some(sim) => {
                if ultimo.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                    continue;
                }
                let tecla: &[u8] = if sim { b"\r" } else { b"\x1b[B" };
                if trocar::tecla(&ws, tab, tecla).is_err() {
                    return 0;
                }
                ultimo = Some(Instant::now());
                if sim {
                    return 0;
                }
            }
        }
    }
    0
}

#[cfg(test)]
mod testes {
    use super::*;

    const PERGUNTA: &str = " Quick safety check: Is this a project you created or one you trust?\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";

    #[test]
    fn le_onde_o_ponteiro_esta() {
        assert_eq!(ponteiro(PERGUNTA), Some(false));
        let no_sim = PERGUNTA.replace(" ❯ No, exit", "   No, exit").replace("   Yes, I trust", " ❯ Yes, I trust");
        assert_eq!(ponteiro(&no_sim), Some(true));
        assert_eq!(ponteiro("ele disse: Yes, I trust this folder e No, exit\n❯ "), None);
        assert_eq!(ponteiro("❯ "), None);
    }
}
