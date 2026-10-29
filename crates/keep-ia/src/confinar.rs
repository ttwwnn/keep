//! A pasta a que a IA de um workspace fica presa.
//!
//! O app grava em `~/.config/keep/confinement.json` ({"workspace": "/pasta"})
//! a pasta que a IA do workspace pode acessar. Quem sobe a IA numa aba (`keep ia
//! rodar`, a função do PowerShell, a troca de conta ou de IA e a restauração)
//! passa por `keep ia confinar -- <programa> …`, que decide pelo workspace da
//! aba (`KEEP_WORKSPACE`) e roda o programa num sandbox do macOS
//! (`sandbox-exec`): ele e tudo o que ele roda não leem nem gravam em /Users
//! e /Volumes, salvo na pasta escolhida e no mínimo de que a IA precisa.
//! No Windows o sandbox é um token restrito com permissões só na pasta e no
//! mínimo de que a IA precisa (`confinar_win.rs`): o perfil do usuário fica
//! de fora, para leitura e gravação. Sem a pasta existir, ou sem o sandbox, o
//! programa NÃO sobe: a restrição não se desliga sozinha. Sem restrição para
//! o workspace, a linha continua exatamente a que era.
//!
//! O que manda trabalho para um servidor de fundo, fora do sandbox, é
//! recusado num workspace restrito: `claude agents` e `claude attach` (o
//! daemon do Claude Code), `codex agents` e `codex queue` (o do Codex). O
//! Codex em si sobe com `--no-daemon`, para o servidor dele rodar dentro do
//! próprio processo — e do sandbox.

use crate::caminhos;
use std::path::{Path, PathBuf};

/// Onde o app guarda as pastas. `KEEP_CONFINE_CONFIG` troca (testes).
pub fn arquivo() -> PathBuf {
    match std::env::var_os("KEEP_CONFINE_CONFIG") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => caminhos::casa().join(".config/keep/confinement.json"),
    }
}

/// A pasta permitida à IA do workspace, se houver.
pub fn pasta(ws: &str) -> Option<String> {
    let texto = std::fs::read_to_string(arquivo()).ok()?;
    let json: serde_json::Value = serde_json::from_str(&texto).ok()?;
    json.get(ws)?.as_str().filter(|p| !p.is_empty()).map(str::to_string)
}

/// O `keep` que prende o programa: este mesmo executável (`KEEP_CONFINE_BIN`
/// troca, nos testes).
fn helper() -> Option<PathBuf> {
    match std::env::var_os("KEEP_CONFINE_BIN") {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)).filter(|p| p.is_file()),
        _ => std::env::current_exe().ok(),
    }
}

/// O programa e os argumentos da linha, com `keep ia confinar --` na frente
/// quando o workspace tem pasta permitida; do contrário, os mesmos. Só no
/// macOS e no Windows: o sandbox é o de cada um.
pub fn envolve(ws: &str, programa: String, args: Vec<String>) -> (String, Vec<String>) {
    if !cfg!(any(target_os = "macos", windows)) || pasta(ws).is_none() {
        return (programa, args);
    }
    match helper() {
        Some(h) => {
            let mut novos = vec!["ia".to_string(), "confinar".into(), "--".into(), programa];
            novos.extend(args);
            (h.to_string_lossy().into_owned(), novos)
        }
        None => (programa, args),
    }
}

/// Um caminho como literal do perfil do sandbox.
fn literal(p: &str) -> String {
    format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""))
}

fn escapa_regex(p: &str) -> String {
    let mut s = String::new();
    for c in p.chars() {
        if ".^$|?*+()[]{}\\".contains(c) {
            s.push('\\');
        }
        s.push(c);
    }
    s
}

/// As pastas do git que uma worktree usa e que ficam fora dela.
pub(crate) fn pastas_do_git(pasta: &Path) -> Vec<String> {
    let mut saida = Vec::new();
    let Ok(texto) = std::fs::read_to_string(pasta.join(".git")) else { return saida };
    let Some(gitdir) = texto.trim().strip_prefix("gitdir:") else { return saida };
    let gitdir = pasta.join(gitdir.trim());
    let gitdir = gitdir.canonicalize().unwrap_or(gitdir);
    if let Ok(comum) = std::fs::read_to_string(gitdir.join("commondir")) {
        let c = gitdir.join(comum.trim());
        saida.push(c.canonicalize().unwrap_or(c).to_string_lossy().into_owned());
    }
    saida.insert(0, gitdir.to_string_lossy().into_owned());
    saida
}

