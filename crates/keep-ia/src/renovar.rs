//! Renovação proativa dos logins do Claude: nenhuma conta cai, nem a parada.
//!
//! O que a Clínica faz com as contas dela (`SetupClaudeService::
//! renovarTokenSePossivel` + o `ia:healthcheck`), e o kit faz com o login
//! global (`claude-conectado`), aqui para todo armazém que o Keep cuida:
//!
//! - perto de vencer (acesso com menos de [`FOLGA_MS`]): renova antes do
//!   próprio Claude Code, que só tenta nos 5 min finais — é nesses minutos
//!   que várias abas cruzam o limiar juntas e uma derruba a outra;
//! - parada (refresh sem girar há mais de [`PARADA_MS`]): renova para o
//!   refresh token, que vive umas quatro semanas, não morrer em silêncio — a
//!   conta parada é a que tem de estar viva na hora do aperto.
//!
//! Armazéns: as pastas fixas, sempre; o login global só sem gerente externo
//! (o kit renova o global sozinho). Nunca o cofre do kit, nunca cópia de um
//! armazém para outro.
//!
//! Tudo sob a trava de renovação do próprio Claude Code
//! (`~/.claude/.oauth_refresh.lock`, um diretório; velha após 60 s), para a
//! nossa renovação nunca correr junto com a de uma aba, e com a gravação em
//! compare-and-swap sob a trava de escrita dele (`.storage-write.lock`): se o
//! refresh do armazém mudou enquanto o pedido estava no ar, a resposta é
//! descartada. Só a chave `claudeAiOauth` muda; o resto do JSON fica.
//!
//! Recusa do servidor, no critério do CLI: `morto` (invalid_grant) marca o
//! armazém, que passa a pedir login e não é tentado de novo até a credencial
//! mudar; `espera` (account_on_hold) tenta de novo em 1 h; o resto (rede,
//! 429, 5xx, Cloudflare) é passageiro e tenta no próximo tique.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::contas::credencial::{self, Lido, Local, Oauth};
use crate::contas::{self, Armazem, donos};
use crate::{caminhos, programas, rede};

/// Renova o acesso que vence antes disto.
pub const FOLGA_MS: u64 = 60 * 60_000;
/// Renova o login cujo refresh não gira há mais disto.
pub const PARADA_MS: u64 = 7 * 24 * 3_600_000;
/// Conta em espera no servidor: a próxima tentativa.
pub const ESPERA_MS: u64 = 3_600_000;
/// Quanto vive um acesso (`expires_in` do servidor): o acesso que vence em
/// `t` nasceu, e girou o refresh, em `t - DURACAO_ACESSO_MS`.
const DURACAO_ACESSO_MS: u64 = 8 * 3_600_000;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
/// Os escopos que o CLI pede quando o login não diz os dele.
const ESCOPOS: [&str; 5] =
    ["user:profile", "user:inference", "user:sessions:claude_code", "user:mcp_servers", "user:file_upload"];

/// Um armazém que o Keep renova.
#[derive(Clone, Debug)]
pub struct Alvo {
    /// Identidade no estado: `global` ou `fixa:<pasta>`.
    pub id: String,
    /// `global` ou `fixa`.
    pub armazem: &'static str,
    /// A chave da conta (`claude:reserva`), ou a pasta, se não se sabe.
    pub conta: String,
    pub local: Local,
}

/// O que se sabe de um armazém entre um tique e outro. Nunca o token: só a
/// impressão do refresh.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Marca {
    /// sha256[:12] do refresh visto por último.
    pub impressao: String,
    /// Quando esse refresh foi visto girar (unix ms).
    pub girado_em: u64,
    /// O refresh que o servidor recusou (impressão): o armazém precisa de login.
    pub morto: Option<String>,
    /// Conta em espera no servidor: não tentar antes disto (unix ms).
    pub espera_ate: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recusa {
    Morto,
    Espera,
    Transitorio,
}

