//! O consumo de cada conta: medido nos endpoints que o Claude Code e o Codex
//! usam, guardado sem token, com a cadência e as pausas que os serviços
//! pedem. Ver `docs/ia.md` ("Consumo").
//!
//! As regras são as do rodapé do app do macOS, que vieram do painel da
//! Clínica: cinco minutos entre leituras de uma conta; "Medir agora" no
//! máximo a cada 15 s; um 429 espera o `Retry-After` (pelo menos um minuto,
//! dez sem ele) e nem o clique pula essa espera; sem rede, tenta de novo em
//! um minuto. Token vencido não é renovado daqui — quem renova é a
//! ferramenta dona do login, ao usar.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cli::{Args, responde};
use crate::contas::{self, Conta, Motor, Token};
use crate::{caminhos, programas, rede};

const PERIODO: f64 = 300.0;
const ENTRE_CLIQUES: f64 = 15.0;

/// Uma janela de consumo: uma barra no rodapé.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Janela {
    /// O nome curto ("5h", "7d", "Fable").
    pub label: String,
    /// O nome longo ("Sessão (5h)").
    pub title: String,
    /// Gasto, de 0 a 100.
    pub percent: f64,
    /// Quando recomeça, em segundos unix.
    pub resets_at: Option<f64>,
}

/// Tudo o que uma resposta diz de uma conta.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Leitura {
    pub windows: Vec<Janela>,
    /// O serviço diz que a conta está no limite, pelas janelas gerais.
    pub limit_reached: bool,
}

/// O que se guarda de cada conta entre uma medição e outra.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Registro {
    reading: Option<Leitura>,
    measured_at: Option<f64>,
    problem: Option<String>,
    /// Até quando esperar antes de perguntar de novo, e se nem um clique
    /// passa por cima (o 429 do serviço).
    pausa_ate: Option<f64>,
    #[serde(default)]
    pausa_firme: bool,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Cache {
    #[serde(default)]
    contas: BTreeMap<String, Registro>,
    #[serde(default)]
    ultimo_clique: f64,
}

/// Uma linha do rodapé: a conta e o que ela disse por último. Os nomes são os
/// do app do macOS (`AccountUsage`).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Linha {
    pub account: Conta,
    pub reading: Option<Leitura>,
    pub measured_at: Option<f64>,
    pub problem: Option<String>,
}

impl Linha {
    /// Se a conta pode receber trabalho agora: nada errado com o login, e
    /// não está no limite (só 100% da janela de 5 h ou da semanal tira a
    /// conta da fila). Sem medição, ganha o benefício da dúvida.
    pub fn disponivel(&self) -> bool {
        if self.account.warning.is_some() {
            return false;
        }
        let Some(l) = &self.reading else { return true };
        if l.limit_reached {
            return false;
        }
        !l.windows.iter().any(|j| (j.label == "5h" || j.label == "7d") && j.percent >= 100.0)
    }
}

fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("uso.json")
}

fn le_cache() -> Cache {
    std::fs::read_to_string(arquivo()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn grava_cache(c: &Cache) {
    let caminho = arquivo();
    if let Some(dir) = caminho.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(texto) = serde_json::to_vec_pretty(c) {
        let tmp = caminho.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, &caminho);
        }
    }
}

/// Uma medição por vez entre os processos: o app e a sincronização chamam
/// juntos, e duas rodadas ao mesmo tempo dobram as consultas (e os 429).
struct Trava(PathBuf);

impl Trava {
    fn pega() -> Option<Trava> {
        let caminho = caminhos::estado().join("uso.trava");
        let _ = std::fs::create_dir_all(caminhos::estado());
        for _ in 0..2 {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&caminho) {
                Ok(_) => return Some(Trava(caminho)),
                Err(_) => {
                    // Uma trava de quem morreu no meio não prende ninguém
                    // por mais de um minuto.
                    let velha = std::fs::metadata(&caminho)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|m| m.elapsed().ok())
                        .is_some_and(|d| d > Duration::from_secs(60));
                    if !velha {
                        return None;
                    }
                    let _ = std::fs::remove_file(&caminho);
                }
            }
        }
        None
    }
}

