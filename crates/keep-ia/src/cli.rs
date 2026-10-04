//! A linha de comando: `keep ia …` e `keep worktrees …`, em JSON.
//!
//! Sempre um objeto com `"versao": 1`. Saída 0 = `ok: true`; 1 = recusado
//! (`ok: false`, com `motivo` e `detalhe` para mostrar a quem usa); 2 = uso
//! errado.

use serde_json::{Value, json};

use crate::{VERSAO, contas};

/// Os argumentos de um comando: as posições, e as opções `--nome=valor` ou
/// `--nome` (valor vazio). Um valor que começa com "-" não vira opção, porque
/// só vale depois do `=`.
pub struct Args {
    pub posicoes: Vec<String>,
    pub opcoes: Vec<(String, String)>,
}

impl Args {
    pub fn de(args: &[String]) -> Args {
        let mut posicoes = Vec::new();
        let mut opcoes = Vec::new();
        for a in args {
            if let Some(resto) = a.strip_prefix("--") {
                match resto.split_once('=') {
                    Some((n, v)) => opcoes.push((n.to_string(), v.to_string())),
                    None => opcoes.push((resto.to_string(), String::new())),
                }
            } else {
                posicoes.push(a.clone());
            }
        }
        Args { posicoes, opcoes }
    }

    pub fn opcao(&self, nome: &str) -> Option<&str> {
        self.opcoes.iter().find(|(n, _)| n == nome).map(|(_, v)| v.as_str())
    }

    pub fn tem(&self, nome: &str) -> bool {
        self.opcao(nome).is_some()
    }
}

/// Imprime a resposta e devolve o código de saída.
pub fn responde(mut corpo: Value) -> i32 {
    if let Some(o) = corpo.as_object_mut() {
        o.insert("versao".into(), json!(VERSAO));
    }
    let ok = corpo.get("ok").and_then(Value::as_bool).unwrap_or(false);
    println!("{}", serde_json::to_string(&corpo).unwrap_or_else(|_| "{}".into()));
    if ok { 0 } else { 1 }
}

pub fn recusa(motivo: &str, detalhe: &str) -> Value {
    json!({ "ok": false, "motivo": motivo, "detalhe": detalhe })
}

pub fn uso_errado(texto: &str) -> i32 {
    println!("{}", json!({ "versao": VERSAO, "ok": false, "motivo": "uso", "detalhe": texto }));
    2
}

/// `keep ia <args>`.
pub fn ia(args: &[String]) -> i32 {
    let a = Args::de(args);
    let comando = a.posicoes.first().map(String::as_str).unwrap_or("");
    match comando {
        "contas" => {
            let lista = contas::listar(true);
            responde(json!({
                "ok": true,
                "contas": lista,
                "ordem": contas::ordem_efetiva(&lista),
                "gerenteExterno": contas::gerente_externo(),
            }))
        }
        "ordem" => match a.posicoes.get(1).map(String::as_str) {
            None => {
                let lista = contas::listar(false);
                responde(json!({ "ok": true, "ordem": contas::ordem_efetiva(&lista) }))
            }
            Some("mover") => {
                let (Some(chave), Some(sentido)) = (a.posicoes.get(2), a.posicoes.get(3)) else {
                    return uso_errado("keep ia ordem mover <chave> cima|baixo");
                };
                let cima = match sentido.as_str() {
                    "cima" => true,
                    "baixo" => false,
                    _ => return uso_errado("o sentido é cima ou baixo"),
                };
                match contas::mover(chave, cima) {
                    Ok(ordem) => responde(json!({ "ok": true, "ordem": ordem })),
                    Err(detalhe) => responde(recusa("sem-conta", &detalhe)),
                }
            }
            Some(_) => uso_errado("keep ia ordem [mover <chave> cima|baixo]"),
        },
        "uso" => crate::uso::cli(&a),
        "abas" => match crate::abas::ler() {
            Ok((r, ia)) => responde(json!({
                "ok": true,
                "keepd": { "pid": r.daemon.pid, "inicioMs": r.daemon.started_ms },
                "exato": r.exato,
                "ia": ia,
            })),
            Err(e) => responde(recusa("sem-keepd", &e)),
        },
        "trocar" => crate::trocar::cli(&a),
        "entrar" => crate::login::cli_entrar(&a),
        "login" => crate::login::cli_login(&a),
        "sincronizar" => crate::sincronizar::cli(),
        _ => uso_errado("keep ia contas | uso | ordem [mover] | abas | trocar | entrar | login | sincronizar"),
    }
}

/// `keep worktrees <args>`.
pub fn worktrees(args: &[String]) -> i32 {
    crate::worktrees::cli(args)
}
