//! O retrato: as abas do keepd de agora, gravadas para o keepd seguinte.
//!
//! `retrato.json` é o do keepd atual. Quando o keepd muda (o computador
//! reiniciou), o do anterior vai para `anterior.json` — é dele que a
//! restauração parte —, salvo se a restauração do anterior ainda não acabou:
//! aí o `anterior.json` que ela usava fica (um segundo reinício no meio não
//! troca as abas que faltavam pelo pouco que já tinha voltado).

use std::collections::HashMap;

use keep_ia::daemon::Retrato as Lista;
use keep_ia::worktrees::ambiente::Ambiente;

use crate::{AbaGuardada, Ia, Keepd, Retrato};

pub const ARQUIVO: &str = "retrato.json";
pub const ANTERIOR: &str = "anterior.json";

/// O workspace do terminal rápido não volta: é do app, e nasce sozinho.
pub const RAPIDO: &str = "quick";

/// O keepd de uma lista, com o nascimento do socket.
pub fn keepd_da_lista(l: &Lista, amb: &Ambiente) -> Keepd {
    Keepd {
        pid: l.daemon.pid,
        inicio_ms: l.daemon.started_ms,
        socket_nasceu: keep_ia::worktrees::abas::nascimento_do_socket(amb),
    }
}

/// As abas do keepd de agora.
pub fn fotografar() -> Result<Retrato, String> {
    let amb = Ambiente::do_sistema();
    let lista = keep_ia::daemon::listar()?;
    let keepd = keepd_da_lista(&lista, &amb);
    let contas = keep_ia::contas::listar(false);
    let ias: HashMap<(String, u32), keep_ia::abas::IaAba> = if lista.exato {
        keep_ia::abas::das_abas(&lista, &contas).into_iter().map(|i| ((i.workspace.clone(), i.aba), i)).collect()
    } else {
        HashMap::new()
    };
    let nomes = keep_ia::worktrees::abas::nomes_do_app(&amb, keepd.socket_nasceu);
    let adormecidas = crate::adormecer::do_keepd(&keepd);
    let mut workspaces: Vec<String> = Vec::new();
    let mut abas = Vec::new();
    for a in &lista.abas {
        if a.info.finished || a.ws == RAPIDO {
            continue;
        }
        if !workspaces.contains(&a.ws) {
            workspaces.push(a.ws.clone());
        }
        let ia = ias.get(&(a.ws.clone(), a.info.id)).and_then(|i| ia_da_aba(i, &amb));
        // A aba sem IA cuja conversa adormeceu leva a conversa: depois de um reinício ela volta adormecida.
        let dormindo = if ia.is_none() && lista.exato {
            adormecidas.iter().find(|d| d.workspace == a.ws && d.aba == a.info.id).map(|d| d.ia.clone())
        } else {
            None
        };
        let adormecida = dormindo.is_some();
        let ia = ia.or(dormindo);
        abas.push(AbaGuardada {
            workspace: a.ws.clone(),
            aba: a.info.id,
            cwd: a.info.cwd.clone(),
            split_of: a.info.split_of,
            split_dir: a.info.split_dir,
            cols: a.info.cols,
            rows: a.info.rows,
            nome: nomes.get(&(a.ws.clone(), a.info.id)).cloned(),
            ia,
            adormecida,
        });
    }
    Ok(Retrato { versao: crate::VERSAO, gravado_ms: crate::agora_ms(), keepd, workspaces, abas })
}

/// A conversa, a conta e os parâmetros da IA de uma aba. Sem o id da conversa
/// (o Codex aberto sem `resume` só o tem nos travamentos da thread), nada.
pub(crate) fn ia_da_aba(i: &keep_ia::abas::IaAba, amb: &Ambiente) -> Option<Ia> {
    let conversa = i.conversa.clone().or_else(|| {
        (i.agente == "codex")
            .then(|| keep_ia::worktrees::abas::threads_do_codex(amb, i.pid).into_iter().next())
            .flatten()
    })?;
    let argv = keep_ia::processos::argv(i.pid).unwrap_or_default();
    Some(Ia {
        agente: i.agente.clone(),
        conversa,
        conta: i.conta.clone(),
        parametros: keep_ia::linha::parametros(&i.agente, &argv),
    })
}

