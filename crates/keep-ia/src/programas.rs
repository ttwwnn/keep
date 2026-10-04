//! Onde estão o Claude Code e o Codex, e em que versão.
//!
//! Um app aberto pelo Finder ou pelo menu Iniciar não herda o PATH do shell:
//! os lugares onde os instaladores põem cada um são procurados por nome.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::caminhos::casa;

fn executavel(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.is_file() && p.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(windows)]
    {
        p.is_file()
    }
}

fn no_path(nomes: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for nome in nomes {
            let p = dir.join(nome);
            if executavel(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// O Claude Code: `KEEP_IA_CLAUDE_BIN`, o PATH, e os lugares dos instaladores.
pub fn claude() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KEEP_IA_CLAUDE_BIN") {
        return Some(PathBuf::from(p));
    }
    #[cfg(windows)]
    let nomes = ["claude.exe", "claude.cmd"];
    #[cfg(not(windows))]
    let nomes = ["claude"];
    if let Some(p) = no_path(&nomes) {
        return Some(p);
    }
    let h = casa();
    let mut lugares = vec![h.join(".local/bin").join(nomes[0]), h.join(".claude/local").join(nomes[0])];
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        lugares.push(PathBuf::from(appdata).join("npm").join("claude.cmd"));
    }
    #[cfg(not(windows))]
    lugares.extend([
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        h.join(".npm-global/bin/claude"),
    ]);
    lugares.into_iter().find(|p| executavel(p))
}

/// O Codex: `KEEP_IA_CODEX_BIN`, o PATH, e os lugares dos instaladores.
pub fn codex() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KEEP_IA_CODEX_BIN") {
        return Some(PathBuf::from(p));
    }
    #[cfg(windows)]
    let nomes = ["codex.exe", "codex.cmd"];
    #[cfg(not(windows))]
    let nomes = ["codex"];
    if let Some(p) = no_path(&nomes) {
        return Some(p);
    }
    let h = casa();
    let mut lugares = vec![h.join(".local/bin").join(nomes[0])];
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        lugares.push(PathBuf::from(appdata).join("npm").join("codex.cmd"));
    }
    #[cfg(not(windows))]
    lugares.extend([
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
        h.join(".npm-global/bin/codex"),
    ]);
    lugares.into_iter().find(|p| executavel(p))
}

/// `2.1.289` dentro de um caminho ou de um texto.
fn versao_em(texto: &str) -> Option<String> {
    let bytes = texto.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let inicio = i;
            let mut pontos = 0;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                if bytes[i] == b'.' {
                    pontos += 1;
                }
                i += 1;
            }
            let achado = texto[inicio..i].trim_end_matches('.');
            if pontos >= 2 && achado.split('.').all(|p| !p.is_empty()) {
                return Some(achado.split('.').take(3).collect::<Vec<_>>().join("."));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// A versão de um programa: pelo caminho para onde o lançador aponta (o
/// instalador do Claude liga a `versions/<x.y.z>`), senão pelo `--version`.
fn versao_de(bin: Option<PathBuf>, padrao: &str) -> String {
    let Some(bin) = bin else { return padrao.to_string() };
    let real = std::fs::canonicalize(&bin).unwrap_or(bin.clone());
    if let Some(v) = versao_em(&real.to_string_lossy()) {
        return v;
    }
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("--version").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.output()
        .ok()
        .and_then(|o| versao_em(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_else(|| padrao.to_string())
}

/// A versão do Claude Code, para o `User-Agent` que os endpoints exigem.
pub fn versao_claude() -> String {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| versao_de(claude(), "2.1.289")).clone()
}

pub fn versao_codex() -> String {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| versao_de(codex(), "0.159.2")).clone()
}

/// `claude-cli/<versão> (external, cli)`: sem um agente de CLI, a porta da
/// frente do endpoint responde 403 (e, a Clínica viu, 429 para sempre).
pub fn agente_claude() -> String {
    format!("claude-cli/{} (external, cli)", versao_claude())
}

#[cfg(test)]
mod testes {
    use super::versao_em;

    #[test]
    fn a_versao_sai_do_caminho_ou_do_texto() {
        assert_eq!(versao_em("/Users/a/.local/share/claude/versions/2.1.289").as_deref(), Some("2.1.289"));
        assert_eq!(versao_em("codex-cli 0.159.2").as_deref(), Some("0.159.2"));
        assert_eq!(versao_em("2.1.289 (Claude Code)").as_deref(), Some("2.1.289"));
        assert_eq!(versao_em("releases/0.157.0-aarch64-apple-darwin/bin/codex").as_deref(), Some("0.157.0"));
        assert_eq!(versao_em("sem versão 1.2"), None);
    }
}
