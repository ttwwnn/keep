//! O saldo do Jev: os créditos do OpenRouter, que o Jev (o modelo que decide
//! no lugar do Claude) gasta a cada decisão. Aparece no rodapé "Consumo de
//! IA" ao lado das contas. Ver `docs/ia.md` ("Jev").
//!
//! A chave é a que a skill `jev` guarda: no macOS, o item `openrouter-api-key`
//! do Chaveiro (lido com `security`, como as credenciais do Claude); nos
//! outros sistemas, `OPENROUTER_API_KEY`. Sem chave, não há Jev e o rodapé não
//! mostra nada. A chave nunca sai daqui nem vai para o cache.
//!
//! Duas perguntas ao OpenRouter: `/api/v1/credits` (o que a conta comprou e
//! gastou) e `/api/v1/key` (o teto e o gasto da chave do Jev). O que vale é o
//! menor dos dois saldos. Mesma cadência do consumo das contas: cinco minutos
//! entre leituras, "Medir agora" no máximo a cada 15 s, 429 espera o
//! `Retry-After`, sem rede tenta de novo em um minuto.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cli::{Args, responde};
use crate::{caminhos, rede};

const PERIODO: f64 = 300.0;
const ENTRE_CLIQUES: f64 = 15.0;
/// O item do Chaveiro em que a skill `jev` guarda a chave.
const SERVICO: &str = "openrouter-api-key";

/// O que o OpenRouter diz do crédito, em dólares.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Credito {
    /// O que a conta comprou e o que já gastou.
    pub total: f64,
    pub used: f64,
    /// O teto da chave do Jev e o que ela já gastou, quando a chave tem teto.
    pub key_limit: Option<f64>,
    pub key_used: Option<f64>,
    /// O que a chave gastou hoje e no mês.
    pub used_today: Option<f64>,
    pub used_this_month: Option<f64>,
}

impl Credito {
    /// O saldo da conta.
    pub fn saldo_da_conta(&self) -> f64 {
        (self.total - self.used).max(0.0)
    }

    /// O saldo da chave, se ela tem teto.
    pub fn saldo_da_chave(&self) -> Option<f64> {
        Some((self.key_limit? - self.key_used.unwrap_or(0.0)).max(0.0))
    }

    /// Quanto o Jev ainda pode gastar: o menor dos dois.
    pub fn disponivel(&self) -> f64 {
        self.saldo_da_chave().map_or(self.saldo_da_conta(), |c| c.min(self.saldo_da_conta()))
    }
}

/// O que se guarda entre uma medição e outra.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct Registro {
    credit: Option<Credito>,
    measured_at: Option<f64>,
    problem: Option<String>,
    pausa_ate: Option<f64>,
    #[serde(default)]
    pausa_firme: bool,
    #[serde(default)]
    ultimo_clique: f64,
}

fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("jev.json")
}

fn le_cache() -> Registro {
    std::fs::read_to_string(arquivo()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn grava_cache(r: &Registro) {
    let caminho = arquivo();
    if let Some(dir) = caminho.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(texto) = serde_json::to_vec_pretty(r) {
        let tmp = caminho.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, &caminho);
        }
    }
}

/// A chave do Jev, se esta máquina tem uma.
#[cfg(target_os = "macos")]
fn chave() -> Option<String> {
    let security = std::env::var("KEEP_IA_SECURITY").unwrap_or_else(|_| "/usr/bin/security".into());
    let mut filho = std::process::Command::new(security)
        .args(["find-generic-password", "-w", "-s", SERVICO])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // Um pedido de senha na tela não pode prender quem pergunta.
    let fim = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match filho.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < fim => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = filho.kill();
                let _ = filho.wait();
                return None;
            }
        }
    }
    let saida = filho.wait_with_output().ok()?;
    if !saida.status.success() {
        return None;
    }
    let texto = String::from_utf8_lossy(&saida.stdout).trim().to_string();
    (!texto.is_empty()).then_some(texto)
}

#[cfg(not(target_os = "macos"))]
fn chave() -> Option<String> {
    let _ = SERVICO;
    std::env::var("OPENROUTER_API_KEY").ok().map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
}