/// O que fazer com o retrato que já está gravado, diante do novo.
#[derive(Debug, PartialEq, Eq)]
pub enum Gravacao {
    /// Grava por cima (o mesmo keepd, ou não havia nada).
    Grava,
    /// O keepd mudou: o gravado vira o `anterior.json` e o novo entra.
    GiraEGrava,
    /// O keepd mudou, mas a restauração do anterior não acabou: o
    /// `anterior.json` fica, e o novo entra no lugar do gravado.
    GravaSemGirar,
    /// Uma lista vazia não apaga um retrato com abas do mesmo keepd (uma
    /// leitura que falhou não pode levar o que a restauração vai precisar).
    Mantem,
}

/// A decisão, pura: `restaurado_feito` diz se a restauração no keepd do
/// retrato gravado acabou (ou não havia o que restaurar nele).
pub fn decidir(gravado: Option<&Retrato>, novo: &Retrato, restaurado_feito: bool) -> Gravacao {
    let Some(g) = gravado else { return Gravacao::Grava };
    if g.keepd.mesmo(&novo.keepd) {
        return if novo.abas.is_empty() && !g.abas.is_empty() { Gravacao::Mantem } else { Gravacao::Grava };
    }
    if g.abas.is_empty() {
        return Gravacao::Grava;
    }
    if restaurado_feito { Gravacao::GiraEGrava } else { Gravacao::GravaSemGirar }
}

/// Fotografa e grava. Devolve o retrato e o que foi feito.
pub fn gravar() -> Result<(Retrato, Gravacao), String> {
    let novo = fotografar()?;
    let pasta = crate::pasta();
    let gravado: Option<Retrato> = crate::ler(&pasta.join(ARQUIVO));
    let feito = gravado.as_ref().is_none_or(|g| crate::restaurar::restauracao_acabou(&g.keepd));
    let decisao = decidir(gravado.as_ref(), &novo, feito);
    match decisao {
        Gravacao::Mantem => {}
        Gravacao::GiraEGrava => {
            std::fs::rename(pasta.join(ARQUIVO), pasta.join(ANTERIOR)).map_err(|e| e.to_string())?;
            crate::gravar(&pasta.join(ARQUIVO), &novo).map_err(|e| e.to_string())?;
        }
        Gravacao::Grava | Gravacao::GravaSemGirar => {
            crate::gravar(&pasta.join(ARQUIVO), &novo).map_err(|e| e.to_string())?;
        }
    }
    Ok((novo, decisao))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn r(pid: u32, inicio: u64, n: usize) -> Retrato {
        let aba = |i: usize| AbaGuardada {
            workspace: "W".into(), aba: i as u32 + 1, cwd: "/".into(), split_of: 0, split_dir: 0, cols: 80, rows: 24,
            nome: None, ia: None, adormecida: false,
        };
        Retrato {
            versao: 1, gravado_ms: inicio, keepd: Keepd { pid, inicio_ms: inicio, socket_nasceu: None },
            workspaces: vec!["W".into()], abas: (0..n).map(aba).collect(),
        }
    }

    #[test]
    fn mesmo_keepd_grava_e_vazio_nao_apaga() {
        assert_eq!(decidir(None, &r(1, 10, 0), true), Gravacao::Grava);
        assert_eq!(decidir(Some(&r(1, 10, 3)), &r(1, 10, 4), true), Gravacao::Grava);
        assert_eq!(decidir(Some(&r(1, 10, 3)), &r(1, 10, 0), true), Gravacao::Mantem);
    }

    #[test]
    fn keepd_novo_gira_so_com_a_restauracao_do_anterior_acabada() {
        assert_eq!(decidir(Some(&r(1, 10, 3)), &r(2, 20, 1), true), Gravacao::GiraEGrava);
        assert_eq!(decidir(Some(&r(1, 10, 3)), &r(2, 20, 1), false), Gravacao::GravaSemGirar);
        assert_eq!(decidir(Some(&r(1, 10, 0)), &r(2, 20, 1), false), Gravacao::Grava, "nada a guardar do anterior");
    }
}
