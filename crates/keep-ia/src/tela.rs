//! O que a tela de uma aba diz: qual programa roda, se ele está livre,
//! trabalhando, com uma pergunta aberta ou com texto na caixa, se há
//! trabalho em segundo plano, se ele parou no limite da conta, e o id da
//! conversa que o Claude e o Codex imprimem ao sair.
//!
//! As regras são as do kit, que as aprendeu com as telas do Claude Code
//! (2.1.28x) e do Codex (0.15x): a leitura da tela é um indício, e quem
//! tem como provar (a sessão do Claude, o daemon do Codex) é consultado antes.

use regex::Regex;
use std::sync::OnceLock;

/// O que segura o terminal de uma aba.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Programa {
    /// Um shell no prompt; o nome diz a sintaxe da linha a digitar.
    Shell(String),
    Claude,
    Codex,
    Outro(String),
    /// O daemon não disse (antigo, ou a aba acabou de abrir).
    Desconhecido,
}

pub const SHELLS: &[&str] = &[
    "zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "csh", "nu", "login", "pwsh", "powershell", "cmd",
];

/// Pelo nome do processo da frente (o `command` do daemon).
pub fn programa(comando: &str) -> Programa {
    let nome = comando.strip_prefix('-').unwrap_or(comando);
    let nome = nome.strip_suffix(".exe").unwrap_or(nome).to_ascii_lowercase();
    if nome.is_empty() {
        return Programa::Desconhecido;
    }
    if SHELLS.contains(&nome.as_str()) {
        return Programa::Shell(nome);
    }
    if nome == "claude" || versao(&nome) {
        // O instalador do Claude guarda cada versão num arquivo com o nome
        // dela, e o processo se chama como o arquivo.
        return Programa::Claude;
    }
    if nome == "codex" {
        return Programa::Codex;
    }
    Programa::Outro(nome)
}

fn versao(nome: &str) -> bool {
    let partes: Vec<&str> = nome.split('.').collect();
    partes.len() >= 2 && partes.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

impl Programa {
    /// `claude` ou `codex`, para quem roda uma IA.
    pub fn agente(&self) -> Option<&'static str> {
        match self {
            Programa::Claude => Some("claude"),
            Programa::Codex => Some("codex"),
            _ => None,
        }
    }
}

// ------------------------------------------------------------- as linhas

/// A tela em VT (`PreviewVt`) em linhas, com o SGR guardado e o resto das
/// sequências fora. As linhas vêm separadas por `\r\n`.
pub fn linhas_vt(vt: &str) -> Vec<String> {
    let mut saida = String::with_capacity(vt.len());
    let mut it = vt.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                Some('[') => {
                    let mut seq = String::from("\x1b[");
                    let mut privado = false;
                    let mut fim = None;
                    for d in it.by_ref() {
                        seq.push(d);
                        if ('\x40'..='\x7e').contains(&d) {
                            fim = Some(d);
                            break;
                        }
                        if matches!(d, '<' | '=' | '>' | '?') {
                            privado = true;
                        }
                    }
                    // Só o SGR de verdade fica: `\x1b[>4m` é outra coisa.
                    if fim == Some('m') && !privado {
                        saida.push_str(&seq);
                    }
                }
                Some(']') | Some('P') | Some('_') | Some('^') => {
                    // OSC, DCS, APC, PM: até o BEL ou o ST.
                    while let Some(d) = it.next() {
                        if d == '\x07' {
                            break;
                        }
                        if d == '\x1b' {
                            if it.peek() == Some(&'\\') {
                                it.next();
                            }
                            break;
                        }
                    }
                }
                Some('(') | Some(')') | Some('*') | Some('+') => {
                    it.next();
                }
                _ => {}
            },
            '\r' => {}
            _ => saida.push(c),
        }
    }
    saida.split('\n').map(str::to_string).collect()
}

fn re(padrao: &'static str, guarda: &'static OnceLock<Regex>) -> &'static Regex {
    guarda.get_or_init(|| Regex::new(padrao).expect("regex válida"))
}

/// A linha sem cor.
pub fn sem_cor(linha: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"\x1b\[[0-9;:]*m", &R).replace_all(linha, "").into_owned()
}

