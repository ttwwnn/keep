//! `keep ia statusline`: a statusLine do Claude Code no Keep.
//!
//! O Claude Code roda este comando com o JSON da sessão na entrada (modelo,
//! esforço, limites de uso). Ele o grava em `<pasta>/<pid do shell da
//! aba>.json` — o shell da aba é o ancestral cujo pai é o keepd — para a
//! barra do app mostrar o modelo e o esforço de cada conversa. Não imprime
//! nada: o que sair aqui o Claude Code escreveria no rodapé dele.

use std::io::Read;
use std::path::PathBuf;

use crate::{caminhos, processos};

/// Onde os arquivos ficam. `KEEP_STATUS_DIR` troca (testes).
pub fn pasta() -> PathBuf {
    match std::env::var_os("KEEP_STATUS_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => caminhos::casa().join(".local/state/keep/claude"),
    }
}

/// O shell da aba de um processo: o ancestral cujo pai é o keepd.
pub fn shell_da_aba(mut pid: u32) -> Option<u32> {
    for _ in 0..24 {
        let pai = processos::pai(pid)?;
        if pai <= 1 {
            return None;
        }
        let nome = processos::um(pai).map(|p| p.nome).unwrap_or_default();
        if nome == "keepd" || nome.ends_with("/keepd") {
            return Some(pid);
        }
        pid = pai;
    }
    None
}

/// Grava a entrada para o shell da aba. Fora do Keep, nada.
pub fn cli() -> i32 {
    let mut entrada = Vec::new();
    if std::io::stdin().read_to_end(&mut entrada).is_err() || entrada.is_empty() {
        return 0;
    }
    let Some(shell) = shell_da_aba(std::process::id()) else { return 0 };
    let dir = pasta();
    if std::fs::create_dir_all(&dir).is_err() {
        return 0;
    }
    let tmp = dir.join(format!(".in.{}", std::process::id()));
    if std::fs::write(&tmp, &entrada).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(format!("{shell}.json")));
    }
    let _ = std::fs::remove_file(&tmp);
    0
}
