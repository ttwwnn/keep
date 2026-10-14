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
//!
//! Aqui moram também `keep nova` (uma conversa nova do Claude numa aba nova, o
//! caminho para uma sessão abrir outra e o dos links `claude-cli://`) e `keep
//! continuar` (a conversa que parou numa queda de login, de rede ou de limite
//! recebe "continue" quando a causa passa).

pub mod cli;
pub mod continuar;
pub mod daemon;
pub mod deeplink;
pub mod links;
pub mod membro_json;
pub mod nova;
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

/// Dá nome a abas no app (`names.json` da pasta de estado dele, do keepd cujo socket nasceu em `nasceu`), sob a
/// trava com que o app grava o estado dele. `sobrescrever`: troca o nome que a aba já tinha.
pub fn nomear_no_app(amb: &keep_ia::worktrees::ambiente::Ambiente, nasceu: Option<f64>, nomes: &[(String, u32, String)], sobrescrever: bool) {
    let (Some(app), Some(nasceu)) = (&amb.app, nasceu) else { return };
    if nomes.is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(app);
    let _trava = trava_do_app(app);
    let caminho = app.join("names.json");
    let mut v: serde_json::Value = ler(&caminho).unwrap_or_else(|| serde_json::json!({}));
    let mesmo = v.get("daemonStart").and_then(|d| d.as_f64()).is_some_and(|d| (d - nasceu).abs() < 0.01);
    if !mesmo || !v.is_object() {
        let ws = v.get("workspaces").cloned().unwrap_or_else(|| serde_json::json!({}));
        v = serde_json::json!({ "daemonStart": nasceu, "tabs": {}, "workspaces": ws });
    }
    if !v["tabs"].is_object() {
        v["tabs"] = serde_json::json!({});
    }
    for (ws, aba, nome) in nomes {
        let k = format!("{ws}\u{1f}{aba}");
        if sobrescrever || v["tabs"].get(&k).is_none() {
            v["tabs"][k] = serde_json::Value::String(nome.clone());
        }
    }
    let _ = gravar(&caminho, &v);
}

/// A mesma trava com que o app grava o estado dele (`.state.lock`): esperada até ser dada, e solta ao largar o arquivo.
fn trava_do_app(app: &std::path::Path) -> Option<std::fs::File> {
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(app.join(".state.lock")).ok()?;
    // A trava do sistema de arquivos de cada sistema: `flock` no Unix, `LockFileEx` no Windows.
    f.lock().ok()?;
    Some(f)
}

/// Uma trava entre processos (o app pode chamar duas vezes): `None` se outro a tem. Solta ao largar o arquivo, ou
/// quando o processo acaba.
pub struct Trava {
    _arquivo: std::fs::File,
}

impl Trava {
    pub fn pegar(nome: &str) -> Option<Trava> {
        Trava::pegar_em(&pasta(), nome)
    }

    fn pegar_em(dir: &std::path::Path, nome: &str) -> Option<Trava> {
        std::fs::create_dir_all(dir).ok()?;
        let arquivo = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(nome)).ok()?;
        arquivo.try_lock().ok()?;
        Some(Trava { _arquivo: arquivo })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_trava_e_de_um_so_ate_ser_largada() {
        let dir = tempfile::tempdir().unwrap();
        let primeira = Trava::pegar_em(dir.path(), ".x.lock").expect("a primeira pega");
        assert!(Trava::pegar_em(dir.path(), ".x.lock").is_none(), "outra rodada com a trava na mão");
        assert!(Trava::pegar_em(dir.path(), ".y.lock").is_some(), "cada nome tem a sua");
        drop(primeira);
        assert!(Trava::pegar_em(dir.path(), ".x.lock").is_some(), "largada, a trava volta");
    }

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
