//! A renovação numa casa falsa: logins escritos à mão (Chaveiro falso no
//! macOS, arquivo nos demais) e um servidor de tokens falso nesta máquina.
//! Nenhum teste toca em conta de verdade.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use super::*;
use crate::contas::testes::{Casa, casa, perfis};

const HORA: u64 = 3_600_000;

/// O que roda no servidor quando um pedido chega.
type NoMeio = Arc<Mutex<Option<Box<dyn Fn() + Send>>>>;

/// O servidor de tokens: refresh → (status, corpo); conta os pedidos e
/// guarda o último `User-Agent` e o último corpo.
#[derive(Clone, Default)]
struct Servidor {
    respostas: Arc<Mutex<HashMap<String, (u16, String)>>>,
    pedidos: Arc<Mutex<Vec<(String, Value)>>>,
    /// Roda ao chegar um pedido, antes da resposta (o "CLI" mexendo no meio).
    no_meio: NoMeio,
}

impl Servidor {
    fn sobe() -> Servidor {
        let s = Servidor::default();
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let eu = s.clone();
        std::thread::spawn(move || {
            for conexao in ouvinte.incoming() {
                let Ok(mut c) = conexao else { continue };
                let mut dados = Vec::new();
                let mut buf = [0u8; 8192];
                // Cabeçalhos e corpo (Content-Length).
                loop {
                    let n = c.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    dados.extend_from_slice(&buf[..n]);
                    let texto = String::from_utf8_lossy(&dados).to_string();
                    if let Some(i) = texto.find("\r\n\r\n") {
                        let tamanho = texto[..i]
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if dados.len() >= i + 4 + tamanho {
                            break;
                        }
                    }
                }
                let texto = String::from_utf8_lossy(&dados).to_string();
                let (cab, corpo) = texto.split_once("\r\n\r\n").unwrap_or((&texto, ""));
                let agente = cab
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("user-agent").then(|| v.trim().to_string())
                    })
                    .unwrap_or_default();
                let json: Value = serde_json::from_str(corpo).unwrap_or(Value::Null);
                let refresh = json.get("refresh_token").and_then(Value::as_str).unwrap_or("").to_string();
                eu.pedidos.lock().unwrap().push((agente, json));
                if let Some(f) = eu.no_meio.lock().unwrap().as_ref() {
                    f();
                }
                let (status, corpo) = eu
                    .respostas
                    .lock()
                    .unwrap()
                    .get(&refresh)
                    .cloned()
                    .unwrap_or((400, r#"{"error":"invalid_grant"}"#.into()));
                let r = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
                    corpo.len()
                );
                let _ = c.write_all(r.as_bytes());
            }
        });
        // SAFETY: chamado com a casa (e a trava) de pé.
        unsafe { std::env::set_var("KEEP_IA_TOKEN_URL", format!("http://127.0.0.1:{porta}/v1/oauth/token")) };
        s
    }

    fn responde(&self, refresh: &str, status: u16, corpo: &str) {
        self.respostas.lock().unwrap().insert(refresh.into(), (status, corpo.into()));
    }

    fn gira(&self, refresh: &str, acesso_novo: &str, refresh_novo: &str) {
        self.responde(
            refresh,
            200,
            &format!(
                r#"{{"access_token":"{acesso_novo}","refresh_token":"{refresh_novo}","expires_in":28800,"scope":"user:inference user:profile"}}"#
            ),
        );
    }

    fn quantos(&self) -> usize {
        self.pedidos.lock().unwrap().len()
    }
}

/// O JSON de um login, com uma chave a mais que a renovação tem de manter.
fn login(acesso: &str, refresh: &str, expira_ms: u64) -> Value {
    serde_json::from_str(&format!(
        r#"{{"claudeAiOauth":{{"accessToken":"{acesso}","refreshToken":"{refresh}","expiresAt":{expira_ms},"rateLimitTier":"default_claude_max_20x","subscriptionType":"max","scopes":["user:inference"]}},"mcpOAuth":{{"srv":{{"token":"m"}}}}}}"#
    ))
    .unwrap()
}

/// Grava o login de uma pasta fixa (nova, com keep.json) ou do global.
fn poe(c: &Casa, onde: Option<&str>, v: &Value) -> Local {
    let local = match onde {
        Some(nome) => {
            let pasta = c.fixa(nome, None, Some((nome, &format!("{nome}@x.com"), &format!("u-{nome}"))));
            credencial::local_fixa(&pasta)
        }
        None => credencial::local_global(),
    };
    escreve(c, &local, v);
    local
}

