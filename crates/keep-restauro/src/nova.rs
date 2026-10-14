//! `keep nova`: uma conversa nova do Claude numa aba nova do Keep.
//!
//! É o caminho para qualquer sessão (Claude ou Codex) abrir outra conversa — e o
//! dos links `claude-cli://`. O Terminal do macOS só a pessoa abre, à mão
//! (pedido de 27/09/2026): uma conversa aberta por `open -a Terminal` nasceu com
//! o ambiente da sessão que a abriu e nem gravou transcrito. A aba daqui nasce do
//! keepd, com o ambiente limpo dele, e aparece na barra lateral.
//!
//! Na aba, digita `claude …` e aperta Enter: o shell da aba aplica a função
//! `claude` de quem tiver uma (o esforço padrão, a pasta presa do workspace).
//!
//!   keep nova --workspace W --pasta P --passagem ARQUIVO    lê o arquivo inteiro e segue
//!   keep nova --workspace W --pasta P --pedido "texto"      o texto vai para um arquivo, lido do mesmo jeito
//!   keep nova [--workspace W] --pasta P --argumentos -- ARG…  `claude ARG…` (sem workspace: o da aba usada por último, fora o `quick`)
//!       [--nome "rótulo"]  nome da aba no app      [--sem-enter]  digita e não aperta Enter
//!   keep nova --listar                                       workspaces e abas
//!
//! Saída 0: aba aberta e comando enviado (imprime o workspace, a aba e o comando).
//! 1: o Keep fechado, workspace ou pasta que não existe, prompt que não apareceu.
//! 2: uso errado.

use std::path::{Path, PathBuf};

use keep_ia::caminhos;
use keep_ia::daemon;
use keep_ia::linha::{self, Subida};

const TAMANHO_PADRAO: (u16, u16) = (144, 47);

/// O que a conversa nova recebe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conteudo {
    /// Um arquivo que ela lê inteiro e segue.
    Passagem(PathBuf),
    /// Um pedido, gravado num arquivo (texto longo, aspas e quebras não passam pela linha).
    Pedido(String),
    /// Os argumentos do `claude`, como vieram (um link `claude-cli://`).
    Argumentos(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pedido {
    pub workspace: Option<String>,
    pub pasta: PathBuf,
    pub conteudo: Conteudo,
    pub nome: Option<String>,
    pub sem_enter: bool,
}

/// Lê os argumentos de `keep nova`. Tudo depois de `--argumentos` (e de um `--`
/// logo em seguida) é do `claude`, mesmo o que começa com `--`.
pub fn ler_argumentos(args: &[String]) -> Result<Option<Pedido>, String> {
    let (proprios, do_claude) = match args.iter().position(|a| a == "--argumentos") {
        Some(i) => {
            let resto = &args[i + 1..];
            let resto = if resto.first().map(String::as_str) == Some("--") { &resto[1..] } else { resto };
            (&args[..i], Some(resto.to_vec()))
        }
        None => (args, None),
    };
    let mut workspace = None;
    let mut pasta = None;
    let mut passagem = None;
    let mut pedido = None;
    let mut nome = None;
    let mut sem_enter = false;
    let mut i = 0;
    while i < proprios.len() {
        let a = proprios[i].as_str();
        let valor = |i: usize| proprios.get(i + 1).cloned().ok_or_else(|| format!("{a} sem valor"));
        match a {
            "--listar" => return Ok(None),
            "--sem-enter" => sem_enter = true,
            "--workspace" => {
                workspace = Some(valor(i)?);
                i += 1;
            }
            "--pasta" => {
                pasta = Some(valor(i)?);
                i += 1;
            }
            "--passagem" => {
                passagem = Some(valor(i)?);
                i += 1;
            }
            "--pedido" => {
                pedido = Some(valor(i)?);
                i += 1;
            }
            "--nome" => {
                nome = Some(valor(i)?);
                i += 1;
            }
            outro => return Err(format!("opção desconhecida: {outro}")),
        }
        i += 1;
    }
    let conteudo = match (passagem, pedido, do_claude) {
        (Some(p), None, None) => Conteudo::Passagem(expande(&p)),
        (None, Some(t), None) => Conteudo::Pedido(t),
        (None, None, Some(a)) if !a.is_empty() => Conteudo::Argumentos(a),
        (None, None, Some(_)) => return Err("--argumentos sem argumento nenhum".into()),
        _ => return Err("diga uma (e só uma) de --passagem, --pedido ou --argumentos".into()),
    };
    if workspace.is_none() && !matches!(conteudo, Conteudo::Argumentos(_)) {
        return Err("faltou --workspace".into());
    }
    let pasta = pasta.ok_or("faltou --pasta")?;
    Ok(Some(Pedido { workspace, pasta: expande(&pasta), conteudo, nome, sem_enter }))
}

fn expande(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(resto) => caminhos::casa().join(resto),
        None if p == "~" => caminhos::casa(),
        None => PathBuf::from(p),
    }
}

