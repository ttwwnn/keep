//! De quem é um token do Claude: perguntado ao servidor, nunca ao
//! `~/.claude.json`, que o CLI regrava com a conta do próprio token de
//! qualquer aba (uma aba presa noutra conta faria o arquivo mentir para todas).
//!
//! `GET /api/oauth/profile` com o token: um token inválido dá 401, então a
//! resposta prova de quem ele é. Guardado por 12 h pela impressão do token
//! (nunca o token), no estado do Keep; o cache do kit, com a mesma chave, é
//! lido também, para quem já o tinha.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{caminhos, programas, rede};

const VALIDADE_MS: u64 = 12 * 3600 * 1000;

/// O dono de um token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dono {
    pub uuid: String,
    #[serde(default)]
    pub email: String,
}

#[derive(Serialize, Deserialize)]
struct Entrada {
    uuid: String,
    #[serde(default)]
    email: String,
    em: u64,
}

pub fn agora_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A chave do cache: sha256 do token, 16 hex — a mesma do kit.
pub fn impressao(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("donos.json")
}

fn le(caminho: &PathBuf) -> BTreeMap<String, Entrada> {
    std::fs::read_to_string(caminho)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn em_cache(chave: &str) -> Option<Dono> {
    let agora = agora_ms();
    for caminho in [arquivo(), caminhos::claude().join(".donos-dos-tokens.json")] {
        if let Some(e) = le(&caminho).get(chave) {
            if !e.uuid.is_empty() && agora.saturating_sub(e.em) < VALIDADE_MS {
                return Some(Dono { uuid: e.uuid.clone(), email: e.email.clone() });
            }
        }
    }
    None
}

fn guarda(chave: &str, dono: &Dono) {
    let caminho = arquivo();
    let agora = agora_ms();
    let mut cache = le(&caminho);
    cache.retain(|_, e| agora.saturating_sub(e.em) < 2 * VALIDADE_MS);
    cache.insert(chave.to_string(), Entrada { uuid: dono.uuid.clone(), email: dono.email.clone(), em: agora });
    if let Some(dir) = caminho.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(texto) = serde_json::to_vec_pretty(&cache) {
        let tmp = caminho.with_extension("json.tmp");
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, &caminho);
        }
    }
}

/// O perfil do servidor para este token, ou `None` (sem rede, recusado).
pub fn perfil(acesso: &str) -> Option<Value> {
    let url = rede::url_de_teste(
        &["KEEP_IA_PERFIL_URL", "CLAUDE_KIT_PERFIL_URL"],
        "https://api.anthropic.com/api/oauth/profile",
    );
    let resposta = rede::get(
        &url,
        &[
            ("Authorization", format!("Bearer {acesso}")),
            ("Accept", "application/json".into()),
            ("anthropic-beta", "oauth-2025-04-20".into()),
            ("User-Agent", programas::agente_claude()),
        ],
        Duration::from_secs(15),
    )
    .ok()?;
    if resposta.status != 200 {
        return None;
    }
    serde_json::from_slice(&resposta.corpo).ok()
}

/// O dono deste token: do cache, ou perguntado. `None` = não sei (sem rede,
/// token vencido) — e "não sei" nunca vira a conta de um arquivo.
pub fn dono(acesso: &str, perguntar: bool) -> Option<Dono> {
    if acesso.is_empty() {
        return None;
    }
    let chave = impressao(acesso);
    if let Some(d) = em_cache(&chave) {
        return Some(d);
    }
    if !perguntar {
        return None;
    }
    let p = perfil(acesso)?;
    let conta = p.get("account")?;
    let uuid = conta.get("uuid")?.as_str()?.trim().to_string();
    if uuid.is_empty() {
        return None;
    }
    let email = conta
        .get("email_address")
        .or_else(|| conta.get("email"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let d = Dono { uuid, email };
    guarda(&chave, &d);
    Some(d)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_impressao_e_a_do_kit() {
        // hashlib.sha256(b"abc").hexdigest()[:16]
        assert_eq!(impressao("abc"), "ba7816bf8f01cfea");
    }
}
