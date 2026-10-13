//! `keep continuar`: a conversa do Claude que parou numa queda recebe
//! "continue" quando a causa passa.
//!
//! Quando o login cai no meio de um trabalho, a chamada à API volta "Not logged
//! in · Please run /login" e a conversa PARA ali; o login volta em segundos,
//! mas a aba fica parada até alguém digitar (em 23/09/2026 uma ficou 2h43
//! assim). O mesmo na queda de REDE (o Claude Code tenta 10 vezes e grava
//! `server_error`: "API Error: Connection lost…", "Request timed out") e no
//! LIMITE DE USO ("You've hit your session limit · resets 5:10pm"). O app roda
//! isto a cada 15 s.
//!
//! Quem diz que a aba parou é o TRANSCRITO: a última fala da conversa
//! (user/assistant, fora de subagente, sem comando local) é a fala sintética de
//! erro que o Claude Code grava, e ela aconteceu depois que este processo
//! abriu. A tela só confirma que é seguro digitar: a caixa vazia (ou já com a
//! mensagem, faltando o Enter), nada rodando, nenhuma tarefa do rodapé
//! selecionada e o erro à vista. Com a vista desviada (modo transcrito, painel
//! de agentes, diálogo do limite, conversa rolada para cima), volta à conversa
//! só se a pessoa está longe do teclado há 2 min.
//!
//! A porta de cada causa: login — a credencial da aba (a global, ou a da pasta
//! fixa dela) tem acesso válido; rede — a API responde; limite — a conta da aba
//! saiu do limite (a medição do rodapé "Consumo de IA"). No máximo 3 vezes
//! seguidas sem a conversa andar, com 2 min entre elas; a contagem recomeça
//! quando a conversa anda ou depois de 1 h. A mensagem tem de aparecer no
//! transcrito para contar como entregue.
//!
//! Era o `cc-continuar` do kit-mac (backend Keep), que passou para cá em
//! 13/10/2026.
//!
//!   keep continuar             varre e destrava (o app, a cada 15 s)
//!   keep continuar --seco      diz o que faria, sem tocar em aba nenhuma
//!   keep continuar --status    o que vê em cada conversa aberta (só leitura)

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use keep_ia::daemon;

// ------------------------------------------------------------------ regras

/// O erro precisa ter ao menos esta idade: dá ao Claude Code o tempo de reler a credencial reposta.
const IDADE_MIN_MS: u64 = 5_000;
/// Sobrecarga, 5xx: a rede está boa, mas vale um respiro antes de pedir de novo.
const IDADE_SERVIDOR_MS: u64 = 60_000;
/// Mexer na vista da aba só com a pessoa longe do teclado há este tempo.
const OCIOSO_PARA_MEXER_S: f64 = 120.0;
const ESPERA_MS: u64 = 120_000;
const MAX_SEGUIDAS: u32 = 3;
const RECOMECA_MS: u64 = 60 * 60_000;
const ESQUECE_MS: u64 = 60 * 60_000;
/// A mensagem tem este prazo para aparecer no transcrito. Um gancho UserPromptSubmit pode segurá-la antes de gravar
/// (em 13/10/2026, no teste real, o do Jev rodou um /effort antes e ela entrou 11 s depois do Enter).
const CONFIRMA: Duration = Duration::from_secs(30);
const BLOCO: u64 = 64 << 10;
const TETO_LEITURA: u64 = 16 << 20;

pub const MSG_LOGIN: &str = "continue: o login caiu e já voltou sozinho; siga exatamente de onde parou.";
pub const MSG_REDE: &str = "continue: a conexão com a API caiu e já voltou; siga exatamente de onde parou.";
pub const MSG_LIMITE: &str =
    "continue: o limite de uso já liberou (ou a conta com folga está ativa); siga exatamente de onde parou.";

const COMANDO_LOCAL: &[&str] = &[
    "<command-name>",
    "<command-message>",
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<local-command-caveat>",
    "<bash-input>",
    "<bash-stdout>",
    "<bash-stderr>",
];
const MARCAS_DE_LOGIN: &[&str] = &["Please run /login", "Not logged in", "Login expired", "Could not refresh your login"];

fn re(padrao: &'static str, guarda: &'static OnceLock<Regex>) -> &'static Regex {
    guarda.get_or_init(|| Regex::new(padrao).expect("regex válida"))
}

fn erro_de_login(t: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"Not logged in|Login expired|OAuth (?:access )?token|Please run /login", &R).is_match(t)
}
fn erro_de_trava(t: &str) -> bool {
    t.contains("Could not refresh your login")
}
fn erro_de_config(t: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"(?i)Invalid API key|won't change|disabled API key|trusted device|Credit balance", &R).is_match(t)
}
fn erro_de_rede(t: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"^(?:API Error|Request timed out)", &R).is_match(t.trim())
}
fn erro_de_servidor(t: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(r"(?i)\b(?:5\d\d|Overloaded|Internal server error|Server error)\b", &R).is_match(t)
}
fn erro_de_limite(t: &str) -> bool {
    t.trim().starts_with("You've hit your ") && t.contains("limit")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tipo {
    Login,
    Rede,
    Limite,
}

impl Tipo {
    pub fn mensagem(self) -> &'static str {
        match self {
            Tipo::Login => MSG_LOGIN,
            Tipo::Rede => MSG_REDE,
            Tipo::Limite => MSG_LIMITE,
        }
    }
    fn nome(self) -> &'static str {
        match self {
            Tipo::Login => "o login",
            Tipo::Rede => "a rede",
            Tipo::Limite => "o limite de uso",
        }
    }
}

