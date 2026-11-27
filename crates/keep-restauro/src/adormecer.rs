//! A conversa parada há muito tempo sai da memória, e a aba fica.
//!
//! Cada Claude Code aberto pesa de 200 a 600 MB, e num Mac de 8 GB as abas
//! esquecidas somam gigabytes. Pedido de 21/10/2026: a conversa
//! parada há mais de 12 h é fechada, a aba segue no Keep e a conversa volta
//! quando a pessoa a chama.
//!
//! - **Adormecer** (`keep restauro adormecer`, o app a cada minuto): a aba do
//!   Claude fora de vista, livre (caixa vazia, nada rodando, nenhuma pergunta,
//!   nada em segundo plano) e sem fala na conversa há `PARADA`, sai do Claude
//!   pelo `/exit`. A conversa, a conta e as opções
//!   ficam em `adormecidas.json` (por keepd) antes da saída.
//! - **Memória** (pedido de 25/11/2026): com a memória apertada, ou mais
//!   conversas vivas que o teto da RAM (`memoria`), a mesma rodada tira também
//!   as paradas há 10 min, a parada há mais tempo primeiro (a aba geral por
//!   último), uma por vez, medindo de novo entre elas, até aliviar.
//!   `abrir_vaga` faz só isso, para uma aba nova (`keep nova`, a fila), poupando
//!   as abas que o app mostrou há pouco (`vistas.json`), e só quando hibernar
//!   abre o lugar (o teto de conversas, a memória disponível; a pressão sozinha não).
//! - **De volta pela fila** (pedido de 27/11/2026): a conversa que saiu pela
//!   memória não tinha terminado, até onde o Keep sabe; ela entra na fila de
//!   memória (`fila`) e acorda quando chegar a vez dela. Uma volta só: a que a
//!   fila acordou e ninguém falou com ela desde então não volta de novo.
//! - **A aba geral** (pedido de 27/11/2026) não hiberna sozinha, nem pelas
//!   12 h nem pela memória, enquanto outra aba do workspace tiver conversa viva
//!   (do Claude ou do Codex; a aba só com terminal não conta): sai depois que
//!   todas as outras hibernaram. O zzz da pessoa vale para ela como para as outras.
//! - **Agora** (`keep restauro adormecer --agora W [--aba N]`, o zzz da aba na
//!   barra lateral): o mesmo, para a aba N (ou as livres do workspace W), sem
//!   esperar as 12 h.
//! - **Adormecidas** (`keep restauro adormecidas`, o app do Windows): quais abas
//!   do keepd de agora estão adormecidas, para a barra lateral riscar o título.
//! - **Acordar** (`keep restauro acordar`, o app ao mostrar a aba): a
//!   conversa volta na mesma aba, como a restauração a retoma. A acordada não
//!   volta a dormir antes da mensagem que a acordou (até 30 min), e a parada
//!   dela conta do acordar (`acordadas.json`).
//! - O retrato leva a aba adormecida com a conversa dela: depois de um
//!   reinício ela volta adormecida e acorda do mesmo jeito.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use keep_ia::abas::IaAba;
use keep_ia::daemon;
use keep_ia::tela::{self, Programa};
use keep_ia::worktrees::ambiente::Ambiente;

use crate::{Ia, Keepd, memoria};

pub const ARQUIVO: &str = "adormecidas.json";
/// Quando cada aba foi acordada, por keepd.
const ACORDADAS: &str = "acordadas.json";
/// Quando o app mostrou cada aba por último (nas rodadas, e ao mostrá-la), por keepd.
const VISTAS: &str = "vistas.json";
/// Quando a fila de memória acordou cada aba, por keepd.
const DA_FILA: &str = "acordadas-pela-fila.json";

/// Quanto tempo parada antes de sair da memória.
const PARADA: Duration = Duration::from_secs(12 * 3600);
/// O mesmo, com a memória apertada.
const PARADA_APERTO: Duration = Duration::from_secs(10 * 60);
/// A aba acordada sem fala nenhuma desde então espera a mensagem que a acordou este tanto antes de poder dormir.
const ESPERA_MENSAGEM_MS: u64 = 30 * 60_000;
/// Uma aba que o app mostrou conta como à vista este tanto, para quem abre lugar na memória sem as vistas da hora.
const VISTA_MS: u64 = 5 * 60_000;
/// As marcas de acordada e de vista somem depois disto.
const LEMBRA_MS: u64 = 24 * 3600 * 1000;

