//! O câmbio dos valores do OpenRouter (pedido de 27/11/2026): o Keep mostra em reais o que o Jev gasta, com
//! R$ = US$ × PTAX × (1 + 2% de spread) × (1 + 3,5% de IOF).
//!
//! A PTAX é a cotação de venda do dólar no boletim de fechamento do Banco Central (API Olinda,
//! `CotacaoDolarPeriodo`): o último fechamento publicado. O Banco Central publica o do dia por volta das 13h de
//! Brasília; antes disso, e em fim de semana ou feriado, vale o anterior. O Keep busca uma vez por dia: quando o
//! guardado é mais velho que o fechamento que já deve estar publicado (`esperado`), e no máximo uma tentativa por
//! hora enquanto ele não vem (sem rede, ou o Banco Central ainda sem o boletim). Com o Keep fechado nesse horário, a
//! busca acontece quando ele abre: é a primeira leitura do Jev que a faz. A cotação fica guardada com a data
//! (`ptax.json`, no estado do Keep); a velha segue valendo até chegar a nova.
//!
//! `KEEP_IA_PTAX_URL` (só `http://127.0.0.1`, nos testes) troca o endereço do Banco Central.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{Datelike, Duration as Dias, FixedOffset, NaiveDate, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{caminhos, rede};

/// O spread do banco sobre a PTAX.
pub const SPREAD: f64 = 0.02;
/// O IOF da compra no cartão em moeda estrangeira.
pub const IOF: f64 = 0.035;
/// A hora de Brasília a partir da qual o fechamento do dia costuma estar publicado.
const FECHAMENTO_H: u32 = 13;
/// Entre duas tentativas que não trouxeram o fechamento esperado.
const ENTRE_TENTATIVAS_S: f64 = 3600.0;

/// O câmbio que o rodapé usa: a PTAX de venda de `data` e o que um dólar custa em reais com o spread e o IOF.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cambio {
    /// O dia do fechamento, AAAA-MM-DD.
    pub data: String,
    pub ptax: f64,
    pub spread: f64,
    pub iof: f64,
    /// Reais por dólar: `ptax × (1 + spread) × (1 + iof)`.
    pub fator: f64,
}

impl Cambio {
    pub fn de(data: String, ptax: f64) -> Cambio {
        Cambio { data, ptax, spread: SPREAD, iof: IOF, fator: ptax * (1.0 + SPREAD) * (1.0 + IOF) }
    }

    /// `usd` em reais.
    pub fn reais(&self, usd: f64) -> f64 {
        usd * self.fator
    }
}

/// O que fica guardado entre uma leitura e outra.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Guardado {
    data: Option<String>,
    ptax: Option<f64>,
    /// Quando o Keep foi ao Banco Central pela última vez (segundos desde 1970).
    #[serde(default)]
    tentou_em: f64,
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("ptax.json")
}

fn le() -> Guardado {
    std::fs::read_to_string(arquivo()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn grava(g: &Guardado) {
    let caminho = arquivo();
    if let Some(dir) = caminho.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(texto) = serde_json::to_vec_pretty(g) {
        let tmp = caminho.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, &caminho);
        }
    }
}

fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// O instante `s` (segundos desde 1970) no relógio de Brasília (UTC−3, sem horário de verão desde 2019).
fn em_brasilia(s: f64) -> chrono::DateTime<FixedOffset> {
    let brasilia = FixedOffset::west_opt(3 * 3600).expect("UTC−3");
    brasilia.timestamp_opt(s as i64, 0).single().unwrap_or_else(|| brasilia.timestamp_opt(0, 0).unwrap())
}

/// O fechamento que já deve estar publicado em `s`: o do dia útil, a partir das 13h de Brasília; antes, o do dia útil
/// anterior. Feriado não se sabe daqui: o Banco Central responde com o último que publicou, e a busca espera uma hora.
fn esperado(s: f64) -> NaiveDate {
    let t = em_brasilia(s);
    let mut dia = t.date_naive();
    if t.hour() < FECHAMENTO_H {
        dia = dia.pred_opt().unwrap_or(dia);
    }
    while matches!(dia.weekday(), Weekday::Sat | Weekday::Sun) {
        dia = dia.pred_opt().unwrap_or(dia);
    }
    dia
}

/// Se é hora de ir ao Banco Central: o guardado é mais velho que o fechamento esperado (ou não há nenhum), e a última
/// tentativa foi há mais de uma hora.
fn devida(g: &Guardado, s: f64) -> bool {
    let velho = g.data.as_deref().is_none_or(|d| d < esperado(s).format("%Y-%m-%d").to_string().as_str());
    velho && s - g.tentou_em >= ENTRE_TENTATIVAS_S
}

