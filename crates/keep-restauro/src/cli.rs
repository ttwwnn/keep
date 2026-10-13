//! `keep restauro …`, em JSON, para o app e para quem quiser ver.
//!
//!   keep restauro retrato [--print]        grava o retrato das abas (--print: só mostra)
//!   keep restauro restaurar [--fase=abas] [--seco]
//!   keep restauro manter                   restaura o que faltar e grava o retrato (o app, a cada 30 s)

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
        _ => {
            eprintln!("uso: keep restauro retrato [--print] | restaurar [--fase=abas] [--seco] | manter");
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