/// A linha sem cor e sem o texto esmaecido (SGR 2): a sugestão cinza da
/// caixa vazia do Claude e do Codex parece texto digitado sem as cores.
pub fn sem_esmaecido(linha: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let sgr = re(r"\x1b\[([0-9;:]*)m", &R);
    let mut saida = String::new();
    let mut fraco = false;
    let mut pos = 0;
    for m in sgr.captures_iter(linha) {
        let tudo = m.get(0).expect("o casamento inteiro");
        if !fraco {
            saida.push_str(&linha[pos..tudo.start()]);
        }
        let codigos = m.get(1).map(|c| c.as_str()).unwrap_or("");
        let codigos = if codigos.is_empty() { "0" } else { codigos };
        for cod in codigos.replace(':', ";").split(';') {
            match cod {
                "" | "0" | "22" => fraco = false,
                "2" => fraco = true,
                _ => {}
            }
        }
        pos = tudo.end();
    }
    if !fraco {
        saida.push_str(&linha[pos..]);
    }
    saida
}

/// As linhas da tela, sem cor e sem cor nem esmaecido, com o espaço
/// inseparável que o Claude põe depois do ❯ trocado por um espaço.
pub fn as_duas(vt: &str) -> (Vec<String>, Vec<String>) {
    let brutas = linhas_vt(vt);
    let linhas = brutas.iter().map(|l| sem_cor(l).replace('\u{a0}', " ")).collect();
    let limpas = brutas.iter().map(|l| sem_esmaecido(l).replace('\u{a0}', " ")).collect();
    (linhas, limpas)
}

// -------------------------------------------------------------- situação

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estado {
    Livre,
    Ocupada,
    Dialogo,
    Digitado,
}

fn tira(l: &str, chars: &[char]) -> String {
    l.trim_matches(|c| chars.contains(&c)).to_string()
}

/// Como a tela do Claude ou do Codex está: livre (caixa vazia), ocupada
/// (trabalhando, ou sem a caixa à vista), diálogo (pergunta aberta) ou
/// digitado (texto na caixa). Com o detalhe para mostrar.
pub fn situacao(prog: &Programa, linhas: &[String], limpas: &[String]) -> (Estado, String) {
    static LISTA: OnceLock<Regex> = OnceLock::new();
    static CURSOR: OnceLock<Regex> = OnceLock::new();
    let lista = re(r"^\s*\d+\.", &LISTA);
    let cursor_em_lista = re(r"[❯›»]\s*\d+[.)]", &CURSOR);
    let marcas: &[&str] = if *prog == Programa::Claude { &["❯"] } else { &["»", "›"] };
    let mut entradas: Vec<(usize, &str)> = Vec::new();
    for (i, l) in linhas.iter().enumerate() {
        let aparada = l.trim_start_matches([' ', '│', '\t']);
        for m in marcas {
            if aparada.starts_with(m) {
                let depois = l.split_once(m).map(|(_, d)| d).unwrap_or("");
                if !lista.is_match(depois) {
                    entradas.push((i, m));
                }
            }
        }
    }
    let base: &[String] = match entradas.last() {
        Some((i, _)) => &linhas[i + 1..],
        None => linhas,
    };
    let fim: Vec<&String> = {
        let v: Vec<&String> = base.iter().filter(|l| !l.trim().is_empty()).collect();
        let n = v.len();
        v.into_iter().skip(n.saturating_sub(10)).collect()
    };
    let baixo = fim.iter().map(|l| l.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    let mut fim_conversa = entradas.last().map(|(i, _)| *i).unwrap_or(0);
    let mut fila_codex = false;
    if *prog == Programa::Codex {
        let filas: Vec<usize> = linhas[..fim_conversa]
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("• Messages to be submitted after next tool call"))
            .map(|(i, _)| i)
            .collect();
        if let Some(&f) = filas.last() {
            if linhas[f + 1..fim_conversa].iter().all(|l| l.trim().is_empty() || l.starts_with(char::is_whitespace)) {
                // O rodapé tem mensagens DO USUÁRIO na fila, não uma resposta.
                fila_codex = true;
                fim_conversa = f;
            }
        }
    }
    let mut acima: Vec<String> = linhas[..fim_conversa]
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with(&"─".repeat(12)))
        .collect();
    while *prog == Programa::Codex && acima.last().is_some_and(|l| l.starts_with("└ ")) {
        acima.pop();
    }
    let perguntas =
        ["esc to cancel", "enter to confirm", "enter continue", "esc back", "enter to submit", "enter submit", "trust this folder"];
    let fim_junto = fim.iter().map(|l| l.as_str()).collect::<Vec<_>>().join(" ");
    if perguntas.iter().any(|p| baixo.contains(p)) || cursor_em_lista.is_match(&fim_junto) {
        return (Estado::Dialogo, "com uma pergunta aberta".into());
    }
    if fila_codex
        || baixo.contains("esc to interrupt")
        || acima.last().is_some_and(|l| l.to_lowercase().contains("esc to interrupt"))
    {
        return (Estado::Ocupada, "trabalhando".into());
    }
    if *prog == Programa::Codex
        && acima.last().is_some_and(|l| l.ends_with('?') && !l.starts_with(['>', '$', '↳', '›', '»']))
    {
        let mensagens: Vec<usize> =
            linhas[..fim_conversa].iter().enumerate().filter(|(_, l)| l.starts_with("• ")).map(|(i, _)| i).collect();
        let usuario = linhas[..fim_conversa]
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with('›') || l.starts_with('»'))
            .map(|(i, _)| i as isize)
            .max()
            .unwrap_or(-1);
        if let Some(&ultima) = mensagens.last() {
            let cercas = linhas[ultima..fim_conversa].iter().filter(|l| l.trim().starts_with("```")).count();
            if ultima as isize > usuario && cercas % 2 == 0 {
                return (Estado::Dialogo, "com uma pergunta aberta".into());
            }
        }
    }
    let Some(&(i, marca)) = entradas.last() else {
        return (Estado::Ocupada, "sem a caixa de texto à vista".into());
    };
    let digitado = limpas
        .get(i)
        .and_then(|l| l.split_once(marca).map(|(_, d)| tira(d, &[' ', '│', '\t'])))
        .unwrap_or_default();
    if !digitado.is_empty() {
        let curto: String = digitado.chars().take(30).collect();
        return (Estado::Digitado, format!("há texto na caixa (“{curto}”)"));
    }
    (Estado::Livre, String::new())
}

