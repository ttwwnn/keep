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

/// A tela é a pergunta e mais nada: tem o cabeçalho e o rodapé dela, poucas linhas e nenhuma caixa de entrada (duas
/// réguas em volta do `❯`), que toda conversa traz. Essas linhas impressas dentro de uma conversa (um arquivo que o
/// modelo leu, um `cat`) não são ela: responder apertaria Enter no que a conversa tivesse à vista.
pub fn e_a_pergunta(tela: &str) -> bool {
    let linhas: Vec<&str> = tela.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    linhas.len() <= 16
        && (tela.contains("Quick safety check") || tela.contains("Do you trust the files in this folder"))
        && tela.contains("Enter to confirm")
        && linhas.iter().filter(|l| l.starts_with(&"─".repeat(12))).count() <= 1
}

/// Onde o ponteiro da pergunta está: `Some(true)` no sim, `Some(false)` no
/// não, `None` quando a pergunta não está na tela.
pub fn ponteiro(tela: &str) -> Option<bool> {
    if !e_a_pergunta(tela) || !tela.contains("Yes, I trust this folder") || !tela.contains("No, exit") {
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

    #[test]
    fn a_pergunta_impressa_dentro_de_uma_conversa_nao_e_a_pergunta() {
        let regua = "─".repeat(40);
        // As mesmas linhas, com a caixa de entrada que toda conversa traz embaixo.
        let na_conversa = format!("● Li o arquivo:\n{PERGUNTA}\n{regua}\n❯ \n{regua}\n  ⏵⏵ bypass permissions on\n");
        assert_eq!(ponteiro(&na_conversa), None);
        // Sem o cabeçalho e o rodapé dela, nem a tela inteira conta.
        assert_eq!(ponteiro(" ❯ No, exit\n   Yes, I trust this folder\n"), None);
        // Uma tela cheia de outra coisa, com as linhas no meio.
        let cheia: String = (0..30).map(|i| format!("linha {i}\n")).collect::<String>() + PERGUNTA;
        assert_eq!(ponteiro(&cheia), None);
        // O diálogo antigo, numerado, com o cabeçalho antigo.
        let antigo = "Do you trust the files in this folder?\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Enter to confirm · Esc to cancel\n";
        assert_eq!(ponteiro(antigo), Some(true));
    }
}