/// Uma aba cuja conversa saiu da memória.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adormecida {
    pub workspace: String,
    pub aba: u32,
    pub ia: Ia,
    pub desde_ms: u64,
    /// O nome posto na aba ao adormecer (o título da conversa, numa aba sem nome dado): sai ao acordar.
    #[serde(default)]
    pub nome_posto: Option<String>,
}

/// As adormecidas de um keepd (os números das abas valem só nele).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Estado {
    pub keepd: Keepd,
    #[serde(default)]
    pub abas: Vec<Adormecida>,
}

fn parada() -> Duration {
    std::env::var("KEEP_ADORMECER_PARADA_S").ok().and_then(|s| s.parse().ok()).map(Duration::from_secs).unwrap_or(PARADA)
}

fn parada_aperto() -> Duration {
    std::env::var("KEEP_ADORMECER_APERTO_S").ok().and_then(|s| s.parse().ok()).map(Duration::from_secs).unwrap_or(PARADA_APERTO)
}

/// As adormecidas do keepd `k` (de outro keepd, nenhuma).
pub fn do_keepd(k: &Keepd) -> Vec<Adormecida> {
    crate::ler::<Estado>(&crate::pasta().join(ARQUIVO)).filter(|e| e.keepd.mesmo(k)).map(|e| e.abas).unwrap_or_default()
}

/// As abas adormecidas do keepd de agora, para o app desenhar (o do macOS lê o arquivo; o do Windows pergunta).
pub fn listar() -> Result<Value, String> {
    let lista = daemon::listar()?;
    let keepd = crate::retrato::keepd_da_lista(&lista, &Ambiente::do_sistema());
    let abas: Vec<Value> = do_keepd(&keepd).iter().map(|a| json!({ "workspace": a.workspace, "aba": a.aba })).collect();
    Ok(json!({ "versao": crate::VERSAO, "abas": abas }))
}

/// Lê, muda e grava o estado do keepd `k` sob uma trava (o app adormece e acorda ao mesmo tempo).
fn mudar<T>(k: &Keepd, f: impl FnOnce(&mut Vec<Adormecida>) -> T) -> T {
    let pasta = crate::pasta();
    let _ = std::fs::create_dir_all(&pasta);
    let trava = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(pasta.join(".adormecidas.lock"));
    let _trava = trava.ok().filter(|f| f.lock().is_ok());
    let mut abas = do_keepd(k);
    let antes = abas.clone();
    let saida = f(&mut abas);
    if abas != antes {
        let _ = crate::gravar(&pasta.join(ARQUIVO), &Estado { keepd: k.clone(), abas });
    }
    saida
}

/// Põe uma aba entre as adormecidas (a restauração, para a que voltou de um reinício ainda adormecida).
pub fn registrar(k: &Keepd, ws: &str, aba: u32, ia: Ia, nome_posto: Option<String>) {
    mudar(k, |abas| {
        abas.retain(|a| !(a.workspace == ws && a.aba == aba));
        abas.push(Adormecida { workspace: ws.into(), aba, ia, desde_ms: crate::agora_ms(), nome_posto });
    });
}

/// Uma aba e a hora de alguma coisa que aconteceu com ela (acordou, foi mostrada).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Marca {
    workspace: String,
    aba: u32,
    em_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Marcas {
    keepd: Keepd,
    #[serde(default)]
    abas: Vec<Marca>,
}

fn marcas(arquivo: &str, k: &Keepd) -> Vec<Marca> {
    crate::ler::<Marcas>(&crate::pasta().join(arquivo)).filter(|m| m.keepd.mesmo(k)).map(|m| m.abas).unwrap_or_default()
}

/// Marca agora as `abas` (a marca nova troca a velha da mesma aba); as de mais de um dia saem.
fn marcar(arquivo: &str, k: &Keepd, abas: &[(String, u32)]) {
    if abas.is_empty() {
        return;
    }
    let pasta = crate::pasta();
    let _ = std::fs::create_dir_all(&pasta);
    let trava = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(pasta.join(format!(".{arquivo}.lock")));
    let _trava = trava.ok().filter(|f| f.lock().is_ok());
    let agora = crate::agora_ms();
    let mut lista = marcas(arquivo, k);
    lista.retain(|m| agora.saturating_sub(m.em_ms) < LEMBRA_MS && !abas.iter().any(|(w, a)| *w == m.workspace && *a == m.aba));
    lista.extend(abas.iter().map(|(w, a)| Marca { workspace: w.clone(), aba: *a, em_ms: agora }));
    let _ = crate::gravar(&pasta.join(arquivo), &Marcas { keepd: k.clone(), abas: lista });
}