/// A recusa do servidor de tokens, no critério do CLI (`ma()` no 2.1.282, o
/// mesmo do kit): o código em `error` (texto) ou `error.type` (objeto); conta
/// em espera (`account_on_hold`) se recupera; invalid_grant em 400/401 é
/// refresh morto; o resto é passageiro.
pub fn classificar(status: u16, corpo: &str) -> Recusa {
    if let Ok(Value::Object(j)) = serde_json::from_str::<Value>(corpo) {
        let erro = j.get("error");
        let codigo = match erro {
            Some(Value::String(s)) => Some(s.as_str()),
            Some(Value::Object(o)) => o.get("type").and_then(Value::as_str),
            _ => None,
        };
        let descricao = j.get("error_description").and_then(Value::as_str).or_else(|| match erro {
            Some(Value::Object(o)) => {
                o.get("error_description").or_else(|| o.get("message")).and_then(Value::as_str)
            }
            _ => None,
        });
        if matches!(codigo, Some("invalid_grant" | "access_denied")) && descricao == Some("account_on_hold") {
            return Recusa::Espera;
        }
        if matches!(status, 400 | 401) && codigo == Some("invalid_grant") {
            return Recusa::Morto;
        }
        return Recusa::Transitorio;
    }
    if corpo.contains("account_on_hold") {
        return Recusa::Espera;
    }
    if matches!(status, 400 | 401) && corpo.contains("invalid_grant") {
        return Recusa::Morto;
    }
    Recusa::Transitorio
}

pub fn impressao(refresh: &str) -> String {
    Sha256::digest(refresh.as_bytes()).iter().take(6).map(|b| format!("{b:02x}")).collect()
}

// ------------------------------------------------------------------ estado

fn arquivo_estado() -> PathBuf {
    caminhos::estado().join("renovar.json")
}

pub fn ler_marcas() -> BTreeMap<String, Marca> {
    std::fs::read_to_string(arquivo_estado()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn gravar_marcas(m: &BTreeMap<String, Marca>) {
    let arq = arquivo_estado();
    let _ = std::fs::create_dir_all(caminhos::estado());
    let tmp = arq.with_extension(format!("tmp-{}", std::process::id()));
    if std::fs::write(&tmp, serde_json::to_vec_pretty(m).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, &arq);
    }
}

fn registra(linha: &str) {
    use std::io::Write as _;
    let arq = caminhos::estado().join("renovar.log");
    let _ = std::fs::create_dir_all(caminhos::estado());
    // Um registro curto: passou de 256 KB, recomeça.
    if std::fs::metadata(&arq).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::rename(&arq, arq.with_extension("log.1"));
    }
    let agora = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&arq) {
        let _ = writeln!(f, "{agora}  {linha}");
    }
}

fn id_de(armazem: &Armazem) -> Option<String> {
    match armazem {
        Armazem::Global => Some("global".into()),
        Armazem::Fixa { pasta } => Some(format!("fixa:{}", pasta.to_string_lossy())),
        _ => None,
    }
}

/// Se o servidor recusou o refresh que este armazém guarda: o login dele
/// precisa ser refeito. Quando a credencial muda (um login, ou o CLI que
/// renovou), a impressão deixa de casar e a marca some sozinha.
pub fn morto(armazem: &Armazem, oauth: &Oauth) -> bool {
    let (Some(id), Some(refresh)) = (id_de(armazem), oauth.refresh.as_deref()) else { return false };
    ler_marcas().get(&id).and_then(|m| m.morto.as_deref()).is_some_and(|x| x == impressao(refresh))
}

// ------------------------------------------------------------------ quem

/// Os armazéns que o Keep renova, com a conta de cada um.
pub fn alvos() -> Vec<Alvo> {
    let lista = contas::listar(false);
    let conta_de = |a: &Armazem| {
        lista.iter().find(|c| c.armazens.contains(a)).map(|c| c.order_key.clone())
    };
    let mut saida = Vec::new();
    if !contas::gerente_externo() {
        saida.push(Alvo {
            id: "global".into(),
            armazem: "global",
            conta: conta_de(&Armazem::Global).unwrap_or_else(|| "global".into()),
            local: credencial::local_global(),
        });
    }
    let Ok(dir) = std::fs::read_dir(caminhos::fixas()) else { return saida };
    let mut pastas: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.')))
        .collect();
    pastas.sort();
    for pasta in pastas {
        let armazem = Armazem::Fixa { pasta: pasta.clone() };
        let nome = pasta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        saida.push(Alvo {
            id: id_de(&armazem).unwrap_or_default(),
            armazem: "fixa",
            conta: conta_de(&armazem).unwrap_or(nome),
            local: credencial::local_fixa(&pasta),
        });
    }
    saida
}