fn numero(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// `/api/v1/credits` e `/api/v1/key`, como o OpenRouter responde. Sem os
/// números da conta não há leitura; os da chave são acessórios.
pub fn ler(credits: &[u8], chave: Option<&[u8]>) -> Option<Credito> {
    let c: Value = serde_json::from_slice(credits).ok()?;
    let c = c.get("data")?;
    let k: Option<Value> = chave.and_then(|b| serde_json::from_slice(b).ok());
    let k = k.as_ref().and_then(|v| v.get("data"));
    Some(Credito {
        total: numero(c.get("total_credits"))?,
        used: numero(c.get("total_usage"))?,
        key_limit: numero(k.and_then(|k| k.get("limit"))),
        key_used: numero(k.and_then(|k| k.get("usage"))),
        used_today: numero(k.and_then(|k| k.get("usage_daily"))),
        used_this_month: numero(k.and_then(|k| k.get("usage_monthly"))),
    })
}

enum Resultado {
    Leitura(Credito),
    Falha(String, Option<(f64, bool)>),
}

fn pergunta(chave: &str, agora_s: f64) -> Resultado {
    let base = rede::url_de_teste(&["KEEP_IA_OPENROUTER_URL"], "https://openrouter.ai/api/v1");
    let cabecalhos = vec![("Authorization", format!("Bearer {chave}")), ("Accept", "application/json".to_string())];
    let credits = match rede::get(&format!("{base}/credits"), &cabecalhos, Duration::from_secs(15)) {
        Ok(r) => r,
        Err(_) => return Resultado::Falha("sem rede; tenta de novo em 1 min".into(), Some((agora_s + 60.0, false))),
    };
    match credits.status {
        200 => {}
        s @ (401 | 403) => {
            return Resultado::Falha(
                format!("o OpenRouter recusou a chave do Jev (HTTP {s})"),
                Some((agora_s + PERIODO, false)),
            );
        }
        429 => {
            let espera = credits.retry_after.and_then(|r| r.trim().parse::<f64>().ok()).map(|s| s.max(60.0)).unwrap_or(600.0);
            let ate = agora_s + espera;
            return Resultado::Falha(
                format!("consultas demais (HTTP 429); tenta de novo {}", crate::uso::momento(ate, agora_s)),
                Some((ate, true)),
            );
        }
        s => return Resultado::Falha(format!("sem leitura (HTTP {s})"), Some((agora_s + 60.0, false))),
    }
    // O teto da chave é um acessório: sem ele o saldo é o da conta.
    let da_chave = rede::get(&format!("{base}/key"), &cabecalhos, Duration::from_secs(15))
        .ok()
        .filter(|r| r.status == 200)
        .map(|r| r.corpo);
    match ler(&credits.corpo, da_chave.as_deref()) {
        Some(c) => Resultado::Leitura(c),
        None => Resultado::Falha("resposta sem créditos legíveis".into(), Some((agora_s + PERIODO, false))),
    }
}

/// O que o rodapé mostra: o Jev, ou nada quando não há chave.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Saldo {
    pub credit: Option<Credito>,
    pub measured_at: Option<f64>,
    pub problem: Option<String>,
}

fn saldo(r: Registro) -> Saldo {
    Saldo { credit: r.credit, measured_at: r.measured_at, problem: r.problem }
}

/// O que há guardado, sem ir à rede nem ao Chaveiro.
pub fn em_cache() -> Option<Saldo> {
    let r = le_cache();
    (r.credit.is_some() || r.problem.is_some()).then(|| saldo(r))
}