impl Drop for Trava {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn id(c: &Conta) -> String {
    format!("{}:{}", match c.engine { Motor::Claude => "claude", Motor::Codex => "codex" }, c.key)
}

// ---------------------------------------------------------------- leituras

fn numero(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn data(v: Option<&Value>) -> Option<f64> {
    let texto = v?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(texto).ok().map(|d| d.timestamp() as f64)
}

/// `GET /api/oauth/usage`, como o Claude Code lê: `five_hour` e `seven_day`
/// em percentual (1.0 é um por cento); as janelas por modelo em `limits`
/// (`weekly_scoped`), ou nas chaves antigas quando a lista não cita nenhuma;
/// os créditos extras só quando ligados.
pub fn ler_claude(corpo: &[u8]) -> Option<Leitura> {
    let raiz: Value = serde_json::from_slice(corpo).ok()?;
    let limites: Vec<Value> = raiz.get("limits").and_then(Value::as_array).cloned().unwrap_or_default();
    let do_tipo = |tipo: &str| limites.iter().find(|l| l.get("kind").and_then(Value::as_str) == Some(tipo)).cloned();
    let mut janelas = Vec::new();
    let mut no_limite = false;
    let mut soma = |rotulo: &str, titulo: &str, janela: Option<&Value>, limite: Option<&Value>, geral: bool| {
        let Some(pct) = numero(janela.and_then(|j| j.get("utilization")))
            .or_else(|| numero(limite.and_then(|l| l.get("percent"))))
        else {
            return;
        };
        let reset = data(janela.and_then(|j| j.get("resets_at"))).or_else(|| data(limite.and_then(|l| l.get("resets_at"))));
        janelas.push(Janela { label: rotulo.into(), title: titulo.into(), percent: pct, resets_at: reset });
        if geral && pct >= 100.0 {
            no_limite = true;
        }
    };
    soma("5h", "Sessão (5h)", raiz.get("five_hour"), do_tipo("session").as_ref(), true);
    soma("7d", "Semanal (7 dias)", raiz.get("seven_day"), do_tipo("weekly_all").as_ref(), true);
    let por_modelo: Vec<&Value> =
        limites.iter().filter(|l| l.get("kind").and_then(Value::as_str) == Some("weekly_scoped")).collect();
    for l in &por_modelo {
        let escopo = l.get("scope");
        let nome = escopo
            .and_then(|s| s.get("model"))
            .and_then(|m| m.get("display_name"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                escopo
                    .and_then(|s| s.get("surface"))
                    .and_then(|m| m.get("display_name"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or("modelo");
        soma(nome, &format!("Semanal — {nome}"), None, Some(l), false);
    }
    if por_modelo.is_empty() {
        soma("Opus", "Semanal — Opus", raiz.get("seven_day_opus"), None, false);
        soma("Sonnet", "Semanal — Sonnet", raiz.get("seven_day_sonnet"), None, false);
    }
    if let Some(extra) = raiz.get("extra_usage") {
        if extra.get("is_enabled").and_then(Value::as_bool) == Some(true) {
            soma("Extra", "Créditos extras do mês", Some(extra), None, false);
        }
    }
    (!janelas.is_empty()).then_some(Leitura { windows: janelas, limit_reached: no_limite })
}

/// "5h" para cinco horas, "7d" para uma semana, a duração nos outros casos.
fn nome_da_janela(segundos: Option<f64>) -> (String, String) {
    let Some(s) = segundos.filter(|s| *s > 0.0) else { return ("janela".into(), "Janela".into()) };
    let horas = (s / 3600.0).round() as i64;
    match horas {
        5 => ("5h".into(), "Sessão (5h)".into()),
        168 => ("7d".into(), "Semanal (7 dias)".into()),
        h if h % 24 == 0 => (format!("{}d", h / 24), format!("Janela de {} dias", h / 24)),
        h => (format!("{h}h"), format!("Janela de {h}h")),
    }
}

/// `GET /backend-api/wham/usage`, como o Codex lê: janelas nomeadas pela
/// duração, que a resposta dá em segundos.
pub fn ler_codex(corpo: &[u8]) -> Option<Leitura> {
    let raiz: Value = serde_json::from_slice(corpo).ok()?;
    let mut janelas = Vec::new();
    let mut no_limite = false;
    let mut toma = |limite: Option<&Value>, prefixo: Option<&str>| {
        let Some(limite) = limite else { return };
        if prefixo.is_none() && limite.get("limit_reached").and_then(Value::as_bool) == Some(true) {
            no_limite = true;
        }
        for chave in ["primary_window", "secondary_window"] {
            let Some(j) = limite.get(chave) else { continue };
            let Some(pct) = numero(j.get("used_percent")) else { continue };
            let (rotulo, titulo) = nome_da_janela(numero(j.get("limit_window_seconds")));
            janelas.push(Janela {
                label: prefixo.map(|p| format!("{p} {rotulo}")).unwrap_or(rotulo),
                title: prefixo.map(|p| format!("{titulo} — {p}")).unwrap_or(titulo),
                percent: pct,
                resets_at: numero(j.get("reset_at")),
            });
            if prefixo.is_none() && pct >= 100.0 {
                no_limite = true;
            }
        }
    };
    toma(raiz.get("rate_limit"), None);
    if let Some(extras) = raiz.get("additional_rate_limits").and_then(Value::as_array) {
        for e in extras {
            let nome = e
                .get("limit_name")
                .and_then(Value::as_str)
                .or_else(|| e.get("metered_feature").and_then(Value::as_str))
                .unwrap_or("extra")
                .to_string();
            toma(Some(e.get("rate_limit").unwrap_or(e)), Some(&nome));
        }
    }
    (!janelas.is_empty()).then_some(Leitura { windows: janelas, limit_reached: no_limite })
}

/// "hoje às 01:50", "amanhã às 07:00", "ontem às 23:10", "em 28/09 às 07:00".
pub fn momento(quando: f64, agora: f64) -> String {
    let (Some(q), Some(a)) = (Local.timestamp_opt(quando as i64, 0).single(), Local.timestamp_opt(agora as i64, 0).single())
    else {
        return String::new();
    };
    let hora = q.format("%H:%M").to_string();
    let dia = |d: &chrono::DateTime<Local>| (d.year(), d.ordinal());
    if dia(&q) == dia(&a) {
        return format!("hoje às {hora}");
    }
    if a.checked_add_signed(chrono::Duration::days(1)).is_some_and(|d| dia(&q) == dia(&d)) {
        return format!("amanhã às {hora}");
    }
    if a.checked_sub_signed(chrono::Duration::days(1)).is_some_and(|d| dia(&q) == dia(&d)) {
        return format!("ontem às {hora}");
    }
    format!("em {} às {hora}", q.format("%d/%m"))
}

// ----------------------------------------------------------------- medir

enum Resultado {
    Leitura(Leitura),
    Falha(String, Option<(f64, bool)>),
}

fn pergunta(conta: &Conta, token: &Token, agora_s: f64) -> Resultado {
    let (url, cabecalhos) = match conta.engine {
        Motor::Claude => (
            rede::url_de_teste(
                &["KEEP_IA_CLAUDE_URL", "KEEP_AI_USAGE_CLAUDE_URL"],
                "https://api.anthropic.com/api/oauth/usage",
            ),
            vec![
                ("Authorization", format!("Bearer {}", token.acesso)),
                ("Accept", "application/json".to_string()),
                ("anthropic-beta", "oauth-2025-04-20".to_string()),
                ("User-Agent", programas::agente_claude()),
            ],
        ),
        Motor::Codex => {
            let mut c = vec![
                ("Authorization", format!("Bearer {}", token.acesso)),
                ("Accept", "application/json".to_string()),
                ("User-Agent", format!("codex_cli_rs/{}", programas::versao_codex())),
            ];
            if let Some(conta) = &token.conta_chatgpt {
                c.push(("ChatGPT-Account-Id", conta.clone()));
            }
            (
                rede::url_de_teste(
                    &["KEEP_IA_CODEX_URL", "KEEP_AI_USAGE_CODEX_URL"],
                    "https://chatgpt.com/backend-api/wham/usage",
                ),
                c,
            )
        }
    };
    let resposta = match rede::get(&url, &cabecalhos, Duration::from_secs(15)) {
        Ok(r) => r,
        Err(_) => return Resultado::Falha("sem rede; tenta de novo em 1 min".into(), Some((agora_s + 60.0, false))),
    };
    match resposta.status {
        200 => {
            let leitura = match conta.engine {
                Motor::Claude => ler_claude(&resposta.corpo),
                Motor::Codex => ler_codex(&resposta.corpo),
            };
            match leitura {
                Some(l) => Resultado::Leitura(l),
                None => Resultado::Falha("resposta sem limites legíveis".into(), Some((agora_s + PERIODO, false))),
            }
        }
        s @ (401 | 403) => Resultado::Falha(
            format!("o serviço recusou o acesso (HTTP {s}); renova sozinho ao usar"),
            Some((agora_s + PERIODO, false)),
        ),
        429 => {
            let espera = resposta.retry_after.and_then(|r| r.trim().parse::<f64>().ok()).map(|s| s.max(60.0)).unwrap_or(600.0);
            let ate = agora_s + espera;
            Resultado::Falha(
                format!("consultas demais (HTTP 429); tenta de novo {}", momento(ate, agora_s)),
                Some((ate, true)),
            )
        }
        s => Resultado::Falha(format!("sem leitura (HTTP {s})"), Some((agora_s + 60.0, false))),
    }
}

fn linhas(contas: Vec<Conta>, cache: &Cache) -> Vec<Linha> {
    contas
        .into_iter()
        .map(|c| {
            let r = cache.contas.get(&id(&c)).cloned().unwrap_or_default();
            Linha { account: c, reading: r.reading, measured_at: r.measured_at, problem: r.problem }
        })
        .collect()
}

/// As linhas como estão, sem ir à rede.
pub fn em_cache() -> Vec<Linha> {
    linhas(contas::listar(false), &le_cache())
}

/// Uma rodada: mede as contas devidas (ou `so` esta), em paralelo, e devolve
/// todas as linhas. `forcar` é o "Medir agora".
pub fn medir(forcar: bool, so: Option<&str>) -> Vec<Linha> {
    let lista = contas::listar(true);
    let Some(_trava) = Trava::pega() else {
        // Outra rodada está medindo: o que ela já tem serve.
        return linhas(lista, &le_cache());
    };
    let mut cache = le_cache();
    let agora_s = agora();
    let mut forcar = forcar;
    if forcar {
        if agora_s - cache.ultimo_clique < ENTRE_CLIQUES {
            forcar = false;
        } else {
            cache.ultimo_clique = agora_s;
        }
    }
    let mut tarefas = Vec::new();
    for conta in &lista {
        let chave = id(conta);
        if so.is_some_and(|s| !conta.e(s)) {
            continue;
        }
        let r = cache.contas.get(&chave).cloned().unwrap_or_default();
        if let Some(ate) = r.pausa_ate {
            if ate > agora_s && (!forcar || r.pausa_firme) {
                continue;
            }
        }
        let devida = forcar
            || so.is_some()
            || r.problem.is_some()
            || r.measured_at.is_none_or(|m| agora_s - m >= PERIODO - 5.0);
        if !devida {
            continue;
        }
        let mut novo = r.clone();
        novo.pausa_ate = None;
        novo.pausa_firme = false;
        let Some(token) = conta.token.clone() else {
            novo.problem = Some("sem credencial salva".into());
            cache.contas.insert(chave, novo);
            continue;
        };
        if let Some(expira) = token.expira_em {
            let expira_s = expira as f64 / 1000.0;
            if expira_s <= agora_s + 60.0 {
                // Não é renovado daqui: quem é dono do login renova ao usar.
                novo.problem = Some(if expira_s <= agora_s {
                    format!("acesso vencido {}; renova sozinho ao usar", momento(expira_s, agora_s))
                } else {
                    "acesso vence agora; medindo no próximo minuto".into()
                });
                cache.contas.insert(chave, novo);
                continue;
            }
        }
        let conta = conta.clone();
        tarefas.push(std::thread::spawn(move || {
            let resultado = pergunta(&conta, &token, agora_s);
            (chave, novo, resultado)
        }));
    }
    for tarefa in tarefas {
        let Ok((chave, mut r, resultado)) = tarefa.join() else { continue };
        match resultado {
            Resultado::Leitura(l) => {
                r.reading = Some(l);
                r.measured_at = Some(agora());
                r.problem = None;
            }
            Resultado::Falha(problema, pausa) => {
                r.problem = Some(problema);
                if let Some((ate, firme)) = pausa {
                    r.pausa_ate = Some(ate);
                    r.pausa_firme = firme;
                }
            }
        }
        cache.contas.insert(chave, r);
    }
    // Conta que sumiu sai depois de uma semana sem leitura.
    let vivas: Vec<String> = lista.iter().map(id).collect();
    cache.contas.retain(|k, r| vivas.contains(k) || r.measured_at.is_some_and(|m| agora_s - m < 7.0 * 86400.0));
    grava_cache(&cache);
    linhas(lista, &cache)
}

/// `keep ia uso [--agora] [--conta=<chave>] [--cache]`.
pub fn cli(a: &Args) -> i32 {
    let lista = if a.tem("cache") { em_cache() } else { medir(a.tem("agora"), a.opcao("conta")) };
    let ordem: Vec<String> = lista.iter().map(|l| l.account.order_key.clone()).collect();
    responde(json!({
        "ok": true,
        "linhas": lista,
        "ordem": ordem,
        "gerenteExterno": contas::gerente_externo(),
        "medidoEm": agora(),
    }))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contas::testes::{casa, perfis};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Um pedido que o servidor falso recebeu.
    #[derive(Clone, Debug)]
    struct Pedido {
        caminho: String,
        token: String,
        conta: Option<String>,
    }

    type Resposta = (u16, Vec<(&'static str, String)>, String);

    /// Os endpoints de consumo nesta máquina: `responde` decide por pedido, e
    /// a lista guarda todos.
    fn servidor(responde: impl Fn(&Pedido) -> Resposta + Send + 'static) -> Arc<Mutex<Vec<Pedido>>> {
        let pedidos = Arc::new(Mutex::new(Vec::new()));
        let guarda = pedidos.clone();
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for conexao in ouvinte.incoming() {
                let Ok(mut c) = conexao else { continue };
                let mut buf = [0u8; 8192];
                let n = c.read(&mut buf).unwrap_or(0);
                let texto = String::from_utf8_lossy(&buf[..n]).to_string();
                let caminho = texto.split_whitespace().nth(1).unwrap_or("").to_string();
                let cabecalhos: HashMap<String, String> = texto
                    .lines()
                    .skip(1)
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let p = Pedido {
                    caminho,
                    token: cabecalhos.get("authorization").and_then(|a| a.strip_prefix("Bearer ")).unwrap_or("").into(),
                    conta: cabecalhos.get("chatgpt-account-id").cloned(),
                };
                let (status, extras, corpo) = responde(&p);
                guarda.lock().unwrap().push(p);
                let mut r = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n", corpo.len());
                for (k, v) in extras {
                    r.push_str(&format!("{k}: {v}\r\n"));
                }
                r.push_str("\r\n");
                r.push_str(&corpo);
                let _ = c.write_all(r.as_bytes());
            }
        });
        // SAFETY: chamado com a casa (e a trava) de pé.
        unsafe {
            std::env::set_var("KEEP_IA_CLAUDE_URL", format!("http://127.0.0.1:{porta}/claude"));
            std::env::set_var("KEEP_IA_CODEX_URL", format!("http://127.0.0.1:{porta}/codex"));
        }
        pedidos
    }

    fn quantos(p: &Arc<Mutex<Vec<Pedido>>>) -> usize {
        p.lock().unwrap().len()
    }

    fn zera_o_clique() {
        let mut c = le_cache();
        c.ultimo_clique = 0.0;
        grava_cache(&c);
    }

    const CHEIA: &str = r#"{"five_hour":{"utilization":12},"seven_day":{"utilization":100}}"#;
    const CODEX: &str = r#"{"rate_limit":{"primary_window":{"used_percent":30,"limit_window_seconds":18000}}}"#;

    #[test]
    fn a_rodada_mede_as_devidas_e_respeita_as_pausas() {
        let c = casa("uso-rodada");
        perfis(&[("segredo-ana", "u-1", "ana@x.com"), ("segredo-bia", "u-2", "bia@y.com")]);
        c.global("segredo-ana", "r");
        c.fixa("k-2", Some(("segredo-bia", "r")), Some(("bia", "bia@y.com", "u-2")));
        c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
        let pedidos = servidor(|p| match (p.caminho.as_str(), p.token.as_str()) {
            ("/claude", "segredo-ana") => (200, vec![], CHEIA.into()),
            ("/claude", _) => (429, vec![("Retry-After", "120".into())], String::new()),
            _ => (200, vec![], CODEX.into()),
        });

        let l = medir(false, None);
        assert_eq!(l.iter().map(|l| l.account.order_key.as_str()).collect::<Vec<_>>(), ["claude:ana", "claude:bia", "gpt:principal"]);
        assert_eq!(quantos(&pedidos), 3);
        assert!(l[0].reading.is_some() && l[0].measured_at.is_some() && l[0].problem.is_none());
        assert!(!l[0].disponivel(), "a semanal a 100% tira a conta da fila");
        assert!(l[1].problem.as_deref().is_some_and(|p| p.starts_with("consultas demais (HTTP 429); tenta de novo ")), "{:?}", l[1].problem);
        assert!(l[1].disponivel(), "sem leitura, o benefício da dúvida");
        assert!(l[2].disponivel());
        assert_eq!(pedidos.lock().unwrap().iter().find(|p| p.caminho == "/codex").and_then(|p| p.conta.clone()).as_deref(), Some("acct-1"));

        medir(false, None);
        assert_eq!(quantos(&pedidos), 3, "medidas há pouco e uma em pausa: ninguém pergunta");

        medir(true, None);
        assert_eq!(quantos(&pedidos), 5, "o clique mede de novo, menos a do 429");
        medir(true, None);
        assert_eq!(quantos(&pedidos), 5, "dois cliques em 15 s valem um");
        zera_o_clique();
        medir(false, Some("claude:bia"));
        medir(true, Some("claude:bia"));
        assert_eq!(quantos(&pedidos), 5, "nem pedida pelo nome a do 429 pergunta antes da hora");

        let guardado = std::fs::read_to_string(caminhos::estado().join("uso.json")).unwrap();
        assert!(!guardado.contains("segredo"), "o cache não guarda token");
        let cache = em_cache();
        assert_eq!(cache.len(), 3);
        assert!(cache[1].problem.as_deref().is_some_and(|p| p.contains("HTTP 429")));
        assert_eq!(cache[0].reading.as_ref().map(|l| l.windows.len()), Some(2));
    }

    #[test]
    fn sem_credencial_ou_com_acesso_vencido_nao_vai_a_rede() {
        let c = casa("uso-vencido");
        perfis(&[("segredo-velho", "u-1", "ana@x.com")]);
        c.global_ate("segredo-velho", "r", crate::contas::donos::agora_ms() - 3_600_000);
        c.fixa("k-2", None, Some(("bia", "bia@y.com", "u-2")));
        let pedidos = servidor(|_| (200, vec![], CHEIA.into()));
        let l = medir(true, None);
        assert_eq!(quantos(&pedidos), 0);
        let problemas: Vec<&str> = l.iter().filter_map(|l| l.problem.as_deref()).collect();
        assert_eq!(problemas.len(), 2, "{problemas:?}");
        assert!(problemas[0].starts_with("acesso vencido ") && problemas[0].ends_with("; renova sozinho ao usar"), "{problemas:?}");
        assert_eq!(problemas[1], "sem credencial salva");
    }

    #[test]
    fn recusa_e_resposta_estranha_viram_problema_com_pausa() {
        let c = casa("uso-recusa");
        perfis(&[("segredo-ana", "u-1", "ana@x.com")]);
        c.global("segredo-ana", "r");
        let pedidos = servidor(|_| (401, vec![], String::new()));
        let l = medir(false, None);
        assert_eq!(l[0].problem.as_deref(), Some("o serviço recusou o acesso (HTTP 401); renova sozinho ao usar"));
        medir(false, None);
        assert_eq!(quantos(&pedidos), 1, "a recusa espera o período");
        zera_o_clique();
        medir(true, None);
        assert_eq!(quantos(&pedidos), 2, "mas o clique passa por cima");
    }

    #[test]
    fn o_momento_fala_em_hoje_amanha_ou_na_data() {
        let meio_dia = Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .and_then(|d| d.and_local_timezone(Local).single())
            .unwrap()
            .timestamp() as f64;
        assert_eq!(momento(meio_dia + 1800.0, meio_dia), "hoje às 12:30");
        assert_eq!(momento(meio_dia + 86400.0, meio_dia), "amanhã às 12:00");
        assert_eq!(momento(meio_dia - 86400.0, meio_dia), "ontem às 12:00");
        assert!(momento(meio_dia + 3.0 * 86400.0, meio_dia).starts_with("em "));
    }

    #[test]
    fn o_claude_le_as_janelas_gerais_e_as_do_modelo() {
        let corpo = br#"{"five_hour":{"utilization":12.0,"resets_at":"2026-10-04T05:00:00.123456+00:00"},
            "seven_day":{"utilization":100,"resets_at":"2026-10-05T07:00:00+00:00"},
            "limits":[{"kind":"weekly_scoped","percent":40,"scope":{"model":{"display_name":"Fable"}}}],
            "extra_usage":{"is_enabled":false,"utilization":3}}"#;
        let l = ler_claude(corpo).unwrap();
        let rotulos: Vec<&str> = l.windows.iter().map(|j| j.label.as_str()).collect();
        assert_eq!(rotulos, ["5h", "7d", "Fable"]);
        assert!(l.limit_reached, "a semanal a 100% fecha a conta");
        assert_eq!(l.windows[0].resets_at, Some(1_791_090_000.0));
    }

    #[test]
    fn o_claude_antigo_le_opus_e_sonnet() {
        let corpo = br#"{"five_hour":{"utilization":0},"seven_day":{"utilization":1},"seven_day_opus":{"utilization":5}}"#;
        let l = ler_claude(corpo).unwrap();
        assert_eq!(l.windows.iter().map(|j| j.label.as_str()).collect::<Vec<_>>(), ["5h", "7d", "Opus"]);
        assert!(!l.limit_reached);
        assert_eq!(l.windows[0].percent, 0.0, "zero por cento é uma janela, não um booleano");
    }

    #[test]
    fn o_codex_nomeia_as_janelas_pela_duracao() {
        let corpo = br#"{"rate_limit":{"limit_reached":false,
            "primary_window":{"used_percent":30,"limit_window_seconds":18000,"reset_at":1791090000},
            "secondary_window":{"used_percent":100,"limit_window_seconds":604800}},
            "additional_rate_limits":[{"limit_name":"GPT-5","rate_limit":{"primary_window":{"used_percent":100,"limit_window_seconds":604800}}}]}"#;
        let l = ler_codex(corpo).unwrap();
        let rotulos: Vec<&str> = l.windows.iter().map(|j| j.label.as_str()).collect();
        assert_eq!(rotulos, ["5h", "7d", "GPT-5 7d"]);
        assert!(l.limit_reached, "a semanal do plano a 100% fecha");
    }

    #[test]
    fn um_limite_extra_cheio_nao_fecha_a_conta() {
        let corpo = br#"{"rate_limit":{"primary_window":{"used_percent":10,"limit_window_seconds":18000}},
            "additional_rate_limits":[{"limit_name":"GPT-5","rate_limit":{"limit_reached":true,"primary_window":{"used_percent":100,"limit_window_seconds":604800}}}]}"#;
        assert!(!ler_codex(corpo).unwrap().limit_reached);
    }
}