/// O erro em que a conversa parou.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Erro {
    pub uuid: String,
    pub quando_ms: u64,
    pub texto: String,
    pub tipo: Tipo,
}

// ------------------------------------------------------------------ transcrito

fn texto(d: &Value) -> String {
    match d.get("message").and_then(|m| m.get("content")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(itens)) => {
            itens.iter().filter_map(|i| i.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join(" ")
        }
        _ => String::new(),
    }
}

fn tem(d: &Value, tipo: &str) -> bool {
    d.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .is_some_and(|c| c.iter().any(|i| i.get("type").and_then(Value::as_str) == Some(tipo)))
}

fn carimbo_ms(d: &Value) -> u64 {
    d.get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp_millis().max(0) as u64)
        .unwrap_or(0)
}

/// As falas da conversa principal, da última para a primeira (`continued-in` também: o fim deste transcrito
/// deixou de ser o fim da conversa). Lê o arquivo de trás para a frente, em blocos, e para quando `f` diz.
pub fn falas_de_tras(caminho: &Path, mut f: impl FnMut(&Value) -> bool) {
    let Ok(mut fh) = std::fs::File::open(caminho) else { return };
    let Ok(tamanho) = fh.seek(SeekFrom::End(0)) else { return };
    let mut pos = tamanho;
    let mut resto: Vec<u8> = Vec::new();
    let mut lidos = 0u64;
    let mut visita = |linha: &[u8]| -> bool {
        let tem_tipo = |t: &[u8]| linha.windows(t.len()).any(|w| w == t);
        if !tem_tipo(b"\"type\":\"user\"") && !tem_tipo(b"\"type\":\"assistant\"") && !tem_tipo(b"\"type\":\"continued-in\"") {
            return true;
        }
        let Ok(d) = serde_json::from_slice::<Value>(linha) else { return true };
        let tipo = d.get("type").and_then(Value::as_str).unwrap_or("");
        if tipo == "continued-in" {
            return f(&d);
        }
        if !matches!(tipo, "user" | "assistant") {
            return true;
        }
        if d.get("isSidechain").and_then(Value::as_bool) == Some(true) || d.get("isMeta").and_then(Value::as_bool) == Some(true) {
            return true;
        }
        if tipo == "user" {
            let t = texto(&d);
            let t = t.trim_start();
            if COMANDO_LOCAL.iter().any(|c| t.starts_with(c)) {
                return true;
            }
        }
        f(&d)
    };
    while pos > 0 && lidos < TETO_LEITURA {
        let n = BLOCO.min(pos);
        pos -= n;
        lidos += n;
        let mut bloco = vec![0u8; n as usize];
        if fh.seek(SeekFrom::Start(pos)).is_err() || fh.read_exact(&mut bloco).is_err() {
            return;
        }
        bloco.extend_from_slice(&resto);
        let mut partes: Vec<&[u8]> = bloco.split(|b| *b == b'\n').collect();
        // a primeira parte pode ser o fim de uma linha que começa no bloco anterior
        let primeira = if pos > 0 { partes.remove(0).to_vec() } else { Vec::new() };
        for linha in partes.iter().rev() {
            if !linha.is_empty() && !visita(linha) {
                return;
            }
        }
        resto = primeira;
    }
    if pos == 0 && !resto.is_empty() {
        visita(&resto);
    }
}

/// O .jsonl da conversa (onde ela nasceu: um `--resume` de outra pasta continua gravando no original).
pub fn transcrito(sid: &str) -> Option<PathBuf> {
    keep_ia::sessoes::transcritos_claude(sid)
        .into_iter()
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// A conversa parou num erro que isto conserta? `inicio_ms`: quando o processo abriu (o erro anterior a uma
/// reabertura é de quem reabriu decidir).
pub fn avaliar(caminho: &Path, inicio_ms: u64) -> Result<Erro, String> {
    let mut ultima = None;
    falas_de_tras(caminho, |d| {
        ultima = Some(d.clone());
        false
    });
    let d = ultima.ok_or("sem fala no transcrito")?;
    if d.get("type").and_then(Value::as_str) == Some("continued-in") {
        return Err("a conversa seguiu em outra sessão".into());
    }
    let msg = texto(&d);
    let erro = d.get("error").and_then(Value::as_str).unwrap_or("");
    let tipo_fala = d.get("type").and_then(Value::as_str).unwrap_or("");
    if tipo_fala != "assistant" || !matches!(erro, "authentication_failed" | "server_error" | "rate_limit") {
        return Err(format!("última fala: {tipo_fala}{}", if erro.is_empty() { String::new() } else { format!(" ({erro})") }));
    }
    let tipo = match erro {
        "server_error" if erro_de_trava(&msg) => Tipo::Login,
        "server_error" if erro_de_rede(&msg) => Tipo::Rede,
        "server_error" => return Err(format!("erro de servidor desconhecido ({:?})", corta(&msg, 50))),
        "rate_limit" if erro_de_limite(&msg) => Tipo::Limite,
        "rate_limit" => return Err(format!("limite desconhecido ({:?})", corta(&msg, 50))),
        _ if erro_de_config(&msg) || !erro_de_login(&msg) => {
            return Err(format!("erro de autenticação que login não cura ({:?})", corta(&msg, 50)))
        }
        _ => Tipo::Login,
    };
    let quando_ms = carimbo_ms(&d);
    if quando_ms < inicio_ms {
        return Err("erro anterior a esta abertura".into());
    }
    let uuid = d.get("uuid").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| quando_ms.to_string());
    Ok(Erro { uuid, quando_ms, texto: msg, tipo })
}

