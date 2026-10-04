//! O login do Claude Code em cada armazém, só para ler.
//!
//! O Keep nunca grava aqui e nunca renova: o refresh token gira a cada
//! renovação, e quem renovasse por fora mataria o token que as abas abertas
//! seguram. Quem renova é o próprio Claude Code, a cada uso.
//!
//! Como o Claude Code guarda (binário 2.1.289, funções `Nb()` e `x$()`):
//! - macOS: item genérico do Chaveiro, conta `$USER` (ou `claude-code-user`),
//!   serviço `Claude Code-credentials`, com `-<sha256(pasta)[:8]>` quando a
//!   pasta vem de `CLAUDE_SECURESTORAGE_CONFIG_DIR` (ou de `CLAUDE_CONFIG_DIR`);
//!   lido com `security find-generic-password -w`, como o próprio CLI faz —
//!   e por isso sem pedir senha: o item confia no `security`.
//! - demais sistemas: `<pasta>/.credentials.json`, a pasta sendo a mesma.

use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// O que um armazém tem.
#[derive(Debug, Clone)]
pub enum Lido {
    Ok(Value),
    Ausente,
    /// Chaveiro trancado, pedido de senha na tela, JSON quebrado: nunca é o
    /// mesmo que ausente.
    Ilegivel(String),
}

/// As partes do login que interessam ao Keep.
#[derive(Debug, Clone, Default)]
pub struct Oauth {
    pub acesso: Option<String>,
    pub refresh: Option<String>,
    /// Unix ms.
    pub expira_em: Option<u64>,
    pub nivel: Option<String>,
    pub assinatura: Option<String>,
}

impl Oauth {
    pub fn de(valor: &Value) -> Option<Oauth> {
        let o = valor.get("claudeAiOauth")?.as_object()?;
        let texto = |k: &str| o.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        Some(Oauth {
            acesso: texto("accessToken"),
            refresh: texto("refreshToken"),
            expira_em: o.get("expiresAt").and_then(Value::as_u64),
            nivel: texto("rateLimitTier"),
            assinatura: texto("subscriptionType"),
        })
    }

    /// O CLI zera o login quando o servidor recusa a renovação: sem refresh
    /// não há como voltar sem um login novo.
    pub fn recusado(&self) -> bool {
        self.refresh.is_none()
    }
}

/// O nome do serviço do Chaveiro de uma pasta, exatamente como o CLI calcula.
pub fn servico_de(pasta: &str) -> String {
    let normal: String = pasta.nfc().collect();
    let hash = Sha256::digest(normal.as_bytes());
    let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("Claude Code-credentials-{hex}")
}

pub const SERVICO_GLOBAL: &str = "Claude Code-credentials";

/// A conta do item no Chaveiro: `$USER`, se só tiver caracteres comuns.
pub fn conta_do_item() -> String {
    let usuario = std::env::var("USER").unwrap_or_default();
    if !usuario.is_empty() && usuario.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        usuario
    } else {
        "claude-code-user".into()
    }
}

fn decodifica(texto: &str) -> Lido {
    let s = texto.trim();
    if s.is_empty() {
        return Lido::Ilegivel("vazio".into());
    }
    // `-w` devolve HEX quando o valor tem algo que não imprime.
    let s = if !s.starts_with('{') && s.len() % 2 == 0 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes: Option<Vec<u8>> =
            (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
        match bytes.and_then(|b| String::from_utf8(b).ok()) {
            Some(t) => t,
            None => return Lido::Ilegivel("hex fora do UTF-8".into()),
        }
    } else {
        s.to_string()
    };
    match serde_json::from_str::<Value>(&s) {
        Ok(v) if v.is_object() => Lido::Ok(v),
        _ => Lido::Ilegivel(format!("JSON quebrado ({} bytes)", s.len())),
    }
}