fn escreve(c: &Casa, local: &Local, v: &Value) {
    match local {
        Local::Chaveiro(s) => std::fs::write(c.raiz.join("chaveiro").join(s), v.to_string()).unwrap(),
        Local::Arquivo(a) => std::fs::write(a, v.to_string()).unwrap(),
    }
}

fn le(local: &Local) -> Value {
    match credencial::ler(local) {
        Lido::Ok(v) => v,
        outro => panic!("login ilegível: {outro:?}"),
    }
}

fn oauth(local: &Local) -> Oauth {
    Oauth::de(&le(local)).unwrap()
}

#[test]
fn renova_o_que_vence_em_menos_de_uma_hora_e_mantem_o_resto() {
    let c = casa("renova-perto");
    perfis(&[]);
    let s = Servidor::sobe();
    s.gira("R1", "A2", "R2");
    let agora = donos::agora_ms();
    let local = poe(&c, Some("k-1"), &login("A1", "R1", agora + 30 * 60_000));
    let r = rodada(false);
    assert_eq!(r.renovados.len(), 1, "{r:?}");
    assert_eq!(r.renovados[0].2, Motivo::PertoDeVencer);
    let v = le(&local);
    let o = Oauth::de(&v).unwrap();
    assert_eq!((o.acesso.as_deref(), o.refresh.as_deref()), (Some("A2"), Some("R2")));
    assert!(o.expira_em.unwrap() >= agora + 7 * HORA, "o acesso novo vale 8 h");
    assert_eq!(o.nivel.as_deref(), Some("default_claude_max_20x"), "o resto do claudeAiOauth fica");
    assert_eq!(v["mcpOAuth"]["srv"]["token"], "m", "as outras chaves ficam");
    assert_eq!(v["claudeAiOauth"]["scopes"], json!(["user:inference", "user:profile"]));
    let (agente, corpo) = s.pedidos.lock().unwrap()[0].clone();
    assert!(agente.starts_with("claude-cli/") && agente.ends_with("(external, cli)"), "{agente}");
    assert_eq!(corpo["grant_type"], "refresh_token");
    assert_eq!(corpo["client_id"], CLIENT_ID);
    assert_eq!(corpo["scope"], "user:inference", "pede os escopos que o login tem");
    // As travas do CLI foram devolvidas.
    assert!(!caminhos::claude().join(".oauth_refresh.lock").exists());
    assert!(!caminhos::claude().join(".storage-write.lock").exists());
    // A rodada seguinte não renova de novo.
    assert!(rodada(false).renovados.is_empty());
    assert_eq!(s.quantos(), 1);
}

#[test]
fn nao_renova_o_que_ainda_tem_folga() {
    let c = casa("renova-folga");
    perfis(&[]);
    let s = Servidor::sobe();
    s.gira("R1", "A2", "R2");
    let local = poe(&c, Some("k-1"), &login("A1", "R1", donos::agora_ms() + 3 * HORA));
    let r = rodada(false);
    assert!(r.renovados.is_empty() && r.falhas.is_empty(), "{r:?}");
    assert_eq!(s.quantos(), 0);
    assert_eq!(oauth(&local).acesso.as_deref(), Some("A1"));
    // "Renovar agora" renova assim mesmo.
    assert_eq!(rodada(true).renovados.len(), 1);
    assert_eq!(oauth(&local).acesso.as_deref(), Some("A2"));
}

#[test]
fn renova_a_conta_parada_ha_mais_de_sete_dias_e_deixa_a_recente() {
    let c = casa("renova-parada");
    perfis(&[]);
    let s = Servidor::sobe();
    s.gira("RV", "AV2", "RV2");
    s.gira("RN", "AN2", "RN2");
    let agora = donos::agora_ms();
    // Venceu há 7 dias: girou há 7 dias e 8 h.
    let velha = poe(&c, Some("k-velha"), &login("AV", "RV", agora - 7 * 24 * HORA));
    // Venceu há 2 dias: girou há 2 dias e 8 h.
    let nova = poe(&c, Some("k-nova"), &login("AN", "RN", agora - 2 * 24 * HORA));
    let r = rodada(false);
    assert_eq!(r.renovados.len(), 1, "{r:?}");
    assert_eq!(r.renovados[0].2, Motivo::Parada);
    assert_eq!(oauth(&velha).refresh.as_deref(), Some("RV2"));
    assert_eq!(oauth(&nova).refresh.as_deref(), Some("RN"), "a parada há 2 dias espera");
}

