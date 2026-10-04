//! A IA de cada aba: que programa roda (Claude ou Codex), em que conta, e
//! qual a escolha da aba (seguir a ordem, ou uma conta) — lido do ambiente
//! do processo da frente, nunca adivinhado pelo título.
//!
//! - `KEEP_IA_ESCOLHA` (posta pelo Keep na linha que sobe a IA) ou
//!   `CLAUDE_KIT_CONTA` (a do kit) dizem a escolha;
//! - `CLAUDE_SECURESTORAGE_CONFIG_DIR` diz o login em que o Claude roda (sem
//!   ela, o global); `CODEX_HOME`, o do Codex (sem ela, o `~/.codex`).

use std::path::Path;

use serde::Serialize;

use crate::contas::{self, Armazem, Conta, Motor};
use crate::daemon::{self, Retrato};
use crate::{processos, sessoes, tela, vinculo};

pub const SEGUIR_ORDEM: &str = "claude:ordem";

/// Uma aba com IA, como o app a lê.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct IaAba {
    pub workspace: String,
    pub aba: u32,
    /// `claude` ou `codex`.
    pub agente: String,
    /// A escolha da aba: `claude:ordem`, ou a chave de uma conta.
    pub conta: String,
    /// A conta em que ela roda agora.
    pub atual: Option<String>,
    /// `exato` (o daemon disse o processo) ou `provavel` (deduzido).
    pub vinculo: String,
    pub pid: u32,
    /// A conversa (id), quando se sabe.
    pub conversa: Option<String>,
}

/// Uma chave que o Keep aceita: `claude:<apelido>`, `gpt:<apelido>` ou
/// `claude:ordem`.
pub fn chave_valida(chave: &str) -> bool {
    let Some((motor, ap)) = chave.split_once(':') else { return false };
    matches!(motor, "claude" | "gpt") && !ap.is_empty() && !ap.chars().any(|c| c.is_whitespace() || c == '/' || c == ':')
}

fn mesma_pasta(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// A conta (chave) que usa este armazém, se alguma.
fn dona<'a>(contas: &'a [Conta], achar: impl Fn(&Armazem) -> bool) -> Option<&'a Conta> {
    contas.iter().find(|c| c.armazens.iter().any(&achar))
}

/// A escolha e a conta atual de um processo de IA, pelo ambiente dele.
pub fn conta_do_processo(pid: u32, agente: &str, contas: &[Conta]) -> (String, Option<String>) {
    let ambiente = processos::ambiente(pid).unwrap_or_default();
    let var = |nome: &str| -> Option<String> {
        ambiente
            .iter()
            .find(|(n, _)| if cfg!(windows) { n.eq_ignore_ascii_case(nome) } else { n == nome })
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
    };
    let marcada = var("KEEP_IA_ESCOLHA").or_else(|| var("CLAUDE_KIT_CONTA")).filter(|c| chave_valida(c));
    if agente == "codex" {
        let home = var("CODEX_HOME");
        let atual = match &home {
            Some(h) => dona(contas, |a| matches!(a, Armazem::Codex { home: x, .. } if mesma_pasta(x, Path::new(h)))),
            None => dona(contas, |a| matches!(a, Armazem::Codex { principal: true, .. })),
        }
        .map(|c| c.order_key.clone());
        let escolha = marcada.or_else(|| atual.clone()).unwrap_or_else(|| "gpt:principal".into());
        return (escolha, atual);
    }
    let fixa = var("CLAUDE_SECURESTORAGE_CONFIG_DIR");
    let atual = match &fixa {
        Some(p) => dona(contas, |a| matches!(a, Armazem::Fixa { pasta } if mesma_pasta(pasta, Path::new(p)))),
        None => dona(contas, |a| matches!(a, Armazem::Global)),
    }
    .filter(|c| c.engine == Motor::Claude)
    .map(|c| c.order_key.clone());
    let escolha = match (marcada, &fixa) {
        (Some(m), _) => m,
        // Uma pasta própria sem marca: a aba foi presa nessa conta.
        (None, Some(_)) => atual.clone().unwrap_or_else(|| SEGUIR_ORDEM.into()),
        (None, None) => SEGUIR_ORDEM.into(),
    };
    (escolha, atual)
}