/// Por que renovar agora, se for o caso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motivo {
    PertoDeVencer,
    Parada,
    Pedido,
}

/// A decisão de um armazém, com a marca dele atualizada para o que se vê.
pub fn decide(oauth: &Oauth, marca: &mut Marca, agora: u64, forcar: bool) -> Option<Motivo> {
    let refresh = oauth.refresh.as_deref()?;
    let imp = impressao(refresh);
    if marca.impressao != imp {
        // Girou (o CLI renovou, um login novo), ou é a primeira vez: o
        // acesso diz quando nasceu; sem ele, conta de agora.
        marca.impressao = imp.clone();
        marca.girado_em = oauth.expira_em.map(|e| e.saturating_sub(DURACAO_ACESSO_MS).min(agora)).unwrap_or(agora);
        marca.espera_ate = 0;
    }
    if marca.morto.as_deref() == Some(imp.as_str()) {
        return None;
    }
    marca.morto = None;
    if marca.espera_ate > agora {
        return None;
    }
    if forcar {
        return Some(Motivo::Pedido);
    }
    if let Some(e) = oauth.expira_em {
        if e > agora && e - agora < FOLGA_MS {
            return Some(Motivo::PertoDeVencer);
        }
    }
    (agora.saturating_sub(marca.girado_em) > PARADA_MS).then_some(Motivo::Parada)
}

// ------------------------------------------------------------------ travas do CLI

/// Uma trava do Claude Code (proper-lockfile: um diretório criado com
/// `mkdir`, velho depois de `velha` sem ser tocado). Enquanto viva, é tocada
/// a cada 2 s, como o CLI toca a dele: ninguém a toma por abandonada.
struct TravaDoCli {
    caminho: PathBuf,
    viva: std::sync::Arc<std::sync::atomic::AtomicBool>,
    tocador: Option<std::thread::JoinHandle<()>>,
}

impl TravaDoCli {
    fn pega(nome: &str, velha: Duration, espera: Duration) -> Option<TravaDoCli> {
        let caminho = caminhos::claude().join(nome);
        let fim = std::time::Instant::now() + espera;
        loop {
            match std::fs::create_dir(&caminho) {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let idade = std::fs::metadata(&caminho)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|m| m.elapsed().ok());
                    if idade.is_some_and(|i| i > velha) {
                        // Abandonada: o CLI faz o mesmo.
                        let _ = std::fs::remove_dir(&caminho);
                        continue;
                    }
                    if std::time::Instant::now() >= fim {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(_) => return None,
            }
        }
        let viva = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (v, c) = (viva.clone(), caminho.clone());
        let tocador = std::thread::spawn(move || {
            while v.load(std::sync::atomic::Ordering::Relaxed) {
                for _ in 0..20 {
                    std::thread::sleep(Duration::from_millis(100));
                    if !v.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                }
                toca(&c);
            }
        });
        Some(TravaDoCli { caminho, viva, tocador: Some(tocador) })
    }
}

impl Drop for TravaDoCli {
    fn drop(&mut self) {
        self.viva.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.tocador.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_dir(&self.caminho);
    }
}

fn toca(caminho: &Path) {
    #[cfg(unix)]
    if let Ok(f) = std::fs::File::open(caminho) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
    #[cfg(not(unix))]
    let _ = caminho;
}

// ------------------------------------------------------------------ renovar

/// O que um armazém deu nesta rodada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fim {
    Renovado(Motivo),
    Nada,
    Falhou(String),
}