fn corta(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn compacto(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Depois de `desde_ms` a conversa andou DE VERDADE (ferramenta executada, pedido da pessoa, texto do modelo
/// seguido de fala sem erro)? O pedaço que o Claude Code grava quando a conexão cai no meio da resposta não conta.
pub fn houve_progresso(caminho: &Path, desde_ms: u64) -> bool {
    let nossas: Vec<String> = [MSG_LOGIN, MSG_REDE, MSG_LIMITE].iter().map(|m| compacto(m)).collect();
    let mut depois: Option<Value> = None;
    let mut andou = false;
    falas_de_tras(caminho, |d| {
        let quando = carimbo_ms(d);
        if quando != 0 && quando < desde_ms {
            return false;
        }
        match d.get("type").and_then(Value::as_str) {
            Some("user") => {
                if tem(d, "tool_result") {
                    andou = true;
                    return false;
                }
                let t = texto(d);
                let t = t.trim();
                if !t.is_empty() && !t.starts_with('<') && !nossas.contains(&compacto(t)) {
                    andou = true;
                    return false;
                }
            }
            Some("assistant") => {
                let sem_erro = |x: &Value| x.get("error").is_none_or(Value::is_null);
                if sem_erro(d) && tem(d, "text") && depois.as_ref().is_some_and(sem_erro) {
                    andou = true;
                    return false;
                }
            }
            _ => {}
        }
        depois = Some(d.clone());
        true
    });
    andou
}

/// A mensagem virou fala da pessoa (mesmo que o modelo já tenha respondido depois)?
pub fn chegou(caminho: &Path, desde_ms: u64, mensagem: &str) -> bool {
    let alvo = compacto(mensagem);
    let mut achou = false;
    falas_de_tras(caminho, |d| {
        let quando = carimbo_ms(d);
        if quando != 0 && quando + 2000 < desde_ms {
            return false;
        }
        if d.get("type").and_then(Value::as_str) == Some("user") && compacto(&texto(d)).contains(&alvo) {
            achou = true;
            return false;
        }
        true
    });
    achou
}

// ------------------------------------------------------------------ tela

/// A tela da aba: as linhas sem cor, a linha da caixa (❯), o que está digitado nela (sem a sugestão
/// esmaecida, que não é texto de ninguém) e onde começa o rodapé.
#[derive(Clone, Debug, Default)]
pub struct Tela {
    pub linhas: Vec<String>,
    pub caixa: Option<usize>,
    pub digitado: String,
    pub rodape: usize,
}

pub fn ler_tela(vt: &str) -> Tela {
    static BORDA: OnceLock<Regex> = OnceLock::new();
    static ESPACOS: OnceLock<Regex> = OnceLock::new();
    let (linhas, limpas) = keep_ia::tela::as_duas(vt);
    let Some(i) = linhas.iter().rposition(|l| l.contains('❯')) else {
        let n = linhas.len();
        return Tela { linhas, caixa: None, digitado: String::new(), rodape: n };
    };
    let borda = re(r"^\s*[─━╰╭└┌]", &BORDA);
    let mut partes = vec![limpas.get(i).map(|l| l.split_once('❯').map(|(_, d)| d.to_string()).unwrap_or_default()).unwrap_or_default()];
    let mut fim = i;
    while fim + 1 < linhas.len() && !borda.is_match(&linhas[fim + 1]) {
        fim += 1;
        partes.push(limpas.get(fim).cloned().unwrap_or_default());
    }
    let digitado = re(r"[\s│]+", &ESPACOS).replace_all(&partes.join(" "), " ").trim().to_string();
    Tela { linhas, caixa: Some(i), digitado, rodape: fim + 1 }
}

/// A vista não mostra a conversa ao vivo: (o que é, as teclas que voltam a ela).
pub fn vista_desviada(t: &Tela) -> Option<(&'static str, &'static [u8])> {
    let cheias: Vec<&String> = t.linhas.iter().filter(|l| !l.trim().is_empty()).collect();
    let base = &cheias[cheias.len().saturating_sub(4)..];
    if base.iter().any(|l| l.contains("Showing detailed transcript")) {
        return Some(("no modo transcrito (ctrl+o)", b"\x0f"));
    }
    if dialogo_do_limite(&cheias[cheias.len().saturating_sub(9)..]) {
        return Some(("no diálogo do limite de uso", b"\x1b"));
    }
    if base.iter().any(|l| l.contains("enter to return") && l.contains("space to reply")) {
        return Some(("no painel de agentes", b"\x1b"));
    }
    if let Some(i) = t.caixa {
        static ROLADA: OnceLock<Regex> = OnceLock::new();
        let acima = t.linhas[i.saturating_sub(4)..i].join("\n");
        if re(r"new messages? \(ctrl\+End\)|Jump to bottom", &ROLADA).is_match(&acima) {
            return Some(("rolada para cima dentro do Claude", b"\x1b[1;5F"));
        }
    }
    None
}

/// O menu do limite de uso ("What do you want to do?" / "1. Stop…" / "Esc to cancel"), os três juntos.
fn dialogo_do_limite(ultimas: &[&String]) -> bool {
    static PARAR: OnceLock<Regex> = OnceLock::new();
    let parar = re(r"^\s*(?:❯\s*)?1\.\s+Stop(?: and wait for limit to reset)?\s*$", &PARAR);
    ultimas.iter().any(|l| l.contains("What do you want to do?"))
        && ultimas.iter().any(|l| parar.is_match(l))
        && ultimas.last().is_some_and(|l| l.contains("Esc to cancel"))
}

/// Pode digitar? (sim, por quê, só falta o Enter).
pub fn tela_segura(t: &Tela, erro: &Erro) -> (bool, String, bool) {
    let Some(caixa) = t.caixa else { return (false, "sem caixa de entrada à vista".into(), false) };
    let rodape = t.linhas[t.rodape.min(t.linhas.len())..].join("\n");
    if rodape.contains("esc to interrupt") {
        return (false, "ocupada (esc to interrupt)".into(), false);
    }
    if rodape.contains("to view tasks") {
        return (false, "tarefa do rodapé selecionada".into(), false);
    }
    let acima = compacto(&t.linhas[..caixa].join("\n"));
    let mut marcas = vec![corta(erro.texto.trim(), 24)];
    if erro.tipo == Tipo::Login {
        marcas.extend(MARCAS_DE_LOGIN.iter().map(|m| m.to_string()));
    }
    if !marcas.iter().any(|m| !m.is_empty() && acima.contains(&compacto(m))) {
        return (false, "o erro não aparece na tela".into(), false);
    }
    if !t.digitado.is_empty() {
        if compacto(&t.digitado) == compacto(erro.tipo.mensagem()) {
            return (true, "a mensagem já está na caixa, falta o Enter".into(), true);
        }
        return (false, format!("caixa com texto ({:?})", corta(&t.digitado, 30)), false);
    }
    (true, "ociosa, caixa vazia, erro à vista".into(), false)
}

/// Segundos sem teclado nem mouse (no Mac, o HIDIdleTime; no Windows, o `GetLastInputInfo`). Sem leitura, 0: na
/// dúvida, a pessoa está por perto.
pub fn ocioso_ha() -> f64 {
    if let Some(v) = std::env::var("KEEP_CONTINUAR_OCIOSO_S").ok().and_then(|v| v.parse().ok()) {
        return v;
    }
    ocioso_do_sistema()
}

#[cfg(target_os = "macos")]
fn ocioso_do_sistema() -> f64 {
    let Ok(saida) = std::process::Command::new("/usr/sbin/ioreg").args(["-c", "IOHIDSystem", "-d", "4", "-r", "-k", "HIDIdleTime"]).output()
    else {
        return 0.0;
    };
    static R: OnceLock<Regex> = OnceLock::new();
    re(r#""HIDIdleTime"\s*=\s*(\d+)"#, &R)
        .captures(&String::from_utf8_lossy(&saida.stdout))
        .and_then(|c| c[1].parse::<f64>().ok())
        .map(|ns| ns / 1e9)
        .unwrap_or(0.0)
}

/// O Windows guarda quando foi a última entrada da sessão da pessoa (teclado, mouse) no mesmo relógio de `GetTickCount`.
#[cfg(windows)]
fn ocioso_do_sistema() -> f64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` tem o tamanho dito em `cbSize`, como a chamada pede; `GetTickCount` não recebe nada.
    unsafe {
        if GetLastInputInfo(&mut info) == 0 {
            return 0.0;
        }
        ocioso_de_ticks(GetTickCount(), info.dwTime)
    }
}

/// Sem leitura do sistema (Linux): na dúvida, a pessoa está por perto.
#[cfg(not(any(target_os = "macos", windows)))]
fn ocioso_do_sistema() -> f64 {
    0.0
}

/// Os segundos entre a última entrada e agora, dados os dois relógios do Windows (ms desde que ele ligou, em 32
/// bits: voltam a zero a cada 49,7 dias, e a subtração em ciclo acerta a conta nessa virada).
#[cfg_attr(not(windows), allow(dead_code))]
fn ocioso_de_ticks(agora: u32, ultimo: u32) -> f64 {
    f64::from(agora.wrapping_sub(ultimo)) / 1000.0
}

// ------------------------------------------------------------------ portas

/// O login da aba voltou: acesso e renovação presentes, acesso dentro do prazo.
fn credencial_boa(fixa: Option<&str>) -> (bool, String) {
    use keep_ia::contas::credencial::{self, Lido, Oauth};
    let lido = match fixa {
        Some(p) => credencial::fixa(Path::new(p)),
        None => credencial::global(),
    };
    let Lido::Ok(v) = lido else { return (false, "credencial ilegível ou ausente".into()) };
    let Some(o) = Oauth::de(&v) else { return (false, "credencial zerada".into()) };
    if o.acesso.is_none() || o.refresh.is_none() {
        return (false, "credencial zerada".into());
    }
    let falta = o.expira_em.unwrap_or(0) as i64 - crate::agora_ms() as i64;
    if falta <= 0 {
        return (false, "acesso expirado".into());
    }
    (true, format!("ok (expira em {} min)", falta / 60_000))
}

/// A API responde? Qualquer resposta HTTP prova DNS, conexão e TLS.
fn rede_de_volta() -> (bool, String) {
    let url = std::env::var("KEEP_CONTINUAR_SONDA").unwrap_or_else(|_| "https://api.anthropic.com/api/hello".into());
    match keep_ia::rede::get(&url, &[("User-Agent", "keep-continuar".to_string())], Duration::from_secs(10)) {
        Ok(r) => (true, format!("a API respondeu (HTTP {})", r.status)),
        Err(e) => (false, format!("a API não responde ({e})")),
    }
}

/// A conta da aba saiu do limite? Pela medição do rodapé "Consumo de IA" (a conta global, ou a da pasta fixa).
fn cota_livre(fixa: Option<&str>) -> (bool, String) {
    let linhas = keep_ia::uso::medir(false, None);
    let da_aba = linhas.iter().find(|l| match fixa {
        Some(p) => l.account.fixa().is_some_and(|f| f.to_string_lossy() == p),
        None => l.account.global(),
    });
    match da_aba {
        Some(l) if l.disponivel() => (true, "a conta da aba tem folga".into()),
        Some(_) => (false, "a conta da aba segue no limite".into()),
        None => (false, "não sei qual conta a aba usa".into()),
    }
}

// ------------------------------------------------------------------ conversas abertas

#[derive(Clone, Debug)]
pub struct Conversa {
    pub ws: String,
    pub tab: u32,
    pub pid: u32,
    pub sid: String,
    pub inicio_ms: u64,
    pub fixa: Option<String>,
    pub rotulo: String,
}

fn sessoes_brutas() -> HashMap<u32, Value> {
    let pasta = keep_ia::caminhos::claude().join("sessions");
    let mut saida = HashMap::new();
    for e in std::fs::read_dir(&pasta).into_iter().flatten().flatten() {
        let Ok(v) = serde_json::from_slice::<Value>(&std::fs::read(e.path()).unwrap_or_default()) else { continue };
        if let Some(pid) = v.get("pid").and_then(Value::as_u64) {
            saida.insert(pid as u32, v);
        }
    }
    saida
}

/// As conversas do Claude abertas em abas do Keep, cada uma com a sessão que a aba mostra (a estacionada em
/// segundo plano mostra a do processo `bg`).
pub fn conversas(lista: &daemon::Retrato) -> (Vec<Conversa>, Vec<(String, String)>) {
    let contas = keep_ia::contas::listar(false);
    let brutas = sessoes_brutas();
    let fundo: HashMap<String, &Value> = brutas
        .values()
        .filter(|v| v.get("kind").and_then(Value::as_str) == Some("bg"))
        .filter_map(|v| Some((v.get("jobId")?.as_str()?.to_string(), v)))
        .collect();
    let (mut validas, mut ignoradas) = (Vec::new(), Vec::new());
    for ia in keep_ia::abas::das_abas(lista, &contas) {
        let rotulo = format!("{}#{}", ia.workspace, ia.aba);
        if ia.agente != "claude" {
            continue;
        }
        if ia.vinculo != "exato" {
            ignoradas.push((rotulo, "aba sem vínculo exato (keepd antigo): não digito".into()));
            continue;
        }
        let Some(sessao) = keep_ia::sessoes::sessao_claude(ia.pid) else {
            ignoradas.push((rotulo, "sem a sessão do Claude (ou pid reaproveitado)".into()));
            continue;
        };
        if !matches!(sessao.tipo.as_str(), "" | "interactive") {
            continue;
        }
        let bruta = brutas.get(&ia.pid);
        let mut sid = sessao.id.clone();
        let mut inicio = bruta.and_then(|v| v.get("startedAt")).and_then(Value::as_u64).unwrap_or(0);
        if let Some(job) = bruta.and_then(|v| v.get("parkedJobId")).and_then(Value::as_str) {
            let Some(bg) = fundo.get(job) else {
                ignoradas.push((rotulo, format!("estacionada em segundo plano ({job}) sem a sessão bg")));
                continue;
            };
            let Some(bsid) = bg.get("sessionId").and_then(Value::as_str) else { continue };
            sid = bsid.to_string();
            inicio = bg.get("startedAt").and_then(Value::as_u64).unwrap_or(0);
        }
        let fixa = keep_ia::processos::variavel(ia.pid, "CLAUDE_SECURESTORAGE_CONFIG_DIR").filter(|v| !v.is_empty());
        validas.push(Conversa { ws: ia.workspace, tab: ia.aba, pid: ia.pid, sid, inicio_ms: inicio, fixa, rotulo });
    }
    (validas, ignoradas)
}

// ------------------------------------------------------------------ estado e registro

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Registro {
    #[serde(default)]
    erro: String,
    #[serde(default)]
    at: u64,
    #[serde(default)]
    seguidas: u32,
    #[serde(default)]
    aviso: String,
}

fn arquivo_estado() -> PathBuf {
    crate::pasta().join("continuar.json")
}

fn registrar(texto: &str) {
    use std::io::Write;
    let arq = crate::pasta().join("continuar.log");
    let _ = std::fs::create_dir_all(crate::pasta());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(arq) {
        let _ = writeln!(f, "{}  {texto}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

fn tela_da_aba(ws: &str, tab: u32) -> Option<Tela> {
    daemon::tela_vt(ws, tab).ok().map(|vt| ler_tela(&vt))
}

fn manda(ws: &str, tab: u32, dados: &[u8]) -> bool {
    let Ok(l) = daemon::listar() else { return false };
    let Some(a) = l.aba(ws, tab) else { return false };
    let Ok(mut t) = daemon::Teclado::liga(a) else { return false };
    t.manda(dados).is_ok()
}

/// Digita a mensagem e só aperta Enter se a caixa mostrar exatamente ela.
fn digitar(c: &Conversa, mensagem: &str, so_enter: bool) -> Result<(), String> {
    let l = daemon::listar().map_err(|e| e.to_string())?;
    let a = l.aba(&c.ws, c.tab).ok_or("a aba sumiu")?;
    let mut t = daemon::Teclado::liga(a)?;
    if !so_enter {
        t.manda(mensagem.as_bytes())?;
    }
    let mut visto = String::new();
    for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(200));
        visto = tela_da_aba(&c.ws, c.tab).map(|x| x.digitado).unwrap_or_default();
        if compacto(&visto) == compacto(mensagem) {
            t.manda(b"\r")?;
            std::thread::sleep(Duration::from_millis(100));
            return Ok(());
        }
        if !visto.is_empty() && !compacto(mensagem).starts_with(&compacto(&visto)) {
            break;
        }
    }
    Err(if visto.is_empty() {
        "a aba não mostrou a mensagem na caixa -- não apertei Enter".into()
    } else {
        format!("a caixa ficou {:?} -- não apertei Enter", corta(&visto, 60))
    })
}

/// Uma rodada. `seco`: diz o que faria. `status`: o que vê, só leitura.
pub fn rodada(seco: bool, status: bool) -> Result<Value, String> {
    let Some(_trava) = crate::Trava::pegar(".continuar.lock") else {
        return Ok(json!({ "motivo": "outra rodada em andamento" }));
    };
    let lista = daemon::listar()?;
    let (abertas, ignoradas) = conversas(&lista);
    let mut estado: BTreeMap<String, Registro> = crate::ler(&arquivo_estado()).unwrap_or_default();
    let agora = crate::agora_ms();
    let vivas: Vec<&str> = abertas.iter().map(|c| c.sid.as_str()).collect();
    estado.retain(|sid, r| vivas.contains(&sid.as_str()) || agora.saturating_sub(r.at) <= ESQUECE_MS);

    let mut vistas = Vec::new();
    let mut portas: HashMap<(Tipo, Option<String>), (bool, String)> = HashMap::new();
    let mut enviadas: Vec<(Conversa, PathBuf, Tipo, u64, u32)> = Vec::new();
    for c in &abertas {
        let Some(caminho) = transcrito(&c.sid) else {
            vistas.push(json!({ "aba": c.rotulo, "situacao": "sem transcrito" }));
            continue;
        };
        let erro = match avaliar(&caminho, c.inicio_ms) {
            Ok(e) => e,
            Err(motivo) => {
                vistas.push(json!({ "aba": c.rotulo, "situacao": motivo }));
                continue;
            }
        };
        let r = estado.entry(c.sid.clone()).or_default();
        let aviso = |r: &mut Registro, chave: &str, texto: String| {
            let marca = format!("{}|{chave}", erro.uuid);
            if r.aviso != marca && !seco && !status {
                registrar(&format!("{}: {texto}", c.rotulo));
            }
            r.aviso = marca;
        };
        let idade_min = if erro.tipo == Tipo::Rede && erro_de_servidor(&erro.texto) { IDADE_SERVIDOR_MS } else { IDADE_MIN_MS };
        if agora.saturating_sub(erro.quando_ms) < idade_min {
            vistas.push(json!({ "aba": c.rotulo, "situacao": "parada há pouco: dou tempo" }));
            continue;
        }
        let fixa = c.fixa.clone();
        if erro.tipo == Tipo::Login && fixa.is_some() {
            vistas.push(json!({ "aba": c.rotulo, "situacao": "aba presa a uma conta: o login dela só volta pelo menu da aba" }));
            continue;
        }
        let chave = (erro.tipo, if erro.tipo == Tipo::Rede { None } else { fixa.clone() });
        let (boa, porque) = portas
            .entry(chave)
            .or_insert_with(|| match erro.tipo {
                Tipo::Login => credencial_boa(fixa.as_deref()),
                Tipo::Rede => rede_de_volta(),
                Tipo::Limite => cota_livre(fixa.as_deref()),
            })
            .clone();
        if !boa {
            aviso(r, "porta", format!("parada; {} ainda não voltou ({porque}) -- espero", erro.tipo.nome()));
            vistas.push(json!({ "aba": c.rotulo, "situacao": format!("parada; {} ainda não voltou ({porque})", erro.tipo.nome()) }));
            continue;
        }
        // a tela: volta à conversa se a vista está desviada e a pessoa longe
        let mut tela = tela_da_aba(&c.ws, c.tab).unwrap_or_default();
        let mut motivo_desvio = None;
        for _ in 0..3 {
            let Some((oque, teclas)) = vista_desviada(&tela) else {
                motivo_desvio = None;
                break;
            };
            motivo_desvio = Some(oque);
            if seco || status || ocioso_ha() < OCIOSO_PARA_MEXER_S || !manda(&c.ws, c.tab, teclas) {
                break;
            }
            std::thread::sleep(Duration::from_millis(400));
            tela = tela_da_aba(&c.ws, c.tab).unwrap_or_default();
        }
        if let Some(oque) = motivo_desvio {
            let texto = format!("parada, mas a vista está {oque} e você está por perto");
            aviso(r, "vista", texto.clone());
            vistas.push(json!({ "aba": c.rotulo, "situacao": texto }));
            continue;
        }
        let (pode, motivo, so_enter) = tela_segura(&tela, &erro);
        if !pode {
            aviso(r, &format!("tela:{motivo}"), format!("parada, mas não digito: {motivo}"));
            vistas.push(json!({ "aba": c.rotulo, "situacao": format!("parada, mas não digito: {motivo}") }));
            continue;
        }
        let mut seguidas = r.seguidas;
        if r.at != 0 && (houve_progresso(&caminho, r.at) || agora.saturating_sub(r.at) > RECOMECA_MS) {
            seguidas = 0;
        } else if agora.saturating_sub(r.at) < ESPERA_MS {
            vistas.push(json!({ "aba": c.rotulo, "situacao": "mandei há pouco: espero" }));
            continue;
        }
        if seguidas >= MAX_SEGUIDAS {
            aviso(r, "desisti", format!("{seguidas} continuações sem a conversa andar -- desisto (volto se ela andar ou em 1 h)"));
            vistas.push(json!({ "aba": c.rotulo, "situacao": "desisti por agora" }));
            continue;
        }
        if seco || status {
            vistas.push(json!({ "aba": c.rotulo, "situacao": format!("mandaria continuar ({motivo})") }));
            continue;
        }
        // o estado vai ao disco ANTES de digitar: sem ele, a espera e o teto não valeriam
        *r = Registro { erro: erro.uuid.clone(), at: agora, seguidas: seguidas + 1, aviso: String::new() };
        crate::gravar(&arquivo_estado(), &estado).map_err(|e| format!("não gravei o estado ({e}): não digito"))?;
        match digitar(c, erro.tipo.mensagem(), so_enter) {
            Ok(()) => enviadas.push((c.clone(), caminho, erro.tipo, agora, seguidas + 1)),
            Err(e) => {
                registrar(&format!("{}: parou em {:?}, {} de volta, mas {e}", c.rotulo, corta(&erro.texto, 40), erro.tipo.nome()));
                vistas.push(json!({ "aba": c.rotulo, "situacao": e }));
            }
        }
    }
    // digita em todas primeiro e confere depois, num prazo único
    let fim = std::time::Instant::now() + CONFIRMA;
    let mut confirmadas = Vec::new();
    while confirmadas.len() < enviadas.len() && std::time::Instant::now() < fim {
        std::thread::sleep(Duration::from_millis(500));
        for (c, caminho, tipo, desde, _) in &enviadas {
            if !confirmadas.contains(&c.sid) && chegou(caminho, *desde, tipo.mensagem()) {
                confirmadas.push(c.sid.clone());
            }
        }
    }
    for (c, _, tipo, _, n) in &enviadas {
        let ok = confirmadas.contains(&c.sid);
        registrar(&format!(
            "{}: {} de volta -> mandei continuar ({n}ª) -- {}",
            c.rotulo,
            tipo.nome(),
            if ok { "a mensagem entrou" } else { "SEM confirmação no transcrito" }
        ));
        vistas.push(json!({ "aba": c.rotulo, "situacao": if ok { "mandei continuar" } else { "mandei continuar, sem confirmação" } }));
    }
    if !seco && !status {
        let _ = crate::gravar(&arquivo_estado(), &estado);
    }
    // no --status, as três portas da aba global, para ver de longe se cada causa voltou
    let portas_globais = status.then(|| {
        let porta = |(boa, porque): (bool, String)| json!({ "aberta": boa, "porque": porque });
        json!({ "login": porta(credencial_boa(None)), "rede": porta(rede_de_volta()), "limite": porta(cota_livre(None)) })
    });
    Ok(json!({
        "versao": crate::VERSAO,
        "portas": portas_globais,
        "conversas": vistas,
        "ignoradas": ignoradas.iter().map(|(a, m)| json!({ "aba": a, "motivo": m })).collect::<Vec<_>>(),
        "enviadas": enviadas.len(),
        "confirmadas": confirmadas.len(),
    }))
}

pub fn cli(args: &[String]) -> i32 {
    let tem = |o: &str| args.iter().any(|a| a == o);
    match rodada(tem("--seco"), tem("--status")) {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            println!("{}", json!({ "versao": crate::VERSAO, "erro": e }));
            1
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn transcrito_com(linhas: &[Value]) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), linhas.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
        f
    }

    fn fala(tipo: &str, texto: &str, quando: &str) -> Value {
        json!({ "type": tipo, "timestamp": quando, "uuid": format!("u-{quando}"), "message": { "content": [{ "type": "text", "text": texto }] } })
    }

    fn erro(tipo_erro: &str, texto: &str, quando: &str) -> Value {
        let mut v = fala("assistant", texto, quando);
        v["error"] = json!(tipo_erro);
        v
    }

    #[test]
    fn so_a_ultima_fala_de_erro_conta() {
        let f = transcrito_com(&[
            fala("user", "faça", "2026-10-13T01:00:00Z"),
            erro("authentication_failed", "Not logged in · Please run /login", "2026-10-13T01:00:05Z"),
            json!({ "type": "system", "timestamp": "2026-10-13T01:00:06Z" }),
            json!({ "type": "user", "isMeta": true, "message": { "content": "meta" } }),
            fala("user", "<local-command-stdout>Login successful</local-command-stdout>", "2026-10-13T01:00:07Z"),
        ]);
        let e = avaliar(f.path(), 0).unwrap();
        assert_eq!(e.tipo, Tipo::Login);
        assert!(avaliar(f.path(), carimbo_ms(&json!({ "timestamp": "2026-10-13T01:00:06Z" }))).is_err(), "erro anterior à abertura");
        let mencao = transcrito_com(&[fala("assistant", "o erro Not logged in aparece quando…", "2026-10-13T01:00:00Z")]);
        assert!(avaliar(mencao.path(), 0).is_err(), "só mencionar a frase não é parar");
    }

    #[test]
    fn rede_limite_e_o_que_login_nao_cura() {
        let rede = transcrito_com(&[erro("server_error", "API Error: Connection lost mid-response", "2026-10-13T01:00:00Z")]);
        assert_eq!(avaliar(rede.path(), 0).unwrap().tipo, Tipo::Rede);
        let lim = transcrito_com(&[erro("rate_limit", "You've hit your session limit · resets 5:10pm", "2026-10-13T01:00:00Z")]);
        assert_eq!(avaliar(lim.path(), 0).unwrap().tipo, Tipo::Limite);
        let chave = transcrito_com(&[erro("authentication_failed", "Invalid API key · Please run /login", "2026-10-13T01:00:00Z")]);
        assert!(avaliar(chave.path(), 0).is_err());
        let seguiu = transcrito_com(&[erro("authentication_failed", "Not logged in", "2026-10-13T01:00:00Z"), json!({ "type": "continued-in" })]);
        assert!(avaliar(seguiu.path(), 0).is_err());
    }

    #[test]
    fn transcrito_grande_lido_de_tras_em_blocos() {
        let mut linhas: Vec<Value> = (0..3000).map(|i| fala("user", &format!("pedido {i} {}", "x".repeat(100)), "2026-10-13T00:00:00Z")).collect();
        linhas.push(erro("authentication_failed", "Not logged in · Please run /login", "2026-10-13T01:00:00Z"));
        let f = transcrito_com(&linhas);
        assert!(std::fs::metadata(f.path()).unwrap().len() > BLOCO * 3);
        assert_eq!(avaliar(f.path(), 0).unwrap().tipo, Tipo::Login);
    }

    #[test]
    fn progresso_e_chegada() {
        let f = transcrito_com(&[
            erro("authentication_failed", "Not logged in", "2026-10-13T01:00:00Z"),
            fala("user", MSG_LOGIN, "2026-10-13T01:01:00Z"),
            erro("authentication_failed", "Not logged in", "2026-10-13T01:01:05Z"),
        ]);
        let desde = carimbo_ms(&json!({ "timestamp": "2026-10-13T01:00:30Z" }));
        assert!(chegou(f.path(), desde, MSG_LOGIN));
        assert!(!houve_progresso(f.path(), desde), "a nossa mensagem e um erro de novo não são andar");
        let andou = transcrito_com(&[
            fala("user", MSG_LOGIN, "2026-10-13T01:01:00Z"),
            json!({ "type": "user", "timestamp": "2026-10-13T01:01:10Z", "message": { "content": [{ "type": "tool_result", "content": "ok" }] } }),
            erro("server_error", "API Error: Connection lost", "2026-10-13T01:02:00Z"),
        ]);
        assert!(houve_progresso(andou.path(), desde));
    }

    const TELA_PARADA: &str = "● Rodei os testes.\r\n  ⎿  Not logged in · Please run /login\r\n\r\n───────────────\r\n❯ \x1b[2mTry \"fix lint\"\x1b[22m\r\n───────────────\r\n  ⏵⏵ bypass permissions on · ← for agents";

    #[test]
    fn a_tela_parada_e_segura_e_a_sugestao_cinza_nao_e_texto() {
        let t = ler_tela(TELA_PARADA);
        assert_eq!(t.digitado, "", "a sugestão esmaecida não conta");
        let e = Erro { uuid: "u".into(), quando_ms: 0, texto: "Not logged in · Please run /login".into(), tipo: Tipo::Login };
        assert!(tela_segura(&t, &e).0);
        let ocupada = ler_tela(&TELA_PARADA.replace("for agents", "for agents · esc to interrupt"));
        assert!(!tela_segura(&ocupada, &e).0);
        let digitando = ler_tela(&TELA_PARADA.replace("\x1b[2mTry \"fix lint\"\x1b[22m", "meu rascunho"));
        assert!(!tela_segura(&digitando, &e).0);
        let com_a_nossa = ler_tela(&TELA_PARADA.replace("\x1b[2mTry \"fix lint\"\x1b[22m", MSG_LOGIN));
        assert_eq!(tela_segura(&com_a_nossa, &e), (true, "a mensagem já está na caixa, falta o Enter".into(), true));
        let sem_erro = ler_tela(&TELA_PARADA.replace("Not logged in · Please run /login", "tudo certo"));
        assert!(!tela_segura(&sem_erro, &e).0);
    }

    #[test]
    fn o_ocioso_do_windows_sai_dos_dois_relogios_mesmo_na_virada() {
        assert_eq!(ocioso_de_ticks(10_000, 7_500), 2.5);
        assert_eq!(ocioso_de_ticks(5_000, 5_000), 0.0);
        // O relógio de 32 bits deu a volta entre a última entrada e agora.
        assert_eq!(ocioso_de_ticks(1_000, u32::MAX - 999), 2.0);
        assert_eq!(ocioso_de_ticks(0, u32::MAX), 0.001);
    }

    #[test]
    fn vistas_desviadas() {
        let transcrito = ler_tela("❯ faça X\r\n● fiz\r\n  Showing detailed transcript · ctrl+o to toggle");
        assert_eq!(vista_desviada(&transcrito).map(|v| v.1), Some(&b"\x0f"[..]));
        let rolada = ler_tela("● a\r\n  3 new messages (ctrl+End)\r\n───\r\n❯ \r\n───");
        assert_eq!(vista_desviada(&rolada).map(|v| v.1), Some(&b"\x1b[1;5F"[..]));
        let limite = ler_tela(" What do you want to do?\r\n ❯ 1. Stop and wait for limit to reset\r\n   2. Wait here\r\n Esc to cancel");
        assert_eq!(vista_desviada(&limite).map(|v| v.1), Some(&b"\x1b"[..]));
        assert!(vista_desviada(&ler_tela(TELA_PARADA)).is_none());
    }
}
