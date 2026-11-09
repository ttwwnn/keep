//! `keep ia confiar <pid do shell>`: responde "sim" à pergunta do Claude Code
//! sobre confiar na pasta ("Quick safety check: Is this a project you created
//! or one you trust?"), na aba cujo shell é o pid dado — quem digitou `claude`
//! ali já disse sim. Pelo keepd: vale com a aba à vista ou não.
//!
//! O ponteiro começa em "No, exit": desce até "Yes, I trust this folder" e só
//! então Enter, e só depois de ver a pergunta (tecla cedo vira Esc, que é
//! "No, exit"). Sai ao responder, quando o Claude já está no prompt, ou em
//! 40 s. Enquanto roda, `~/.local/state/keep/trust/<pid>` existe e o app não
//! responde na mesma aba (duas setas passariam do ponto).
//!
//! Só onde a pasta não traz nada que o Claude Code execute ao abrir
//! (`por_que_nao_confiar`): a pergunta é a barreira dele contra a pasta de
//! terceiros (um repositório clonado, um zip, o ramo de um PR) cujo
//! `.claude/settings.json` roda um gancho assim que o `claude` abre. Ali a
//! pergunta fica para a pessoa. O app do Mac e o do Windows, que também
//! respondem, perguntam o mesmo ao núcleo (`keep ia pode-confiar`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::cli::{Args, recusa, responde, uso_errado};
use crate::{caminhos, daemon, trocar};

/// As chaves de `.claude/settings.json` e `settings.local.json` que o Claude Code não executa ao abrir. Qualquer outra
/// (`hooks`, `statusLine`, `env`, `apiKeyHelper`, `enabledPlugins`, `enableAllProjectMcpServers` e as que ainda vierem)
/// deixa a pergunta para a pessoa.
const CHAVES_INERTES: [&str; 12] = [
    "$schema",
    "permissions",
    "model",
    "outputStyle",
    "language",
    "includeCoAuthoredBy",
    "cleanupPeriodDays",
    "alwaysThinkingEnabled",
    "effortLevel",
    "spinnerTipsEnabled",
    "respectGitignore",
    "disableAllHooks",
];

/// Por que o Keep não responde sozinho "sim" à pergunta de confiança numa pasta: ela, ou uma acima dela, traz o que o
/// Claude Code executa ao abrir, antes de qualquer conversa — servidores MCP (`.mcp.json`; os que a configuração da
/// pessoa já aprova pelo nome sobem sozinhos) ou, em `.claude/settings.json` e `settings.local.json`, uma chave fora
/// das inertes, ou um arquivo que não dá para ler como objeto JSON. As pastas de cima vão até a casa, que fica de
/// fora (a `.claude` dela é a configuração da própria pessoa); fora da casa, até a raiz. `None`: pode responder.
pub fn por_que_nao_confiar(pasta: &Path) -> Option<String> {
    por_que_nao_confiar_em(pasta, &caminhos::casa())
}

fn por_que_nao_confiar_em(pasta: &Path, casa: &Path) -> Option<String> {
    if !pasta.is_absolute() {
        return Some(format!("a pasta não é um caminho absoluto ({})", pasta.display()));
    }
    // Existe (como arquivo, pasta ou link, até quebrado); `Err` quando não dá para saber.
    let existe = |p: &Path| match std::fs::symlink_metadata(p) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", p.display())),
    };
    // A casa como o sistema a compara: no Windows, sem caixa (`C:\Users\Ana` é `c:\users\ana`).
    let e_a_casa = |d: &Path| {
        if cfg!(windows) {
            let (d, c) = (d.to_string_lossy(), casa.to_string_lossy());
            crate::confinar_win::dentro_de(&c, &d) && crate::confinar_win::dentro_de(&d, &c)
        } else {
            d == casa
        }
    };
    for dir in pasta.ancestors().take_while(|d| !e_a_casa(d)) {
        let mcp = dir.join(".mcp.json");
        match existe(&mcp) {
            Ok(false) => {}
            Ok(true) => return Some(format!("{} traz servidores MCP", mcp.display())),
            Err(e) => return Some(e),
        }
        for nome in ["settings.json", "settings.local.json"] {
            let arquivo = dir.join(".claude").join(nome);
            match existe(&arquivo) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(e) => return Some(e),
            }
            let objeto = caminhos::ler_texto_ate(&arquivo, caminhos::LEITURA_MAXIMA)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v.as_object().cloned());
            let Some(objeto) = objeto else {
                return Some(format!("{} não dá para ler como configuração", arquivo.display()));
            };
            if let Some(chave) = objeto.keys().find(|k| !CHAVES_INERTES.contains(&k.as_str())) {
                return Some(format!("{} tem `{chave}`", arquivo.display()));
            }
        }
    }
    None
}