/// O perfil do sandbox para uma pasta: nada de /Users e /Volumes, salvo a
/// pasta (e o git dela), a casa do Claude Code e do Codex, o estado e os
/// caches, o Chaveiro; para ler, os programas e a configuração do git.
///
/// As permissões usam as MESMAS operações da negação e vêm depois dela: uma
/// permissão de `file-read*` não vence uma negação de `file-read-data`, que
/// é mais específica, e a última regra que casa é a que vale.
pub fn perfil(pasta: &Path) -> String {
    let casa = caminhos::casa().to_string_lossy().into_owned();
    let mut rw = vec![pasta.to_string_lossy().into_owned()];
    rw.extend(pastas_do_git(pasta));
    for sub in [".claude", ".codex", ".local/state", ".cache", "Library/Caches", "Library/Keychains"] {
        rw.push(format!("{casa}/{sub}"));
    }
    let mut linhas = vec![
        "(version 1)".to_string(),
        "(allow default)".into(),
        "(deny file-read-data file-write* (subpath \"/Users\"))".into(),
        "(deny file-read-data file-write* (subpath \"/Volumes\"))".into(),
    ];
    for p in &rw {
        linhas.push(format!("(allow file-read-data file-write* (subpath {}))", literal(p)));
    }
    linhas.push(format!("(allow file-read-data file-write* (literal {}))", literal(&format!("{casa}/.claude.json"))));
    linhas.push(format!(
        "(allow file-read-data file-write* (regex #{}))",
        literal(&format!("^{}.*", escapa_regex(&format!("{casa}/.claude.json."))))
    ));
    for sub in [".local/bin", ".local/share/claude", ".config/git"] {
        linhas.push(format!("(allow file-read-data (subpath {}))", literal(&format!("{casa}/{sub}"))));
    }
    linhas.push(format!("(allow file-read-data (literal {}))", literal(&format!("{casa}/.gitconfig"))));
    linhas.push(format!(
        "(allow file-read-data (regex #{}))",
        literal(&format!("^{}.*", escapa_regex(&format!("{casa}/Library/Preferences/com.apple.security"))))
    ));
    linhas.join("\n") + "\n"
}

/// O nome do programa, sem a pasta (de `/` ou de `\`) e, no Windows, sem a
/// extensão (`claude.exe`, `codex.cmd`), em minúsculas.
fn nome(programa: &str) -> String {
    let base = programa.rsplit(['/', '\\']).next().unwrap_or(programa);
    let base = match base.rsplit_once('.') {
        Some((tronco, ext)) if matches!(ext.to_ascii_lowercase().as_str(), "exe" | "cmd" | "bat" | "com") => tronco,
        _ => base,
    };
    base.to_ascii_lowercase()
}

/// O primeiro argumento que não é opção: o subcomando, se houver.
fn subcomando(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| !a.starts_with('-'))
}

/// O que o Claude Code lê para ficar sem a ferramenta Bash (a regra de
/// permissão que a nega por inteiro).
pub const CONFIGURACAO_SEM_BASH: &str = r#"{"permissions":{"deny":["Bash"]}}"#;

/// O nome do arquivo com essa regra: quem lê a linha de um Claude que anda
/// (para subi-lo de novo na aba) a reconhece por ele e a deixa de fora, senão a
/// regra iria junto para um Claude que não está preso.
pub const ARQUIVO_SEM_BASH: &str = "claude-sem-bash.json";

/// A ferramenta de shell do Claude Code no Windows presa a uma pasta. O Git
/// Bash (MSYS2) não sobe sob o token restrito: ele cria de início um canal de
/// sinais com nome e uma DACL só do usuário, e a segunda checagem do token
/// (que não leva o usuário) o recusa (`couldn't create signal pipe`). O Claude
/// Code tem, no lugar, a ferramenta PowerShell: negada a Bash, e com
/// `CLAUDE_CODE_USE_POWERSHELL_TOOL=1` (o ambiente do programa preso leva), ele
/// fica só com ela. A regra vai por um arquivo (`configuracao`), não pela
/// linha: um argumento com aspas não passa por um atalho `.cmd` do npm.
pub fn claude_sem_o_bash(args: &[String], configuracao: &str) -> Vec<String> {
    let mut v = vec!["--settings".to_string(), configuracao.to_string()];
    v.extend(args.iter().cloned());
    v
}

