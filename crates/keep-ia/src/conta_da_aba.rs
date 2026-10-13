//! `keep ia conta-da-aba`: diz ao Claude de uma aba em que conta ele roda.
//!
//! A troca a quente (`quente.rs`) muda só o armazém da aba; o
//! `~/.claude.json` (o `oauthAccount` que o `/status` mostra) é o login
//! global da máquina e não acompanha. Perguntado em que conta está, o Claude
//! lia esse arquivo e dizia a conta global (13/10/2026: a aba na IA·2 disse
//! "assinaturas" e mandou trocar a extensão do Chrome para ela).
//!
//! Com `--gancho` é o `UserPromptSubmit` do Claude Code: lê o JSON do evento
//! na entrada e, quando a conta da aba não é a do login global — ou mudou
//! desde o último aviso nesta conversa —, devolve o aviso como contexto
//! adicional. Sem `--gancho`, imprime a linha (para quem perguntar à mão).

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::contas::{self, Motor};
use crate::{caminhos, quente, trocar};

const VAR: &str = "CLAUDE_SECURESTORAGE_CONFIG_DIR";

/// A conta do Claude desta aba: o dono, o nome na lista ("IA·2") e o e-mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaAba {
    pub dono: String,
    pub nome: String,
    pub email: Option<String>,
}

/// A conta do armazém `pasta`, se ela é o armazém de uma aba.
pub fn da_pasta(pasta: &Path) -> Option<DaAba> {
    let (_, r) = quente::registro_da_pasta(pasta)?;
    if r.dono.is_empty() {
        return None;
    }
    let lista = contas::listar(false);
    let conta = lista.iter().find(|c| c.engine == Motor::Claude && c.key == r.dono);
    let chave = conta.map(|c| c.order_key.clone()).unwrap_or(r.conta.clone());
    Some(DaAba {
        dono: r.dono.clone(),
        nome: trocar::rotulo_em(&lista, &chave),
        email: conta.and_then(|c| c.email.clone()),
    })
}

/// O dono do login global: o `oauthAccount` do `~/.claude.json`.
fn dono_global() -> Option<String> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()).map(PathBuf::from);
    let arquivo = match dir {
        Some(d) => d.join(".claude.json"),
        None => caminhos::casa().join(".claude.json"),
    };
    let v: Value = serde_json::from_slice(&std::fs::read(arquivo).ok()?).ok()?;
    v.get("oauthAccount")?.get("accountUuid")?.as_str().map(String::from)
}

/// A frase que o Claude recebe.
pub fn aviso(c: &DaAba, global: bool) -> String {
    let quem = match &c.email {
        Some(e) => format!("{} ({e})", c.nome),
        None => c.nome.clone(),
    };
    if global {
        format!("Conta do Claude nesta aba do Keep: {quem}, a mesma do login global da máquina.")
    } else {
        format!(
            "Conta do Claude nesta aba do Keep: {quem}. O ~/.claude.json (oauthAccount), o /status e a variável \
             KEEP_IA_ESCOLHA mostram o login global da máquina ou a escolha de quando o Claude abriu, não a conta \
             desta aba: para dizer em que conta esta aba está, vale esta linha (ou `keep ia conta-da-aba`)."
        )
    }
}

/// Se o gancho fala: a conta da aba não é a global, ou mudou desde o último
/// aviso nesta conversa.
pub fn deve_avisar(dono: &str, global: Option<&str>, ultimo: Option<&str>) -> bool {
    global != Some(dono) || ultimo.is_some_and(|u| u != dono)
}

fn arquivo_do_ultimo(sessao: &str) -> Option<PathBuf> {
    let limpo: String = sessao.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    (!limpo.is_empty()).then(|| caminhos::estado().join("avisos-de-conta").join(limpo))
}

pub fn cli(args: &[String]) -> i32 {
    let gancho = args.iter().any(|a| a == "--gancho");
    let mut entrada = Vec::new();
    if gancho {
        let _ = std::io::stdin().read_to_end(&mut entrada);
    }
    let Some(pasta) = std::env::var_os(VAR).filter(|v| !v.is_empty()).map(PathBuf::from) else {
        if !gancho {
            println!("Esta aba usa o login global da máquina (o do ~/.claude.json).");
        }
        return 0;
    };
    let Some(conta) = da_pasta(&pasta) else {
        if !gancho {
            println!("O armazém desta aba ({}) não é um que o Keep conheça.", pasta.display());
        }
        return 0;
    };
    let global = dono_global();
    let eh_global = global.as_deref() == Some(conta.dono.as_str());
    if !gancho {
        println!("{}", aviso(&conta, eh_global));
        return 0;
    }
    let sessao = serde_json::from_slice::<Value>(&entrada)
        .ok()
        .and_then(|v| v.get("session_id").and_then(Value::as_str).map(String::from))
        .unwrap_or_default();
    let guardado = arquivo_do_ultimo(&sessao);
    let ultimo = guardado.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
    if !deve_avisar(&conta.dono, global.as_deref(), ultimo.as_deref().map(str::trim)) {
        return 0;
    }
    if let Some(p) = &guardado {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, &conta.dono);
    }
    println!(
        "{}",
        json!({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": aviso(&conta, eh_global),
            }
        })
    );
    0
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn avisa_quando_a_aba_nao_e_a_global_ou_quando_mudou() {
        // Na IA·2 com a global na IA·1: sempre.
        assert!(deve_avisar("u-2", Some("u-1"), None));
        assert!(deve_avisar("u-2", Some("u-1"), Some("u-2")));
        // Sem saber da global: avisa (melhor dizer a mais).
        assert!(deve_avisar("u-2", None, None));
        // Na global, sem aviso antes: nada a dizer.
        assert!(!deve_avisar("u-1", Some("u-1"), None));
        assert!(!deve_avisar("u-1", Some("u-1"), Some("u-1")));
        // De volta à global depois de um aviso de outra: avisa a volta.
        assert!(deve_avisar("u-1", Some("u-1"), Some("u-2")));
    }

    #[test]
    fn a_frase_diz_o_numero_o_email_e_onde_nao_olhar() {
        let c = DaAba { dono: "u-2".into(), nome: "IA·2".into(), email: Some("b@x".into()) };
        let fora = aviso(&c, false);
        assert!(fora.starts_with("Conta do Claude nesta aba do Keep: IA·2 (b@x)."), "{fora}");
        assert!(fora.contains("~/.claude.json") && fora.contains("/status"), "{fora}");
        let dentro = aviso(&c, true);
        assert!(dentro.contains("mesma do login global") && !dentro.contains("/status"), "{dentro}");
    }

    #[test]
    fn a_conversa_vira_nome_de_arquivo_sem_sair_da_pasta() {
        let p = arquivo_do_ultimo("../../etc/abc-1").unwrap();
        assert_eq!(p.file_name().unwrap(), "etcabc-1");
        assert!(arquivo_do_ultimo("").is_none() && arquivo_do_ultimo("../").is_none());
    }
}