fn pede(oauth: &Oauth, escopos: &[String]) -> Result<Value, (Recusa, String)> {
    let refresh = oauth.refresh.clone().unwrap_or_default();
    let escopo = if escopos.is_empty() { ESCOPOS.join(" ") } else { escopos.join(" ") };
    let corpo = json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "client_id": CLIENT_ID,
        "scope": escopo,
    });
    let url = rede::url_de_teste(&["KEEP_IA_TOKEN_URL"], TOKEN_URL);
    let cabecalhos = [("Accept", "application/json".to_string()), ("User-Agent", programas::agente_claude())];
    let r = rede::post_json(&url, &cabecalhos, &corpo, Duration::from_secs(30))
        .map_err(|e| (Recusa::Transitorio, format!("sem resposta ({})", e.chars().take(120).collect::<String>())))?;
    let texto = String::from_utf8_lossy(&r.corpo).into_owned();
    if r.status != 200 {
        let tipo = classificar(r.status, &texto);
        // O motivo leva o código do servidor, nunca o corpo inteiro.
        let codigo = serde_json::from_str::<Value>(&texto)
            .ok()
            .and_then(|j| match j.get("error") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Object(o)) => o.get("type").and_then(Value::as_str).map(str::to_string),
                _ => None,
            })
            .unwrap_or_default();
        return Err((tipo, format!("HTTP {} {codigo}", r.status).trim().to_string()));
    }
    let v: Value = serde_json::from_str(&texto).map_err(|_| (Recusa::Transitorio, "resposta sem JSON".to_string()))?;
    if v.get("access_token").and_then(Value::as_str).is_none_or(|s| s.trim().is_empty()) {
        return Err((Recusa::Transitorio, "resposta sem access_token".into()));
    }
    Ok(v)
}

/// O `claudeAiOauth` novo: o antigo, com o que a resposta trouxe.
fn oauth_novo(antigo: &Value, resposta: &Value, agora: u64) -> Value {
    let mut o = antigo.as_object().cloned().unwrap_or_default();
    let texto = |k: &str| resposta.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    if let Some(a) = texto("access_token") {
        o.insert("accessToken".into(), json!(a));
    }
    if let Some(r) = texto("refresh_token") {
        o.insert("refreshToken".into(), json!(r));
    }
    if let Some(s) = resposta.get("expires_in").and_then(Value::as_u64) {
        o.insert("expiresAt".into(), json!(agora + s * 1000));
    }
    if let Some(s) = resposta.get("refresh_token_expires_in").and_then(Value::as_u64) {
        o.insert("refreshTokenExpiresAt".into(), json!(agora + s * 1000));
    }
    if let Some(s) = texto("scope") {
        o.insert("scopes".into(), json!(s.split_whitespace().collect::<Vec<_>>()));
    }
    Value::Object(o)
}

fn le_oauth(local: &Local) -> Result<(Value, Oauth), String> {
    match credencial::ler(local) {
        Lido::Ok(v) => {
            let o = Oauth::de(&v).ok_or("o login não tem claudeAiOauth")?;
            Ok((v, o))
        }
        Lido::Ausente => Err("sem login".into()),
        Lido::Ilegivel(por) => Err(format!("login ilegível ({por})")),
    }
}