/// Lê um item do Chaveiro com o `security` (ou `KEEP_IA_SECURITY`, nos testes).
#[cfg(target_os = "macos")]
fn chaveiro(servico: &str) -> Lido {
    let security = std::env::var("KEEP_IA_SECURITY").unwrap_or_else(|_| "/usr/bin/security".into());
    let mut filho = match std::process::Command::new(&security)
        .args(["find-generic-password", "-a", &conta_do_item(), "-w", "-s", servico])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(f) => f,
        Err(e) => return Lido::Ilegivel(format!("security: {e}")),
    };
    // O mesmo prazo que o CLI dá ao `security`: um pedido de senha na tela
    // não pode prender quem pergunta.
    let fim = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match filho.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < fim => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            _ => {
                let _ = filho.kill();
                let _ = filho.wait();
                return Lido::Ilegivel("o security não respondeu (pedido de senha na tela?)".into());
            }
        }
    }
    let saida = match filho.wait_with_output() {
        Ok(s) => s,
        Err(e) => return Lido::Ilegivel(format!("security: {e}")),
    };
    // 44 = errSecItemNotFound.
    match saida.status.code() {
        Some(0) => decodifica(&String::from_utf8_lossy(&saida.stdout)),
        Some(44) => Lido::Ausente,
        codigo => Lido::Ilegivel(format!("security saiu {codigo:?}")),
    }
}

fn arquivo(pasta: &Path) -> Lido {
    match std::fs::read_to_string(pasta.join(".credentials.json")) {
        Ok(texto) => decodifica(&texto),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Lido::Ausente,
        Err(e) => Lido::Ilegivel(e.to_string()),
    }
}

/// O login global: o que uma aba usa sem variável nenhuma.
pub fn global() -> Lido {
    #[cfg(target_os = "macos")]
    {
        // Um `.credentials.json` no macOS vira a cópia que o CLI lê quando o
        // Chaveiro está vazio; o Chaveiro vem primeiro.
        match chaveiro(SERVICO_GLOBAL) {
            Lido::Ausente => arquivo(&crate::caminhos::claude()),
            outro => outro,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        arquivo(&crate::caminhos::claude())
    }
}

/// O login de uma pasta fixa: o que uma aba usa com
/// `CLAUDE_SECURESTORAGE_CONFIG_DIR=<pasta>`.
pub fn fixa(pasta: &Path) -> Lido {
    #[cfg(target_os = "macos")]
    {
        chaveiro(&servico_de(&pasta.to_string_lossy()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        arquivo(pasta)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_servico_de_uma_pasta_e_o_do_cli() {
        // sha256("/Users/thiago/.claude/contas/fixas/4f35c383")[:8], o item
        // que o kit e o CLI usam para a aba fixa dessa conta.
        let s = servico_de("/Users/thiago/.claude/contas/fixas/4f35c383");
        assert!(s.starts_with("Claude Code-credentials-"));
        assert_eq!(s.len(), "Claude Code-credentials-".len() + 8);
        // NFC: a mesma pasta escrita decomposta dá o mesmo item.
        assert_eq!(servico_de("/x/João"), servico_de("/x/Joa\u{0303}o"));
    }

    #[test]
    fn json_e_hex_decodificam_e_lixo_nao() {
        assert!(matches!(decodifica(r#"{"claudeAiOauth":{}}"#), Lido::Ok(_)));
        let hex: String = r#"{"a":1}"#.bytes().map(|b| format!("{b:02x}")).collect();
        assert!(matches!(decodifica(&hex), Lido::Ok(_)));
        assert!(matches!(decodifica("{quebrado"), Lido::Ilegivel(_)));
        assert!(matches!(decodifica(""), Lido::Ilegivel(_)));
    }

    #[test]
    fn o_login_recusado_e_o_sem_refresh() {
        let v: Value = serde_json::from_str(
            r#"{"claudeAiOauth":{"accessToken":"a","refreshToken":"","expiresAt":1,"rateLimitTier":"default_claude_max_20x"}}"#,
        )
        .unwrap();
        let o = Oauth::de(&v).unwrap();
        assert!(o.recusado());
        assert_eq!(o.expira_em, Some(1));
        assert_eq!(o.nivel.as_deref(), Some("default_claude_max_20x"));
    }
}
