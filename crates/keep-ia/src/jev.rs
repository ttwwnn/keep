//! O saldo do Jev: os créditos do OpenRouter, que o Jev (o modelo que decide
//! no lugar do Claude) gasta a cada decisão. Aparece no rodapé "Consumo de
//! IA" ao lado das contas. Ver `docs/ia.md` ("Jev").
//!
//! A chave é a que a skill `jev` guarda: no macOS, o item `openrouter-api-key`
//! do Chaveiro (lido com `security`, como as credenciais do Claude); no
//! Windows, a credencial genérica de mesmo nome do Gerenciador de Credenciais;
//! nos outros, um arquivo só do dono na pasta de estado do Keep (ou
//! `OPENROUTER_API_KEY`). Sem chave, não há Jev e o rodapé não mostra nada. A
//! chave nunca sai daqui nem vai para o cache.
//!
//! Quem não tem chave conecta pelo "+" do rodapé: `keep ia entrar openrouter`
//! abre uma aba com `keep ia login openrouter`, o login do OpenRouter no
//! navegador (OAuth PKCE, como o `jev chave` da skill), que cria a chave
//! "Jev (Keep)" e a guarda onde o Jev a lê (`login`).
//!
//! Duas perguntas ao OpenRouter: `/api/v1/credits` (o que a conta comprou e
//! gastou) e `/api/v1/key` (o teto e o gasto da chave do Jev). O que vale é o
//! menor dos dois saldos. Uma leitura por minuto (são duas consultas de
//! metadados, que o OpenRouter não cobra), "Medir agora" no máximo a cada
//! 15 s, 429 espera o `Retry-After`, sem rede tenta de novo em um minuto.
//!
//! O OpenRouter leva uns dois minutos para contar uma decisão (medido em
//! 05/10/2026: 2 min 20 s). Para o rodapé andar a cada decisão, a skill anota
//! o custo de cada chamada que faz nesta máquina em
//! `~/.claude/jev/chamadas.jsonl` (`livro`), e o saldo mostrado é a última
//! leitura mais o que o livro tem que ela ainda não contou. Cada leitura nova
//! casa o que a chave gastou desde a anterior com as chamadas mais antigas do
//! livro (`concilia`); uma chamada que o OpenRouter não contou em dez minutos
//! sai da conta.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine as _;
use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cli::{Args, responde};
use crate::{caminhos, rede};

const PERIODO: f64 = 60.0;
const ENTRE_CLIQUES: f64 = 15.0;
/// Uma chamada do livro que o OpenRouter não contou nesse tempo já não
/// entra no saldo: foi contada sem casar, ou não será.
const ESPERA_DO_OPENROUTER: f64 = 600.0;
/// O livro é lido do fim: o que passa disto é antigo demais para importar.
const LIVRO_MAXIMO: u64 = 1 << 20;
/// O item do Chaveiro (e a credencial do Windows) em que a skill `jev` guarda a chave.
const SERVICO: &str = "openrouter-api-key";
/// O nome da chave que o login cria no OpenRouter.
const ROTULO: &str = "Jev (Keep)";

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

    /// A leitura com o que as chamadas desta máquina gastaram e o
    /// OpenRouter ainda não contou: gasto pela chave do Jev, logo pela conta.
    fn com_pendente(mut self, pendente: f64) -> Credito {
        if pendente > 0.0 {
            self.used += pendente;
            self.key_used = self.key_used.map(|u| u + pendente);
            self.used_today = self.used_today.map(|u| u + pendente);
            self.used_this_month = self.used_this_month.map(|u| u + pendente);
        }
        self
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
    /// A chave que mediu (`id_da_chave`): o livro só conta as chamadas dela.
    #[serde(default)]
    chave_id: Option<String>,
    /// As chamadas do livro até este momento já estão no que o OpenRouter
    /// disse.
    #[serde(default)]
    livro_ate: Option<f64>,
    /// Gasto que o OpenRouter já contou e ainda não casou com uma chamada
    /// inteira do livro: parte da próxima.
    #[serde(default)]
    livro_sobra: f64,
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
fn chave() -> Option<String> {
    #[cfg(not(target_os = "macos"))]
    if let Some(c) = std::env::var("OPENROUTER_API_KEY").ok().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()) {
        return Some(c);
    }
    chave_guardada()
}