/// Os argumentos do `claude` na aba: os do link, ou o pedido de ler o arquivo.
pub fn argumentos_do_claude(conteudo: &Conteudo, arquivo: Option<&Path>) -> Vec<String> {
    match conteudo {
        Conteudo::Argumentos(a) => a.clone(),
        _ => vec![format!(
            "Leia o arquivo {} inteiro antes de agir e siga o que ele pede.",
            arquivo.map(|a| a.display().to_string()).unwrap_or_default()
        )],
    }
}

/// Abre a aba e digita a linha. Devolve o workspace, a aba e a linha.
pub fn abrir(p: &Pedido) -> Result<(String, u32, String), String> {
    let lista = daemon::listar().map_err(|e| format!("O Keep não está aberto ({e}). Abra o Keep; não use o Terminal."))?;
    // Sem o `\\?\` que o Windows põe: a pasta vai à aba, e o arquivo, ao texto da linha.
    let pasta = p.pasta.canonicalize().map(caminhos::sem_prefixo).map_err(|_| format!("pasta inexistente: {}", p.pasta.display()))?;
    if !pasta.is_dir() {
        return Err(format!("pasta inexistente: {}", pasta.display()));
    }
    let arquivo = match &p.conteudo {
        Conteudo::Passagem(a) => {
            Some(a.canonicalize().map(caminhos::sem_prefixo).map_err(|_| format!("passagem inexistente: {}", a.display()))?)
        }
        Conteudo::Pedido(t) => Some(grava_pedido(t)?),
        Conteudo::Argumentos(_) => None,
    };
    let vivas: Vec<&daemon::Aba> = lista.abas.iter().filter(|a| !a.info.finished).collect();
    let ws = match &p.workspace {
        Some(w) => w.clone(),
        // O workspace `quick` (a janela rápida) não conta: a barra lateral não o mostra, e a aba abriria onde ninguém vê.
        None => vivas
            .iter()
            .filter(|a| a.ws != crate::retrato::RAPIDO)
            .max_by_key(|a| a.info.last_active)
            .map(|a| a.ws.clone())
            .ok_or("o Keep não tem aba aberta; diga --workspace")?,
    };
    if !vivas.iter().any(|a| a.ws == ws) {
        let mut existem: Vec<&str> = vivas.iter().map(|a| a.ws.as_str()).collect();
        existem.dedup();
        return Err(format!("workspace {ws:?} não existe no Keep; existem: {}", existem.join(", ")));
    }
    let (cols, rows) = vivas.iter().find(|a| a.ws == ws).map(|a| (a.info.cols, a.info.rows)).unwrap_or(TAMANHO_PADRAO);
    let tab = crate::daemon::nova_aba(&ws, Some(&pasta.to_string_lossy()), cols, rows, 0, 0)?;
    let shell = crate::restaurar::espera_shell(&ws, tab).ok_or_else(|| format!("a aba {tab} de {ws} não mostrou o prompt do shell"))?;
    let s = Subida {
        programa: "claude".into(),
        args: argumentos_do_claude(&p.conteudo, arquivo.as_deref()),
        definir: Vec::new(),
        tirar: Vec::new(),
        pasta: None,
    };
    let texto = linha::linha(linha::sintaxe(&shell), &s);
    match keep_ia::trocar::digita(&ws, tab, &texto, !p.sem_enter) {
        Ok(true) => {}
        Ok(false) => return Err(format!("o comando não apareceu inteiro na aba {tab} de {ws}; nada foi executado")),
        Err(r) => return Err(r.json().to_string()),
    }
    if let Some(nome) = &p.nome {
        let amb = keep_ia::worktrees::ambiente::Ambiente::do_sistema();
        let nasceu = keep_ia::worktrees::abas::nascimento_do_socket(&amb);
        crate::nomear_no_app(&amb, nasceu, &[(ws.clone(), tab, nome.clone())], true);
    }
    Ok((ws, tab, texto))
}