/// Como a linha sobe num workspace restrito: `Err` com o motivo quando o
/// comando mandaria trabalho para fora do sandbox; o Codex ganha
/// `--no-daemon` (o servidor dele dentro do processo).
pub fn linha_presa(programa: &str, args: &[String]) -> Result<Vec<String>, String> {
    let sub = subcomando(args);
    match nome(programa).as_str() {
        "claude" if matches!(sub, Some("agents" | "attach")) => Err(format!(
            "`claude {}` roda as sessões no servidor de fundo do Claude Code, fora da pasta permitida.",
            sub.unwrap_or_default()
        )),
        "codex" if matches!(sub, Some("agents" | "queue")) => Err(format!(
            "`codex {}` depende do servidor de fundo do Codex, fora da pasta permitida.",
            sub.unwrap_or_default()
        )),
        "codex"
            if !args.iter().any(|a| a == "--no-daemon")
                && !matches!(sub, Some("app-server" | "mcp" | "login" | "logout" | "features" | "exec-server" | "remote-control" | "completion" | "help")) =>
        {
            let mut v = vec!["--no-daemon".to_string()];
            v.extend(args.iter().cloned());
            Ok(v)
        }
        _ => Ok(args.to_vec()),
    }
}

/// `keep ia confinar -- <programa> [argumentos…]`: roda o programa preso à
/// pasta do workspace da aba, ou como sempre quando não há pasta.
#[cfg(unix)]
pub fn cli(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    let args: Vec<String> = match args.first().map(String::as_str) {
        Some("--") => args[1..].to_vec(),
        _ => args.to_vec(),
    };
    let Some((programa, resto)) = args.split_first() else {
        eprintln!("uso: keep ia confinar -- <programa> [argumentos...]");
        return 2;
    };
    let ws = std::env::var("KEEP_WORKSPACE").unwrap_or_default();
    let pasta_permitida = if ws.is_empty() || !cfg!(target_os = "macos") { None } else { pasta(&ws) };
    let Some(pasta_permitida) = pasta_permitida else {
        let erro = std::process::Command::new(programa).args(resto).exec();
        eprintln!("Keep: não foi possível rodar {programa}: {erro}");
        return 127;
    };
    let pasta_permitida = PathBuf::from(&pasta_permitida);
    let Ok(pasta_real) = pasta_permitida.canonicalize().map_err(|_| ()).and_then(|p| if p.is_dir() { Ok(p) } else { Err(()) }) else {
        eprintln!(
            "Keep: a pasta permitida para a IA do workspace {ws} não existe ({}). A IA não sobe sem a restrição; escolha outra pasta no workspace.",
            pasta_permitida.display()
        );
        return 1;
    };
    let linha = match linha_presa(programa, resto) {
        Ok(l) => l,
        Err(motivo) => {
            eprintln!("Keep: {motivo} Este workspace restringe a IA a {}.", pasta_real.display());
            return 1;
        }
    };
    let sandbox = "/usr/bin/sandbox-exec";
    if !Path::new(sandbox).is_file() {
        eprintln!("Keep: sandbox-exec não está disponível; a IA não sobe sem a restrição.");
        return 1;
    }
    let aqui = std::env::current_dir().ok().and_then(|p| p.canonicalize().ok());
    if aqui.as_ref().is_none_or(|a| !a.starts_with(&pasta_real)) {
        let _ = std::env::set_current_dir(&pasta_real);
        eprintln!("Keep: a IA deste workspace só acessa {}; subindo nela.", pasta_real.display());
    } else {
        eprintln!("Keep: a IA deste workspace só acessa {}.", pasta_real.display());
    }
    let erro = std::process::Command::new(sandbox)
        .arg("-p")
        .arg(perfil(&pasta_real))
        .arg(programa)
        .args(&linha)
        .env("KEEP_CONFINED_TO", &pasta_real)
        .exec();
    eprintln!("Keep: não foi possível prender {programa}: {erro}");
    1
}