/// Uma rodada num armazém.
fn renova_um(alvo: &Alvo, marca: &mut Marca, forcar: bool) -> Fim {
    let agora = donos::agora_ms();
    // Sem login, ou ilegível (Chaveiro trancado): nada a fazer aqui.
    let Ok((_, oauth)) = le_oauth(&alvo.local) else { return Fim::Nada };
    if decide(&oauth, marca, agora, forcar).is_none() {
        return Fim::Nada;
    }
    let Some(_trava) = TravaDoCli::pega(".oauth_refresh.lock", Duration::from_secs(60), Duration::from_secs(5)) else {
        return Fim::Falhou("o Claude Code está renovando agora".into());
    };
    // Sob a trava, de novo: o CLI pode ter renovado enquanto esperávamos.
    let (valor, oauth) = match le_oauth(&alvo.local) {
        Ok(x) => x,
        Err(e) => return Fim::Falhou(e),
    };
    let agora = donos::agora_ms();
    let Some(motivo) = decide(&oauth, marca, agora, forcar) else { return Fim::Nada };
    let refresh = oauth.refresh.clone().unwrap_or_default();
    let antigo = valor.get("claudeAiOauth").cloned().unwrap_or(Value::Null);
    let escopos: Vec<String> = antigo
        .get("scopes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let resposta = match pede(&oauth, &escopos) {
        Ok(r) => r,
        Err((Recusa::Morto, m)) => {
            marca.morto = Some(impressao(&refresh));
            return Fim::Falhou(format!("login recusado pelo servidor ({m}): entre de novo"));
        }
        Err((Recusa::Espera, m)) => {
            marca.espera_ate = agora + ESPERA_MS;
            return Fim::Falhou(format!("conta em espera no servidor ({m}): tento de novo em 1 h"));
        }
        Err((Recusa::Transitorio, m)) => return Fim::Falhou(m),
    };
    let agora = donos::agora_ms();
    let novo = oauth_novo(&antigo, &resposta, agora);
    // Gravar: compare-and-swap sob a trava de escrita do CLI.
    let Some(_escrita) = TravaDoCli::pega(".storage-write.lock", Duration::from_secs(15), Duration::from_secs(5)) else {
        return Fim::Falhou("não gravei: o Claude Code está gravando o login agora".into());
    };
    let mut ultima = String::new();
    for _ in 0..3 {
        let (mut atual, o) = match le_oauth(&alvo.local) {
            Ok(x) => x,
            Err(e) => return Fim::Falhou(format!("não gravei: {e}")),
        };
        if o.refresh.as_deref() != Some(refresh.as_str()) {
            return Fim::Falhou("não gravei: o login mudou enquanto renovava".into());
        }
        if let Some(obj) = atual.as_object_mut() {
            obj.insert("claudeAiOauth".into(), novo.clone());
        }
        match credencial::gravar(&alvo.local, &atual) {
            Ok(()) => {
                let novo_refresh = novo.get("refreshToken").and_then(Value::as_str).unwrap_or_default();
                marca.impressao = impressao(novo_refresh);
                marca.girado_em = agora;
                marca.morto = None;
                marca.espera_ate = 0;
                return Fim::Renovado(motivo);
            }
            Err(e) => ultima = e,
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Fim::Falhou(format!("FALHA ao gravar o login renovado ({ultima})"))
}

/// O resultado de uma rodada inteira.
#[derive(Debug, Default)]
pub struct Rodada {
    pub em_andamento: bool,
    pub renovados: Vec<(String, String, Motivo)>,
    pub falhas: Vec<(String, String, String)>,
}

/// Uma rodada em todos os armazéns. Uma por vez: outra em curso, esta não faz
/// nada (`em_andamento`).
pub fn rodada(forcar: bool) -> Rodada {
    let _ = std::fs::create_dir_all(caminhos::estado());
    let Ok(trava) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(caminhos::estado().join("renovar.trava"))
    else {
        return Rodada { em_andamento: true, ..Default::default() };
    };
    if trava.try_lock().is_err() {
        return Rodada { em_andamento: true, ..Default::default() };
    }
    let mut marcas = ler_marcas();
    let mut saida = Rodada::default();
    let alvos = alvos();
    for alvo in &alvos {
        let mut marca = marcas.get(&alvo.id).cloned().unwrap_or_default();
        let fim = renova_um(alvo, &mut marca, forcar);
        match &fim {
            Fim::Renovado(m) => {
                let por = match m {
                    Motivo::PertoDeVencer => "perto de vencer",
                    Motivo::Parada => "parada",
                    Motivo::Pedido => "pedido",
                };
                registra(&format!("{} ({}): renovado ({por})", alvo.conta, alvo.armazem));
                saida.renovados.push((alvo.conta.clone(), alvo.armazem.to_string(), *m));
            }
            Fim::Falhou(m) => {
                registra(&format!("{} ({}): {m}", alvo.conta, alvo.armazem));
                saida.falhas.push((alvo.conta.clone(), alvo.armazem.to_string(), m.clone()));
            }
            Fim::Nada => {}
        }
        if marca != Marca::default() {
            marcas.insert(alvo.id.clone(), marca);
        }
    }
    // Armazém que sumiu (pasta apagada) não fica no estado.
    marcas.retain(|id, _| alvos.iter().any(|a| &a.id == id) || id == "global");
    gravar_marcas(&marcas);
    let _ = trava.unlock();
    saida
}

/// `keep ia renovar [--agora] --json`.
pub fn cli(a: &crate::cli::Args) -> i32 {
    let r = rodada(a.tem("agora"));
    if r.em_andamento {
        return crate::cli::responde(json!({ "ok": true, "estado": "em-andamento", "renovados": [], "falhas": [] }));
    }
    crate::cli::responde(json!({
        "ok": true,
        "estado": "feito",
        "renovados": r.renovados.iter().map(|(c, a, _)| json!({ "conta": c, "armazem": a })).collect::<Vec<_>>(),
        "falhas": r.falhas.iter().map(|(c, a, m)| json!({ "conta": c, "armazem": a, "motivo": m })).collect::<Vec<_>>(),
    }))
}

#[cfg(all(test, unix))]
mod testes;