fn grava_pedido(texto: &str) -> Result<PathBuf, String> {
    let pasta = keep_ia::caminhos::claude().join("handoffs").join("abas");
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o700));
    }
    let arq = pasta.join(format!("{}-{}.md", crate::agora_ms(), std::process::id()));
    std::fs::write(&arq, format!("{}\n", texto.trim_end())).map_err(|e| e.to_string())?;
    Ok(arq)
}

fn listar() -> Result<(), String> {
    let l = daemon::listar().map_err(|e| format!("O Keep não está aberto ({e})."))?;
    let mut atual = String::new();
    for a in l.abas.iter().filter(|a| !a.info.finished) {
        if a.ws != atual {
            println!("{}", a.ws);
            atual = a.ws.clone();
        }
        println!("  aba {}  {}  {}", a.info.id, a.info.title, a.info.cwd);
    }
    Ok(())
}

pub fn cli(args: &[String]) -> i32 {
    let pedido = match ler_argumentos(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}\nuso: keep nova --workspace W --pasta P (--passagem ARQ | --pedido TEXTO | --argumentos -- ARG…) [--nome N] [--sem-enter] | --listar");
            return 2;
        }
    };
    let Some(pedido) = pedido else {
        return match listar() {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
    };
    match abrir(&pedido) {
        Ok((ws, tab, texto)) => {
            println!("{ws} aba {tab}: {texto}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn argumentos_do_link_passam_inteiros() {
        let p = ler_argumentos(&v(&["--pasta", "/p", "--argumentos", "--", "--deep-link-origin", "--prefill-b64=eA"])).unwrap().unwrap();
        assert_eq!(p.conteudo, Conteudo::Argumentos(v(&["--deep-link-origin", "--prefill-b64=eA"])));
        assert_eq!(p.workspace, None);
        let sem = ler_argumentos(&v(&["--pasta", "/p", "--argumentos", "-r"])).unwrap().unwrap();
        assert_eq!(sem.conteudo, Conteudo::Argumentos(v(&["-r"])));
    }

    #[test]
    fn pedido_e_passagem_exigem_workspace_e_um_so() {
        assert!(ler_argumentos(&v(&["--pasta", "/p", "--pedido", "x"])).is_err(), "sem workspace");
        assert!(ler_argumentos(&v(&["--workspace", "W", "--pasta", "/p", "--pedido", "x", "--passagem", "/a"])).is_err());
        assert!(ler_argumentos(&v(&["--workspace", "W", "--pedido", "x"])).is_err(), "sem pasta");
        let p = ler_argumentos(&v(&["--workspace", "W", "--pasta", "/p", "--pedido", "faça", "--nome", "N", "--sem-enter"])).unwrap().unwrap();
        assert_eq!((p.workspace.as_deref(), p.nome.as_deref(), p.sem_enter), (Some("W"), Some("N"), true));
        assert_eq!(ler_argumentos(&v(&["--listar"])).unwrap(), None);
    }

    #[test]
    fn a_conversa_le_o_arquivo() {
        let a = argumentos_do_claude(&Conteudo::Pedido("x".into()), Some(Path::new("/h/abas/1.md")));
        assert_eq!(a, vec!["Leia o arquivo /h/abas/1.md inteiro antes de agir e siga o que ele pede."]);
    }
}
