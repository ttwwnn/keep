//! `keep geral`: a aba geral de cada workspace (pedido de 24/11/2026).
//!
//! A aba do topo da lista, com o nome do workspace, é a que a pessoa usa para conduzir as demais abas dele, sem
//! deixar de poder falar direto com qualquer uma (`keep_ia::geral`). A rodada (a cada 15 s, com o `keep continuar`
//! que os dois apps já rodam):
//!
//! - acha a aba geral de cada workspace: a guardada (pelo número no mesmo keepd, pela conversa depois de um
//!   reinício) ou, na primeira vez, a que a pessoa já usava assim (a aba chamada "Geral", com conversa); sem nenhuma, abre uma (`keep nova`) assim que o workspace tem uma conversa viva — o que só
//!   dorme não pede coordenação, e um Claude parado em cada workspace pesaria na memória;
//! - grava qual é em `geral.json` (o formato das adormecidas: o app a põe no topo, com destaque);
//! - anota o que a pessoa pediu em cada outra aba e o que ela respondeu ao terminar (`geral-eventos.jsonl`), que a
//!   aba geral recebe pelo gancho na mensagem seguinte;
//! - a aba aberta depois da aba geral que terminou e ficou parada (10 min sem a pessoa voltar a ela) é levada à
//!   aba geral, digitada na caixa dela quando ela está parada e de caixa vazia e a pessoa há 20 s sem teclado nem
//!   mouse, para que ela confira e feche a que
//!   ficou sem pendências (`keep geral fechar`).
//!
//! ```text
//! keep geral                                         → {"abas":[{workspace, aba}]}  (as gerais deste keepd)
//! keep geral rodada [--seco]                         → o que a rodada fez (ou faria)
//! keep geral abas --workspace W                      → as abas de W: número, nome, programa, situação
//! keep geral mandar --workspace W --aba N --texto T  → digita T na aba parada de caixa vazia, e Enter
//! keep geral fechar --workspace W --aba N            → fecha a aba (nunca a geral, nem a que trabalha)
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use keep_ia::daemon;
use keep_ia::geral::{ARQUIVO, Evento, Geral};
use keep_ia::sessoes::{falas_de_tras, texto_da_fala, transcrito_claude};
use keep_ia::worktrees::ambiente::Ambiente;

use crate::continuar::{self, Conversa};
use crate::{Keepd, adormecer, nova};

const ESTADO: &str = "geral-estado.json";
const REGISTRO: &str = "geral.log";
/// Uma aba que terminou vai à aba geral depois deste tanto parada, sem a pessoa voltar a ela.
const QUIETO_MS: u64 = 10 * 60_000;
/// Num keepd novo, a restauração ainda traz as conversas de volta: a aba geral de antes tem este prazo para voltar
/// antes de outra ser aberta.
const ESPERA_RESTAURO_MS: u64 = 15 * 60_000;
/// Entre duas tentativas de abrir a aba geral de um workspace.
const NOVA_TENTATIVA_MS: u64 = 10 * 60_000;
/// Uma conversa que sumiu sai do estado depois disto.
const ESQUECE_MS: u64 = 30 * 24 * 3600 * 1000;
/// Segundos sem teclado nem mouse antes de digitar o aviso na aba geral.
const OCIOSA_S: f64 = 20.0;
const PEDIDO_MAX: usize = 300;

/// Um prazo em ms, trocado nos testes por `var` (em segundos).
fn prazo(var: &str, padrao: u64) -> u64 {
    std::env::var(var).ok().and_then(|s| s.parse::<u64>().ok()).map_or(padrao, |s| s * 1000)
}
const FIM_MAX: usize = 400;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Arquivo {
    keepd: Keepd,
    #[serde(default)]
    abas: Vec<Geral>,
}

/// O que a rodada já viu de cada conversa.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Vista {
    ws: String,
    /// O uuid do último pedido e da última resposta anotados.
    #[serde(default)]
    pedido: String,
    #[serde(default)]
    fim: String,
    /// Aberta depois da aba geral do workspace: ao terminar, vai a ela para conferir.
    #[serde(default)]
    nova: bool,
    #[serde(default)]
    visto_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Estado {
    #[serde(default)]
    conversas: BTreeMap<String, Vista>,
    /// Por workspace, as abas que terminaram e esperam a aba geral conferir.
    #[serde(default)]
    verificar: BTreeMap<String, Vec<u32>>,
    /// Por workspace, quando a aba geral foi aberta por último (para não abrir uma atrás da outra).
    #[serde(default)]
    tentativas: BTreeMap<String, u64>,
}