// ------------------------------------------------- trabalho em segundo plano

/// O que o rodapé do Claude conta em segundo plano ("2 shells"), nas três
/// últimas linhas com texto. O "← for agents" aparece sempre e não conta.
pub fn segundo_plano_no_rodape(linhas: &[String]) -> Vec<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let pilula = re(
        r"(?i)(?:^|·)\s*(\d+)\s+((?:background\s+)?(?:shell|monitor|agent|subagent|task|workflow|teammate|job)s?)\b",
        &R,
    );
    let com_texto: Vec<&String> = linhas.iter().filter(|l| !l.trim().is_empty()).collect();
    let mut achados = Vec::new();
    for l in com_texto.iter().skip(com_texto.len().saturating_sub(3)) {
        let l = l.replace('\u{a0}', " ");
        for c in pilula.captures_iter(&l) {
            if c[1].parse::<u32>().unwrap_or(0) > 0 {
                achados.push(format!("{} {}", &c[1], &c[2]));
            }
        }
    }
    achados
}

/// A pergunta que o `/exit` abre quando há trabalho em segundo plano:
/// "Background work is running … ❯ 1. Exit and stop tasks / 2. Move to
/// background and exit / 3. Stay". Os números são lidos da tela.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerguntaDeSaida {
    pub tarefas: Vec<String>,
    pub sair: Option<String>,
    pub ficar: Option<String>,
}

pub fn pergunta_de_saida(linhas: &[String]) -> Option<PerguntaDeSaida> {
    static R: OnceLock<Regex> = OnceLock::new();
    let opcao = re(r"^(\d)\.\s+(.+)$", &R);
    let inicio = linhas.iter().rposition(|l| l.contains("Background work is running"))?;
    let mut tarefas = Vec::new();
    let mut opcoes: Vec<(String, String)> = Vec::new();
    let mut lendo = false;
    for l in &linhas[inicio + 1..] {
        let s = l.replace('\u{a0}', " ");
        let s = s.trim().trim_start_matches(['❯', '›', '>']).trim().to_string();
        if let Some(c) = opcao.captures(&s) {
            opcoes.push((c[2].trim().to_lowercase(), c[1].to_string()));
            lendo = false;
        } else if s.to_lowercase().contains("will stop when you exit") {
            lendo = true;
        } else if lendo && !s.is_empty() {
            tarefas.push(s);
        }
    }
    let acha = |prefixo: &str| opcoes.iter().find(|(t, _)| t.starts_with(prefixo)).map(|(_, n)| n.clone());
    Some(PerguntaDeSaida { tarefas, sair: acha("exit and stop"), ficar: acha("stay") })
}