/// Nos testes, `KEEP_IA_TESTE_CHAVEIRO` (uma pasta, um arquivo por item) fica
/// no lugar do Gerenciador de Credenciais e do arquivo; no macOS, o
/// `security` de mentira dos testes já lê e grava nela.
#[cfg(not(target_os = "macos"))]
fn chaveiro_de_teste() -> Option<PathBuf> {
    std::env::var_os("KEEP_IA_TESTE_CHAVEIRO").map(|d| PathBuf::from(d).join(SERVICO))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn arquivo_da_chave() -> PathBuf {
    chaveiro_de_teste().unwrap_or_else(|| caminhos::estado().join("openrouter.key"))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn chave_guardada() -> Option<String> {
    let texto = std::fs::read_to_string(arquivo_da_chave()).ok()?.trim().to_string();
    (!texto.is_empty()).then_some(texto)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn guarda_chave(chave: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let arq = arquivo_da_chave();
    if let Some(pasta) = arq.parent() {
        std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    }
    let tmp = arq.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    f.write_all(chave.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &arq).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn chave_guardada() -> Option<String> {
    if let Some(arq) = chaveiro_de_teste() {
        let texto = std::fs::read_to_string(arq).ok()?.trim().to_string();
        return (!texto.is_empty()).then_some(texto);
    }
    credencial_windows::le(SERVICO)
}

#[cfg(windows)]
fn guarda_chave(chave: &str) -> Result<(), String> {
    if let Some(arq) = chaveiro_de_teste() {
        if let Some(pasta) = arq.parent() {
            std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
        }
        return std::fs::write(arq, chave).map_err(|e| e.to_string());
    }
    credencial_windows::grava(SERVICO, chave)
}

/// Uma credencial genérica do Gerenciador de Credenciais do Windows, só do
/// usuário (como o Git e o `gh` guardam as deles).
#[cfg(windows)]
mod credencial_windows {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW, CredWriteW,
    };

    fn largo(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn le(alvo: &str) -> Option<String> {
        let nome = largo(alvo);
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `nome` termina em zero e vive até o fim da chamada; o
        // ponteiro devolvido é liberado com `CredFree` logo abaixo.
        let ok = unsafe { CredReadW(nome.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) };
        if ok == 0 || cred.is_null() {
            return None;
        }
        // SAFETY: `cred` é a credencial que o Windows acabou de alocar; o blob
        // tem `CredentialBlobSize` bytes.
        let texto = unsafe {
            let c = &*cred;
            let bytes = if c.CredentialBlob.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
            };
            String::from_utf8(bytes.to_vec()).ok()
        };
        // SAFETY: liberada uma vez, a que o CredReadW alocou.
        unsafe { CredFree(cred as *const _) };
        texto.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }

    pub fn grava(alvo: &str, valor: &str) -> Result<(), String> {
        let mut nome = largo(alvo);
        let mut usuario = largo("Keep");
        let mut blob = valor.as_bytes().to_vec();
        let cred = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: nome.as_mut_ptr(),
            Comment: std::ptr::null_mut(),
            LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: std::ptr::null_mut(),
            UserName: usuario.as_mut_ptr(),
        };
        // SAFETY: todos os ponteiros da estrutura apontam para buffers vivos
        // até o fim da chamada, que copia o que precisa.
        if unsafe { CredWriteW(&cred, 0) } == 0 {
            return Err(format!("o Windows não guardou a chave ({})", std::io::Error::last_os_error()));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn guarda_chave(chave: &str) -> Result<(), String> {
    crate::contas::credencial::grava_chaveiro(SERVICO, chave)
}

#[cfg(target_os = "macos")]
fn chave_guardada() -> Option<String> {
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

// ------------------------------------------------------------ o livro

/// O livro das chamadas: a skill `jev` anota cada decisão que pede nesta
/// máquina, uma linha JSON por chamada — `{"quando": <segundos desde 1970>,
/// "custo_usd": <dólares>, "chave": <id_da_chave>}`.
fn livro() -> PathBuf {
    caminhos::claude().join("jev").join("chamadas.jsonl")
}

/// A chave sem ser a chave: os 12 primeiros dígitos hexadecimais do SHA-256
/// dela. O livro e o cache guardam isto, que não leva de volta à chave.
fn id_da_chave(chave: &str) -> String {
    Sha256::digest(chave.trim().as_bytes()).iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// Uma chamada do livro.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chamada {
    quando: f64,
    custo: f64,
}

/// As chamadas da chave `id` depois de `desde`, da mais antiga à mais nova.
/// Linha que não se lê, de outra chave ou sem custo fica de fora.
fn chamadas(id: &str, desde: f64) -> Vec<Chamada> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut arquivo) = std::fs::File::open(livro()) else { return Vec::new() };
    let tamanho = arquivo.metadata().map(|m| m.len()).unwrap_or(0);
    let pulo = tamanho.saturating_sub(LIVRO_MAXIMO);
    if pulo > 0 && arquivo.seek(SeekFrom::Start(pulo)).is_err() {
        return Vec::new();
    }
    let mut bruto = Vec::new();
    if arquivo.read_to_end(&mut bruto).is_err() {
        return Vec::new();
    }
    let texto = String::from_utf8_lossy(&bruto);
    // Lido do meio, a primeira linha vem cortada.
    let linhas = texto.lines().skip(usize::from(pulo > 0));
    let mut lista: Vec<Chamada> = linhas
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.get("chave").and_then(Value::as_str) == Some(id))
        .filter_map(|v| {
            let quando = numero(v.get("quando"))?;
            let custo = numero(v.get("custo_usd"))?;
            (quando > desde && custo.is_finite() && custo > 0.0).then_some(Chamada { quando, custo })
        })
        .collect();
    lista.sort_by(|a, b| a.quando.total_cmp(&b.quando));
    lista
}

/// Casa o que a chave gastou desde a leitura anterior (`gasto`) com as
/// chamadas do livro que ainda não tinham sido contadas, das mais antigas
/// para as mais novas: as que couberem passam a estar na leitura. O que
/// sobra sem cobrir a próxima fica guardado para a leitura seguinte; o que
/// sobra sem chamada à espera é gasto de fora do livro, e se esquece. Uma
/// chamada vencida (`ESPERA_DO_OPENROUTER`) sai sem gastar nada.
fn concilia(r: &mut Registro, gasto: f64, agora_s: f64) {
    let Some(id) = r.chave_id.clone() else { return };
    let mut ate = r.livro_ate.unwrap_or(agora_s);
    let mut resta = gasto.max(0.0) + r.livro_sobra;
    let mut esperando = false;
    for c in chamadas(&id, ate) {
        if c.quando < agora_s - ESPERA_DO_OPENROUTER {
            ate = c.quando;
        } else if resta + 1e-12 >= c.custo * 0.99 {
            // 1% de folga: o OpenRouter pode arredondar o que contou.
            resta = (resta - c.custo).max(0.0);
            ate = c.quando;
        } else {
            esperando = true;
            break;
        }
    }
    r.livro_ate = Some(ate);
    r.livro_sobra = if esperando { resta } else { 0.0 };
}

/// O que as chamadas desta máquina gastaram e o OpenRouter ainda não contou.
fn pendente(r: &Registro, agora_s: f64) -> f64 {
    let (Some(id), Some(desde)) = (r.chave_id.as_deref(), r.livro_ate) else { return 0.0 };
    let soma: f64 = chamadas(id, desde)
        .iter()
        .filter(|c| c.quando >= agora_s - ESPERA_DO_OPENROUTER)
        .map(|c| c.custo)
        .sum();
    (soma - r.livro_sobra).max(0.0)
}

/// O que o rodapé mostra: o Jev, ou nada quando não há chave. `credit` já
/// tem `pending` somado: o que as chamadas desta máquina gastaram depois da
/// leitura.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Saldo {
    pub credit: Option<Credito>,
    pub measured_at: Option<f64>,
    pub problem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<f64>,
}

fn saldo(r: Registro) -> Saldo {
    let p = pendente(&r, agora());
    Saldo {
        credit: r.credit.map(|c| c.com_pendente(p)),
        measured_at: r.measured_at,
        problem: r.problem,
        pending: (p > 0.0).then_some(p),
    }
}

/// O que há guardado, mais o livro, sem ir à rede nem ao Chaveiro.
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
    // Chave nova (ou a primeira): o livro conta a partir de agora, só as
    // chamadas dela.
    let id = id_da_chave(&chave);
    let outra_chave = r.chave_id.as_deref() != Some(id.as_str());
    if outra_chave {
        r.chave_id = Some(id);
        r.livro_ate = Some(agora_s);
        r.livro_sobra = 0.0;
    }
    let mut forcar = forcar;
    if forcar {
        if agora_s - r.ultimo_clique < ENTRE_CLIQUES {
            forcar = false;
        } else {
            r.ultimo_clique = agora_s;
        }
    }
    let espera = r.pausa_ate.is_some_and(|ate| ate > agora_s && (!forcar || r.pausa_firme));
    let devida = forcar || r.problem.is_some() || r.measured_at.is_none_or(|m| agora_s - m >= PERIODO - 5.0);
    if espera || !devida {
        if outra_chave {
            grava_cache(&r);
        }
        return Some(saldo(r));
    }
    r.pausa_ate = None;
    r.pausa_firme = false;
    match pergunta(&chave, agora_s) {
        Resultado::Leitura(c) => {
            // O que a chave gastou desde a leitura anterior casa com o livro;
            // sem o gasto da chave nas duas, vale o da conta.
            if !outra_chave {
                if let Some(antes) = &r.credit {
                    let gasto = match (antes.key_used, c.key_used) {
                        (Some(a), Some(n)) => n - a,
                        _ => c.used - antes.used,
                    };
                    concilia(&mut r, gasto, agora_s);
                }
            }
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

// ------------------------------------------------------------ conectar

/// O endereço da página de autorização: a do OpenRouter, ou a de um teste
/// nesta máquina (`KEEP_IA_OPENROUTER_AUTH_URL`, só `http://127.0.0.1`).
fn url_de_autorizar() -> String {
    rede::url_de_teste(&["KEEP_IA_OPENROUTER_AUTH_URL"], "https://openrouter.ai/auth")
}

fn url_da_api() -> String {
    rede::url_de_teste(&["KEEP_IA_OPENROUTER_URL"], "https://openrouter.ai/api/v1")
}

fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn aleatorio(n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| format!("sem números aleatórios do sistema ({e})"))?;
    Ok(b)
}

/// O texto de um parâmetro de URL: o que não for letra, número ou `-._~` vira `%XX`.
fn codifica(texto: &str) -> String {
    texto
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decodifica(texto: &str) -> String {
    let b = texto.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => saida.push(b' '),
            b'%' if i + 2 < b.len() => {
                match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        saida.push(v);
                        i += 2;
                    }
                    None => saida.push(b'%'),
                }
            }
            c => saida.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

/// Os parâmetros de uma linha `GET /caminho?a=1&b=2 HTTP/1.1`, e o caminho.
fn le_pedido(linha: &str) -> (String, Vec<(String, String)>) {
    let alvo = linha.split_whitespace().nth(1).unwrap_or("");
    let (caminho, consulta) = alvo.split_once('?').unwrap_or((alvo, ""));
    let pares = consulta
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decodifica(k), decodifica(v))
        })
        .collect();
    (caminho.to_string(), pares)
}