/// A conversa de um processo de IA.
pub fn conversa_do_processo(pid: u32, agente: &str) -> Option<String> {
    if agente == "claude" {
        if let Some(s) = sessoes::sessao_claude(pid) {
            return Some(s.id);
        }
        return processos::argv(pid).and_then(|a| sessoes::id_claude_do_argv(&a));
    }
    processos::argv(pid).and_then(|a| sessoes::id_codex_do_argv(&a))
}

/// Toda aba com Claude ou Codex vivo, a partir de uma lista do daemon.
pub fn das_abas(r: &Retrato, contas: &[Conta]) -> Vec<IaAba> {
    let vinculos = vinculo::vincular(r);
    let mut saida = Vec::new();
    for a in &r.abas {
        if a.info.finished {
            continue;
        }
        let prog = tela::programa(&a.info.command);
        let Some(agente) = prog.agente() else { continue };
        let Some(v) = vinculos.get(&(a.ws.clone(), a.info.id)) else { continue };
        let (conta, atual) = conta_do_processo(v.frente, agente, contas);
        saida.push(IaAba {
            workspace: a.ws.clone(),
            aba: a.info.id,
            agente: agente.into(),
            conta,
            atual,
            vinculo: if v.exato { "exato" } else { "provavel" }.into(),
            pid: v.frente,
            conversa: conversa_do_processo(v.frente, agente),
        });
    }
    saida
}

/// `keep ia abas`: a lista do daemon e a IA de cada aba.
pub fn ler() -> Result<(Retrato, Vec<IaAba>), String> {
    let r = daemon::listar()?;
    let contas = contas::listar(false);
    let ia = das_abas(&r, &contas);
    Ok((r, ia))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn chaves() {
        assert!(chave_valida("claude:ordem"));
        assert!(chave_valida("gpt:principal"));
        assert!(!chave_valida("claude:"));
        assert!(!chave_valida("claude:a/b"));
        assert!(!chave_valida("outro:x"));
    }

    #[test]
    fn a_conta_sai_do_ambiente_do_processo() {
        let c = crate::contas::testes::casa("abas");
        crate::contas::testes::perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
        c.global("G", "r");
        let fixa = c.fixa("k-2", Some(("F", "r")), Some(("bia", "bia@y.com", "u-2")));
        c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
        let contas = crate::contas::listar(true);
        let filho = |vars: &[(&str, &str)]| {
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.args(["--exact", "processos::testes::dorminhoco", "--ignored", "-q"])
                .env("KEEP_IA_TESTE_PROCESSO", "1")
                .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR")
                .env_remove("CODEX_HOME")
                .env_remove("KEEP_IA_ESCOLHA")
                .env_remove("CLAUDE_KIT_CONTA")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            for (n, v) in vars {
                cmd.env(n, v);
            }
            let f = cmd.spawn().unwrap();
            for _ in 0..100 {
                if processos::variavel(f.id(), "KEEP_IA_TESTE_PROCESSO").is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            f
        };
        let mut a = filho(&[]);
        assert_eq!(conta_do_processo(a.id(), "claude", &contas), ("claude:ordem".into(), Some("claude:ana".into())));
        let pasta = fixa.to_string_lossy().into_owned();
        let mut b = filho(&[("CLAUDE_SECURESTORAGE_CONFIG_DIR", &pasta)]);
        assert_eq!(conta_do_processo(b.id(), "claude", &contas), ("claude:bia".into(), Some("claude:bia".into())));
        let mut d = filho(&[("CLAUDE_SECURESTORAGE_CONFIG_DIR", &pasta), ("KEEP_IA_ESCOLHA", "claude:ordem")]);
        assert_eq!(conta_do_processo(d.id(), "claude", &contas), ("claude:ordem".into(), Some("claude:bia".into())));
        let mut e = filho(&[("KEEP_IA_ESCOLHA", "claude:ordem")]);
        assert_eq!(conta_do_processo(e.id(), "codex", &contas), ("claude:ordem".into(), Some("gpt:principal".into())));
        for p in [&mut a, &mut b, &mut d, &mut e] {
            p.kill().ok();
            p.wait().ok();
        }
    }
}