/// Os jobs que a tela diz terem ido para segundo plano ("backgrounded · 7fb66140").
pub fn jobs_na_tela(linhas: &[String]) -> Vec<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let job = re(r"backgrounded\s+·\s+([0-9a-f]{6,})", &R);
    let texto = linhas[linhas.len().saturating_sub(40)..].join(" ");
    job.captures_iter(&texto).map(|c| c[1].to_string()).collect()
}

const UUID: &str = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";

pub fn e_uuid(s: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"^(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", &R).is_match(s)
}

/// O id que o Claude (`claude --resume <id>`) ou o Codex (`codex resume
/// <id>`) imprime ao sair, o último das 30 linhas de baixo.
pub fn id_na_tela(linhas: &[String], agente: &str) -> Option<String> {
    static C: OnceLock<Regex> = OnceLock::new();
    static X: OnceLock<Regex> = OnceLock::new();
    let r = if agente == "claude" {
        C.get_or_init(|| Regex::new(&format!(r"claude(?:\s+--worktree\s+\S+)?\s+--resume\s+({UUID})")).unwrap())
    } else {
        X.get_or_init(|| Regex::new(&format!(r"codex\s+resume\s+({UUID})")).unwrap())
    };
    let texto = linhas[linhas.len().saturating_sub(30)..].join(" ");
    r.captures_iter(&texto).last().map(|c| c[1].to_string())
}

// ----------------------------------------------------------------- limite

/// O diálogo do Claude no limite: "What do you want to do? ❯ 1. Stop and wait
/// for limit to reset … Esc to cancel".
pub fn dialogo_limite(linhas: &[String]) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let parar = re(r"^\s*(?:❯\s*)?1\.\s+Stop(?: and wait for limit to reset)?\s*$", &R);
    let com_texto: Vec<&String> = linhas.iter().filter(|l| !l.trim().is_empty()).collect();
    let fim = &com_texto[com_texto.len().saturating_sub(9)..];
    fim.iter().any(|l| l.contains("What do you want to do?"))
        && fim.iter().any(|l| parar.is_match(l))
        && fim.last().is_some_and(|l| l.contains("Esc to cancel"))
}

/// O Claude esperando o limite passar, no diálogo ou no rodapé ("⚠ Usage
/// limit reached … continuing automatically … esc to cancel").
pub fn espera_limite(linhas: &[String]) -> bool {
    if dialogo_limite(linhas) {
        return true;
    }
    let com_texto: Vec<String> = linhas.iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    let fim = &com_texto[com_texto.len().saturating_sub(4)..];
    let texto = fim.join(" ").to_lowercase();
    fim.iter().any(|l| l.to_lowercase().starts_with("⚠ usage limit reached"))
        && texto.contains("esc to cancel")
        && texto.contains("continuing automatically")
}

/// Os avisos de limite do CLI no rodapé (nunca uma menção qualquer a
/// "limit" na conversa).
pub fn limite_na_tela(linhas: &[String]) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    let aviso = re(
        r"^\s*(?:[■⎿●⚠]\s*)?(?:You've hit your (?:usage|session|weekly) limit|Usage limit reached)\b",
        &R,
    );
    linhas[linhas.len().saturating_sub(12)..].iter().any(|l| aviso.is_match(l)) || espera_limite(linhas)
}