/// Uma rodada: mede se é devida (ou `forcar`). `None` quando não há chave,
/// e o que sobrou de uma chave que foi embora some junto.
pub fn medir(forcar: bool) -> Option<Saldo> {
    let Some(chave) = chave() else {
        let _ = std::fs::remove_file(arquivo());
        return None;
    };
    let mut r = le_cache();
    let agora_s = agora();
    let mut forcar = forcar;
    if forcar {
        if agora_s - r.ultimo_clique < ENTRE_CLIQUES {
            forcar = false;
        } else {
            r.ultimo_clique = agora_s;
        }
    }
    if let Some(ate) = r.pausa_ate {
        if ate > agora_s && (!forcar || r.pausa_firme) {
            return Some(saldo(r));
        }
    }
    let devida = forcar || r.problem.is_some() || r.measured_at.is_none_or(|m| agora_s - m >= PERIODO - 5.0);
    if !devida {
        return Some(saldo(r));
    }
    r.pausa_ate = None;
    r.pausa_firme = false;
    match pergunta(&chave, agora_s) {
        Resultado::Leitura(c) => {
            r.credit = Some(c);
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
    grava_cache(&r);
    Some(saldo(r))
}

/// `keep ia jev [--agora] [--cache]`. `jev: null` quando não há Jev nesta
/// máquina.
pub fn cli(a: &Args) -> i32 {
    let s = if a.tem("cache") { em_cache() } else { medir(a.tem("agora")) };
    responde(json!({ "ok": true, "jev": s, "medidoEm": agora() }))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contas::testes::casa;
    use crate::uso::testes::servidor;

    const CREDITOS: &str = r#"{"data":{"total_credits":10,"total_usage":0.0246}}"#;
    const CHAVE: &str = r#"{"data":{"limit":5,"limit_remaining":4.9958,"usage":0.0042,"usage_daily":0.0018,"usage_monthly":0.0042}}"#;

    fn com_chave(c: &crate::contas::testes::Casa) {
        std::fs::write(c.raiz.join("chaveiro").join(SERVICO), "sk-or-v1-segredo\n").unwrap();
    }

    fn aponta(porta_do_servidor: &str) {
        // SAFETY: a casa (e a trava) de pé: um teste por vez mexe no ambiente.
        unsafe { std::env::set_var("KEEP_IA_OPENROUTER_URL", porta_do_servidor) };
    }

    #[test]
    fn o_saldo_e_o_menor_dos_dois() {
        let l = ler(CREDITOS.as_bytes(), Some(CHAVE.as_bytes())).unwrap();
        assert!((l.saldo_da_conta() - 9.9754).abs() < 1e-9);
        assert!((l.saldo_da_chave().unwrap() - 4.9958).abs() < 1e-9);
        assert!((l.disponivel() - 4.9958).abs() < 1e-9, "a chave tem teto menor que a conta");
        assert_eq!(l.used_today, Some(0.0018));

        let sem_teto = ler(CREDITOS.as_bytes(), Some(br#"{"data":{"limit":null,"usage":1}}"#)).unwrap();
        assert!(sem_teto.saldo_da_chave().is_none());
        assert!((sem_teto.disponivel() - 9.9754).abs() < 1e-9, "sem teto vale o saldo da conta");

        let gasta = ler(br#"{"data":{"total_credits":10,"total_usage":10.5}}"#, None).unwrap();
        assert_eq!(gasta.disponivel(), 0.0, "gasto além do comprado não dá saldo negativo");

        assert!(ler(br#"{"data":{"total_credits":10}}"#, None).is_none(), "sem o gasto não há leitura");
        assert!(ler(b"nao e json", None).is_none());
    }

    #[test]
    fn sem_chave_nao_ha_jev_e_nada_vai_a_rede() {
        let c = casa("jev-sem-chave");
        let pedidos = servidor(|_| (200, vec![], CREDITOS.into()));
        let _ = &c;
        assert!(medir(true).is_none());
        assert!(pedidos.lock().unwrap().is_empty());
        assert!(em_cache().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mede_com_a_chave_do_chaveiro_e_respeita_a_cadencia() {
        let c = casa("jev-mede");
        com_chave(&c);
        let pedidos = servidor(|p| match p.caminho.as_str() {
            "/credits" => (200, vec![], CREDITOS.into()),
            "/key" => (200, vec![], CHAVE.into()),
            _ => (404, vec![], String::new()),
        });
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        aponta(&porta);

        let s = medir(false).unwrap();
        let l = s.credit.unwrap();
        assert!((l.disponivel() - 4.9958).abs() < 1e-9);
        assert!(s.problem.is_none() && s.measured_at.is_some());
        let feitos = pedidos.lock().unwrap().clone();
        assert_eq!(feitos.len(), 2);
        assert!(feitos.iter().all(|p| p.token == "sk-or-v1-segredo"), "a chave vai como Bearer, sem o fim de linha");

        medir(false);
        assert_eq!(pedidos.lock().unwrap().len(), 2, "medido há pouco: ninguém pergunta");
        medir(true);
        assert_eq!(pedidos.lock().unwrap().len(), 4, "o clique mede de novo");
        medir(true);
        assert_eq!(pedidos.lock().unwrap().len(), 4, "dois cliques em 15 s valem um");

        let guardado = std::fs::read_to_string(arquivo()).unwrap();
        assert!(!guardado.contains("segredo"), "o cache não guarda a chave");
        assert!(em_cache().unwrap().credit.is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recusa_e_429_viram_problema_e_guardam_a_ultima_leitura() {
        let c = casa("jev-recusa");
        com_chave(&c);
        let boa = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let estado = boa.clone();
        let pedidos = servidor(move |p| match (estado.load(std::sync::atomic::Ordering::SeqCst), p.caminho.as_str()) {
            (0, "/credits") => (200, vec![], CREDITOS.into()),
            (0, _) => (200, vec![], CHAVE.into()),
            (1, _) => (429, vec![("Retry-After", "120".into())], String::new()),
            _ => (401, vec![], String::new()),
        });
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        aponta(&porta);

        assert!(medir(false).unwrap().credit.is_some());
        boa.store(1, std::sync::atomic::Ordering::SeqCst);
        let s = medir(true).unwrap();
        assert!(s.problem.as_deref().is_some_and(|p| p.starts_with("consultas demais (HTTP 429)")), "{:?}", s.problem);
        assert!(s.credit.is_some(), "a última leitura fica, marcada como velha pelo problema");
        let antes = pedidos.lock().unwrap().len();
        medir(false);
        medir(true);
        assert_eq!(pedidos.lock().unwrap().len(), antes, "nem o clique passa por cima do 429");

        let mut r = le_cache();
        r.pausa_ate = None;
        r.ultimo_clique = 0.0;
        grava_cache(&r);
        boa.store(2, std::sync::atomic::Ordering::SeqCst);
        let s = medir(true).unwrap();
        assert_eq!(s.problem.as_deref(), Some("o OpenRouter recusou a chave do Jev (HTTP 401)"));
    }
}