#[test]
fn desiste_de_gravar_quando_o_login_muda_no_meio() {
    let c = casa("renova-cas");
    perfis(&[]);
    let s = Servidor::sobe();
    s.gira("R1", "A2", "R2");
    let local = poe(&c, Some("k-1"), &login("A1", "R1", donos::agora_ms() + 10 * 60_000));
    // O CLI (um login novo) grava outro refresh enquanto o pedido está no ar.
    let (raiz, l2) = (c.raiz.clone(), local.clone());
    *s.no_meio.lock().unwrap() = Some(Box::new(move || {
        let v = login("AX", "RX", donos::agora_ms() + 8 * HORA);
        match &l2 {
            Local::Chaveiro(n) => std::fs::write(raiz.join("chaveiro").join(n), v.to_string()).unwrap(),
            Local::Arquivo(a) => std::fs::write(a, v.to_string()).unwrap(),
        }
    }));
    let r = rodada(false);
    assert!(r.renovados.is_empty(), "{r:?}");
    assert!(r.falhas[0].2.contains("mudou"), "{r:?}");
    assert_eq!(oauth(&local).refresh.as_deref(), Some("RX"), "o login de quem mexeu ficou");
}

#[test]
fn refresh_morto_marca_o_armazem_e_nao_insiste() {
    let c = casa("renova-morto");
    perfis(&[("A1", "u-k-1", "k-1@x.com")]);
    let s = Servidor::sobe();
    s.responde("R1", 400, r#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#);
    let local = poe(&c, Some("k-1"), &login("A1", "R1", donos::agora_ms() + 10 * 60_000));
    let r = rodada(false);
    assert!(r.falhas[0].2.contains("entre de novo"), "{r:?}");
    assert!(rodada(false).falhas.is_empty(), "não tenta de novo");
    assert_eq!(s.quantos(), 1);
    // A conta passa a pedir login: aviso no rodapé, e a aba não sobe nela.
    let contas = contas::listar(true);
    let k1 = contas.iter().find(|c| c.alias == "k-1").expect("a conta");
    assert!(k1.warning.as_deref().is_some_and(|w| w.contains("recusado")), "{:?}", k1.warning);
    assert!(!k1.roda);
    assert!(matches!(contas::ambiente(k1), Err(contas::Falta::PrecisaLogin { .. })));
    // Um login novo (outro refresh) apaga a marca sozinho.
    s.gira("R9", "A10", "R10");
    escreve(&c, &local, &login("A9", "R9", donos::agora_ms() + 10 * 60_000));
    assert_eq!(rodada(false).renovados.len(), 1);
    assert!(contas::ambiente(contas::listar(false).iter().find(|c| c.alias == "k-1").unwrap()).is_ok());
}

#[test]
fn conta_em_espera_tenta_de_novo_so_depois_de_uma_hora() {
    let c = casa("renova-espera");
    perfis(&[]);
    let s = Servidor::sobe();
    s.responde("R1", 400, r#"{"error":"invalid_grant","error_description":"account_on_hold"}"#);
    poe(&c, Some("k-1"), &login("A1", "R1", donos::agora_ms() + 10 * 60_000));
    assert!(rodada(false).falhas[0].2.contains("espera"));
    assert!(rodada(false).falhas.is_empty());
    assert_eq!(s.quantos(), 1);
    let marca = ler_marcas().into_values().next().unwrap();
    assert!(marca.morto.is_none(), "espera não é morte");
    assert!(marca.espera_ate > donos::agora_ms() + 50 * 60_000);
}

#[test]
fn falha_passageira_tenta_no_tique_seguinte() {
    let c = casa("renova-passageira");
    perfis(&[]);
    let s = Servidor::sobe();
    s.responde("R1", 503, r#"{"error":{"type":"overloaded_error"}}"#);
    let local = poe(&c, Some("k-1"), &login("A1", "R1", donos::agora_ms() + 10 * 60_000));
    assert_eq!(rodada(false).falhas.len(), 1);
    s.gira("R1", "A2", "R2");
    assert_eq!(rodada(false).renovados.len(), 1);
    assert_eq!(oauth(&local).refresh.as_deref(), Some("R2"));
    // Sem servidor nenhum (sem rede) também não é morte.
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_TOKEN_URL", "http://127.0.0.1:9/nada") };
    let r = rodada(true);
    assert_eq!(r.falhas.len(), 1);
    assert!(ler_marcas().into_values().all(|m| m.morto.is_none()));
}

#[test]
fn o_global_so_quando_nao_ha_gerente_externo() {
    let c = casa("renova-global");
    perfis(&[]);
    let s = Servidor::sobe();
    s.gira("RG", "AG2", "RG2");
    let local = poe(&c, None, &login("AG", "RG", donos::agora_ms() + 10 * 60_000));
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_GERENTE_EXTERNO", "1") };
    assert!(rodada(false).renovados.is_empty(), "o kit renova o global");
    assert_eq!(s.quantos(), 0);
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_GERENTE_EXTERNO", "0") };
    let r = rodada(false);
    assert_eq!(r.renovados.len(), 1, "{r:?}");
    assert_eq!(r.renovados[0].1, "global");
    assert_eq!(oauth(&local).refresh.as_deref(), Some("RG2"));
}

#[test]
fn nunca_toca_no_cofre_do_kit() {
    let c = casa("renova-cofre");
    perfis(&[]);
    let s = Servidor::sobe();
    c.slot("reserva", "u-9", "r@x.com", "S1");
    let antes = std::fs::read_to_string(caminhos::contas().join("reserva.json")).unwrap();
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_GERENTE_EXTERNO", "1") };
    assert!(rodada(true).renovados.is_empty());
    assert_eq!(s.quantos(), 0);
    assert_eq!(std::fs::read_to_string(caminhos::contas().join("reserva.json")).unwrap(), antes);
}

#[test]
fn espera_a_trava_do_cli_e_desiste_se_ele_esta_renovando() {
    let c = casa("renova-trava");
    perfis(&[]);
    let s = Servidor::sobe();
    s.gira("R1", "A2", "R2");
    let local = poe(&c, Some("k-1"), &login("A1", "R1", donos::agora_ms() + 10 * 60_000));
    let trava = caminhos::claude().join(".oauth_refresh.lock");
    std::fs::create_dir(&trava).unwrap();
    let r = rodada(false);
    assert!(r.falhas[0].2.contains("renovando"), "{r:?}");
    assert_eq!(s.quantos(), 0);
    assert!(trava.exists(), "a trava do CLI não é nossa para tirar");
    std::fs::remove_dir(&trava).unwrap();
    assert_eq!(rodada(false).renovados.len(), 1);
    assert_eq!(oauth(&local).refresh.as_deref(), Some("R2"));
}

#[test]
fn a_recusa_se_classifica_como_no_cli() {
    assert_eq!(classificar(400, r#"{"error":"invalid_grant"}"#), Recusa::Morto);
    assert_eq!(classificar(401, r#"{"error":{"type":"invalid_grant"}}"#), Recusa::Morto);
    assert_eq!(
        classificar(400, r#"{"error":"invalid_grant","error_description":"account_on_hold"}"#),
        Recusa::Espera
    );
    assert_eq!(
        classificar(403, r#"{"error":{"type":"access_denied","error_description":"account_on_hold"}}"#),
        Recusa::Espera
    );
    assert_eq!(classificar(429, r#"{"error":"rate_limited"}"#), Recusa::Transitorio);
    assert_eq!(classificar(500, r#"{"error":"invalid_grant"}"#), Recusa::Transitorio, "5xx nunca é morte");
    assert_eq!(classificar(403, "error code: 1010"), Recusa::Transitorio, "o Cloudflare");
    assert_eq!(classificar(400, "invalid_grant"), Recusa::Morto);
}

#[test]
fn a_decisao_usa_o_nascimento_do_acesso() {
    let agora = 100 * 24 * HORA;
    let o = |exp: u64| Oauth { refresh: Some("R".into()), expira_em: Some(exp), ..Default::default() };
    let mut m = Marca::default();
    assert_eq!(decide(&o(agora + 30 * 60_000), &mut m, agora, false), Some(Motivo::PertoDeVencer));
    let mut m = Marca::default();
    assert_eq!(decide(&o(agora + 2 * HORA), &mut m, agora, false), None);
    let mut m = Marca::default();
    assert_eq!(decide(&o(agora - 7 * 24 * HORA), &mut m, agora, false), Some(Motivo::Parada));
    let mut m = Marca::default();
    assert_eq!(decide(&o(agora - HORA), &mut m, agora, false), None, "venceu há pouco: o CLI renova ao usar");
    // Sem refresh (o CLI zerou): nada a fazer, só um login.
    let mut m = Marca::default();
    assert_eq!(decide(&Oauth::default(), &mut m, agora, true), None);
}
