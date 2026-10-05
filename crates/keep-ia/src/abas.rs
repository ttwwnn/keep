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
    let mut lista: Option<Vec<processos::Processo>> = None;
    let mut saida = Vec::new();
    for a in &r.abas {
        if a.info.finished {
            continue;
        }
        let prog = tela::programa(&a.info.command);
        let Some(v) = vinculos.get(&(a.ws.clone(), a.info.id)) else { continue };
        let (agente, pid) = match prog.agente() {
            Some(agente) => (agente, v.frente),
            // O keepd antigo diz "zsh" de uma aba em que a IA roda com um filho
            // dela na frente (um servidor MCP, um php): vale a IA do shell.
            None if !v.exato && matches!(prog, tela::Programa::Shell(_)) => {
                let lista = lista.get_or_insert_with(processos::todos);
                match ia_na_frente(lista, v.shell, v.frente) {
                    Some(achada) => achada,
                    None => continue,
                }
            }
            None => continue,
        };
        let (conta, atual) = conta_do_processo(pid, agente, contas);
        saida.push(IaAba {
            workspace: a.ws.clone(),
            aba: a.info.id,
            agente: agente.into(),
            conta,
            atual,
            vinculo: if v.exato { "exato" } else { "provavel" }.into(),
            pid,
            conversa: conversa_do_processo(pid, agente),
        });
    }
    saida
}

/// O Claude ou o Codex debaixo de `shell` que segura o terminal: ele mesmo
/// na frente, ou um ancestral do processo da frente. Com o shell no prompt,
/// nenhum.
fn ia_na_frente(lista: &[processos::Processo], shell: u32, frente: u32) -> Option<(&'static str, u32)> {
    if frente == shell {
        return None;
    }
    let debaixo = processos::descendentes_em(lista, shell);
    if !debaixo.contains(&frente) {
        return None;
    }
    // Sobe da frente até o shell: o primeiro Claude ou Codex do caminho.
    let mut atual = frente;
    for _ in 0..64 {
        if atual == shell || atual == 0 {
            return None;
        }
        let p = lista.iter().find(|p| p.pid == atual)?;
        let nomes = [Some(p.nome.clone()), processos::programa(atual)];
        for n in nomes.into_iter().flatten() {
            if let Some(agente) = tela::programa(&n).agente() {
                return Some((agente, atual));
            }
        }
        atual = p.ppid;
    }
    None
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
    fn a_ia_do_shell_e_a_que_segura_o_terminal() {
        // Números altos: nenhum processo de verdade.
        let p = |pid: u32, ppid: u32, nome: &str| processos::Processo { pid, ppid, nome: nome.into(), inicio_ms: 0 };
        let lista = vec![p(4_000_010, 1, "zsh"), p(4_000_011, 4_000_010, "claude"), p(4_000_012, 4_000_011, "php")];
        assert_eq!(ia_na_frente(&lista, 4_000_010, 4_000_012), Some(("claude", 4_000_011)));
        assert_eq!(ia_na_frente(&lista, 4_000_010, 4_000_011), Some(("claude", 4_000_011)));
        // O shell no prompt, ou uma frente de fora da árvore dele: nenhuma.
        assert_eq!(ia_na_frente(&lista, 4_000_010, 4_000_010), None);
        assert_eq!(ia_na_frente(&lista, 4_000_010, 4_000_099), None);
        let sem_ia = vec![p(4_000_010, 1, "zsh"), p(4_000_013, 4_000_010, "vim")];
        assert_eq!(ia_na_frente(&sem_ia, 4_000_010, 4_000_013), None);
    }

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