/// `keep ia pode-confiar --pasta=<pasta>`: `ok` quando o Keep pode responder sozinho "sim" à pergunta de confiança
/// nessa pasta; recusa `config-executavel`, com o porquê, quando não.
pub fn cli_pode(a: &Args) -> i32 {
    let Some(pasta) = a.opcao("pasta").filter(|p| Path::new(p).is_absolute()) else {
        return uso_errado("keep ia pode-confiar --pasta=<pasta absoluta>");
    };
    match por_que_nao_confiar(Path::new(pasta)) {
        None => responde(json!({ "ok": true })),
        Some(porque) => responde(recusa("config-executavel", &porque)),
    }
}

/// A tela é a pergunta e mais nada: tem o cabeçalho e o rodapé dela, poucas linhas e nenhuma caixa de entrada (duas
/// réguas em volta do `❯`), que toda conversa traz. Essas linhas impressas dentro de uma conversa (um arquivo que o
/// modelo leu, um `cat`) não são ela: responder apertaria Enter no que a conversa tivesse à vista.
pub fn e_a_pergunta(tela: &str) -> bool {
    let linhas: Vec<&str> = tela.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    linhas.len() <= 16
        && (tela.contains("Quick safety check") || tela.contains("Do you trust the files in this folder"))
        && tela.contains("Enter to confirm")
        && linhas.iter().filter(|l| l.starts_with(&"─".repeat(12))).count() <= 1
}

/// Onde o ponteiro da pergunta está: `Some(true)` no sim, `Some(false)` no
/// não, `None` quando a pergunta não está na tela.
pub fn ponteiro(tela: &str) -> Option<bool> {
    if !e_a_pergunta(tela) || !tela.contains("Yes, I trust this folder") || !tela.contains("No, exit") {
        return None;
    }
    for linha in tela.lines() {
        let t = linha.trim();
        if !t.starts_with('❯') {
            continue;
        }
        if t.contains("Yes, I trust this folder") {
            return Some(true);
        }
        if t.contains("No, exit") {
            return Some(false);
        }
    }
    None
}

pub fn marca(shell: u32) -> PathBuf {
    caminhos::casa().join(".local/state/keep/trust").join(shell.to_string())
}

fn aba_do_shell(shell: u32) -> Option<(String, u32)> {
    let r = daemon::listar().ok()?;
    r.abas
        .iter()
        .find(|a| a.info.shell_pid == shell && !a.info.finished)
        .map(|a| (a.ws.clone(), a.info.id))
}

/// A pasta do programa da frente da aba (o Claude que pergunta), como o keepd a vê agora.
fn pasta_da_aba(ws: &str, tab: u32) -> Option<PathBuf> {
    let r = daemon::listar().ok()?;
    let aba = r.abas.iter().find(|a| a.ws == ws && a.info.id == tab)?;
    Some(PathBuf::from(&aba.info.cwd)).filter(|p| p.is_absolute())
}

pub fn cli(args: &[String]) -> i32 {
    let Some(shell) = args.first().and_then(|s| s.parse::<u32>().ok()) else { return 2 };
    let m = marca(shell);
    if let Some(dir) = m.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&m, b"");
    let resultado = responder(shell);
    let _ = std::fs::remove_file(&m);
    resultado
}

fn responder(shell: u32) -> i32 {
    let Some((ws, tab)) = aba_do_shell(shell) else { return 0 };
    let fim = Instant::now() + Duration::from_secs(40);
    let mut ultimo: Option<Instant> = None;
    let mut pasta_conferida = false;
    while Instant::now() < fim {
        std::thread::sleep(Duration::from_millis(400));
        let Ok(tela) = daemon::tela(&ws, tab) else { return 0 };
        match ponteiro(&tela) {
            None => {
                if tela.contains("bypass permissions") || tela.contains("? for shortcuts") {
                    return 0;
                }
            }
            Some(sim) => {
                // Com a pergunta na tela, a pasta é a do Claude que pergunta (a de quem digitou `claude` pode ser
                // outra: o `confinar` sobe na pasta presa). Sem pasta vista, ou com o que ele executa ao abrir, a
                // pergunta fica para a pessoa.
                if !pasta_conferida {
                    match pasta_da_aba(&ws, tab) {
                        Some(pasta) if por_que_nao_confiar(&pasta).is_none() => pasta_conferida = true,
                        _ => return 0,
                    }
                }
                if ultimo.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                    continue;
                }
                let tecla: &[u8] = if sim { b"\r" } else { b"\x1b[B" };
                if trocar::tecla(&ws, tab, tecla).is_err() {
                    return 0;
                }
                ultimo = Some(Instant::now());
                if sim {
                    return 0;
                }
            }
        }
    }
    0
}

#[cfg(test)]
mod testes {
    use super::*;

    const PERGUNTA: &str = " Quick safety check: Is this a project you created or one you trust?\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";

    #[test]
    fn le_onde_o_ponteiro_esta() {
        assert_eq!(ponteiro(PERGUNTA), Some(false));
        let no_sim = PERGUNTA.replace(" ❯ No, exit", "   No, exit").replace("   Yes, I trust", " ❯ Yes, I trust");
        assert_eq!(ponteiro(&no_sim), Some(true));
        assert_eq!(ponteiro("ele disse: Yes, I trust this folder e No, exit\n❯ "), None);
        assert_eq!(ponteiro("❯ "), None);
    }

