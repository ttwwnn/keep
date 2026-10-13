//! As abas do Keep de volta depois de reiniciar o computador.
//!
//! O keepd guarda os shells de todas as abas; quando o computador reinicia,
//! ele morre com eles. Este crate fotografa as abas enquanto o keepd vive
//! (`retrato`) e, no keepd seguinte, as recria (`restaurar`): o mesmo
//! workspace, a mesma pasta, o painel ao lado da aba de que ele era painel, o
//! nome dado no app e, na aba que tinha o Claude ou o Codex, a mesma conversa,
//! retomada direto (sem cartão a aceitar), com os parâmetros e a conta da aba.
//!
//! O app do macOS chama `keep restauro …` (JSON): uma vez ao abrir com um keepd
//! novo, para as abas (`--fase abas`), e depois a cada 30 s, para as conversas e
//! o retrato (`manter`).

pub mod cli;
pub mod daemon;
pub mod restaurar;
pub mod retrato;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Versão dos arquivos e das respostas em JSON.
pub const VERSAO: u32 = 1;

/// A IA de uma aba: o bastante para retomar a conversa nela.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ia {
    /// `claude` ou `codex`.
    pub agente: String,
    /// O id da conversa (sessão do Claude, thread do Codex).
    pub conversa: String,
    /// A escolha de conta da aba (`claude:ordem`, `claude:<apelido>`, `gpt:<apelido>`).
    #[serde(default)]
    pub conta: String,
    /// As opções com que o programa rodava (`--model`, `--effort`…), sem a conversa.
    #[serde(default)]
    pub parametros: Vec<String>,
}

/// Uma aba como ela estava.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbaGuardada {
    pub workspace: String,
    /// O número da aba no keepd que a tinha.
    pub aba: u32,
    pub cwd: String,
    /// A aba de que esta era painel (0 = solta) e de que lado.
    #[serde(default)]
    pub split_of: u32,
    #[serde(default)]
    pub split_dir: u8,
    pub cols: u16,
    pub rows: u16,
    /// O nome que a pessoa deu à aba no app.
    #[serde(default)]
    pub nome: Option<String>,
    #[serde(default)]
    pub ia: Option<Ia>,
}

/// De que keepd é um retrato: o pid e o nascimento (List3), e o nascimento do
/// socket, que é como o app do macOS marca o que vale só para um keepd.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Keepd {
    pub pid: u32,
    pub inicio_ms: u64,
    #[serde(default)]
    pub socket_nasceu: Option<f64>,
}

impl Keepd {
    /// O mesmo keepd (pid reaproveitado depois do reinício não engana: o nascimento muda).
    pub fn mesmo(&self, outro: &Keepd) -> bool {
        if self.inicio_ms != 0 || outro.inicio_ms != 0 {
            return self.pid == outro.pid && self.inicio_ms == outro.inicio_ms;
        }
        match (self.socket_nasceu, outro.socket_nasceu) {
            (Some(a), Some(b)) => (a - b).abs() < 0.01,
            _ => self.pid == outro.pid,
        }
    }
}

/// Tudo o que um keepd tinha.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Retrato {
    pub versao: u32,
    pub gravado_ms: u64,
    pub keepd: Keepd,
    /// Os workspaces, na ordem do keepd.
    pub workspaces: Vec<String>,
    pub abas: Vec<AbaGuardada>,
}

/// Onde o Keep guarda os retratos: `KEEP_RESTAURO_ESTADO`, senão `restauro`
/// ao lado do estado da IA (no macOS, `~/Library/Application Support/Keep/restauro`).
pub fn pasta() -> PathBuf {
    if let Some(d) = std::env::var_os("KEEP_RESTAURO_ESTADO").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    let ia = keep_ia::caminhos::estado();
    ia.parent().map(|p| p.join("restauro")).unwrap_or_else(|| ia.join("restauro"))
}

pub fn agora_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Lê um JSON; `None` se não existe ou não se lê.
pub fn ler<T: for<'de> Deserialize<'de>>(caminho: &std::path::Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(caminho).ok()?).ok()
}

/// Grava um JSON por inteiro ou nada (arquivo temporário e troca), só para o dono.
pub fn gravar<T: Serialize>(caminho: &std::path::Path, valor: &T) -> std::io::Result<()> {
    if let Some(p) = caminho.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = caminho.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(valor).map_err(std::io::Error::other)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, caminho)
}

/// Uma trava entre processos (o app pode chamar duas vezes): `None` se outro a tem.
pub struct Trava {
    #[cfg(unix)]
    _arquivo: std::fs::File,
}

impl Trava {
    pub fn pegar(nome: &str) -> Option<Trava> {
        let caminho = pasta().join(nome);
        std::fs::create_dir_all(pasta()).ok()?;
        let arquivo = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(caminho).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            if unsafe { libc::flock(arquivo.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return None;
            }
            Some(Trava { _arquivo: arquivo })
        }
        #[cfg(not(unix))]
        {
            let _ = arquivo;
            Some(Trava {})
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mesmo_keepd_pelo_nascimento() {
        let a = Keepd { pid: 941, inicio_ms: 1000, socket_nasceu: Some(5.0) };
        assert!(a.mesmo(&Keepd { pid: 941, inicio_ms: 1000, socket_nasceu: None }));
        assert!(!a.mesmo(&Keepd { pid: 941, inicio_ms: 2000, socket_nasceu: Some(5.0) }), "pid reaproveitado");
        let velho = Keepd { pid: 7, inicio_ms: 0, socket_nasceu: Some(5.0) };
        assert!(velho.mesmo(&Keepd { pid: 8, inicio_ms: 0, socket_nasceu: Some(5.001) }));
        assert!(!velho.mesmo(&Keepd { pid: 7, inicio_ms: 0, socket_nasceu: Some(9.0) }));
    }
}