/// Uma aba solta de um workspace, como a rodada a vê.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AbaVista {
    pub aba: u32,
    pub nome: String,
    /// A conversa dela, viva ou adormecida.
    pub conversa: Option<String>,
    /// Há um Claude (ou Codex) rodando nela agora.
    pub viva: bool,
}

/// A aba geral do workspace `ws` entre as `abas` dele: a de `anterior` (pelo número no mesmo keepd, senão pela
/// conversa) ou, sem nenhuma guardada, a que a pessoa já usava como geral.
pub fn achar(ws: &str, anterior: Option<&Geral>, mesmo_keepd: bool, abas: &[AbaVista], agora: u64) -> Option<Geral> {
    let com = |a: &AbaVista, desde: u64, conversa: &str| Geral {
        workspace: ws.into(),
        aba: a.aba,
        conversa: a.conversa.clone().unwrap_or_else(|| conversa.to_string()),
        desde_ms: desde,
        nome: String::new(),
    };
    if let Some(g) = anterior {
        if let Some(a) = abas.iter().find(|a| mesmo_keepd && a.aba == g.aba) {
            return Some(com(a, g.desde_ms, &g.conversa));
        }
        if let Some(a) = abas.iter().find(|a| !g.conversa.is_empty() && a.conversa.as_deref() == Some(g.conversa.as_str())) {
            return Some(com(a, g.desde_ms, &g.conversa));
        }
        return None;
    }
    // Só a que se chama "Geral": uma aba com o nome do workspace pode ser uma conversa de trabalho sobre ele.
    abas.iter().find(|a| a.conversa.is_some() && a.nome.trim().eq_ignore_ascii_case("geral")).map(|a| com(a, agora, ""))
}

/// Uma fala do transcrito: (uuid, quando em ms, texto).
type Fala = (String, u64, String);

/// O último pedido da pessoa e a última resposta do modelo depois dele.
fn ultimas(caminho: &Path) -> (Option<Fala>, Option<Fala>) {
    let (mut pedido, mut fim) = (None, None);
    let mut vistas = 0;
    falas_de_tras(caminho, |d| {
        vistas += 1;
        let tipo = d.get("type").and_then(Value::as_str).unwrap_or("");
        if tipo == "continued-in" {
            return false;
        }
        let texto = texto_da_fala(d);
        let texto = texto.trim();
        let quando = keep_ia::sessoes::carimbo_ms(d);
        // A marca de cada fala: o uuid que o Claude Code grava, senão a hora dela.
        let uuid = d.get("uuid").and_then(Value::as_str).filter(|u| !u.is_empty()).map_or_else(|| quando.to_string(), String::from);
        if tipo == "assistant" && fim.is_none() && !texto.is_empty() {
            fim = Some((uuid, quando, texto.to_string()));
        } else if tipo == "user" && !texto.is_empty() && !texto.starts_with('<') {
            pedido = Some((uuid, quando, texto.to_string()));
            return false;
        }
        vistas < 3000
    });
    (pedido, fim)
}