    #[test]
    fn a_pergunta_impressa_dentro_de_uma_conversa_nao_e_a_pergunta() {
        let regua = "─".repeat(40);
        // As mesmas linhas, com a caixa de entrada que toda conversa traz embaixo.
        let na_conversa = format!("● Li o arquivo:\n{PERGUNTA}\n{regua}\n❯ \n{regua}\n  ⏵⏵ bypass permissions on\n");
        assert_eq!(ponteiro(&na_conversa), None);
        // Sem o cabeçalho e o rodapé dela, nem a tela inteira conta.
        assert_eq!(ponteiro(" ❯ No, exit\n   Yes, I trust this folder\n"), None);
        // Uma tela cheia de outra coisa, com as linhas no meio.
        let cheia: String = (0..30).map(|i| format!("linha {i}\n")).collect::<String>() + PERGUNTA;
        assert_eq!(ponteiro(&cheia), None);
        // O diálogo antigo, numerado, com o cabeçalho antigo.
        let antigo = "Do you trust the files in this folder?\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n\n Enter to confirm · Esc to cancel\n";
        assert_eq!(ponteiro(antigo), Some(true));
    }

    #[test]
    fn so_confia_sozinho_onde_a_pasta_nao_traz_o_que_o_claude_executa_ao_abrir() {
        let d = tempfile::tempdir().unwrap();
        let casa = d.path().join("casa");
        let repo = casa.join("Projetos").join("repo");
        let sub = repo.join("sub");
        let config = |dir: &Path, nome: &str, texto: &str| {
            std::fs::create_dir_all(dir.join(".claude")).unwrap();
            std::fs::write(dir.join(".claude").join(nome), texto).unwrap();
        };
        std::fs::create_dir_all(&sub).unwrap();
        // A pasta sem nada, e a que só traz o inerte (as permissões, o modelo).
        assert_eq!(por_que_nao_confiar_em(&sub, &casa), None);
        config(&sub, "settings.json", r#"{"$schema":"x","permissions":{"allow":["Bash(ls:*)"]},"model":"opus"}"#);
        assert_eq!(por_que_nao_confiar_em(&sub, &casa), None);
        // O que o Claude Code executa ao abrir, e a chave que ainda não existe hoje.
        for chave in ["hooks", "statusLine", "env", "apiKeyHelper", "enabledPlugins", "enableAllProjectMcpServers", "nova"] {
            config(&sub, "settings.local.json", &format!(r#"{{"model":"opus","{chave}":{{}}}}"#));
            let porque = por_que_nao_confiar_em(&sub, &casa).unwrap_or_else(|| panic!("{chave}"));
            assert!(porque.contains(&format!("`{chave}`")) && porque.contains("settings.local.json"), "{porque}");
        }
        std::fs::remove_file(sub.join(".claude").join("settings.local.json")).unwrap();
        // O que não dá para ler como configuração: a pergunta fica.
        for texto in ["[1]", "{", ""] {
            config(&sub, "settings.json", texto);
            assert!(por_que_nao_confiar_em(&sub, &casa).is_some(), "{texto:?}");
        }
        config(&sub, "settings.json", "{}");
        // Servidores MCP do projeto, na pasta ou numa de cima.
        std::fs::write(repo.join(".mcp.json"), "{}").unwrap();
        assert!(por_que_nao_confiar_em(&sub, &casa).unwrap().contains(".mcp.json"));
        std::fs::remove_file(repo.join(".mcp.json")).unwrap();
        // Um gancho no repositório de cima (uma worktree ou uma subpasta dele).
        config(&repo, "settings.json", r#"{"hooks":{"SessionStart":[]}}"#);
        assert!(por_que_nao_confiar_em(&sub, &casa).is_some());
        std::fs::remove_file(repo.join(".claude").join("settings.json")).unwrap();
        // A casa é da pessoa: os ganchos e o `.mcp.json` dela não contam, nem o que fica acima dela.
        config(&casa, "settings.json", r#"{"hooks":{}}"#);
        std::fs::write(casa.join(".mcp.json"), "{}").unwrap();
        config(d.path(), "settings.json", r#"{"hooks":{}}"#);
        assert_eq!(por_que_nao_confiar_em(&sub, &casa), None);
        assert_eq!(por_que_nao_confiar_em(&casa, &casa), None);
        // Fora da casa, as de cima contam até a raiz.
        let fora = d.path().join("fora").join("x");
        std::fs::create_dir_all(&fora).unwrap();
        assert!(por_que_nao_confiar_em(&fora, &casa).is_some());
        // Caminho relativo: nada a conferir, nada a responder.
        assert!(por_que_nao_confiar_em(Path::new("repo"), &casa).is_some());
    }
}
