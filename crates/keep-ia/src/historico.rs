//! O que o Keep lembra de cada aba entre uma troca e outra: a última
//! conversa do Claude, a última do Codex em cada conta, a conversa que
//! saiu na última troca (para levar o contexto adiante) e os parâmetros com
//! que cada IA rodava nela.
//!
//! Por aba de um daemon: os números das abas recomeçam a cada daemon, e o
//! que é de outro é jogado fora.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::caminhos;
use crate::daemon::Retrato;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Origem {
    pub motor: String,
    pub id: Option<String>,
    pub conta: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entrada {
    pub claude: Option<String>,
    pub codex_por_conta: BTreeMap<String, String>,
    pub ultima_conversa: Option<Origem>,
    pub parametros: BTreeMap<String, Vec<String>>,
    pub conta: Option<String>,
    pub contexto: Option<String>,
    pub em: u64,
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("ia-abas.json")
}

/// Quando o daemon nasceu: o que ele diz, senão o nascimento do socket
/// (daemon antigo).
pub fn nascimento(r: &Retrato) -> u64 {
    if r.daemon.started_ms != 0 {
        return r.daemon.started_ms;
    }
    std::fs::metadata(crate::daemon::socket())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn chave(r: &Retrato, ws: &str, aba: u32) -> String {
    format!("{}|{ws}|{aba}", nascimento(r))
}

fn tudo() -> BTreeMap<String, Entrada> {
    std::fs::read_to_string(arquivo()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn le(chave: &str) -> Entrada {
    tudo().remove(chave).unwrap_or_default()
}

/// Muda a entrada da aba e grava; o que é de outro daemon sai.
pub fn anota(chave: &str, muda: impl FnOnce(&mut Entrada)) {
    let prefixo = chave.split('|').next().unwrap_or("").to_string();
    let mut t = tudo();
    t.retain(|k, _| k.split('|').next() == Some(prefixo.as_str()));
    let e = t.entry(chave.to_string()).or_default();
    muda(e);
    e.em = crate::contas::donos::agora_ms();
    let caminho = arquivo();
    if let Some(dir) = caminho.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(texto) = serde_json::to_vec_pretty(&t) {
        let tmp = caminho.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, &caminho);
        }
    }
}