#[cfg(windows)]
pub fn cli(args: &[String]) -> i32 {
    crate::confinar_win::executar::cli(args)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_prende_o_workspace_que_tem_pasta() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("confinement.json");
        std::fs::write(&cfg, r#"{"presa": "/pasta/x", "vazia": ""}"#).unwrap();
        let helper = dir.path().join("keep-confine");
        std::fs::write(&helper, "#!/bin/sh\n").unwrap();
        // Variáveis de processo: um teste só mexe nelas.
        unsafe {
            std::env::set_var("KEEP_CONFINE_CONFIG", &cfg);
            std::env::set_var("KEEP_CONFINE_BIN", &helper);
        }
        assert_eq!(pasta("presa").as_deref(), Some("/pasta/x"));
        assert_eq!(pasta("vazia"), None);
        assert_eq!(pasta("livre"), None);
        let (p, a) = envolve("presa", "/bin/claude".into(), vec!["--effort".into(), "max".into()]);
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(p, helper.to_string_lossy());
            assert_eq!(a, vec!["ia", "confinar", "--", "/bin/claude", "--effort", "max"]);
        } else {
            // Sem sandbox neste sistema (Linux): a linha fica a de sempre.
            assert_eq!((p.as_str(), a), ("/bin/claude", vec!["--effort".to_string(), "max".to_string()]));
        }
        let (p, a) = envolve("livre", "/bin/claude".into(), vec!["--effort".into()]);
        assert_eq!((p.as_str(), a), ("/bin/claude", vec!["--effort".to_string()]));
        // Sem o helper, a linha é a de sempre.
        std::fs::remove_file(&helper).unwrap();
        let (p, _) = envolve("presa", "/bin/claude".into(), vec![]);
        assert_eq!(p, "/bin/claude");
    }

    #[test]
    fn o_que_sai_do_sandbox_e_recusado_e_o_codex_sobe_sem_daemon() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(linha_presa("/x/claude", &v(&["agents"])).is_err());
        assert!(linha_presa("claude", &v(&["attach", "abc"])).is_err());
        assert_eq!(linha_presa("claude", &v(&["--effort", "max"])).unwrap(), v(&["--effort", "max"]));
        // No Windows o Claude fica sem a ferramenta Bash (o Git Bash não sobe preso).
        assert_eq!(
            claude_sem_o_bash(&v(&["--effort", "max"]), r"C:\k\sem-bash.json"),
            v(&["--settings", r"C:\k\sem-bash.json", "--effort", "max"])
        );
        assert!(!CONFIGURACAO_SEM_BASH.contains(' '));
        assert!(linha_presa("codex", &v(&["agents"])).is_err());
        assert!(linha_presa("codex", &v(&["queue", "x"])).is_err());
        assert_eq!(linha_presa("/b/codex", &v(&["resume", "id"])).unwrap(), v(&["--no-daemon", "resume", "id"]));
        assert_eq!(linha_presa("codex", &v(&["--no-daemon"])).unwrap(), v(&["--no-daemon"]));
        assert_eq!(linha_presa("codex", &v(&["login"])).unwrap(), v(&["login"]));
    }

    #[test]
    fn o_programa_do_windows_tem_pasta_com_barra_invertida_e_extensao() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(linha_presa(r"C:\Users\a\.local\bin\claude.exe", &v(&["agents"])).is_err());
        assert!(linha_presa(r"C:\Users\a\.local\bin\Claude.EXE", &v(&["attach", "x"])).is_err());
        assert!(linha_presa(r"C:\npm\codex.cmd", &v(&["queue"])).is_err());
        assert_eq!(linha_presa(r"C:\npm\codex.cmd", &v(&["resume", "id"])).unwrap(), v(&["--no-daemon", "resume", "id"]));
        assert_eq!(linha_presa(r"C:\u\codex.exe", &v(&["--no-daemon"])).unwrap(), v(&["--no-daemon"]));
        assert_eq!(linha_presa(r"C:\u\claude.exe", &v(&["--effort", "max"])).unwrap(), v(&["--effort", "max"]));
        // O que só parece com o nome não é o programa.
        assert_eq!(linha_presa(r"C:\u\claude-code.exe", &v(&["agents"])).unwrap(), v(&["agents"]));
        assert_eq!(linha_presa(r"C:\u\claude.exe.bak", &v(&["agents"])).unwrap(), v(&["agents"]));
    }

    #[test]
    fn o_perfil_nega_a_casa_e_libera_a_pasta_depois() {
        let p = perfil(Path::new("/Users/x/projeto"));
        let nega = p.find("(deny file-read-data file-write* (subpath \"/Users\"))").unwrap();
        let libera = p.find("(allow file-read-data file-write* (subpath \"/Users/x/projeto\"))").unwrap();
        assert!(libera > nega, "a permissão precisa vir depois da negação");
        assert!(p.contains("/.claude\"))") && p.contains("/.codex\"))"));
        assert!(!p.contains("(allow file-read*"), "permissão com operações diferentes da negação perde para ela");
    }
}