/// O último fechamento da resposta do Banco Central: (dia, PTAX de venda).
fn ler(corpo: &[u8]) -> Option<(String, f64)> {
    let v: Value = serde_json::from_slice(corpo).ok()?;
    v.get("value")?
        .as_array()?
        .iter()
        .filter_map(|c| {
            let quando = c.get("dataHoraCotacao")?.as_str()?;
            let venda = c.get("cotacaoVenda")?.as_f64().filter(|v| *v > 0.5 && *v < 50.0)?;
            Some((quando.get(..10)?.to_string(), venda))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// Os fechamentos dos últimos dez dias até `s`; o mais recente deles.
fn buscar(s: f64) -> Option<(String, f64)> {
    let base = rede::url_de_teste(&["KEEP_IA_PTAX_URL"], "https://olinda.bcb.gov.br/olinda/servico/PTAX/versao/v1/odata");
    let hoje = em_brasilia(s).date_naive();
    let de = hoje - Dias::days(10);
    let url = format!(
        "{base}/CotacaoDolarPeriodo(dataInicial=@dataInicial,dataFinalCotacao=@dataFinalCotacao)\
         ?@dataInicial='{}'&@dataFinalCotacao='{}'&$format=json&$select=cotacaoVenda,dataHoraCotacao",
        de.format("%m-%d-%Y"),
        hoje.format("%m-%d-%Y")
    );
    let r = rede::get(&url, &[("Accept", "application/json".to_string())], Duration::from_secs(15)).ok()?;
    (r.status == 200).then(|| ler(&r.corpo)).flatten()
}

/// O câmbio guardado, sem ir à rede.
pub fn guardado() -> Option<Cambio> {
    let g = le();
    Some(Cambio::de(g.data?, g.ptax?))
}

/// Vai ao Banco Central se for hora (`devida`), guarda o que vier, e devolve o câmbio de agora.
pub fn atualizar() -> Option<Cambio> {
    atualizar_em(agora())
}

fn atualizar_em(s: f64) -> Option<Cambio> {
    let mut g = le();
    if devida(&g, s) {
        g.tentou_em = s;
        // Só um fechamento mais novo que o guardado o troca.
        if let Some((data, ptax)) = buscar(s).filter(|(d, _)| g.data.as_deref().is_none_or(|velha| d.as_str() >= velha)) {
            g.data = Some(data);
            g.ptax = Some(ptax);
        }
        grava(&g);
    }
    Some(Cambio::de(g.data?, g.ptax?))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contas::testes::casa;
    use crate::uso::testes::servidor;

    /// O instante de Brasília dado, em segundos desde 1970.
    fn as_(ano: i32, mes: u32, dia: u32, h: u32, min: u32) -> f64 {
        FixedOffset::west_opt(3 * 3600).unwrap().with_ymd_and_hms(ano, mes, dia, h, min, 0).unwrap().timestamp() as f64
    }

    fn boletim(fechamentos: &[(&str, f64)]) -> String {
        let itens: Vec<String> = fechamentos
            .iter()
            .map(|(d, v)| format!(r#"{{"cotacaoCompra":{},"cotacaoVenda":{v},"dataHoraCotacao":"{d} 13:04:31.123"}}"#, v - 0.0006))
            .collect();
        format!(r#"{{"@odata.context":"x","value":[{}]}}"#, itens.join(","))
    }

    #[test]
    fn a_conta_e_ptax_vezes_spread_vezes_iof() {
        let c = Cambio::de("2026-11-26".into(), 5.0);
        assert!((c.fator - 5.0 * 1.02 * 1.035).abs() < 1e-12, "{}", c.fator);
        assert!((c.reais(16.61) - 16.61 * 5.2785).abs() < 1e-9, "{}", c.reais(16.61));
        assert!((c.reais(16.61) - 87.675885).abs() < 1e-6);
        assert_eq!((c.spread, c.iof), (0.02, 0.035));
    }

    #[test]
    fn o_fechamento_esperado_e_o_do_dia_util_depois_das_13h() {
        let d = |s| esperado(s).format("%Y-%m-%d").to_string();
        // sexta 27/11/2026
        assert_eq!(d(as_(2026, 11, 27, 12, 59)), "2026-11-26", "antes das 13h, o de ontem");
        assert_eq!(d(as_(2026, 11, 27, 13, 0)), "2026-11-27", "a partir das 13h, o de hoje");
        // fim de semana e segunda de manhã: o da sexta
        assert_eq!(d(as_(2026, 11, 28, 15, 0)), "2026-11-27");
        assert_eq!(d(as_(2026, 11, 29, 9, 0)), "2026-11-27");
        assert_eq!(d(as_(2026, 11, 30, 10, 0)), "2026-11-27");
        assert_eq!(d(as_(2026, 11, 30, 14, 0)), "2026-11-30");
        // meia-noite em UTC ainda é a véspera em Brasília
        assert_eq!(d(as_(2026, 11, 26, 22, 30)), "2026-11-26");
    }

    #[test]
    fn le_o_ultimo_fechamento_e_recusa_o_absurdo() {
        let corpo = boletim(&[("2026-11-25", 5.31), ("2026-11-26", 5.2351), ("2026-11-24", 5.4)]);
        assert_eq!(ler(corpo.as_bytes()), Some(("2026-11-26".into(), 5.2351)));
        assert_eq!(ler(br#"{"value":[]}"#), None);
        assert_eq!(ler(boletim(&[("2026-11-26", 0.0)]).as_bytes()), None, "zero não é cotação");
        assert_eq!(ler(b"nada"), None);
    }

    #[test]
    fn busca_uma_vez_por_dia_ao_abrir_e_sem_rede_segue_com_a_velha() {
        let c = casa("ptax");
        let _ = &c;
        let pedidos = servidor(|p| {
            if p.caminho.contains("CotacaoDolarPeriodo") && p.caminho.contains("$format=json") {
                (200, vec![], boletim(&[("2026-11-25", 5.31), ("2026-11-26", 5.2351)]))
            } else {
                (404, vec![], String::new())
            }
        });
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        // SAFETY: a casa (e a trava) de pé: um teste por vez mexe no ambiente.
        unsafe { std::env::set_var("KEEP_IA_PTAX_URL", &porta) };
        let quantos = || pedidos.lock().unwrap().len();

        // A primeira leitura (o Keep abriu): vai ao Banco Central e guarda o último fechamento.
        assert!(guardado().is_none());
        let hoje_cedo = as_(2026, 11, 27, 9, 0);
        let c1 = atualizar_em(hoje_cedo).expect("o câmbio");
        assert_eq!((c1.data.as_str(), c1.ptax), ("2026-11-26", 5.2351));
        assert_eq!(quantos(), 1);
        assert!(pedidos.lock().unwrap()[0].caminho.contains("@dataFinalCotacao='11-27-2026'"), "{:?}", pedidos.lock().unwrap()[0]);
        assert_eq!(guardado(), Some(c1.clone()));

        // O resto da manhã: o de ontem é o esperado; nada de rede.
        for min in [1, 30, 200] {
            assert_eq!(atualizar_em(hoje_cedo + min as f64 * 60.0), Some(c1.clone()));
        }
        assert_eq!(quantos(), 1, "uma vez por dia");

        // 13h: o de hoje é o esperado. O Banco Central ainda não o tem: uma tentativa, e a próxima só daqui a uma hora,
        // com a cotação de ontem valendo.
        let treze = as_(2026, 11, 27, 13, 5);
        assert_eq!(atualizar_em(treze), Some(c1.clone()));
        assert_eq!(quantos(), 2);
        assert_eq!(atualizar_em(treze + 600.0), Some(c1.clone()));
        assert_eq!(quantos(), 2, "uma tentativa por hora enquanto não vem");

        // Publicado: a próxima tentativa o traz, e o resto do dia não vai mais à rede.
        drop(pedidos);
        let pedidos = servidor(|_| (200, vec![], boletim(&[("2026-11-26", 5.2351), ("2026-11-27", 5.1987)])));
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        unsafe { std::env::set_var("KEEP_IA_PTAX_URL", &porta) };
        let c2 = atualizar_em(treze + 3700.0).expect("o câmbio");
        assert_eq!((c2.data.as_str(), c2.ptax), ("2026-11-27", 5.1987));
        assert_eq!(pedidos.lock().unwrap().len(), 1);
        assert_eq!(atualizar_em(as_(2026, 11, 27, 23, 0)), Some(c2.clone()));
        // Fim de semana: o de sexta segue o esperado; nada de rede até a segunda às 13h.
        assert_eq!(atualizar_em(as_(2026, 11, 29, 12, 0)), Some(c2.clone()));
        assert_eq!(pedidos.lock().unwrap().len(), 1, "o fim de semana não busca");

        // Sem rede na segunda depois das 13h: segue com a de sexta, com a data dela.
        unsafe { std::env::set_var("KEEP_IA_PTAX_URL", "http://127.0.0.1:9") };
        let segunda = as_(2026, 11, 30, 14, 0);
        assert_eq!(atualizar_em(segunda), Some(c2.clone()));
        assert_eq!(guardado().map(|c| c.data), Some("2026-11-27".into()));
    }

    #[test]
    fn sem_cotacao_nenhuma_e_sem_rede_nao_ha_cambio() {
        let c = casa("ptax-sem-rede");
        let _ = &c;
        unsafe { std::env::set_var("KEEP_IA_PTAX_URL", "http://127.0.0.1:9") };
        assert_eq!(atualizar_em(as_(2026, 11, 27, 15, 0)), None);
        assert!(guardado().is_none());
    }
}
