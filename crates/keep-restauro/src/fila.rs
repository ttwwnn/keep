//! A fila das abas novas que a memória segurou (pedido de 25/11/2026).
//!
//! `keep nova` (a aba geral abrindo trabalho) e `keep geral iniciar` (o workspace novo) não sobem uma conversa a mais
//! com a memória apertada ou no teto de conversas vivas (`memoria`): primeiro hibernam as paradas
//! (`adormecer::abrir_vaga`) e, se ainda faltar, o pedido entra aqui, e `keep nova` responde "na fila: memória cheia
//! (X livres, N conversas vivas)". A fila anda pela ordem a cada 15 s (`keep continuar`, que os dois apps rodam),
//! enquanto couber mais uma; a aba geral do workspace fica sabendo de cada pedido que abriu, ou que saiu sem abrir,
//! pelas novidades. Um pedido que não abriu em 12 h sai, com aviso.
//!
//! ```text
//! keep restauro fila    → os pedidos que esperam
//! ```

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use keep_ia::daemon;
use keep_ia::geral::Evento;

use crate::{Keepd, adormecer, memoria, nova};

const ARQUIVO: &str = "fila.json";
const REGISTRO: &str = "fila.log";
/// Um pedido que não abriu neste tempo sai da fila.
const VALIDADE_MS: u64 = 12 * 3600 * 1000;
/// Entre duas rodadas que hibernam para a fila andar (medir é barato; hibernar lê a tela de cada aba).
const ENTRE_ALIVIOS_MS: u64 = 60_000;