/// A página que o navegador mostra ao voltar do OpenRouter.
fn pagina(conexao: &mut TcpStream, titulo: &str, texto: &str) {
    let escapa = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let corpo = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Keep</title>\
         <body style=\"font:17px/1.5 -apple-system,system-ui,sans-serif;max-width:40em;margin:4em auto;padding:0 1em\">\
         <h1 style=\"font-size:1.4em\">{}</h1><p>{}</p></body>",
        escapa(titulo),
        escapa(texto)
    );
    let _ = write!(
        conexao,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        corpo.len(),
        corpo
    );
    let _ = conexao.flush();
}

/// Abre o endereço no navegador padrão; um teste aponta outro programa
/// (`KEEP_IA_NAVEGADOR`), que recebe o endereço como argumento.
fn abre_no_navegador(url: &str) -> bool {
    let mut cmd = match std::env::var_os("KEEP_IA_NAVEGADOR") {
        Some(p) => std::process::Command::new(p),
        #[cfg(target_os = "macos")]
        None => std::process::Command::new("/usr/bin/open"),
        #[cfg(windows)]
        None => {
            let mut c = std::process::Command::new("rundll32.exe");
            c.arg("url.dll,FileProtocolHandler");
            c
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        None => std::process::Command::new("xdg-open"),
    };
    cmd.arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Troca o código da volta pela chave (`POST /api/v1/auth/keys`).
fn troca_o_codigo(codigo: &str, verificador: &str) -> Result<String, String> {
    let corpo = json!({ "code": codigo, "code_verifier": verificador, "code_challenge_method": "S256" });
    let r = rede::post_json(&format!("{}/auth/keys", url_da_api()), &[], &corpo, Duration::from_secs(30))
        .map_err(|_| "sem conexão com o OpenRouter".to_string())?;
    if r.status != 200 {
        return Err(format!("o OpenRouter não trocou o código pela chave (HTTP {})", r.status));
    }
    let v: Value = serde_json::from_slice(&r.corpo).map_err(|_| "o OpenRouter respondeu sem a chave".to_string())?;
    let chave = v.get("key").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if !chave.starts_with("sk-or-") {
        return Err("o OpenRouter respondeu sem a chave".into());
    }
    Ok(chave)
}

/// O que o OpenRouter diz da chave nova: o teto, ou nenhum.
fn confere_a_chave(chave: &str) -> Result<Option<f64>, String> {
    let r = rede::get(&format!("{}/key", url_da_api()), &[("Authorization", format!("Bearer {chave}"))], Duration::from_secs(20))
        .map_err(|_| "sem conexão com o OpenRouter para conferir a chave".to_string())?;
    if r.status != 200 {
        return Err(format!("o OpenRouter recusou a chave nova (HTTP {})", r.status));
    }
    let v: Value = serde_json::from_slice(&r.corpo).unwrap_or(Value::Null);
    Ok(v.get("data").and_then(|d| d.get("limit")).and_then(Value::as_f64))
}

/// `keep ia login openrouter`: o login do OpenRouter no navegador, que cria a
/// chave "Jev (Keep)" e a guarda onde o Jev a lê. Roda numa aba (o "+" do
/// rodapé a abre): diz o que faz, espera a volta do navegador em
/// `http://localhost:<porta>/callback` e mede o saldo, para o rodapé mostrar
/// o Jev em seguida. Devolve o resumo para a aba.
pub fn login() -> Result<String, String> {
    println!("Conectar o Jev ao OpenRouter\n");
    println!("O Jev é um modelo que decide no lugar da IA (que modelo usar, qual opção escolher) e cobra");
    println!("frações de centavo por decisão, do crédito da sua conta no OpenRouter. O navegador vai abrir");
    println!("a página do OpenRouter para você autorizar a chave \"{ROTULO}\". Deixe o limite de crédito em");
    println!("branco para o Jev poder usar todo o crédito da conta.\n");
    let verificador = base64url(&aleatorio(64)?);
    let desafio = base64url(&Sha256::digest(verificador.as_bytes()));
    let estado = base64url(&aleatorio(16)?);
    let ouvinte = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("não deu para esperar a volta do navegador ({e})"))?;
    let porta = ouvinte.local_addr().map_err(|e| e.to_string())?.port();
    // "localhost" pode resolver para ::1 antes de 127.0.0.1.
    let ouvintes: Vec<TcpListener> =
        std::iter::once(ouvinte).chain(TcpListener::bind(("::1", porta)).ok()).collect();
    for o in &ouvintes {
        o.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    let retorno = format!("http://localhost:{porta}/callback");
    let url = format!(
        "{}?callback_url={}&code_challenge={desafio}&code_challenge_method=S256&key_label={}&state={estado}",
        url_de_autorizar(),
        codifica(&retorno),
        codifica(ROTULO)
    );
    if !abre_no_navegador(&url) {
        println!("O navegador não abriu sozinho.");
    }
    println!("Se o navegador não abrir, copie este endereço para ele:\n{url}\n");
    println!("Esperando a autorização (até 15 minutos)…");
    let prazo = std::env::var("KEEP_IA_LOGIN_PRAZO").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(900);
    let fim = Instant::now() + Duration::from_secs(prazo);
    loop {
        if Instant::now() > fim {
            return Err("Ninguém autorizou a tempo; nenhuma chave foi guardada.".into());
        }
        let Some(mut conexao) = ouvintes.iter().find_map(|o| o.accept().ok().map(|(c, _)| c)) else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let _ = conexao.set_nonblocking(false);
        let _ = conexao.set_read_timeout(Some(Duration::from_secs(10)));
        let mut primeira = String::new();
        if BufReader::new(&conexao).read_line(&mut primeira).is_err() {
            continue;
        }
        let (caminho, pares) = le_pedido(&primeira);
        if caminho != "/callback" {
            let _ = write!(conexao, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        }
        let valor = |k: &str| pares.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let resultado = if valor("state").as_deref() != Some(estado.as_str()) {
            Err("A volta do navegador não é a deste pedido; nenhuma chave foi guardada.".to_string())
        } else if let Some(codigo) = valor("code").filter(|c| !c.is_empty()) {
            troca_o_codigo(&codigo, &verificador).and_then(|chave| {
                let teto = confere_a_chave(&chave)?;
                guarda_chave(&chave)?;
                if chave_guardada().as_deref() != Some(chave.as_str()) {
                    return Err("a chave não ficou guardada".into());
                }
                Ok(teto)
            })
        } else {
            Err(format!(
                "O OpenRouter não autorizou ({}); nenhuma chave foi guardada.",
                valor("error").unwrap_or_else(|| "sem código".into())
            ))
        };
        return match resultado {
            Ok(teto) => {
                let limite = match teto {
                    Some(t) => format!("limite de US$ {t:.2} na chave"),
                    None => "sem limite na chave: usa todo o crédito da conta".into(),
                };
                pagina(&mut conexao, "Pronto", "O Jev está conectado ao OpenRouter. Pode fechar esta aba e voltar ao Keep.");
                let _ = medir(true);
                Ok(format!("Pronto: o Jev está conectado ao OpenRouter (chave \"{ROTULO}\", {limite})."))
            }
            Err(e) => {
                pagina(&mut conexao, "Não deu certo", &e);
                Err(e)
            }
        };
    }
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
        let mut r = le_cache();
        r.measured_at = Some(agora() - 50.0);
        grava_cache(&r);
        medir(false);
        assert_eq!(pedidos.lock().unwrap().len(), 4, "medido há 50 s: ainda não");
        let mut r = le_cache();
        r.measured_at = Some(agora() - 61.0);
        grava_cache(&r);
        medir(false);
        assert_eq!(pedidos.lock().unwrap().len(), 6, "medido há mais de um minuto: mede de novo");

        let guardado = std::fs::read_to_string(arquivo()).unwrap();
        assert!(!guardado.contains("segredo"), "o cache não guarda a chave");
        assert!(em_cache().unwrap().credit.is_some());
    }

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

    // ---------------------------------------------------------------- o livro

    /// Uma linha do livro, como a skill escreve.
    fn anota(chave: &str, quando: f64, custo: f64) {
        let linha = json!({ "quando": quando, "custo_usd": custo, "chave": id_da_chave(chave) });
        let caminho = livro();
        std::fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(caminho).unwrap();
        writeln!(f, "{linha}").unwrap();
    }

    /// O OpenRouter de mentira com o gasto que o teste manda: o da conta e o da chave.
    fn openrouter(gasto: std::sync::Arc<std::sync::Mutex<(f64, f64)>>) {
        servidor(move |p| {
            let (conta, chave) = *gasto.lock().unwrap();
            match p.caminho.as_str() {
                "/credits" => (200, vec![], format!(r#"{{"data":{{"total_credits":10,"total_usage":{conta}}}}}"#)),
                "/key" => (200, vec![], format!(r#"{{"data":{{"limit":null,"usage":{chave},"usage_daily":{chave}}}}}"#)),
                _ => (404, vec![], String::new()),
            }
        });
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        aponta(&porta);
    }

    /// Mede de novo já, como o minuto seguinte.
    fn mede_de_novo() -> Saldo {
        let mut r = le_cache();
        r.measured_at = Some(agora() - PERIODO);
        grava_cache(&r);
        medir(false).unwrap()
    }

    fn perto(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn o_livro_soma_na_hora_o_que_o_openrouter_ainda_nao_contou() {
        let c = casa("jev-livro");
        com_chave(&c);
        let gasto = std::sync::Arc::new(std::sync::Mutex::new((0.03, 0.01)));
        openrouter(gasto.clone());
        let chave = "sk-or-v1-segredo";

        let s = medir(false).unwrap();
        assert!(s.pending.is_none());
        assert!(perto(s.credit.unwrap().key_used.unwrap(), 0.01));

        // Duas decisões desta máquina; uma de outra chave, uma de antes da
        // leitura e uma linha que não se lê ficam de fora.
        let antes_da_leitura = le_cache().livro_ate.unwrap() - 1.0;
        anota(chave, antes_da_leitura, 0.5);
        anota("sk-or-v1-outra", agora(), 0.5);
        std::fs::OpenOptions::new().append(true).open(livro()).unwrap().write_all(b"{cortada\n").unwrap();
        anota(chave, agora(), 0.0001);
        anota(chave, agora() + 0.001, 0.0002);
        let s = em_cache().unwrap();
        assert!(perto(s.pending.unwrap(), 0.0003), "{:?}", s.pending);
        let l = s.credit.unwrap();
        assert!(perto(l.used, 0.0303) && perto(l.key_used.unwrap(), 0.0103), "{l:?}");
        assert!(perto(l.used_today.unwrap(), 0.0103));
        assert!(!std::fs::read_to_string(livro()).unwrap().contains("segredo"), "o livro não leva a chave");
        assert!(!std::fs::read_to_string(arquivo()).unwrap().contains("segredo"), "nem o cache");

        // O OpenRouter ainda não contou nada: o minuto seguinte mostra o mesmo.
        let s = mede_de_novo();
        assert!(perto(s.pending.unwrap(), 0.0003));
        assert!(perto(s.credit.unwrap().key_used.unwrap(), 0.0103));

        // Contou a primeira: ela sai do livro, a segunda fica.
        *gasto.lock().unwrap() = (0.0301, 0.0101);
        let s = mede_de_novo();
        assert!(perto(s.pending.unwrap(), 0.0002), "{:?}", s.pending);
        assert!(perto(s.credit.unwrap().key_used.unwrap(), 0.0103));

        // Contou a segunda e mais uma que não passou pelo livro: nada pendente,
        // e a de fora não come a próxima.
        *gasto.lock().unwrap() = (0.0309, 0.0109);
        let s = mede_de_novo();
        assert!(s.pending.is_none(), "{:?}", s.pending);
        assert!(perto(s.credit.unwrap().key_used.unwrap(), 0.0109));
        assert_eq!(le_cache().livro_sobra, 0.0);
        anota(chave, agora() + 0.002, 0.0004);
        assert!(perto(em_cache().unwrap().pending.unwrap(), 0.0004));

        // Contou parte dela: a parte fica guardada, e o resto segue pendente.
        *gasto.lock().unwrap() = (0.0311, 0.0111);
        let s = mede_de_novo();
        assert!(perto(s.pending.unwrap(), 0.0002), "{:?}", s.pending);
        assert!(perto(s.credit.unwrap().key_used.unwrap(), 0.0113));
        *gasto.lock().unwrap() = (0.0313, 0.0113);
        assert!(mede_de_novo().pending.is_none());

        // Arredondada pelo OpenRouter (1% a menos): casa do mesmo jeito.
        anota(chave, agora() + 0.003, 0.0010);
        *gasto.lock().unwrap() = (0.0323 - 0.00001, 0.0123 - 0.00001);
        assert!(mede_de_novo().pending.is_none());
        assert_eq!(le_cache().livro_sobra, 0.0);
    }

    #[test]
    fn chamada_que_o_openrouter_nao_conta_sai_em_dez_minutos() {
        let c = casa("jev-livro-vence");
        com_chave(&c);
        let gasto = std::sync::Arc::new(std::sync::Mutex::new((0.03, 0.01)));
        openrouter(gasto.clone());
        let chave = "sk-or-v1-segredo";
        medir(false).unwrap();
        let mut r = le_cache();
        r.livro_ate = Some(agora() - 2.0 * ESPERA_DO_OPENROUTER);
        grava_cache(&r);
        anota(chave, agora() - ESPERA_DO_OPENROUTER - 60.0, 0.0005);
        anota(chave, agora() - 30.0, 0.0001);
        assert!(perto(em_cache().unwrap().pending.unwrap(), 0.0001), "a vencida não entra");

        // A leitura seguinte passa por cima da vencida e casa a outra.
        *gasto.lock().unwrap() = (0.0301, 0.0101);
        let s = mede_de_novo();
        assert!(s.pending.is_none(), "{:?}", s.pending);
        assert!(le_cache().livro_ate.unwrap() > agora() - 60.0);
    }

    #[test]
    fn chave_nova_comeca_o_livro_do_zero() {
        let c = casa("jev-livro-chave-nova");
        com_chave(&c);
        let gasto = std::sync::Arc::new(std::sync::Mutex::new((0.03, 0.01)));
        openrouter(gasto.clone());
        medir(false).unwrap();
        anota("sk-or-v1-segredo", agora(), 0.0001);
        assert!(em_cache().unwrap().pending.is_some());

        std::fs::write(c.raiz.join("chaveiro").join(SERVICO), "sk-or-v1-trocada\n").unwrap();
        // Mesmo sem medir (a leitura é recente), a troca já vale.
        let s = medir(false).unwrap();
        assert!(s.pending.is_none(), "as chamadas da chave antiga não contam para a nova");
        assert_eq!(le_cache().chave_id.as_deref(), Some(id_da_chave("sk-or-v1-trocada").as_str()));
        anota("sk-or-v1-trocada", agora() + 0.001, 0.0002);
        assert!(perto(em_cache().unwrap().pending.unwrap(), 0.0002));
        assert_eq!(id_da_chave("sk-or-v1-trocada\n"), id_da_chave("sk-or-v1-trocada"), "o fim de linha não muda a chave");
        assert_eq!(id_da_chave("x").len(), 12);
        assert_eq!(id_da_chave("sk-or-v1-segredo"), "4ea63ba423d9", "o mesmo id que a skill escreve no livro");
    }

    // ---------------------------------------------------------------- conectar

    /// Um navegador de mentira: guarda o endereço que recebeu num arquivo.
    #[cfg(unix)]
    fn navegador(c: &crate::contas::testes::Casa) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = c.raiz.join("navegador");
        let saida = c.raiz.join("url-aberta");
        std::fs::write(&script, format!("#!/bin/sh\nprintf %s \"$1\" > '{}'\n", saida.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        // SAFETY: a casa (e a trava) de pé.
        unsafe {
            std::env::set_var("KEEP_IA_NAVEGADOR", &script);
            std::env::set_var("KEEP_IA_LOGIN_PRAZO", "20");
        }
        saida
    }

    /// O endereço que o login abriu, quando abrir, e um parâmetro dele.
    #[cfg(unix)]
    fn endereco_aberto(arquivo: &std::path::Path) -> String {
        for _ in 0..200 {
            if let Ok(u) = std::fs::read_to_string(arquivo) {
                if !u.is_empty() {
                    return u;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("o login não abriu o navegador");
    }

    #[cfg(unix)]
    fn parametro(url: &str, nome: &str) -> String {
        let (_, pares) = le_pedido(&format!("GET /{} HTTP/1.1", url.split_once('?').map(|(_, q)| format!("x?{q}")).unwrap_or_default()));
        pares.into_iter().find(|(k, _)| k == nome).map(|(_, v)| v).unwrap_or_default()
    }

    /// A volta do navegador: o pedido que o OpenRouter manda o navegador fazer.
    #[cfg(unix)]
    fn volta(retorno: &str, consulta: &str) -> String {
        let porta = retorno.trim_start_matches("http://localhost:").trim_end_matches("/callback");
        let mut c = TcpStream::connect(("127.0.0.1", porta.parse::<u16>().unwrap())).unwrap();
        write!(c, "GET /callback?{consulta} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut resposta = String::new();
        let _ = std::io::Read::read_to_string(&mut c, &mut resposta);
        resposta
    }

    #[cfg(unix)]
    #[test]
    fn o_login_cria_a_chave_guarda_onde_o_jev_le_e_mede() {
        let c = casa("jev-login");
        let aberto = navegador(&c);
        let pedidos = servidor(|p| match (p.caminho.as_str(), p.token.as_str()) {
            ("/auth/keys", _) => (200, vec![], r#"{"key":"sk-or-v1-chave-nova"}"#.into()),
            ("/key", "sk-or-v1-chave-nova") => (200, vec![], r#"{"data":{"limit":null,"usage":0}}"#.into()),
            ("/credits", "sk-or-v1-chave-nova") => (200, vec![], CREDITOS.into()),
            _ => (401, vec![], String::new()),
        });
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        aponta(&porta);

        let fio = std::thread::spawn(login);
        let url = endereco_aberto(&aberto);
        assert!(url.starts_with("https://openrouter.ai/auth?"), "{url}");
        assert_eq!(parametro(&url, "key_label"), "Jev (Keep)");
        assert_eq!(parametro(&url, "code_challenge_method"), "S256");
        let retorno = parametro(&url, "callback_url");
        assert!(retorno.starts_with("http://localhost:") && retorno.ends_with("/callback"), "{retorno}");
        let pagina = volta(&retorno, &format!("code=codigo-123&state={}", parametro(&url, "state")));
        assert!(pagina.contains("Pronto"), "{pagina}");
        let resumo = fio.join().unwrap().unwrap();
        assert!(resumo.contains("sem limite na chave"), "{resumo}");

        assert_eq!(chave_guardada().as_deref(), Some("sk-or-v1-chave-nova"), "a chave fica onde o Jev a lê");
        let troca = pedidos.lock().unwrap().iter().find(|p| p.caminho == "/auth/keys").cloned().expect("trocou o código");
        let corpo: Value = serde_json::from_str(&troca.corpo).unwrap();
        assert_eq!(corpo["code"], "codigo-123");
        let verificador = corpo["code_verifier"].as_str().unwrap();
        assert_eq!(base64url(&Sha256::digest(verificador.as_bytes())), parametro(&url, "code_challenge"), "PKCE: o desafio é o do verificador");
        assert!(em_cache().and_then(|s| s.credit).is_some(), "mediu o saldo para o rodapé mostrar em seguida");
        let guardado = std::fs::read_to_string(arquivo()).unwrap();
        assert!(!guardado.contains("chave-nova"), "o cache não guarda a chave");
    }

    #[cfg(unix)]
    #[test]
    fn volta_de_outro_pedido_ou_recusada_nao_guarda_nada() {
        let c = casa("jev-login-recusa");
        let aberto = navegador(&c);
        let pedidos = servidor(|_| (200, vec![], r#"{"key":"sk-or-v1-nao-devia"}"#.into()));
        let porta = std::env::var("KEEP_IA_CLAUDE_URL").unwrap().trim_end_matches("/claude").to_string();
        aponta(&porta);

        let fio = std::thread::spawn(login);
        let url = endereco_aberto(&aberto);
        let pagina = volta(&parametro(&url, "callback_url"), "code=codigo&state=de-outro-pedido");
        assert!(pagina.contains("Não deu certo"), "{pagina}");
        assert!(fio.join().unwrap().unwrap_err().contains("não é a deste pedido"));
        assert!(pedidos.lock().unwrap().is_empty(), "nem trocou o código");
        assert!(chave_guardada().is_none());

        let _ = std::fs::remove_file(&aberto);
        let fio = std::thread::spawn(login);
        let url = endereco_aberto(&aberto);
        volta(&parametro(&url, "callback_url"), &format!("error=access_denied&state={}", parametro(&url, "state")));
        assert!(fio.join().unwrap().unwrap_err().contains("access_denied"));
        assert!(chave_guardada().is_none());
    }

    #[test]
    fn a_url_vai_codificada_e_volta_igual() {
        assert_eq!(codifica("http://localhost:9/callback"), "http%3A%2F%2Flocalhost%3A9%2Fcallback");
        assert_eq!(codifica("Jev (Keep)"), "Jev%20%28Keep%29");
        assert_eq!(decodifica("Jev%20%28Keep%29"), "Jev (Keep)");
        assert_eq!(decodifica("a+b%2"), "a b%2", "% sem dois dígitos fica como veio");
        assert_eq!(decodifica("%C3%A9"), "é");
        let (caminho, pares) = le_pedido("GET /callback?code=x%2By&state=s HTTP/1.1");
        assert_eq!(caminho, "/callback");
        assert_eq!(pares, vec![("code".into(), "x+y".into()), ("state".into(), "s".into())]);
    }
}
