//! `keep ia rodar -- <programa> [argumentos…]`: sobe o Claude ou o Codex
//! numa aba do Keep com tudo o que é do Keep, e nada mais:
//!
//! - o Claude que abre conversa sobe no armazém de login da aba (`keep ia
//!   preparar`, docs/ia.md "Troca a quente"): só o acesso da conta da aba,
//!   que o Keep renova e espalha, e trocar a conta pelo menu da aba é
//!   regravar o armazém, com o Claude aberto. Com
//!   `CLAUDE_SECURESTORAGE_CONFIG_DIR` já posto (um Claude aberto de dentro
//!   de outro herda o da aba; uma conta presa à mão), fica o que veio;
//! - "confiar nesta pasta?" é respondido sim (`keep ia confiar`), em segundo
//!   plano: quem digitou `claude` na aba já disse sim;
//! - o programa fica preso à pasta do workspace quando há uma (`keep ia
//!   confinar`).
//!
//! É a porta do shell: a função `claude()` (ou `codex()`) do zsh chama
//! `keep ia rodar -- claude "$@"`, com as preferências da pessoa já na linha
//! (esforço, modo de permissão). Fora de uma aba do Keep o programa roda
//! como rodaria. Uma falha ao preparar o login nunca impede o Claude de
//! subir: diz numa linha e sobe no login global. O mesmo, no PowerShell do
//! Windows, é o `ai.ps1` do keepd.

use std::path::Path;

/// O que o Keep faz com a linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Roda como rodaria: não abre conversa (`claude mcp`, `--version`…).
    Direto,
    /// Só a pasta do workspace: o que despacha sessões (`claude agents`,
    /// `attach`, `rc`) e o Codex.
    SoConfinar,
    /// Armazém da aba, "confiar nesta pasta" e a pasta do workspace.
    Completo,
}

/// Os subcomandos do Claude Code que não abrem conversa.
const SEM_CONVERSA: &[&str] = &[
    "auth", "auto-mode", "config", "daemon", "doctor", "gateway", "help", "import", "install", "kill", "logs", "mcp",
    "plugin", "plugins", "project", "respawn", "rm", "setup-token", "stop", "ultrareview", "update", "upgrade",
];

fn nome(programa: &str) -> String {
    Path::new(programa).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn modo(programa: &str, args: &[String]) -> Modo {
    match nome(programa).as_str() {
        "claude" => {
            if args.iter().any(|a| matches!(a.as_str(), "-v" | "--version" | "-h" | "--help")) {
                return Modo::Direto;
            }
            match args.first().map(String::as_str) {
                Some(s) if SEM_CONVERSA.contains(&s) => Modo::Direto,
                Some("agents" | "attach" | "rc" | "remote-control") => Modo::SoConfinar,
                _ => Modo::Completo,
            }
        }
        _ => Modo::SoConfinar,
    }
}

#[cfg(unix)]
pub fn cli(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    let args: Vec<String> = match args.first().map(String::as_str) {
        Some("--") => args[1..].to_vec(),
        _ => args.to_vec(),
    };
    let Some((programa, resto)) = args.split_first() else {
        eprintln!("uso: keep ia rodar -- <programa> [argumentos...]");
        return 2;
    };
    let ws = std::env::var("KEEP_WORKSPACE").unwrap_or_default();
    let modo = if ws.is_empty() { Modo::Direto } else { modo(programa, resto) };
    if modo == Modo::Direto {
        let erro = std::process::Command::new(programa).args(resto).exec();
        eprintln!("Keep: não foi possível rodar {programa}: {erro}");
        return 127;
    }
    if modo == Modo::Completo {
        let shell = std::os::unix::process::parent_id();
        confiar_em_segundo_plano(shell);
        if std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR").is_none_or(|v| v.is_empty()) {
            let escolha = std::env::var("KEEP_IA_ESCOLHA").ok().filter(|e| !e.is_empty());
            match crate::quente::preparar(&ws, shell, escolha.as_deref(), None) {
                // SAFETY: um só fio; o processo vira o programa logo adiante.
                Ok(p) => unsafe { std::env::set_var("CLAUDE_SECURESTORAGE_CONFIG_DIR", &p.pasta) },
                Err(r) => eprintln!("Keep: não deu para preparar o login desta aba ({}); o Claude sobe como antes.", r.motivo),
            }
        }
    }
    crate::confinar::cli(&args)
}

#[cfg(windows)]
pub fn cli(_args: &[String]) -> i32 {
    eprintln!("Keep: no Windows quem sobe a IA na aba é a função do PowerShell (ai.ps1).");
    2
}

/// `keep ia confiar <shell>` solto: o `sh` sai na hora e o ajudante fica
/// com o launchd, não como filho do programa que este processo vai virar.
#[cfg(unix)]
fn confiar_em_segundo_plano(shell: u32) {
    let Ok(keep) = std::env::current_exe() else { return };
    let _ = std::process::Command::new("/bin/sh")
        .args(["-c", "\"$0\" ia confiar \"$1\" </dev/null >/dev/null 2>&1 &"])
        .arg(keep)
        .arg(shell.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn so_a_conversa_do_claude_ganha_o_armazem_da_aba() {
        assert_eq!(modo("claude", &v(&["--effort", "max"])), Modo::Completo);
        assert_eq!(modo("/Users/a/.local/bin/claude", &v(&["--resume", "x"])), Modo::Completo);
        assert_eq!(modo("claude", &v(&["-p", "oi"])), Modo::Completo);
        assert_eq!(modo("claude", &v(&[])), Modo::Completo);
        for sub in ["mcp", "update", "auth", "doctor", "plugin"] {
            assert_eq!(modo("claude", &v(&[sub, "list"])), Modo::Direto, "{sub}");
        }
        assert_eq!(modo("claude", &v(&["--version"])), Modo::Direto);
        for sub in ["agents", "attach", "rc", "remote-control"] {
            assert_eq!(modo("claude", &v(&[sub])), Modo::SoConfinar, "{sub}");
        }
        assert_eq!(modo("codex", &v(&["resume"])), Modo::SoConfinar);
    }
}