/// O que espera memória.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "camelCase")]
pub enum Item {
    /// Uma conversa nova numa aba nova (`keep nova`).
    Nova { pedido: nova::Pedido },
    /// O Claude na primeira aba de um workspace novo, que vira a aba geral dele (`keep geral iniciar`); vale só no
    /// keepd em que a aba existe.
    Iniciar { workspace: String, aba: u32, keepd: Keepd },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NaFila {
    pub desde_ms: u64,
    pub item: Item,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Arquivo {
    #[serde(default)]
    itens: Vec<NaFila>,
    /// Quando a fila hibernou paradas por último, para andar.
    #[serde(default)]
    aliviou_ms: u64,
}

fn ler() -> Arquivo {
    crate::ler(&crate::pasta().join(ARQUIVO)).unwrap_or_default()
}

fn gravar(a: &Arquivo) -> Result<(), String> {
    crate::gravar(&crate::pasta().join(ARQUIVO), a).map_err(|e| format!("não gravei a fila: {e}"))
}

fn registrar(texto: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(crate::pasta());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::pasta().join(REGISTRO)) {
        let _ = writeln!(f, "{}  {texto}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

fn hora(ms: u64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

/// O workspace e o rótulo de um pedido, para as novidades e o registro.
fn de_quem(item: &Item) -> (String, String) {
    match item {
        Item::Nova { pedido } => {
            (pedido.workspace.clone().unwrap_or_default(), pedido.nome.clone().unwrap_or_else(|| "aba nova".into()))
        }
        Item::Iniciar { workspace, aba, .. } => (workspace.clone(), format!("o Claude da aba {aba}, a geral do workspace novo")),
    }
}

/// Uma novidade para a aba geral do workspace do pedido.
fn conta_a_geral(item: &Item, aba: u32, texto: String) {
    let (workspace, nome) = de_quem(item);
    if workspace.is_empty() {
        return;
    }
    keep_ia::geral::anotar(&[Evento { quando_ms: crate::agora_ms(), workspace, aba, nome, tipo: "fila".into(), texto }]);
}

/// A memória e as conversas vivas de agora (sem o keepd, nenhuma conversa).
fn situacao() -> memoria::Situacao {
    daemon::listar().map(|l| memoria::Situacao::de(&l)).unwrap_or(memoria::Situacao { leitura: memoria::ler(), vivas: 0 })
}

/// Cabem mais `mais` conversas vivas? Sem caber e com `aliviar`, as paradas hibernam antes de dizer que não.
/// `Err`: por que não, com a memória e as conversas vivas.
fn cabe(mais: usize, aliviar: bool) -> Result<(), (String, memoria::Situacao)> {
    let s = situacao();
    let Some(porque) = s.falta(mais) else { return Ok(()) };
    if !aliviar {
        return Err((porque, s));
    }
    let _ = adormecer::abrir_vaga(mais);
    let s = situacao();
    match s.falta(mais) {
        None => Ok(()),
        Some(porque) => Err((porque, s)),
    }
}

/// Abre o que o item pede: o workspace e a aba.
fn abrir(item: &Item) -> Result<(String, u32), String> {
    match item {
        Item::Nova { pedido } => nova::abrir(pedido).map(|(ws, aba, _)| (ws, aba)),
        Item::Iniciar { workspace, aba, keepd } => {
            let lista = daemon::listar()?;
            let agora = crate::retrato::keepd_da_lista(&lista, &keep_ia::worktrees::ambiente::Ambiente::do_sistema());
            if !agora.mesmo(keepd) {
                return Err("o Keep reiniciou depois do pedido".into());
            }
            crate::geral::iniciar_ja(workspace, *aba).map(|_| (workspace.clone(), *aba))
        }
    }
}

/// O que `keep nova` fez.
pub enum Desfecho {
    /// O workspace, a aba e a linha digitada nela.
    Aberta(String, u32, String),
    /// Por que não abriu (a memória e as conversas vivas) e a posição na fila.
    NaFila { porque: String, resumo: String, posicao: usize },
}

/// `keep nova` com a memória medida: abre, ou põe na fila. Quem já espera vai antes: a fila anda primeiro, e o pedido
/// novo não passa à frente dela.
pub fn nova(p: &nova::Pedido) -> Result<Desfecho, String> {
    let _trava = crate::Trava::esperar(".fila.lock", Duration::from_secs(60)).ok_or("a fila de memória está ocupada; tente de novo")?;
    let mut a = ler();
    if !a.itens.is_empty() {
        andar_com(&mut a);
    }
    let (porque, s) = if a.itens.is_empty() {
        match cabe(1, true) {
            Ok(()) => return nova::abrir(p).map(|(ws, aba, texto)| Desfecho::Aberta(ws, aba, texto)),
            Err(e) => e,
        }
    } else {
        (format!("{} pedido(s) antes deste na fila", a.itens.len()), situacao())
    };
    // O workspace que não existe é recusado já, como na hora de abrir: quem pediu não espera memória por nada.
    if let Some(ws) = &p.workspace {
        let lista = daemon::listar().map_err(|e| format!("O Keep não está aberto ({e}). Abra o Keep; não use o Terminal."))?;
        if !lista.abas.iter().any(|a| !a.info.finished && &a.ws == ws) {
            return Err(format!("workspace {ws:?} não existe no Keep"));
        }
    }
    // O texto do pedido vai já para o arquivo que a aba vai ler: a fila guarda só o caminho.
    let pedido = nova::para_a_fila(p)?;
    a.itens.push(NaFila { desde_ms: crate::agora_ms(), item: Item::Nova { pedido } });
    gravar(&a)?;
    let (ws, nome) = de_quem(&a.itens[a.itens.len() - 1].item);
    registrar(&format!("{ws}: «{nome}» na fila ({porque}; {})", s.resumo()));
    Ok(Desfecho::NaFila { porque, resumo: s.resumo(), posicao: a.itens.len() })
}

/// `keep geral iniciar` com a memória medida: o Claude sobe na aba já, ou quando houver memória.
pub fn iniciar(ws: &str, aba: u32) -> Result<Value, String> {
    let trava = crate::Trava::esperar(".fila.lock", Duration::from_secs(60)).ok_or("a fila de memória está ocupada; tente de novo")?;
    let mut a = ler();
    if !a.itens.is_empty() {
        andar_com(&mut a);
    }
    let falta = if a.itens.is_empty() {
        cabe(1, true).err()
    } else {
        Some((format!("{} pedido(s) antes deste na fila", a.itens.len()), situacao()))
    };
    let Some((porque, s)) = falta else {
        drop(trava);
        return crate::geral::iniciar_ja(ws, aba);
    };
    let lista = daemon::listar()?;
    let keepd = crate::retrato::keepd_da_lista(&lista, &keep_ia::worktrees::ambiente::Ambiente::do_sistema());
    a.itens.push(NaFila { desde_ms: crate::agora_ms(), item: Item::Iniciar { workspace: ws.into(), aba, keepd } });
    gravar(&a)?;
    registrar(&format!("{ws}: o Claude da aba {aba} (workspace novo) na fila ({porque}; {})", s.resumo()));
    Ok(json!({
        "versao": crate::VERSAO,
        "ok": true,
        "fila": true,
        "situacao": format!("na fila: memória cheia ({}); o Claude sobe na aba quando houver memória", s.resumo()),
    }))
}

/// A fila anda (`keep continuar`, a cada 15 s): abre pela ordem enquanto couber mais uma conversa.
pub fn andar() -> Value {
    let Some(_trava) = crate::Trava::pegar(".fila.lock") else {
        return json!({ "motivo": "outra rodada da fila em andamento" });
    };
    let mut a = ler();
    if a.itens.is_empty() {
        return json!({ "abertas": [], "esperam": 0 });
    }
    andar_com(&mut a)
}

/// A fila anda (com a trava na mão) e é gravada.
fn andar_com(a: &mut Arquivo) -> Value {
    let antes = a.clone();
    let agora = crate::agora_ms();
    let mut abertas = Vec::new();
    let mut saiu = Vec::new();
    a.itens.retain(|f| {
        let vence = agora.saturating_sub(f.desde_ms) >= VALIDADE_MS;
        if vence {
            let (ws, nome) = de_quem(&f.item);
            registrar(&format!("{ws}: «{nome}» saiu da fila: 12 h sem memória"));
            conta_a_geral(&f.item, 0, format!("o pedido das {} («{nome}») saiu da fila de memória: 12 h sem memória para abrir", hora(f.desde_ms)));
            saiu.push(nome);
        }
        !vence
    });
    let mut espera = None;
    while let Some(primeiro) = a.itens.first().cloned() {
        // Medir, a cada vez; hibernar paradas para a fila andar, no máximo uma vez por minuto.
        let falta = match cabe(1, false) {
            Err(_) if agora.saturating_sub(a.aliviou_ms) >= ENTRE_ALIVIOS_MS => {
                a.aliviou_ms = agora;
                cabe(1, true).err()
            }
            outro => outro.err(),
        };
        if let Some((porque, s)) = falta {
            espera = Some(format!("{porque}; {}", s.resumo()));
            break;
        }
        a.itens.remove(0);
        let (ws, nome) = de_quem(&primeiro.item);
        match abrir(&primeiro.item) {
            Ok((ws, aba)) => {
                registrar(&format!("{ws}: «{nome}» abriu da fila (aba {aba})"));
                conta_a_geral(&primeiro.item, aba, format!("abriu da fila de memória o pedido das {} («{nome}»)", hora(primeiro.desde_ms)));
                abertas.push(json!({ "workspace": ws, "aba": aba, "nome": nome }));
            }
            Err(e) => {
                registrar(&format!("{ws}: «{nome}» saiu da fila sem abrir: {e}"));
                conta_a_geral(&primeiro.item, 0, format!("o pedido das {} («{nome}») saiu da fila de memória sem abrir: {e}", hora(primeiro.desde_ms)));
                saiu.push(nome);
            }
        }
        // Gravada a cada passo: o que abriu não abre de novo se a rodada cair no meio.
        let _ = gravar(a);
    }
    if *a != antes {
        let _ = gravar(a);
    }
    json!({ "abertas": abertas, "sairam": saiu, "esperam": a.itens.len(), "espera": espera })
}

/// `keep restauro fila`: os pedidos que esperam memória, na ordem.
pub fn listar() -> Result<Value, String> {
    let a = ler();
    let itens: Vec<Value> = a
        .itens
        .iter()
        .map(|f| {
            let (workspace, nome) = de_quem(&f.item);
            json!({ "desde": hora(f.desde_ms), "workspace": workspace, "nome": nome })
        })
        .collect();
    Ok(json!({ "versao": crate::VERSAO, "fila": itens }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_pedido_vai_e_volta_da_fila_igual() {
        let p = nova::Pedido {
            workspace: Some("W".into()),
            pasta: "/p".into(),
            conteudo: nova::Conteudo::Passagem("/h/abas/1.md".into()),
            nome: Some("Compras".into()),
            sem_enter: false,
        };
        let a = Arquivo {
            itens: vec![
                NaFila { desde_ms: 5, item: Item::Nova { pedido: p.clone() } },
                NaFila { desde_ms: 6, item: Item::Iniciar { workspace: "N".into(), aba: 1, keepd: Keepd { pid: 9, inicio_ms: 3, socket_nasceu: None } } },
            ],
            aliviou_ms: 0,
        };
        let texto = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<Arquivo>(&texto).unwrap(), a);
        assert_eq!(de_quem(&a.itens[0].item), ("W".into(), "Compras".into()));
        assert_eq!(de_quem(&a.itens[1].item).0, "N");
    }
}
