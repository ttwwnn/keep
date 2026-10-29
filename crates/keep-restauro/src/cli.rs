//! `keep restauro …`, em JSON, para o app e para quem quiser ver.
//!
//!   keep restauro retrato [--print]        grava o retrato das abas (--print: só mostra)
//!   keep restauro restaurar [--fase=abas] [--seco]
//!   keep restauro manter                   restaura o que faltar e grava o retrato (o app, a cada 30 s)
//!   keep restauro adormecer [--seco] [--vista W/N]…   fecha a conversa parada há 12 h fora de vista (o app, a cada 5 min)
//!   keep restauro adormecer --agora W [--aba N] [--vista W/N]…  o mesmo para a aba N (ou as livres do workspace W), sem esperar as 12 h
//!   keep restauro adormecidas                         as abas adormecidas de agora (o app do Windows, para riscar o título)
//!   keep restauro acordar --workspace W --aba N       a conversa adormecida da aba volta (o app, ao mostrá-la)
//!   keep restauro importar --de PASTA [--disponivel ID]… [--seco]   as abas de outro computador (importar.rs)
//!   keep restauro importado --pid N --inicio MS [--keepd-atual PID:MS]   em que pé está essa importação

use serde_json::json;

use crate::restaurar;
use crate::retrato::{self, Gravacao};

pub fn main(args: &[String]) -> i32 {
    let tem = |o: &str| args.iter().any(|a| a == o);
    let saida = match args.first().map(String::as_str) {
        Some("retrato") if tem("--print") => retrato::fotografar().map(|r| serde_json::to_value(r).unwrap_or_default()),
        Some("retrato") => grava_retrato(),
        Some("restaurar") => restaurar::restaurar(tem("--fase=abas"), tem("--seco"))
            .map(|r| json!({ "versao": crate::VERSAO, "restaurar": r })),
        Some("manter") => {
            let r = restaurar::restaurar(false, false);
            let g = grava_retrato();
            match (r, g) {
                (Ok(r), Ok(g)) => Ok(json!({ "versao": crate::VERSAO, "restaurar": r, "retrato": g["retrato"] })),
                (Err(e), _) | (_, Err(e)) => Err(e),
            }
        }
        Some("adormecer") => {
            let vistas = args.windows(2).filter(|w| w[0] == "--vista").filter_map(|w| crate::adormecer::vista(&w[1])).collect();
            match args.windows(2).find(|w| w[0] == "--agora") {
                Some(w) => {
                    let aba = args.windows(2).find(|a| a[0] == "--aba").and_then(|a| a[1].parse().ok());
                    crate::adormecer::adormecer_agora(&w[1], aba, &vistas)
                }
                None => crate::adormecer::adormecer(&vistas, tem("--seco")),
            }
        }
        Some("adormecidas") => crate::adormecer::listar(),
        Some("importar") => {
            let valor = |o: &str| args.windows(2).find(|w| w[0] == o).map(|w| w[1].clone());
            match valor("--de") {
                Some(de) => {
                    let disponiveis = args.windows(2).filter(|w| w[0] == "--disponivel").map(|w| w[1].clone()).collect();
                    let p = crate::importar::Pedido { de: de.into(), disponiveis, seco: tem("--seco") };
                    match crate::importar::importar(&p) {
                        Ok(v) => {
                            let ok = v["ok"].as_bool().unwrap_or(false);
                            println!("{v}");
                            return if ok { 0 } else { 1 };
                        }
                        Err(e) => Err(e),
                    }
                }
                None => Err("importar pede --de PASTA (com origem.json e restauro/retrato.json)".into()),
            }
        }
        Some("importado") => {
            let valor = |o: &str| args.windows(2).find(|w| w[0] == o).map(|w| w[1].clone());
            let keepd = |pid: Option<String>, inicio: Option<String>| -> Option<crate::Keepd> {
                Some(crate::Keepd { pid: pid?.parse().ok()?, inicio_ms: inicio?.parse().ok()?, socket_nasceu: None })
            };
            let atual = valor("--keepd-atual").and_then(|v| {
                let (pid, inicio) = v.split_once(':')?;
                keepd(Some(pid.into()), Some(inicio.into()))
            });
            match keepd(valor("--pid"), valor("--inicio")) {
                Some(sint) => Ok(crate::importar::importado(&sint, atual.as_ref())),
                None => Err("importado pede --pid N --inicio MS (o keepdSintetico que o importar disse)".into()),
            }
        }
        Some("acordar") => {
            let valor = |o: &str| args.windows(2).find(|w| w[0] == o).map(|w| w[1].clone());
            match (valor("--workspace"), valor("--aba").and_then(|n| n.parse().ok())) {
                (Some(ws), Some(aba)) => crate::adormecer::acordar(&ws, aba),
                _ => Err("acordar pede --workspace W --aba N".into()),
            }
        }
        _ => {
            eprintln!(
                "uso: keep restauro retrato [--print] | restaurar [--fase=abas] [--seco] | manter | adormecer [--seco] [--agora W [--aba N]] [--vista W/N]… | adormecidas | acordar --workspace W --aba N | importar --de PASTA [--disponivel ID]… [--seco] | importado --pid N --inicio MS [--keepd-atual PID:MS]"
            );
            return 2;
        }
    };
    match saida {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            println!("{}", json!({ "versao": crate::VERSAO, "erro": e }));
            1
        }
    }
}

fn grava_retrato() -> Result<serde_json::Value, String> {
    let _trava = crate::Trava::pegar(".retrato.lock").ok_or("outro retrato em andamento")?;
    let (r, g) = retrato::gravar()?;
    let decisao = match g {
        Gravacao::Grava => "gravado",
        Gravacao::GiraEGrava => "gravado; o do keepd anterior foi para anterior.json",
        Gravacao::GravaSemGirar => "gravado; anterior.json mantido (a restauração dele não acabou)",
        Gravacao::Mantem => "mantido (lista vazia não apaga o retrato)",
    };
    Ok(json!({
        "versao": crate::VERSAO,
        "retrato": {
            "abas": r.abas.len(),
            "conversas": r.abas.iter().filter(|a| a.ia.is_some()).count(),
            "decisao": decisao,
        }
    }))
}