/// Uma impressão das últimas linhas, para não contar a mesma tela de
/// limite duas vezes.
pub fn impressao(linhas: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let fim = linhas[linhas.len().saturating_sub(12)..].join("\n");
    Sha256::digest(fim.as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn l(texto: &str) -> Vec<String> {
        texto.lines().map(str::to_string).collect()
    }

    #[test]
    fn programas_pelo_nome() {
        assert_eq!(programa("2.1.289"), Programa::Claude);
        assert_eq!(programa("claude"), Programa::Claude);
        assert_eq!(programa("Codex.exe"), Programa::Codex);
        assert_eq!(programa("-zsh"), Programa::Shell("zsh".into()));
        assert_eq!(programa("pwsh"), Programa::Shell("pwsh".into()));
        assert_eq!(programa("vim"), Programa::Outro("vim".into()));
        assert_eq!(programa(""), Programa::Desconhecido);
    }

    #[test]
    fn o_vt_vira_linhas_com_o_sgr() {
        let vt = "\x1b[?1049h\x1b[>4m\x1b[1mOi\x1b[0m\r\n\x1b]0;título\x07linha 2\x1b[37;3H\x1b[=5;1u";
        assert_eq!(linhas_vt(vt), ["\x1b[1mOi\x1b[0m", "linha 2"]);
    }

    #[test]
    fn o_esmaecido_some() {
        assert_eq!(sem_esmaecido("❯ \x1b[2mTry \"fix\"\x1b[0m"), "❯ ");
        assert_eq!(sem_esmaecido("❯ \x1b[1mfeito\x1b[22m!"), "❯ feito!");
    }

    #[test]
    fn claude_livre_trabalhando_com_pergunta_e_digitado() {
        let p = Programa::Claude;
        let livre = l("● Pronto.\n\n────────────────\n❯ \n────────────────\n  ⏵⏵ bypass permissions on");
        assert_eq!(situacao(&p, &livre, &livre).0, Estado::Livre);
        let ocupada = l("✻ Pensando… (esc to interrupt)\n────────────────────\n❯ \n────────────────────");
        assert_eq!(situacao(&p, &ocupada, &ocupada).0, Estado::Ocupada);
        let dialogo = l("Do you want to proceed?\n❯ 1. Yes\n  2. No\n\nEsc to cancel");
        assert_eq!(situacao(&p, &dialogo, &dialogo).0, Estado::Dialogo);
        let digitado = l("────\n❯ apaga tudo\n────");
        assert_eq!(situacao(&p, &digitado, &digitado).0, Estado::Digitado);
        // a sugestão esmaecida não é texto digitado
        let (linhas, limpas) = as_duas("────\r\n❯ \x1b[2mTry \"refactor\"\x1b[0m\r\n────");
        assert_eq!(situacao(&p, &linhas, &limpas).0, Estado::Livre);
    }

    #[test]
    fn codex_com_mensagens_na_fila_esta_trabalhando() {
        let p = Programa::Codex;
        let t = l("• Messages to be submitted after next tool call\n  ↳ e depois?\n\n› \n");
        assert_eq!(situacao(&p, &t, &t).0, Estado::Ocupada);
    }

    #[test]
    fn segundo_plano_e_a_pergunta_do_exit() {
        let rodape = l("❯ \n  ⏵⏵ bypass permissions on · 2 shells · ← for agents · ↓ to manage");
        assert_eq!(segundo_plano_no_rodape(&rodape), ["2 shells"]);
        let pergunta = l(
            "Background work is running\nThe following will stop when you exit:\nshell · sleep 900\n❯ 1. Exit and stop tasks\n  2. Move to background and exit\n  3. Stay\nEnter to confirm · Esc to cancel",
        );
        let p = pergunta_de_saida(&pergunta).unwrap();
        assert_eq!(p.tarefas, ["shell · sleep 900"]);
        assert_eq!(p.sair.as_deref(), Some("1"));
        assert_eq!(p.ficar.as_deref(), Some("3"));
        assert_eq!(jobs_na_tela(&l("⎿ backgrounded · 7fb66140")), ["7fb66140"]);
    }

    #[test]
    fn o_id_que_os_clis_imprimem_ao_sair() {
        let t = l("Resume this session with:\nclaude --resume 9e2fff2d-f449-4d5f-998f-1b3495689c61\n$ ");
        assert_eq!(id_na_tela(&t, "claude").as_deref(), Some("9e2fff2d-f449-4d5f-998f-1b3495689c61"));
        let t = l("To continue this session, run codex resume 01a103e9-cc76-75b1-b919-7702b666146a");
        assert_eq!(id_na_tela(&t, "codex").as_deref(), Some("01a103e9-cc76-75b1-b919-7702b666146a"));
        assert!(e_uuid("01a103e9-cc76-75b1-b919-7702b666146a"));
    }

    #[test]
    fn as_telas_de_limite() {
        let d = l("You've hit your limit\nWhat do you want to do?\n❯ 1. Stop and wait for limit to reset\n  2. Upgrade\nEsc to cancel");
        assert!(dialogo_limite(&d) && espera_limite(&d) && limite_na_tela(&d));
        let r = l("● ok\n⚠ Usage limit reached · resets 3am\ncontinuing automatically · esc to cancel");
        assert!(espera_limite(&r));
        assert!(!limite_na_tela(&l("● falamos do limit da conta\n❯ ")));
    }
}