/// As abas que o app mostrou há pouco (nas rodadas e ao mostrar cada uma): o que se poupa ao abrir lugar na memória
/// para uma aba nova, sem as vistas da hora, que só o app sabe.
fn vistas_recentes(k: &Keepd) -> HashSet<(String, u32)> {
    let agora = crate::agora_ms();
    marcas(VISTAS, k).into_iter().filter(|m| agora.saturating_sub(m.em_ms) < VISTA_MS).map(|m| (m.workspace, m.aba)).collect()
}

/// O título que o Claude deu à aba, sem a marca de estado da frente (`✳`, o pião).
fn titulo(aba: &daemon::Aba) -> Option<String> {
    let t = aba.info.title.trim_start_matches(|c: char| !c.is_alphanumeric()).trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// A aba de que `tab` é painel, subindo até a solta: é o número com que o app conhece a aba inteira.
fn raiz(lista: &daemon::Retrato, ws: &str, tab: u32) -> u32 {
    let mut atual = tab;
    for _ in 0..32 {
        match lista.aba(ws, atual).map(|a| a.info.split_of) {
            Some(mae) if mae != 0 => atual = mae,
            _ => break,
        }
    }
    atual
}

/// `W/N` (a aba N do workspace W) em (W, N); o workspace pode ter `/`.
pub fn vista(texto: &str) -> Option<(String, u32)> {
    let (ws, n) = texto.rsplit_once('/')?;
    Some((ws.to_string(), n.parse().ok()?))
}

/// Quando a conversa falou por último: a última fala da pessoa ou do Claude no transcrito. Nem o tráfego da
/// aba (o app a redesenha) nem o arquivo (o Remote Control grava nele sem conversa nenhuma) dizem isso; sem
/// fala com hora, vale o arquivo.
/// `None` sem transcrito: a conversa nunca falou (o Claude só o grava na primeira mensagem), e não há o que retomar.
fn ultima_fala_ms(conversa: &str) -> Option<u64> {
    let caminho = keep_ia::sessoes::transcrito_claude(conversa)?;
    let mut quando = 0;
    keep_ia::sessoes::falas_de_tras(&caminho, |d| {
        quando = keep_ia::sessoes::carimbo_ms(d);
        false
    });
    if quando == 0 {
        quando = std::fs::metadata(&caminho)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
    }
    Some(quando)
}

/// Por que a aba não adormece agora, ou `None` se pode. Em palavras que a pessoa lê: o app as mostra quando o
/// zzz da aba é recusado.
fn impedimento(aba: &daemon::Aba, pid: u32) -> Option<String> {
    let (linhas, limpas) = keep_ia::trocar::tela_de(&aba.ws, aba.info.id);
    let (estado, detalhe) = tela::situacao(&Programa::Claude, &linhas, &limpas);
    if estado != tela::Estado::Livre {
        return Some(detalhe);
    }
    if let Some(s) = keep_ia::sessoes::sessao_claude(pid) {
        if s.trabalhando() || s.esperando() {
            return Some(match s.status.as_str() {
                "busy" => "trabalhando num turno".into(),
                "shell" => "rodando um comando".into(),
                "waiting" => "esperando uma resposta sua".into(),
                outro => format!("sessão {outro}"),
            });
        }
    }
    let fundo = tela::segundo_plano_no_rodape(&linhas);
    if !fundo.is_empty() {
        return Some(format!("em segundo plano: {}", fundo.join(", ")));
    }
    None
}

/// Uma rodada: adormece as abas paradas há `PARADA` fora de `vistas` (as abas que o app mostra, pela raiz) e, com a
/// memória apertada, as paradas há pouco, que entram na fila de memória para voltar. `seco`: só diz o que faria.
pub fn adormecer(vistas: &HashSet<(String, u32)>, seco: bool) -> Result<Value, String> {
    let (r, voltam) = rodada(vistas, seco, Modo::Rodada)?;
    crate::fila::voltar(&voltam);
    Ok(r)
}

/// O mesmo para a aba `aba` do workspace `ws` (sem `aba`, as do workspace), parada há pouco ou há muito: o
/// que impede (trabalho, pergunta, caixa digitada) e o que está em `vistas` continuam valendo.
pub fn adormecer_agora(ws: &str, aba: Option<u32>, vistas: &HashSet<(String, u32)>) -> Result<Value, String> {
    rodada(vistas, false, Modo::Agora(ws, aba)).map(|(r, _)| r)
}

/// Lugar na memória para `n` conversas novas (`keep nova`, a fila): no teto de conversas vivas, ou com a memória
/// disponível abaixo do mínimo, as paradas há 10 min saem, como na rodada, poupando as abas que o app mostrou há pouco.
/// Com o resto, as abas que saíram, para a fila acordá-las quando chegar a vez delas.
pub fn abrir_vaga(n: usize) -> Result<(Value, Vec<(String, u32)>), String> {
    rodada(&HashSet::new(), false, Modo::Vaga(n))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modo<'a> {
    /// A rodada do app.
    Rodada,
    /// O pedido da pessoa (o zzz): a aba N de W, ou as livres de W.
    Agora(&'a str, Option<u32>),
    /// Lugar para tantas conversas novas: só a parte da memória da rodada.
    Vaga(usize),
}

/// O que a rodada olha uma vez e cada saída do Claude usa.
struct Contexto<'a> {
    lista: &'a daemon::Retrato,
    amb: &'a Ambiente,
    keepd: &'a Keepd,
    nomes: &'a HashMap<(String, u32), String>,
}

enum Fim {
    Saiu,
    /// `seco`: sairia.
    Seco,
    /// Não pode agora: trabalho, pergunta, caixa digitada, rascunho de feedback, conversa anexada.
    Impede(String),
    Falhou(String),
}

/// Sai do Claude da aba `i`, guardando antes a conversa, a conta e as opções.
fn hibernar(c: &Contexto, i: &IaAba, aba: &daemon::Aba, conversa: &str, seco: bool) -> Fim {
    if keep_ia::processos::argv(i.pid).is_some_and(|a| a.iter().take(4).any(|x| x == "attach")) {
        // O `/exit` só desanexa: a conversa segue no job, na memória.
        return Fim::Impede("conversa em segundo plano anexada".into());
    }
    // Com rascunho de feedback na fila, o `/exit` para na pergunta dele (enviar ou descartar), e a conversa que
    // saísse dali depois ficaria sem a entrada de adormecida: a aba volta só com o shell.
    let com_rascunho = [Some(conversa), i.conversa.as_deref()]
        .into_iter()
        .flatten()
        .any(|x| keep_ia::sessoes::rascunhos_de_feedback(x, Some(i.pid)) > 0);
    let impede = impedimento(aba, i.pid)
        .or_else(|| com_rascunho.then(|| "a conversa tem rascunho de feedback na fila (o /exit pararia na pergunta dele)".to_string()));
    if let Some(motivo) = impede {
        return Fim::Impede(motivo);
    }
    let Some(ia) = crate::retrato::ia_da_aba(i, c.amb).map(|ia| Ia { conversa: conversa.to_string(), ..ia }) else {
        return Fim::Impede("sem a conversa".into());
    };
    if seco {
        return Fim::Seco;
    }
    // Sem o Claude o título some da aba: a que não tem nome dado fica com o da conversa.
    let nome = titulo(aba).filter(|_| !c.nomes.contains_key(&(i.workspace.clone(), i.aba)));
    // Guardada antes de sair: o que cair no meio não perde a conversa da aba.
    registrar(c.keepd, &i.workspace, i.aba, ia.clone(), nome.clone());
    let vinculo = keep_ia::vinculo::vincular(c.lista).get(&(i.workspace.clone(), i.aba)).map(|v| v.shell);
    let sintaxe = keep_ia::trocar::sintaxe_da_aba(aba, vinculo);
    match keep_ia::trocar::sai_do_claude(&i.workspace, i.aba, false, sintaxe) {
        Ok(()) => {
            // Saiu de uma worktree (mantida pelo `sai_do_claude`): a conversa acorda dentro dela.
            let tela = keep_ia::trocar::tela_de(&i.workspace, i.aba).0;
            if let Some(w) = keep_ia::tela::worktree_ao_retomar(&tela).filter(|_| !ia.parametros.iter().any(|p| p == "--worktree")) {
                let mut ia = ia;
                ia.parametros.extend(["--worktree".to_string(), w]);
                registrar(c.keepd, &i.workspace, i.aba, ia, nome.clone());
            }
            if let Some(n) = nome {
                crate::nomear_no_app(c.amb, c.keepd.socket_nasceu, &[(i.workspace.clone(), i.aba, n)], false);
            }
            Fim::Saiu
        }
        Err(r) => {
            mudar(c.keepd, |abas| abas.retain(|a| !(a.workspace == i.workspace && a.aba == i.aba)));
            Fim::Falhou(format!("não saiu: {}", r.detalhe))
        }
    }
}

/// O que a rodada diz de cada aba (uma situação por aba: a última vale).
#[derive(Default)]
struct Relato(Vec<(String, String)>);

impl Relato {
    fn diz(&mut self, aba: &str, situacao: String) {
        match self.0.iter_mut().find(|(a, _)| a == aba) {
            Some(x) => x.1 = situacao,
            None => self.0.push((aba.to_string(), situacao)),
        }
    }

    fn json(&self) -> Vec<Value> {
        self.0.iter().map(|(a, s)| json!({ "aba": a, "situacao": s })).collect()
    }
}

/// Por que a aba geral `mae` de `ws` não hiberna sozinha: as outras abas do workspace com conversa viva (do Claude ou
/// do Codex; a aba só com terminal não conta), fora as que saíram nesta rodada (`saiu`). `None` se todas já hibernam.
fn geral_segura(lista: &daemon::Retrato, ias: &[IaAba], ws: &str, mae: u32, saiu: &HashSet<(String, u32)>) -> Option<String> {
    let mut vivas: Vec<u32> = ias
        .iter()
        .filter(|o| o.workspace == ws && !saiu.contains(&(o.workspace.clone(), o.aba)))
        .map(|o| raiz(lista, ws, o.aba))
        .filter(|r| *r != mae)
        .collect();
    vivas.sort_unstable();
    vivas.dedup();
    match vivas.as_slice() {
        [] => None,
        [n] => Some(format!("geral: a aba {n} ainda tem conversa viva")),
        v => Some(format!("geral: as abas {} ainda têm conversa viva", v.iter().map(u32::to_string).collect::<Vec<_>>().join(", "))),
    }
}

/// A ordem de saída pela memória: a parada há mais tempo primeiro, e a aba geral do workspace por último (é ela que
/// conduz as outras, e o que pede a ela a acorda de novo).
fn ordem_da_memoria(candidatas: &mut [(u64, bool, &IaAba, String, bool)]) {
    candidatas.sort_by_key(|(parada_ha, geral, ..)| (*geral, Reverse(*parada_ha)));
}

/// A rodada, e as abas que saíram pela memória e voltam pela fila.
fn rodada(vistas_do_app: &HashSet<(String, u32)>, seco: bool, modo: Modo) -> Result<(Value, Vec<(String, u32)>), String> {
    let so_do = match modo {
        Modo::Agora(w, a) => Some((w, a)),
        _ => None,
    };
    // O pedido da pessoa e o lugar para uma aba nova esperam a rodada que estiver no meio; a rodada do app desiste
    // da outra.
    let trava = match modo {
        Modo::Rodada => crate::Trava::pegar(".adormecer.lock"),
        Modo::Agora(..) => crate::Trava::esperar(".adormecer.lock", Duration::from_secs(90)),
        Modo::Vaga(_) => crate::Trava::esperar(".adormecer.lock", Duration::from_secs(60)),
    };
    let Some(_trava) = trava else {
        return Ok((json!({ "versao": crate::VERSAO, "motivo": "outra rodada em andamento" }), Vec::new()));
    };
    let lista = daemon::listar()?;
    if !lista.exato {
        return Ok((json!({ "versao": crate::VERSAO, "motivo": "o keepd não diz os processos das abas" }), Vec::new()));
    }
    let amb = Ambiente::do_sistema();
    let keepd = crate::retrato::keepd_da_lista(&lista, &amb);
    let contas = keep_ia::contas::listar(false);
    let ias = keep_ia::abas::das_abas(&lista, &contas);
    let nomes = keep_ia::worktrees::abas::nomes_do_app(&amb, keepd.socket_nasceu);
    let vistas = match modo {
        Modo::Vaga(_) => vistas_recentes(&keepd),
        _ => vistas_do_app.clone(),
    };
    if !seco {
        // A que fechou, ou em que a IA voltou por outro caminho, sai da lista.
        mudar(&keepd, |abas| {
            abas.retain(|a| {
                lista.aba(&a.workspace, a.aba).is_some() && !ias.iter().any(|i| i.workspace == a.workspace && i.aba == a.aba)
            })
        });
        // As abas que o app mostra: quem abre lugar para uma aba nova as poupa.
        if modo == Modo::Rodada {
            marcar(VISTAS, &keepd, &vistas.iter().cloned().collect::<Vec<_>>());
        }
    }
    let c = Contexto { lista: &lista, amb: &amb, keepd: &keepd, nomes: &nomes };
    let acordadas = marcas(ACORDADAS, &keepd);
    let da_fila = marcas(DA_FILA, &keepd);
    let gerais = crate::geral::do_keepd(&keepd);
    let parada = parada().as_millis() as u64;
    let parada_aperto = parada_aperto().as_millis() as u64;
    let agora = crate::agora_ms();
    let mut relato = Relato::default();
    let mut feitas = 0;
    // As abas que saíram (ou, no seco, sairiam) nesta rodada: já não seguram a aba geral.
    let mut saiu: HashSet<(String, u32)> = HashSet::new();
    // As que podem sair se a memória apertar: (parada há, é a aba geral, a IA, a conversa, volta pela fila).
    let mut pela_memoria: Vec<(u64, bool, &IaAba, String, bool)> = Vec::new();
    let mut voltam: Vec<(String, u32)> = Vec::new();
    let e_geral = |i: &IaAba| gerais.iter().any(|(w, a)| *w == i.workspace && *a == raiz(&lista, &i.workspace, i.aba));
    let mut claudes: Vec<&IaAba> = ias.iter().filter(|i| i.agente == "claude").collect();
    // A aba geral por último: as outras do workspace que saem nesta rodada já não a seguram.
    claudes.sort_by_key(|i| e_geral(i));
    for i in claudes {
        if so_do.is_some_and(|(w, a)| w != i.workspace || a.is_some_and(|a| a != raiz(&lista, w, i.aba))) {
            continue;
        }
        let Some(aba) = lista.aba(&i.workspace, i.aba) else { continue };
        let rotulo = format!("{}#{}", i.workspace, i.aba);
        let mae = raiz(&lista, &i.workspace, i.aba);
        if vistas.contains(&(i.workspace.clone(), mae)) {
            relato.diz(&rotulo, "à vista".into());
            continue;
        }
        let Some(conversa) = i.conversa.as_deref().map(keep_ia::sessoes::continuacao) else {
            relato.diz(&rotulo, "sem o id da conversa".into());
            continue;
        };
        // Sem fala nenhuma, o `--resume` ao acordar não acharia a conversa e a aba ficaria só com o shell (a aba geral
        // aberta pelo Keep, 24/11/2026, adormeceu 5 min depois de nascer).
        let Some(ultima) = ultima_fala_ms(&conversa) else {
            relato.diz(&rotulo, "conversa sem nenhuma fala: não há o que retomar".into());
            continue;
        };
        // Acordada há pouco, a parada conta do acordar: a conversa de ontem que a pessoa (ou a aba geral) acabou de
        // chamar não volta a dormir por ser antiga.
        let acordada = acordadas.iter().find(|m| m.workspace == i.workspace && m.aba == i.aba).map(|m| m.em_ms);
        let parada_ha = agora.saturating_sub(ultima.max(acordada.unwrap_or(0)));
        if so_do.is_none() {
            // Acordada para uma mensagem que ainda não chegou (a aba geral acorda e manda em seguida; sem isto, a
            // rodada das 12 h a fazia dormir de novo antes de o pedido entrar): fica no Claude até ela chegar.
            if let Some(em) = acordada.filter(|em| ultima < *em && agora.saturating_sub(*em) < ESPERA_MENSAGEM_MS) {
                relato.diz(&rotulo, format!("acordada há {} min, à espera da mensagem que a acordou", agora.saturating_sub(em) / 60_000));
                continue;
            }
            if matches!(modo, Modo::Vaga(_)) || parada_ha < parada {
                if parada_ha >= parada_aperto {
                    // Uma volta só: a que a fila já acordou e ninguém falou com ela desde então terminou.
                    let volta = !da_fila.iter().any(|m| m.workspace == i.workspace && m.aba == i.aba && ultima < m.em_ms);
                    pela_memoria.push((parada_ha, e_geral(i), i, conversa, volta));
                }
                relato.diz(&rotulo, format!("parada há {} min", parada_ha / 60_000));
                continue;
            }
        }
        let horas = parada_ha / 3_600_000;
        if so_do.is_none() && e_geral(i) {
            if let Some(porque) = geral_segura(&lista, &ias, &i.workspace, mae, &saiu) {
                relato.diz(&rotulo, format!("{porque} (parada há {horas} h)"));
                continue;
            }
        }
        match hibernar(&c, i, aba, &conversa, seco) {
            Fim::Saiu => {
                feitas += 1;
                saiu.insert((i.workspace.clone(), i.aba));
                relato.diz(&rotulo, format!("adormecida (parada há {horas} h)"));
            }
            Fim::Seco => {
                saiu.insert((i.workspace.clone(), i.aba));
                relato.diz(&rotulo, format!("adormeceria (parada há {horas} h)"));
            }
            // Pedida pela pessoa, o motivo vai sozinho: "parada há 0 h" não explica nada.
            Fim::Impede(motivo) if so_do.is_some() => relato.diz(&rotulo, motivo),
            Fim::Impede(motivo) => relato.diz(&rotulo, format!("parada há {horas} h, mas {motivo}")),
            Fim::Falhou(motivo) => relato.diz(&rotulo, motivo),
        }
    }
    // A memória apertada, ou mais conversas vivas que o teto da RAM: as paradas há pouco saem, uma por vez, medindo de
    // novo entre elas, até aliviar.
    let mut memoria_json = Value::Null;
    if so_do.is_none() {
        let mais = match modo {
            Modo::Vaga(n) => n,
            _ => 0,
        };
        let mut s = memoria::Situacao { leitura: memoria::ler(), vivas: ias.len().saturating_sub(feitas) };
        let antes = s.falta(mais);
        if let Some(porque) = &antes {
            ordem_da_memoria(&mut pela_memoria);
            for (parada_ha, geral, i, conversa, volta) in &pela_memoria {
                // Para uma aba nova, só enquanto hibernar abre o lugar: a pressão sozinha não baixa na hora.
                if matches!(modo, Modo::Vaga(_)) && !s.hibernar_resolve(mais) {
                    break;
                }
                let Some(aba) = lista.aba(&i.workspace, i.aba) else { continue };
                let rotulo = format!("{}#{}", i.workspace, i.aba);
                let min = parada_ha / 60_000;
                if *geral {
                    if let Some(porque) = geral_segura(&lista, &ias, &i.workspace, raiz(&lista, &i.workspace, i.aba), &saiu) {
                        relato.diz(&rotulo, format!("{porque} (parada há {min} min)"));
                        continue;
                    }
                }
                match hibernar(&c, i, aba, conversa, seco) {
                    Fim::Saiu => {
                        feitas += 1;
                        saiu.insert((i.workspace.clone(), i.aba));
                        s.vivas = s.vivas.saturating_sub(1);
                        let fila = if *volta {
                            voltam.push((i.workspace.clone(), i.aba));
                            "; volta pela fila de memória"
                        } else {
                            ""
                        };
                        relato.diz(&rotulo, format!("adormecida pela memória ({porque}; parada há {min} min{fila})"));
                        // O que o Claude que saiu ocupava volta ao sistema em instantes.
                        std::thread::sleep(Duration::from_secs(1));
                        s.leitura = memoria::ler();
                        if s.falta(mais).is_none() {
                            break;
                        }
                    }
                    Fim::Seco => {
                        saiu.insert((i.workspace.clone(), i.aba));
                        relato.diz(&rotulo, format!("adormeceria pela memória ({porque}; parada há {min} min)"));
                    }
                    Fim::Impede(motivo) => relato.diz(&rotulo, format!("parada há {min} min, mas {motivo}")),
                    Fim::Falhou(motivo) => relato.diz(&rotulo, motivo),
                }
            }
        }
        memoria_json = json!({ "aperto": antes, "falta": s.falta(mais), "resumo": s.resumo() });
    }
    // Uma aba pedida que não roda o Claude: a pessoa apertou o botão e merece saber por que nada saiu.
    if let Some((w, Some(a))) = so_do {
        if relato.0.is_empty() {
            let ja = do_keepd(&keepd).iter().any(|d| d.workspace == w && raiz(&lista, w, d.aba) == a);
            let porque = if ja { "a conversa já está hibernada" } else { "o Keep não achou o Claude nesta aba" };
            relato.diz(&format!("{w}#{a}"), porque.into());
        }
    }
    Ok((json!({ "versao": crate::VERSAO, "adormecidas": feitas, "abas": relato.json(), "memoria": memoria_json }), voltam))
}

/// A conversa da aba `ws`#`tab` (e dos painéis dela) volta, se ela adormeceu.
pub fn acordar(ws: &str, tab: u32) -> Result<Value, String> {
    acordar_de(ws, tab, false)
}

/// O mesmo, pela fila de memória: a aba não conta como mostrada, e fica marcada como acordada pela fila.
pub(crate) fn acordar_pela_fila(ws: &str, tab: u32) -> Result<Value, String> {
    acordar_de(ws, tab, true)
}

fn acordar_de(ws: &str, tab: u32, pela_fila: bool) -> Result<Value, String> {
    let lista = daemon::listar()?;
    let amb = Ambiente::do_sistema();
    let keepd = crate::retrato::keepd_da_lista(&lista, &amb);
    let alvo = raiz(&lista, ws, tab);
    // O app pergunta ao mostrar cada aba: ela conta como à vista para quem abre lugar na memória.
    if !pela_fila {
        marcar(VISTAS, &keepd, &[(ws.to_string(), alvo)]);
    }
    let e_alvo = |a: &Adormecida| a.workspace == ws && raiz(&lista, ws, a.aba) == alvo;
    if !do_keepd(&keepd).iter().any(|a| e_alvo(a)) {
        return Ok(json!({ "versao": crate::VERSAO, "acordadas": 0 }));
    }
    // Uma de cada vez: duas chamadas para a mesma aba não sobem a conversa duas vezes.
    let pasta = crate::pasta();
    let trava = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(pasta.join(".acordar.lock"));
    let _trava = trava.ok().filter(|f| f.lock().is_ok());
    let alvos: Vec<Adormecida> = mudar(&keepd, |abas| {
        let (sim, nao): (Vec<_>, Vec<_>) = abas.drain(..).partition(|a| e_alvo(a));
        *abas = nao;
        sim
    });
    let contas = keep_ia::contas::listar(false);
    let mut feitas = 0;
    let mut acordou = Vec::new();
    let mut falhas = Vec::new();
    for a in &alvos {
        let rotulo = format!("{}#{}", a.workspace, a.aba);
        let viva = daemon::listar().ok().and_then(|l| l.aba(&a.workspace, a.aba).cloned());
        if let Some(s) = keep_ia::sessoes::sessao_viva(&a.ia.conversa) {
            if viva.as_ref().is_some_and(|v| v.info.pid == s.pid) {
                // O Claude ainda está saindo desta mesma aba (o zzz e o mostrar a aba quase juntos): ela segue
                // adormecida, e a conversa, sem entrada, não ficaria com ninguém.
                registrar(&keepd, &a.workspace, a.aba, a.ia.clone(), a.nome_posto.clone());
                falhas.push(format!("{rotulo}: a conversa ainda está saindo desta aba"));
            } else {
                falhas.push(format!("{rotulo}: a conversa já roda em outro lugar"));
            }
            continue;
        }
        let Some(viva) = viva else { continue };
        if !matches!(tela::programa(&viva.info.command), Programa::Shell(_)) {
            falhas.push(format!("{rotulo}: a aba roda {}", viva.info.command));
            continue;
        }
        match crate::restaurar::retoma(&a.workspace, a.aba, &a.ia, &contas, "acordar da aba hibernada (keep restauro)") {
            Ok(()) => {
                feitas += 1;
                acordou.push((a.workspace.clone(), a.aba));
                // O Claude de volta diz o título de novo.
                if let Some(n) = &a.nome_posto {
                    crate::tirar_nome_no_app(&amb, keepd.socket_nasceu, &a.workspace, a.aba, n);
                }
            }
            Err(e) => {
                // Segue adormecida: a próxima vez que a aba for mostrada tenta de novo.
                registrar(&keepd, &a.workspace, a.aba, a.ia.clone(), a.nome_posto.clone());
                falhas.push(format!("{rotulo}: {e}"));
            }
        }
    }
    // A que acordou não volta a dormir antes da mensagem que a acordou, e a parada dela conta daqui.
    marcar(ACORDADAS, &keepd, &acordou);
    if pela_fila {
        marcar(DA_FILA, &keepd, &acordou);
    }
    Ok(json!({ "versao": crate::VERSAO, "acordadas": feitas, "falhas": falhas }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_vista_separa_o_numero_do_fim() {
        assert_eq!(vista("Vendas/8"), Some(("Vendas".into(), 8)));
        assert_eq!(vista("a/b/12"), Some(("a/b".into(), 12)));
        assert_eq!(vista("sem-numero"), None);
        assert_eq!(vista("W/x"), None);
    }

    #[test]
    fn pela_memoria_sai_primeiro_a_parada_ha_mais_tempo_e_a_geral_por_ultimo() {
        let ia = |aba: u32| IaAba {
            workspace: "W".into(),
            aba,
            agente: "claude".into(),
            conta: String::new(),
            atual: None,
            vinculo: "exato".into(),
            pid: aba,
            conversa: None,
            provada: None,
        };
        let (a, b, g, d) = (ia(1), ia(2), ia(3), ia(4));
        let mut c = vec![
            (20, false, &a, String::new(), true),
            (90, true, &g, String::new(), true),
            (50, false, &b, String::new(), true),
            (70, false, &d, String::new(), true),
        ];
        ordem_da_memoria(&mut c);
        assert_eq!(c.iter().map(|x| x.2.aba).collect::<Vec<_>>(), vec![4, 2, 1, 3]);
    }
}
