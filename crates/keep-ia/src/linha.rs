//! A linha que sobe a IA numa aba: o programa, os parâmetros com que a
//! pessoa costuma abri-lo, a conversa a retomar e o ambiente da conta —
//! escrita na sintaxe do shell da aba (zsh/bash/fish, PowerShell, cmd).
//!
//! O ambiente vai só para o comando (`env -u … VAR=… programa` nos shells
//! de Unix); no PowerShell e no cmd, que não têm isso, as variáveis são
//! postas antes e tiradas depois, na mesma linha.

/// Como o shell da aba escreve uma linha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sintaxe {
    Posix,
    Fish,
    PowerShell,
    Cmd,
}

/// Pelo nome do shell (o `command` da aba no prompt, ou o processo do shell).
pub fn sintaxe(shell: &str) -> Sintaxe {
    let nome = shell.strip_prefix('-').unwrap_or(shell);
    let nome = nome.strip_suffix(".exe").unwrap_or(nome).to_ascii_lowercase();
    match nome.as_str() {
        "fish" => Sintaxe::Fish,
        "pwsh" | "powershell" => Sintaxe::PowerShell,
        "cmd" => Sintaxe::Cmd,
        _ => Sintaxe::Posix,
    }
}

/// O que subir: o programa, os argumentos, as variáveis a pôr e as a tirar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subida {
    pub programa: String,
    pub args: Vec<String>,
    pub definir: Vec<(String, String)>,
    pub tirar: Vec<String>,
}

fn simples(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "@%+=:,./_-".contains(c))
}

fn posix(s: &str) -> String {
    if simples(s) { s.to_string() } else { format!("'{}'", s.replace('\'', r"'\''")) }
}

fn fish(s: &str) -> String {
    if simples(s) { s.to_string() } else { format!("'{}'", s.replace('\\', r"\\").replace('\'', r"\'")) }
}

