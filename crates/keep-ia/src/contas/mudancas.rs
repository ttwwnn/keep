//! Cada mudança da fila de prioridade (`~/.claude/contas/.ordem`) fica registrada: quando, o antes e o depois, e por
//! onde veio (pedido de 25/11/2026: uma regravação da fila pode subir uma conta, ou dar a ela um teto, e levar as
//! abas para outra conta sem que se saiba quem mexeu).
//!
//! - O que o Keep grava (`gravar_ordem`) chega aqui com o caminho: as setas do rodapé, `keep ia ordem`, o login de
//!   uma conta nova.
//! - O que muda por fora (um editor, um script, outra sessão) é visto na leitura seguinte da fila, contra a cópia da
//!   última versão conhecida (`ordem-vista.json`), e registrado como "arquivo editado por fora".
//!
//! O registro: `<estado>/ordem.log`, uma linha JSON por mudança (`quando`, `por`, `antes`, `depois`, e `pai`, o
//! programa que chamou o `keep`, quando foi um comando).

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{caminhos, processos};

pub const POR_FORA: &str = "arquivo editado por fora";

/// A última versão da fila que o Keep conhece, e de quando é.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vista {
    pub texto: String,
    /// Quando ela mudou por último (ms unix; 0 = desde a primeira leitura, sem mudança vista).
    #[serde(default)]
    pub mudou_ms: u64,
    /// Por onde veio essa mudança.
    #[serde(default)]
    pub por: String,
}

fn arquivo_vista() -> PathBuf {
    caminhos::estado().join("ordem-vista.json")
}

fn registro() -> PathBuf {
    caminhos::estado().join("ordem.log")
}

fn agora_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A última versão conhecida da fila.
pub fn vista() -> Option<Vista> {
    serde_json::from_slice(&std::fs::read(arquivo_vista()).ok()?).ok()
}

/// As linhas que valem (sem comentários nem linhas vazias).
fn linhas(texto: &str) -> Vec<String> {
    texto.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect()
}

/// Uma mudança, com a trava do registro na mão.
fn anotar(antes: &str, depois: &str, por: &str, pai: Option<String>) {
    let quando = agora_ms();
    let mut linha = json!({
        "quando": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "por": por,
        "antes": linhas(antes),
        "depois": linhas(depois),
    });
    if let Some(p) = pai {
        linha["pai"] = json!(p);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(registro()) {
        let _ = writeln!(f, "{linha}");
    }
    let v = Vista { texto: depois.to_string(), mudou_ms: quando, por: por.to_string() };
    let _ = caminhos::grava_privado(&arquivo_vista(), &serde_json::to_vec(&v).unwrap_or_default());
}

/// A trava do registro: duas leituras ao mesmo tempo não anotam a mesma mudança duas vezes.
fn trava() -> Option<std::fs::File> {
    let _ = std::fs::create_dir_all(caminhos::estado());
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(caminhos::estado().join("ordem.trava")).ok()?;
    f.lock().ok()?;
    Some(f)
}

/// Quem chamou o `keep` (o programa pai), para saber de onde veio um `keep ia ordem`.
fn pai() -> Option<String> {
    let eu = processos::um(std::process::id())?;
    let pai = processos::um(eu.ppid)?;
    Some(format!("{} (pid {})", pai.nome, pai.pid))
}

/// A cópia conhecida passa a ter este texto, sem mudança a registrar (só os comentários mudaram).
fn guardar_texto(texto: &str) {
    let v = Vista { texto: texto.to_string(), ..vista().unwrap_or_default() };
    let _ = caminhos::grava_privado(&arquivo_vista(), &serde_json::to_vec(&v).unwrap_or_default());
}

/// A fila como ela está no disco (`texto`), lida agora: diferente da última versão conhecida, alguém a mudou por fora.
/// Sem versão conhecida, esta passa a ser.
pub(crate) fn vigiar(texto: &str) {
    if vista().is_some_and(|v| v.texto == texto) {
        return;
    }
    let Some(_t) = trava() else { return };
    // Relida com a trava na mão: outra leitura pode ter anotado esta mesma mudança agora.
    match vista() {
        Some(v) if v.texto == texto => {}
        Some(v) if linhas(&v.texto) == linhas(texto) => guardar_texto(texto),
        Some(v) => anotar(&v.texto, texto, POR_FORA, None),
        None => guardar_texto(texto),
    }
}

/// O Keep vai regravar a fila (`por`: de onde veio o pedido). `grava` grava; o que estava antes, o que ficou e por onde
/// veio vão ao registro, e a cópia conhecida passa a ser a nova. Uma mudança por fora ainda não vista é anotada antes,
/// como dela.
pub(crate) fn gravar(por: &str, depois: &str, grava: impl FnOnce() -> std::io::Result<()>) -> std::io::Result<()> {
    let _t = trava();
    let antes = caminhos::ler_texto_ate(&caminhos::ordem(), caminhos::LEITURA_MAXIMA).unwrap_or_default();
    if let Some(v) = vista() {
        if linhas(&v.texto) != linhas(&antes) {
            anotar(&v.texto, &antes, POR_FORA, None);
        }
    }
    grava()?;
    if linhas(&antes) != linhas(depois) {
        anotar(&antes, depois, por, (por == "keep ia ordem").then(pai).flatten());
    } else {
        guardar_texto(depois);
    }
    Ok(())
}
