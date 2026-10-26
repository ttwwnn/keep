//! A conversa parada há muito tempo sai da memória, e a aba fica.
//!
//! Cada Claude Code aberto pesa de 200 a 400 MB, e num Mac de 8 GB as abas
//! esquecidas há um dia somam gigabytes. Pedido de 21/10/2026: a conversa
//! parada há mais de 12 h é fechada, a aba segue no Keep e a conversa volta
//! quando a pessoa a chama.
//!
//! - **Adormecer** (`keep restauro adormecer`, o app a cada 5 min): a aba do
//!   Claude fora de vista, livre (caixa vazia, nada rodando, nenhuma pergunta,
//!   nada em segundo plano) e sem fala na conversa há `PARADA`, sai do Claude
//!   pelo `/exit`. A conversa, a conta e as opções
//!   ficam em `adormecidas.json` (por keepd) antes da saída.
//! - **Agora** (`keep restauro adormecer --agora W [--aba N]`, o zzz da aba na
//!   barra lateral): o mesmo, para a aba N (ou as livres do workspace W), sem
//!   esperar as 12 h.
//! - **Adormecidas** (`keep restauro adormecidas`, o app do Windows): quais abas
//!   do keepd de agora estão adormecidas, para a barra lateral riscar o título.
//! - **Acordar** (`keep restauro acordar`, o app ao mostrar a aba): a
//!   conversa volta na mesma aba, como a restauração a retoma.
//! - O retrato leva a aba adormecida com a conversa dela: depois de um
//!   reinício ela volta adormecida e acorda do mesmo jeito.

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use keep_ia::daemon;
use keep_ia::tela::{self, Programa};
use keep_ia::worktrees::ambiente::Ambiente;

use crate::{Ia, Keepd};

pub const ARQUIVO: &str = "adormecidas.json";