fn uma_linha(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn comeco(t: &str, n: usize) -> String {
    let t = uma_linha(t);
    if t.chars().count() <= n { t } else { t.chars().take(n).collect::<String>() + "…" }
}

fn final_de(t: &str, n: usize) -> String {
    let t = uma_linha(t);
    let total = t.chars().count();
    if total <= n { t } else { "…".to_string() + &t.chars().skip(total - n).collect::<String>() }
}

fn registrar(texto: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(crate::pasta());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::pasta().join(REGISTRO)) {
        let _ = writeln!(f, "{}  {texto}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

/// O título que o programa deu à aba, sem a marca de estado da frente.
fn titulo(a: &daemon::Aba) -> String {
    a.info.title.trim_start_matches(|c: char| !c.is_alphanumeric()).trim().to_string()
}

/// Tudo o que a rodada e os comandos olham, num instante.
struct Visao {
    lista: daemon::Retrato,
    amb: Ambiente,
    keepd: Keepd,
    nasceu: Option<f64>,
    nomes: std::collections::HashMap<(String, u32), String>,
    /// O nome que a pessoa deu a cada workspace no app (`names.json`, `workspaces`).
    nomes_ws: std::collections::HashMap<String, String>,
    /// (ws, aba) → (conversa, agente, pid) das abas com IA viva.
    vivas: BTreeMap<(String, u32), (Option<String>, String, u32)>,
    dormindo: Vec<adormecer::Adormecida>,
}

impl Visao {
    fn agora() -> Result<Visao, String> {
        let lista = daemon::listar()?;
        let amb = Ambiente::do_sistema();
        let keepd = crate::retrato::keepd_da_lista(&lista, &amb);
        let nasceu = keep_ia::worktrees::abas::nascimento_do_socket(&amb);
        let nomes = keep_ia::worktrees::abas::nomes_do_app(&amb, nasceu);
        let contas = keep_ia::contas::listar(false);
        let vivas = keep_ia::abas::das_abas(&lista, &contas)
            .into_iter()
            .map(|i| ((i.workspace, i.aba), (i.conversa, i.agente, i.pid)))
            .collect();
        let dormindo = adormecer::do_keepd(&keepd);
        let nomes_ws = amb
            .app
            .as_ref()
            .and_then(|app| crate::ler::<Value>(&app.join("names.json")))
            .and_then(|v| v.get("workspaces").and_then(Value::as_object).cloned())
            .map(|m| m.into_iter().filter_map(|(k, v)| Some((k, v.as_str()?.trim().to_string()))).filter(|(_, v)| !v.is_empty()).collect())
            .unwrap_or_default();
        Ok(Visao { lista, amb, keepd, nasceu, nomes, nomes_ws, vivas, dormindo })
    }

    fn workspaces(&self) -> Vec<String> {
        let mut ws: Vec<String> = Vec::new();
        for a in self.lista.abas.iter().filter(|a| !a.info.finished && a.ws != crate::retrato::RAPIDO) {
            if !ws.contains(&a.ws) {
                ws.push(a.ws.clone());
            }
        }
        ws
    }

    /// O nome com que a pessoa vê o workspace: o que ela deu no app, senão o do Keep.
    fn nome_ws(&self, ws: &str) -> String {
        self.nomes_ws.get(ws).cloned().unwrap_or_else(|| ws.to_string())
    }

    fn nome(&self, ws: &str, aba: u32) -> String {
        self.nomes
            .get(&(ws.to_string(), aba))
            .cloned()
            .or_else(|| self.lista.aba(ws, aba).map(titulo))
            .unwrap_or_default()
    }

    fn adormecida(&self, ws: &str, aba: u32) -> Option<&adormecer::Adormecida> {
        self.dormindo.iter().find(|d| d.workspace == ws && d.aba == aba)
    }

    /// As abas soltas de `ws` (os painéis contam com a aba de que são painel).
    fn abas(&self, ws: &str) -> Vec<AbaVista> {
        let soltas = self.lista.abas.iter().filter(|a| a.ws == ws && !a.info.finished && a.info.split_of == 0);
        soltas
            .map(|a| {
                let id = a.info.id;
                let paineis: Vec<u32> = std::iter::once(id)
                    .chain(self.lista.abas.iter().filter(|p| p.ws == ws && p.info.split_of == id).map(|p| p.info.id))
                    .collect();
                let viva = paineis.iter().find_map(|p| self.vivas.get(&(ws.to_string(), *p)));
                let conversa = viva
                    .and_then(|v| v.0.clone())
                    .or_else(|| paineis.iter().find_map(|p| self.adormecida(ws, *p)).map(|d| d.ia.conversa.clone()));
                AbaVista { aba: id, nome: self.nome(ws, id), conversa, viva: viva.is_some() }
            })
            .collect()
    }
}

fn arquivo() -> std::path::PathBuf {
    crate::pasta().join(ARQUIVO)
}

/// Pode digitar na conversa `c`? A sessão parada, a tela na conversa, a caixa à vista e vazia.
fn livre(c: &Conversa) -> Result<(), String> {
    match keep_ia::sessoes::sessao_claude(c.pid).map(|s| s.status) {
        Some(s) if s == "idle" => {}
        Some(s) => return Err(format!("a aba não está parada ({s})")),
        None => return Err("sem a sessão do Claude da aba".into()),
    }
    let t = continuar::tela_da_aba(&c.ws, c.tab).ok_or("não li a tela da aba")?;
    if let Some((oque, _)) = continuar::vista_desviada(&t) {
        return Err(format!("a vista está {oque}"));
    }
    if t.caixa.is_none() {
        return Err("sem caixa de entrada à vista".into());
    }
    if t.linhas[t.rodape.min(t.linhas.len())..].join("\n").contains("esc to interrupt") {
        return Err("a aba trabalha".into());
    }
    if !t.digitado.is_empty() {
        return Err("a caixa tem texto".into());
    }
    Ok(())
}

fn conversa_na(ws: &str, aba: u32) -> Option<Conversa> {
    let lista = daemon::listar().ok()?;
    continuar::conversas(&lista).0.into_iter().find(|c| c.ws == ws && c.tab == aba)
}

/// Uma rodada. `seco`: diz o que faria, sem abrir, digitar nem acordar.
pub fn rodada(seco: bool) -> Result<Value, String> {
    let Some(_trava) = crate::Trava::pegar(".geral.lock") else {
        return Ok(json!({ "motivo": "outra rodada em andamento" }));
    };
    let v = Visao::agora()?;
    let agora = crate::agora_ms();
    let antigo: Arquivo = crate::ler(&arquivo()).unwrap_or_default();
    let mesmo_keepd = antigo.keepd.mesmo(&v.keepd);
    let mut estado: Estado = crate::ler(&crate::pasta().join(ESTADO)).unwrap_or_default();
    let estado_antes = estado.clone();
    let keepd_novo = agora.saturating_sub(v.keepd.inicio_ms) < ESPERA_RESTAURO_MS;

    let mut gerais = Vec::new();
    let mut relato = Vec::new();
    let mut nomear = Vec::new();
    for ws in v.workspaces() {
        let abas = v.abas(&ws);
        let anterior = antigo.abas.iter().find(|g| g.workspace == ws);
        let chamado = v.nome_ws(&ws);
        let nome = if chamado == ws { String::new() } else { chamado.clone() };
        if let Some(g) = achar(&ws, anterior, mesmo_keepd, &abas, agora) {
            let g = Geral { nome: nome.clone(), ..g };
            if anterior.is_none_or(|a| a.aba != g.aba || !mesmo_keepd || a.nome != g.nome) {
                nomear.push((ws.clone(), g.aba, chamado.clone()));
                registrar(&format!("{ws}: a aba {} é a geral", g.aba));
            }
            gerais.push(g);
            continue;
        }
        // Ninguém a coordenar enquanto nenhuma conversa do workspace está viva.
        if !abas.iter().any(|a| a.viva) {
            continue;
        }
        if let Some(g) = anterior.filter(|g| !g.conversa.is_empty() && !mesmo_keepd && keepd_novo) {
            gerais.push(Geral { aba: 0, ..g.clone() });
            relato.push(json!({ "workspace": ws, "situacao": "a aba geral de antes ainda pode voltar com a restauração" }));
            continue;
        }
        if estado.tentativas.get(&ws).is_some_and(|t| agora.saturating_sub(*t) < prazo("KEEP_GERAL_NOVA_TENTATIVA_S", NOVA_TENTATIVA_MS)) {
            relato.push(json!({ "workspace": ws, "situacao": "aba geral aberta há pouco; espero" }));
            continue;
        }
        if seco {
            relato.push(json!({ "workspace": ws, "situacao": "abriria a aba geral" }));
            continue;
        }
        estado.tentativas.insert(ws.clone(), agora);
        let pedido = nova::Pedido {
            workspace: Some(ws.clone()),
            pasta: keep_ia::caminhos::casa(),
            conteudo: nova::Conteudo::Argumentos(Vec::new()),
            nome: Some(chamado.clone()),
            sem_enter: false,
        };
        match nova::abrir(&pedido) {
            Ok((_, aba, _)) => {
                registrar(&format!("{ws}: abri a aba geral (aba {aba})"));
                relato.push(json!({ "workspace": ws, "situacao": format!("abri a aba geral (aba {aba})") }));
                gerais.push(Geral { workspace: ws.clone(), aba, conversa: String::new(), desde_ms: agora, nome });
            }
            Err(e) => {
                registrar(&format!("{ws}: não abri a aba geral: {e}"));
                relato.push(json!({ "workspace": ws, "situacao": format!("não abri a aba geral: {e}") }));
            }
        }
    }
    let novo = Arquivo { keepd: v.keepd.clone(), abas: gerais.clone() };
    if !seco && novo != antigo {
        crate::gravar(&arquivo(), &novo).map_err(|e| format!("não gravei {ARQUIVO}: {e}"))?;
    }
    if !seco {
        crate::nomear_no_app(&v.amb, v.nasceu, &nomear, true);
    }

    // As novidades das outras abas.
    let (abertas, _) = continuar::conversas(&v.lista);
    let mut eventos = Vec::new();
    let vivas: Vec<&str> = abertas.iter().map(|c| c.sid.as_str()).collect();
    estado.conversas.retain(|sid, x| vivas.contains(&sid.as_str()) || agora.saturating_sub(x.visto_ms) < ESQUECE_MS);
    for c in &abertas {
        let Some(g) = gerais.iter().find(|g| g.workspace == c.ws && g.aba != 0) else { continue };
        if c.tab == g.aba || c.sid == g.conversa {
            continue;
        }
        let Some(caminho) = transcrito_claude(&c.sid) else { continue };
        let (pedido, fim) = ultimas(&caminho);
        // A conversa que já existia quando a aba geral chegou entra sem novidades; a de depois, com todas.
        let ja_havia = antigo.abas.iter().any(|a| a.workspace == c.ws);
        let x = estado.conversas.entry(c.sid.clone()).or_insert_with(|| {
            let nova = ja_havia;
            Vista {
                ws: c.ws.clone(),
                pedido: if nova { String::new() } else { pedido.as_ref().map(|p| p.0.clone()).unwrap_or_default() },
                fim: if nova { String::new() } else { fim.as_ref().map(|f| f.0.clone()).unwrap_or_default() },
                nova,
                visto_ms: agora,
            }
        });
        let nome = v.nome(&c.ws, c.tab);
        let evento = |tipo: &str, quando: u64, texto: String| Evento {
            quando_ms: if quando == 0 { agora } else { quando },
            workspace: c.ws.clone(),
            aba: c.tab,
            nome: nome.clone(),
            tipo: tipo.into(),
            texto,
        };
        if let Some((uuid, quando, texto)) = &pedido {
            if *uuid != x.pedido {
                eventos.push(evento("pedido", *quando, comeco(texto, PEDIDO_MAX)));
                x.pedido = uuid.clone();
                x.visto_ms = agora;
            }
        }
        let Some((uuid, quando, texto)) = &fim else { continue };
        if *uuid == x.fim || agora.saturating_sub(*quando) < prazo("KEEP_GERAL_QUIETO_S", QUIETO_MS) {
            continue;
        }
        if keep_ia::sessoes::sessao_claude(c.pid).is_none_or(|s| s.status != "idle") {
            continue;
        }
        eventos.push(evento("fim", *quando, final_de(texto, FIM_MAX)));
        x.fim = uuid.clone();
        x.visto_ms = agora;
        if x.nova {
            let lista = estado.verificar.entry(c.ws.clone()).or_default();
            if !lista.contains(&c.tab) {
                lista.push(c.tab);
            }
        }
    }
    if !seco {
        keep_ia::geral::anotar(&eventos);
    }

    // As que terminaram vão à aba geral.
    let mut avisadas = Vec::new();
    for (ws, abas) in estado.verificar.iter_mut() {
        abas.retain(|t| v.lista.aba(ws, *t).is_some());
        let Some(g) = gerais.iter().find(|g| &g.workspace == ws && g.aba != 0) else { continue };
        if abas.is_empty() {
            continue;
        }
        if v.adormecida(ws, g.aba).is_some() {
            if !seco {
                let _ = adormecer::acordar(ws, g.aba);
            }
            relato.push(json!({ "workspace": ws, "situacao": "acordei a aba geral para conferir as que terminaram" }));
            continue;
        }
        let Some(c) = abertas.iter().find(|c| &c.ws == ws && c.tab == g.aba) else { continue };
        // Com a pessoa no teclado, o aviso podia se misturar ao que ela começa a digitar na aba geral.
        if continuar::ocioso_ha() < OCIOSA_S {
            relato.push(json!({ "workspace": ws, "situacao": "abas a conferir; espero a pessoa parar de digitar" }));
            continue;
        }
        if let Err(porque) = livre(c) {
            relato.push(json!({ "workspace": ws, "situacao": format!("abas a conferir; a aba geral espera: {porque}") }));
            continue;
        }
        let lista: Vec<String> = abas
            .iter()
            .map(|t| match v.nome(ws, *t) {
                n if n.is_empty() => format!("aba {t}"),
                n => format!("aba {t} «{n}»"),
            })
            .collect();
        let texto = format!(
            "[Keep] Terminaram e esperam sua conferência: {}. Veja o que cada uma fez e feche as que não têm pendência.",
            lista.join(", ")
        );
        if seco {
            relato.push(json!({ "workspace": ws, "situacao": format!("digitaria: {texto}") }));
            continue;
        }
        match continuar::digitar(c, &texto, false) {
            Ok(()) => {
                registrar(&format!("{ws}: levei à aba geral: {}", lista.join(", ")));
                avisadas.push(json!({ "workspace": ws, "abas": abas.clone() }));
                abas.clear();
            }
            Err(e) => relato.push(json!({ "workspace": ws, "situacao": format!("não digitei na aba geral: {e}") })),
        }
    }
    estado.verificar.retain(|_, a| !a.is_empty());
    if !seco && estado != estado_antes {
        let _ = crate::gravar(&crate::pasta().join(ESTADO), &estado);
    }
    Ok(json!({
        "versao": crate::VERSAO,
        "gerais": gerais.iter().map(|g| json!({ "workspace": g.workspace, "aba": g.aba })).collect::<Vec<_>>(),
        "eventos": eventos.len(),
        "avisadas": avisadas,
        "relato": relato,
    }))
}

/// As abas gerais deste keepd, para o app desenhar (o do macOS lê o arquivo; o do Windows pergunta).
pub fn listar() -> Result<Value, String> {
    let lista = daemon::listar()?;
    let keepd = crate::retrato::keepd_da_lista(&lista, &Ambiente::do_sistema());
    let a: Arquivo = crate::ler(&arquivo()).unwrap_or_default();
    let abas: Vec<Value> = if a.keepd.mesmo(&keepd) {
        a.abas.iter().filter(|g| g.aba != 0).map(|g| json!({ "workspace": g.workspace, "aba": g.aba })).collect()
    } else {
        Vec::new()
    };
    Ok(json!({ "versao": crate::VERSAO, "abas": abas }))
}

fn geral_de(ws: &str) -> Option<u32> {
    let lista = daemon::listar().ok()?;
    let keepd = crate::retrato::keepd_da_lista(&lista, &Ambiente::do_sistema());
    let a: Arquivo = crate::ler(&arquivo())?;
    a.keepd.mesmo(&keepd).then(|| a.abas.iter().find(|g| g.workspace == ws && g.aba != 0).map(|g| g.aba)).flatten()
}

/// As abas do workspace, para a aba geral.
pub fn abas(ws: &str) -> Result<Value, String> {
    let v = Visao::agora()?;
    if !v.workspaces().iter().any(|w| w == ws) {
        return Err(format!("workspace {ws:?} não existe no Keep"));
    }
    let geral = geral_de(ws);
    let lista: Vec<Value> = v
        .abas(ws)
        .iter()
        .map(|a| {
            let viva = v.vivas.get(&(ws.to_string(), a.aba));
            let (programa, situacao) = match viva {
                Some((_, agente, pid)) => {
                    let s = keep_ia::sessoes::sessao_claude(*pid).map(|s| s.status).unwrap_or_default();
                    let situacao = match s.as_str() {
                        "busy" => "trabalhando",
                        "waiting" => "esperando a pessoa",
                        "idle" => "parada",
                        _ => "aberta",
                    };
                    (agente.clone(), situacao)
                }
                None if v.adormecida(ws, a.aba).is_some() => ("claude".into(), "hibernando"),
                None => ("shell".into(), "shell"),
            };
            json!({
                "aba": a.aba,
                "nome": a.nome,
                "geral": Some(a.aba) == geral,
                "programa": programa,
                "situacao": situacao,
                "conversa": a.conversa,
            })
        })
        .collect();
    Ok(json!({ "versao": crate::VERSAO, "workspace": ws, "geral": geral, "abas": lista }))
}

/// Digita `texto` na conversa da aba, parada e de caixa vazia, e aperta Enter.
pub fn mandar(ws: &str, aba: u32, texto: &str) -> Result<Value, String> {
    let texto = uma_linha(texto);
    if texto.is_empty() {
        return Err("texto vazio".into());
    }
    let c = conversa_na(ws, aba).ok_or_else(|| format!("a aba {aba} de {ws} não tem conversa do Claude aberta"))?;
    livre(&c).map_err(|e| format!("não digito na aba {aba}: {e}"))?;
    continuar::digitar(&c, &texto, false)?;
    Ok(json!({ "versao": crate::VERSAO, "ok": true, "workspace": ws, "aba": aba }))
}

/// Fecha a aba `aba` de `ws`: a conversa sai pelo `/exit` (a worktree dela fica para a varredura das órfãs) e a
/// aba fecha. Nunca a aba geral, nem a que trabalha, espera a pessoa ou tem texto na caixa.
pub fn fechar(ws: &str, aba: u32) -> Result<Value, String> {
    if geral_de(ws) == Some(aba) {
        return Err("esta é a aba geral do workspace; ela não se fecha por aqui".into());
    }
    let lista = daemon::listar()?;
    let a = lista.aba(ws, aba).ok_or_else(|| format!("a aba {aba} de {ws} não existe"))?;
    if a.info.split_of != 0 {
        return Err(format!("a aba {aba} é painel de outra; feche a aba {}", a.info.split_of));
    }
    if let Some(c) = continuar::conversas(&lista).0.into_iter().find(|c| c.ws == ws && c.tab == aba) {
        livre(&c).map_err(|e| format!("não fecho a aba {aba}: {e}"))?;
        let vinculo = keep_ia::vinculo::vincular(&lista).get(&(ws.to_string(), aba)).map(|v| v.shell);
        let sintaxe = keep_ia::trocar::sintaxe_da_aba(a, vinculo);
        keep_ia::trocar::sai_do_claude(ws, aba, false, sintaxe).map_err(|r| format!("não fecho a aba {aba}: {}", r.json()))?;
    } else if lista.abas.iter().any(|p| p.ws == ws && (p.info.id == aba || p.info.split_of == aba) && p.info.busy) {
        return Err(format!("não fecho a aba {aba}: algo roda nela"));
    }
    daemon::fecha_aba(ws, aba)?;
    std::thread::sleep(Duration::from_millis(200));
    registrar(&format!("{ws}: fechei a aba {aba}"));
    Ok(json!({ "versao": crate::VERSAO, "ok": true, "workspace": ws, "aba": aba }))
}

/// O valor de `--nome valor` ou `--nome=valor`.
fn opcao<'a>(args: &'a [String], nome: &str) -> Option<&'a str> {
    let com_igual = format!("{nome}=");
    args.iter().enumerate().find_map(|(i, a)| {
        if a == nome {
            args.get(i + 1).map(String::as_str)
        } else {
            a.strip_prefix(&com_igual)
        }
    })
}

pub fn cli(args: &[String]) -> i32 {
    const USO: &str = "keep geral [rodada [--seco] | abas --workspace W | mandar --workspace W --aba N --texto T | fechar --workspace W --aba N]";
    let ws = opcao(args, "--workspace");
    let aba = opcao(args, "--aba").and_then(|n| n.parse::<u32>().ok());
    let resposta = match (args.first().map(String::as_str), ws, aba) {
        (None, _, _) => listar(),
        (Some("rodada"), _, _) => rodada(args.iter().any(|a| a == "--seco")),
        (Some("abas"), Some(ws), _) => abas(ws),
        (Some("mandar"), Some(ws), Some(aba)) => match opcao(args, "--texto") {
            Some(t) => mandar(ws, aba, t),
            None => Err(format!("falta --texto. uso: {USO}")),
        },
        (Some("fechar"), Some(ws), Some(aba)) => fechar(ws, aba),
        _ => {
            eprintln!("uso: {USO}");
            return 2;
        }
    };
    match resposta {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            println!("{}", json!({ "versao": crate::VERSAO, "ok": false, "erro": e }));
            1
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn aba(n: u32, nome: &str, conversa: Option<&str>) -> AbaVista {
        AbaVista { aba: n, nome: nome.into(), conversa: conversa.map(String::from), viva: conversa.is_some() }
    }

    fn geral(aba: u32, conversa: &str) -> Geral {
        Geral { workspace: "W".into(), aba, conversa: conversa.into(), desde_ms: 5, nome: String::new() }
    }

    #[test]
    fn a_geral_guardada_e_achada_pelo_numero_e_depois_de_reiniciar_pela_conversa() {
        let abas = [aba(1, "X", Some("c-1")), aba(4, "W", Some("c-9"))];
        // mesmo keepd: pelo número, com a conversa de agora (um /clear troca a conversa, a aba segue a geral)
        assert_eq!(achar("W", Some(&geral(1, "velha")), true, &abas, 9), Some(Geral { conversa: "c-1".into(), ..geral(1, "") }));
        // keepd novo: o número não vale; a conversa diz qual é
        assert_eq!(achar("W", Some(&geral(1, "c-9")), false, &abas, 9), Some(geral(4, "c-9")));
        // keepd novo e a conversa ainda não voltou: nenhuma (a rodada espera ou abre outra)
        assert_eq!(achar("W", Some(&geral(1, "c-7")), false, &abas, 9), None);
        // a aba guardada fechou: nenhuma; a de mesmo nome não é adotada no lugar dela
        assert_eq!(achar("W", Some(&geral(2, "")), true, &abas, 9), None);
    }

    #[test]
    fn na_primeira_vez_a_geral_que_a_pessoa_ja_usava_e_adotada() {
        let abas = [aba(1, "Compras", Some("c-1")), aba(5, " Geral ", Some("c-5"))];
        assert_eq!(achar("W", None, true, &abas, 9), Some(Geral { aba: 5, conversa: "c-5".into(), desde_ms: 9, ..geral(0, "") }));
        // a de mesmo nome do workspace, não: pode ser uma conversa de trabalho sobre ele
        let abas = [aba(3, "w", Some("c-3"))];
        assert_eq!(achar("W", None, true, &abas, 9), None);
        // sem conversa (um shell chamado Geral) não é adotada
        let abas = [aba(5, "Geral", None)];
        assert_eq!(achar("W", None, true, &abas, 9), None);
    }

    #[test]
    fn o_ultimo_pedido_e_a_resposta_depois_dele() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let fala = |tipo: &str, uuid: &str, texto: &str| {
            json!({ "type": tipo, "uuid": uuid, "timestamp": "2026-11-24T10:00:00Z", "message": { "content": [{ "type": "text", "text": texto }] } })
                .to_string()
        };
        let resultado = json!({ "type": "user", "uuid": "r", "message": { "content": [{ "type": "tool_result", "content": "ok" }] } }).to_string();
        let linhas = [
            fala("user", "p1", "primeiro"),
            fala("assistant", "a1", "resposta 1"),
            fala("user", "p2", "faça X"),
            fala("assistant", "a2", "vou fazer"),
            resultado,
            fala("user", "s", "<system-reminder>nada</system-reminder>"),
            fala("assistant", "a3", "X feito"),
        ];
        std::fs::write(f.path(), linhas.join("\n") + "\n").unwrap();
        let (p, r) = ultimas(f.path());
        assert_eq!(p.map(|p| (p.0, p.2)), Some(("p2".into(), "faça X".into())));
        assert_eq!(r.map(|r| (r.0, r.2)), Some(("a3".into(), "X feito".into())));

        // pedido ainda sem resposta: nenhuma
        std::fs::write(f.path(), [fala("assistant", "a1", "antes"), fala("user", "p2", "e agora?")].join("\n") + "\n").unwrap();
        let (p, r) = ultimas(f.path());
        assert_eq!((p.map(|p| p.0), r), (Some("p2".into()), None));
    }

    #[test]
    fn textos_curtos_numa_linha() {
        assert_eq!(comeco("a\n b  c", 10), "a b c");
        assert_eq!(comeco("abcdef", 3), "abc…");
        assert_eq!(final_de("abcdef", 3), "…def");
        let args: Vec<String> = ["fechar", "--workspace", "Meu WS", "--aba=7"].iter().map(|s| s.to_string()).collect();
        assert_eq!((opcao(&args, "--workspace"), opcao(&args, "--aba")), (Some("Meu WS"), Some("7")));
    }
}