fn powershell(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn cmd(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || ":.\\/_-=,".contains(c)) {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
}

/// A linha, pronta para digitar (sem o Enter).
pub fn linha(sx: Sintaxe, s: &Subida) -> String {
    let tirar: Vec<&String> = s.tirar.iter().filter(|t| !s.definir.iter().any(|(n, _)| n == *t)).collect();
    match sx {
        Sintaxe::Posix | Sintaxe::Fish => {
            let q = if sx == Sintaxe::Fish { fish } else { posix };
            let mut partes: Vec<String> = Vec::new();
            if !tirar.is_empty() || !s.definir.is_empty() {
                partes.push("env".into());
                for t in &tirar {
                    partes.push(format!("-u {}", q(t)));
                }
                for (n, v) in &s.definir {
                    partes.push(format!("{n}={}", q(v)));
                }
            }
            partes.push(q(&s.programa));
            partes.extend(s.args.iter().map(|a| q(a)));
            partes.join(" ")
        }
        Sintaxe::PowerShell => {
            let mut antes: Vec<String> = s.definir.iter().map(|(n, v)| format!("$env:{n}={}", powershell(v))).collect();
            antes.extend(tirar.iter().map(|t| format!("$env:{t}=$null")));
            let mut comando = format!("& {}", powershell(&s.programa));
            for a in &s.args {
                comando.push(' ');
                comando.push_str(&powershell(a));
            }
            let depois: Vec<String> = s.definir.iter().map(|(n, _)| format!("$env:{n}=$null")).collect();
            let mut partes = antes;
            partes.push(comando);
            partes.extend(depois);
            partes.join("; ")
        }
        Sintaxe::Cmd => {
            let mut partes: Vec<String> = s.definir.iter().map(|(n, v)| format!("set \"{n}={v}\"")).collect();
            partes.extend(tirar.iter().map(|t| format!("set \"{t}=\"")));
            let mut comando = cmd(&s.programa);
            for a in &s.args {
                comando.push(' ');
                comando.push_str(&cmd(a));
            }
            partes.push(comando);
            partes.extend(s.definir.iter().map(|(n, _)| format!("set \"{n}=\"")));
            partes.join(" & ")
        }
    }
}

/// O fim da linha sem espaços, para conferir na tela que ela chegou
/// inteira antes do Enter (a tela quebra linhas compridas).
pub fn marca(texto: &str) -> String {
    let sem: Vec<char> = texto.chars().filter(|c| !c.is_whitespace()).collect();
    sem[sem.len().saturating_sub(24)..].iter().collect()
}

/// Se a tela mostra `marca` perto do fim (onde o cursor está).
pub fn chegou(tela: &str, marca: &str) -> bool {
    let sem: Vec<char> = tela.chars().filter(|c| !c.is_whitespace()).collect();
    let fim: String = sem[sem.len().saturating_sub(600)..].iter().collect();
    fim.contains(marca)
}

// ------------------------------------------------------ os parâmetros de cada um

/// Opções do Claude Code que levam um valor (o seguinte, ou depois do `=`).
const CLAUDE_COM_VALOR: &[&str] = &[
    "--add-dir",
    "--agent",
    "--agents",
    "--allowedTools",
    "--allowed-tools",
    "--disallowedTools",
    "--disallowed-tools",
    "--append-system-prompt",
    "--append-system-prompt-file",
    "--system-prompt",
    "--system-prompt-file",
    "--autocompact",
    "--betas",
    "--debug-file",
    "--effort",
    "--environment",
    "--fallback-model",
    "--mcp-config",
    "--model",
    "--permission-mode",
    "--plugin-dir",
    "--plugin-url",
    "--setting-sources",
    "--settings",
    "--tools",
    "--max-budget-usd",
    "--permission-prompts",
    "--system-prompt-snapshot",
    "--json-schema",
    "--input-format",
    "--output-format",
    "--file",
];

/// As que não seguem para a conversa retomada: elas escolhem outra
/// conversa, outro lugar ou outro modo de rodar. `true` = leva valor.
const CLAUDE_FORA: &[(&str, bool)] = &[
    ("--resume", false),
    ("-r", false),
    ("--continue", false),
    ("-c", false),
    ("--session-id", true),
    ("--fork-session", false),
    ("--from-pr", false),
    ("--teleport", false),
    ("--cloud", false),
    ("--bg", false),
    ("--background", false),
    ("--print", false),
    ("-p", false),
    ("--desktop", false),
    ("--name", true),
    ("-n", true),
    ("--remote-control", false),
    ("--remote-control-session-name-prefix", true),
    ("--tmux", false),
    ("--worktree", false),
    ("-w", false),
    ("--help", false),
    ("-h", false),
    ("--version", false),
    ("-v", false),
];

const CODEX_COM_VALOR: &[&str] = &[
    "-c",
    "--config",
    "--enable",
    "--disable",
    "--remote",
    "--remote-auth-token-env",
    "-m",
    "--model",
    "--local-provider",
    "-p",
    "--profile",
    "-s",
    "--sandbox",
    "-C",
    "--cd",
    "--add-dir",
    "-a",
    "--ask-for-approval",
];

const CODEX_FORA: &[(&str, bool)] = &[
    ("--last", false),
    ("--all", false),
    ("--include-non-interactive", false),
    ("-i", true),
    ("--image", true),
    ("--help", false),
    ("-h", false),
    ("--version", false),
    ("-V", false),
];

/// Os parâmetros da linha de comando de um Claude ou Codex que seguem para
/// a próxima vez que ele subir na aba: o modelo, o esforço, as permissões…
/// Ficam de fora a conversa (`--resume`, `resume <id>`), o texto inicial e
/// o que escolhe outro modo de rodar.
pub fn parametros(agente: &str, argv: &[String]) -> Vec<String> {
    let (com_valor, fora) = if agente == "codex" { (CODEX_COM_VALOR, CODEX_FORA) } else { (CLAUDE_COM_VALOR, CLAUDE_FORA) };
    let mut saida = Vec::new();
    let mut i = 1;
    let mut subcomando_visto = false;
    while i < argv.len() {
        let a = &argv[i];
        i += 1;
        if a == "--" {
            break;
        }
        if !a.starts_with('-') || a == "-" {
            // Texto inicial, id ou subcomando (`codex resume <id>`).
            if agente == "codex" && !subcomando_visto && a == "resume" {
                subcomando_visto = true;
                if argv.get(i).is_some_and(|p| !p.starts_with('-')) {
                    i += 1;
                }
            }
            continue;
        }
        let (nome, junto) = match a.split_once('=') {
            Some((n, _)) if n.starts_with("--") => (n, true),
            _ => (a.as_str(), false),
        };
        if let Some(&(_, leva)) = fora.iter().find(|(n, _)| *n == nome) {
            // As de valor opcional (`--resume [id]`) levam o seguinte junto
            // quando ele não é outra opção.
            let opcional = matches!(nome, "--resume" | "-r" | "--from-pr" | "--teleport" | "--cloud" | "--remote-control" | "--worktree" | "-w");
            if !junto && (leva || opcional) && argv.get(i).is_some_and(|p| !p.starts_with('-')) {
                i += 1;
            }
            continue;
        }
        saida.push(a.clone());
        if !junto && com_valor.contains(&nome) {
            if let Some(v) = argv.get(i) {
                saida.push(v.clone());
                i += 1;
            }
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn subida() -> Subida {
        Subida {
            programa: "/Users/a b/.local/bin/claude".into(),
            args: v(&["--effort", "max", "--resume", "9e2fff2d-f449-4d5f-998f-1b3495689c61"]),
            definir: vec![
                ("KEEP_IA_ESCOLHA".into(), "claude:ordem".into()),
                ("CLAUDE_SECURESTORAGE_CONFIG_DIR".into(), "/x/fixas/k-1".into()),
            ],
            tirar: v(&["CODEX_HOME", "CLAUDE_SECURESTORAGE_CONFIG_DIR"]),
        }
    }

    #[test]
    fn a_linha_em_cada_shell() {
        assert_eq!(
            linha(Sintaxe::Posix, &subida()),
            "env -u CODEX_HOME KEEP_IA_ESCOLHA=claude:ordem CLAUDE_SECURESTORAGE_CONFIG_DIR=/x/fixas/k-1 '/Users/a b/.local/bin/claude' --effort max --resume 9e2fff2d-f449-4d5f-998f-1b3495689c61"
        );
        assert_eq!(
            linha(Sintaxe::PowerShell, &subida()),
            "$env:KEEP_IA_ESCOLHA='claude:ordem'; $env:CLAUDE_SECURESTORAGE_CONFIG_DIR='/x/fixas/k-1'; $env:CODEX_HOME=$null; & '/Users/a b/.local/bin/claude' '--effort' 'max' '--resume' '9e2fff2d-f449-4d5f-998f-1b3495689c61'; $env:KEEP_IA_ESCOLHA=$null; $env:CLAUDE_SECURESTORAGE_CONFIG_DIR=$null"
        );
        assert_eq!(
            linha(Sintaxe::Cmd, &subida()),
            "set \"KEEP_IA_ESCOLHA=claude:ordem\" & set \"CLAUDE_SECURESTORAGE_CONFIG_DIR=/x/fixas/k-1\" & set \"CODEX_HOME=\" & \"/Users/a b/.local/bin/claude\" --effort max --resume 9e2fff2d-f449-4d5f-998f-1b3495689c61 & set \"KEEP_IA_ESCOLHA=\" & set \"CLAUDE_SECURESTORAGE_CONFIG_DIR=\""
        );
        let mut s = subida();
        s.args = v(&["isn't"]);
        assert!(linha(Sintaxe::Posix, &s).ends_with(r"'isn'\''t'"));
        assert!(linha(Sintaxe::Fish, &s).ends_with(r"'isn\'t'"));
        assert_eq!(sintaxe("-zsh"), Sintaxe::Posix);
        assert_eq!(sintaxe("pwsh.exe"), Sintaxe::PowerShell);
    }

    #[test]
    fn a_marca_acha_a_linha_quebrada_na_tela() {
        let l = "env KEEP_IA_ESCOLHA=claude:ordem claude --resume 9e2fff2d-f449-4d5f-998f-1b3495689c61";
        let m = marca(l);
        let tela = "$ env KEEP_IA_ESCOLHA=claude:ordem claude --resume 9e2fff2d-f449-4d5f-99\n8f-1b3495689c61\n\n";
        assert!(chegou(tela, &m));
        assert!(!chegou("$ ", &m));
    }

    #[test]
    fn os_parametros_que_seguem() {
        let argv = v(&[
            "claude",
            "--allow-dangerously-skip-permissions",
            "--effort",
            "max",
            "--resume",
            "9e2fff2d-f449-4d5f-998f-1b3495689c61",
            "Continue a tarefa desta aba",
        ]);
        assert_eq!(parametros("claude", &argv), v(&["--allow-dangerously-skip-permissions", "--effort", "max"]));
        let argv = v(&["claude", "-c", "--model=opus", "-n", "nome", "--verbose"]);
        assert_eq!(parametros("claude", &argv), v(&["--model=opus", "--verbose"]));
        let argv = v(&[
            "codex",
            "resume",
            "01a103e9-cc76-75b1-b919-7702b666146a",
            "--dangerously-bypass-approvals-and-sandbox",
            "-c",
            "model_reasoning_effort=high",
            "faz isso",
        ]);
        assert_eq!(
            parametros("codex", &argv),
            v(&["--dangerously-bypass-approvals-and-sandbox", "-c", "model_reasoning_effort=high"])
        );
    }
}
