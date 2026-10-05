//! O login do Claude Code em cada armazém.
//!
//! O Keep lê daqui, e grava só o que a renovação (`crate::renovar`) traz de
//! volta para o MESMO armazém: nunca copia um login de um armazém para
//! outro. O refresh token gira a cada renovação; duas cópias da mesma família
//! terminam com uma delas morta.
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
    arquivo_em(&pasta.join(".credentials.json"))
}

fn arquivo_em(arq: &Path) -> Lido {
    match std::fs::read_to_string(arq) {
        Ok(texto) => decodifica(&texto),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Lido::Ausente,
        Err(e) => Lido::Ilegivel(e.to_string()),
    }
}

/// Onde um login está guardado, para ler e regravar no mesmo lugar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Local {
    /// Um item do Chaveiro (macOS), pelo serviço.
    Chaveiro(String),
    /// Um `.credentials.json`.
    Arquivo(std::path::PathBuf),
}

/// Onde está o login global agora: no macOS, o Chaveiro, ou o arquivo quando
/// o Chaveiro não tem o item e o arquivo existe (é a cópia que o CLI lê).
pub fn local_global() -> Local {
    #[cfg(target_os = "macos")]
    {
        let arq = crate::caminhos::claude().join(".credentials.json");
        if matches!(chaveiro(SERVICO_GLOBAL), Lido::Ausente) && arq.is_file() {
            return Local::Arquivo(arq);
        }
        Local::Chaveiro(SERVICO_GLOBAL.into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Local::Arquivo(crate::caminhos::claude().join(".credentials.json"))
    }
}

/// Onde está o login de uma pasta fixa.
pub fn local_fixa(pasta: &Path) -> Local {
    #[cfg(target_os = "macos")]
    {
        Local::Chaveiro(servico_de(&pasta.to_string_lossy()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Local::Arquivo(pasta.join(".credentials.json"))
    }
}

/// Lê o login de um local.
pub fn ler(local: &Local) -> Lido {
    match local {
        #[cfg(target_os = "macos")]
        Local::Chaveiro(servico) => chaveiro(servico),
        #[cfg(not(target_os = "macos"))]
        Local::Chaveiro(_) => Lido::Ausente,
        Local::Arquivo(arq) => arquivo_em(arq),
    }
}

/// Grava o JSON inteiro de um login no local, compacto e sem quebra de linha
/// (com "\n" no valor, o `find-generic-password -w` devolve hex e o CLI diz
/// "Not logged in"), e confere lendo de volta.
pub fn gravar(local: &Local, valor: &Value) -> Result<(), String> {
    let texto = serde_json::to_string(valor).map_err(|e| e.to_string())?;
    match local {
        Local::Chaveiro(servico) => grava_chaveiro(servico, &texto)?,
        Local::Arquivo(arq) => grava_arquivo(arq, &texto)?,
    }
    match ler(local) {
        Lido::Ok(v) if &v == valor => Ok(()),
        Lido::Ok(_) => Err("o login lido de volta não é o gravado".into()),
        Lido::Ausente => Err("o login sumiu depois de gravado".into()),
        Lido::Ilegivel(por) => Err(format!("o login gravado não se lê ({por})")),
    }
}

/// Grava um item do Chaveiro como o CLI grava: a linha
/// `add-generic-password -U … -X <hex>` pela entrada padrão do `security -i`
/// (o token não aparece no `ps`); linha longa demais para o modo interativo
/// vai por argv, como no CLI.
pub(crate) fn grava_chaveiro(servico: &str, texto: &str) -> Result<(), String> {
    use std::io::Write as _;
    const LIMITE_LINHA_INTERATIVA: usize = 4032;
    let security = std::env::var("KEEP_IA_SECURITY").unwrap_or_else(|_| "/usr/bin/security".into());
    let hex: String = texto.bytes().map(|b| format!("{b:02x}")).collect();
    let conta = conta_do_item();
    let linha = format!("add-generic-password -U -a \"{conta}\" -s \"{servico}\" -X \"{hex}\"\n");
    let mut cmd = std::process::Command::new(&security);
    let interativo = linha.len() <= LIMITE_LINHA_INTERATIVA;
    if interativo {
        cmd.arg("-i");
    } else {
        cmd.args(["add-generic-password", "-U", "-a", &conta, "-s", servico, "-X", &hex]);
    }
    let mut filho = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("security: {e}"))?;
    if let Some(mut entrada) = filho.stdin.take() {
        if interativo {
            let _ = entrada.write_all(linha.as_bytes());
        }
    }
    let fim = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match filho.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => return Err(format!("security saiu {:?}", st.code())),
            Ok(None) if std::time::Instant::now() < fim => std::thread::sleep(std::time::Duration::from_millis(20)),
            _ => {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err("o security não respondeu (pedido de senha na tela?)".into());
            }
        }
    }
}

/// Grava um `.credentials.json` de uma vez (arquivo ao lado e troca), só
/// para o dono ler.
fn grava_arquivo(arq: &Path, texto: &str) -> Result<(), String> {
    let pasta = arq.parent().ok_or("arquivo sem pasta")?;
    let tmp = pasta.join(format!(".credentials.json.keep-{}", std::process::id()));
    std::fs::write(&tmp, texto).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, arq).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
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