/// Quanto tempo parada antes de sair da memória.
const PARADA: Duration = Duration::from_secs(12 * 3600);

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
fn ultima_fala_ms(conversa: &str) -> u64 {
    let Some(caminho) = crate::continuar::transcrito(conversa) else { return 0 };
    let mut quando = 0;
    crate::continuar::falas_de_tras(&caminho, |d| {
        quando = crate::continuar::carimbo_ms(d);
        false
    });
    if quando == 0 {
        quando = std::fs::metadata(&caminho)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
    }
    quando
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

/// Uma rodada: adormece as abas paradas há `PARADA` fora de `vistas` (as abas que o app mostra, pela raiz).
/// `seco`: só diz o que faria.
pub fn adormecer(vistas: &HashSet<(String, u32)>, seco: bool) -> Result<Value, String> {
    rodada(vistas, seco, None)
}

/// O mesmo para a aba `aba` do workspace `ws` (sem `aba`, as do workspace), parada há pouco ou há muito: o
/// que impede (trabalho, pergunta, caixa digitada) e o que está em `vistas` continuam valendo.
pub fn adormecer_agora(ws: &str, aba: Option<u32>, vistas: &HashSet<(String, u32)>) -> Result<Value, String> {
    rodada(vistas, false, Some((ws, aba)))
}

fn rodada(vistas: &HashSet<(String, u32)>, seco: bool, so_do: Option<(&str, Option<u32>)>) -> Result<Value, String> {
    // O pedido da pessoa espera a rodada que estiver no meio; a rodada de 5 min desiste da outra.
    let trava = match so_do {
        Some(_) => crate::Trava::esperar(".adormecer.lock", Duration::from_secs(90)),
        None => crate::Trava::pegar(".adormecer.lock"),
    };
    let Some(_trava) = trava else {
        return Ok(json!({ "versao": crate::VERSAO, "motivo": "outra rodada em andamento" }));
    };
    let lista = daemon::listar()?;
    if !lista.exato {
        return Ok(json!({ "versao": crate::VERSAO, "motivo": "o keepd não diz os processos das abas" }));
    }
    let amb = Ambiente::do_sistema();
    let keepd = crate::retrato::keepd_da_lista(&lista, &amb);
    let contas = keep_ia::contas::listar(false);
    let ias = keep_ia::abas::das_abas(&lista, &contas);
    let nomes = keep_ia::worktrees::abas::nomes_do_app(&amb, keepd.socket_nasceu);
    // A que fechou, ou em que a IA voltou por outro caminho, sai da lista.
    if !seco {
        mudar(&keepd, |abas| {
            abas.retain(|a| {
                lista.aba(&a.workspace, a.aba).is_some() && !ias.iter().any(|i| i.workspace == a.workspace && i.aba == a.aba)
            })
        });
    }
    let parada = parada().as_millis() as u64;
    let agora = crate::agora_ms();
    let mut relato = Vec::new();
    let mut feitas = 0;
    for i in ias.iter().filter(|i| i.agente == "claude") {
        if so_do.is_some_and(|(w, a)| w != i.workspace || a.is_some_and(|a| a != raiz(&lista, w, i.aba))) {
            continue;
        }
        let Some(aba) = lista.aba(&i.workspace, i.aba) else { continue };
        let rotulo = format!("{}#{}", i.workspace, i.aba);
        let mut diz = |s: String| relato.push(json!({ "aba": rotulo, "situacao": s }));
        if vistas.contains(&(i.workspace.clone(), raiz(&lista, &i.workspace, i.aba))) {
            diz("à vista".into());
            continue;
        }
        let Some(conversa) = i.conversa.as_deref().map(keep_ia::sessoes::continuacao) else {
            diz("sem o id da conversa".into());
            continue;
        };
        let parada_ha = agora.saturating_sub(ultima_fala_ms(&conversa));
        if so_do.is_none() && parada_ha < parada {
            diz(format!("parada há {} min", parada_ha / 60_000));
            continue;
        }
        if keep_ia::processos::argv(i.pid).is_some_and(|a| a.iter().take(4).any(|x| x == "attach")) {
            // O `/exit` só desanexa: a conversa segue no job, na memória.
            diz("conversa em segundo plano anexada".into());
            continue;
        }
        if let Some(motivo) = impedimento(aba, i.pid) {
            // Pedida pela pessoa, o motivo vai sozinho: "parada há 0 h" não explica nada.
            diz(if so_do.is_some() { motivo } else { format!("parada há {} h, mas {motivo}", parada_ha / 3_600_000) });
            continue;
        }
        let Some(ia) = crate::retrato::ia_da_aba(i, &amb).map(|ia| Ia { conversa: conversa.clone(), ..ia }) else {
            diz("sem a conversa".into());
            continue;
        };
        if seco {
            diz(format!("adormeceria (parada há {} h)", parada_ha / 3_600_000));
            continue;
        }
        // Sem o Claude o título some da aba: a que não tem nome dado fica com o da conversa.
        let nome = titulo(aba).filter(|_| !nomes.contains_key(&(i.workspace.clone(), i.aba)));
        // Guardada antes de sair: o que cair no meio não perde a conversa da aba.
        registrar(&keepd, &i.workspace, i.aba, ia, nome.clone());
        let vinculo = keep_ia::vinculo::vincular(&lista).get(&(i.workspace.clone(), i.aba)).map(|v| v.shell);
        let sintaxe = keep_ia::trocar::sintaxe_da_aba(aba, vinculo);
        match keep_ia::trocar::sai_do_claude(&i.workspace, i.aba, false, sintaxe) {
            Ok(()) => {
                feitas += 1;
                if let Some(n) = nome {
                    crate::nomear_no_app(&amb, keepd.socket_nasceu, &[(i.workspace.clone(), i.aba, n)], false);
                }
                diz(format!("adormecida (parada há {} h)", parada_ha / 3_600_000));
            }
            Err(r) => {
                mudar(&keepd, |abas| abas.retain(|a| !(a.workspace == i.workspace && a.aba == i.aba)));
                diz(format!("não saiu: {}", r.detalhe));
            }
        }
    }
    // Uma aba pedida que não roda o Claude: a pessoa apertou o botão e merece saber por que nada saiu.
    if let Some((w, Some(a))) = so_do {
        if relato.is_empty() {
            let ja = do_keepd(&keepd).iter().any(|d| d.workspace == w && raiz(&lista, w, d.aba) == a);
            let porque = if ja { "a conversa já está hibernada" } else { "o Keep não achou o Claude nesta aba" };
            relato.push(json!({ "aba": format!("{w}#{a}"), "situacao": porque }));
        }
    }
    Ok(json!({ "versao": crate::VERSAO, "adormecidas": feitas, "abas": relato }))
}

/// A conversa da aba `ws`#`tab` (e dos painéis dela) volta, se ela adormeceu.
pub fn acordar(ws: &str, tab: u32) -> Result<Value, String> {
    let lista = daemon::listar()?;
    let amb = Ambiente::do_sistema();
    let keepd = crate::retrato::keepd_da_lista(&lista, &amb);
    let alvo = raiz(&lista, ws, tab);
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
        match crate::restaurar::retoma(&a.workspace, a.aba, &a.ia, &contas) {
            Ok(()) => {
                feitas += 1;
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
    Ok(json!({ "versao": crate::VERSAO, "acordadas": feitas, "falhas": falhas }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_vista_separa_o_numero_do_fim() {
        assert_eq!(vista("Clinica/8"), Some(("Clinica".into(), 8)));
        assert_eq!(vista("a/b/12"), Some(("a/b".into(), 12)));
        assert_eq!(vista("sem-numero"), None);
        assert_eq!(vista("W/x"), None);
    }
}
