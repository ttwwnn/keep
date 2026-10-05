//! Seguir a ordem: a aba que segue a fila de prioridade e está numa conta
//! que não pode mais trabalhar (no limite, com o login recusado, parada na
//! tela de limite) vai para a primeira conta do Claude disponível da fila (o
//! GPT nunca entra sozinho: só escolhido no menu da aba) — quando está
//! livre. E a que está numa conta mais abaixo na fila que a primeira
//! disponível sobe para ela, depois de dois minutos sem uso.
//!
//! O app chama a cada 30 s. No máximo 3 abas por rodada; uma aba que
//! recusou a troca espera um pouco antes de ser tentada de novo. Com um
//! gerente externo (o kit), quem faz isso é ele, e aqui nada acontece.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::abas::{self, SEGUIR_ORDEM};
use crate::contas::{self, Conta};
use crate::tela::{self, Estado};
use crate::trocar::{self, Pedido};
use crate::{caminhos, daemon, historico, uso};

const POR_RODADA: usize = 3;
const OCIOSA_MS: u64 = 120_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Bloqueio {
    em: f64,
    ate: f64,
    marca: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Espera {
    alvo: String,
    ate: f64,
    motivo: String,
    #[serde(default)]
    detalhe: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Registro {
    limites: BTreeMap<String, Bloqueio>,
    esperas: BTreeMap<String, Espera>,
    alvo: Option<String>,
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("ia-prioridade.json")
}

fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Uma rodada. Devolve o objeto da resposta (sem `versao`).
pub fn rodada() -> Value {
    if contas::gerente_externo() {
        return json!({ "ok": true, "estado": "gerente-externo", "alteradas": [], "pendentes": [] });
    }
    let _ = std::fs::create_dir_all(caminhos::estado());
    let Ok(trava) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(caminhos::estado().join("sincronizar.trava"))
    else {
        return json!({ "ok": false, "motivo": "erro", "detalhe": "não deu para abrir a trava" });
    };
    if trava.try_lock().is_err() {
        return json!({ "ok": true, "estado": "em-andamento", "alteradas": [], "pendentes": [] });
    }
    let Ok(r) = daemon::listar() else {
        return json!({ "ok": true, "estado": "sem-keepd", "alteradas": [], "pendentes": [] });
    };
    let contas = contas::listar(true);
    let linhas = uso::medir(false, None);
    let t = agora();
    let anterior: Registro = std::fs::read_to_string(arquivo()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut bloqueios = anterior.limites.clone();
    bloqueios.retain(|_, b| b.ate > t);
    let ia = abas::das_abas(&r, &contas);

    // As abas que seguem a ordem, e o que as telas delas dizem.
    struct Seguidora {
        ia: abas::IaAba,
        estado: Estado,
        limite: bool,
        ociosa: bool,
    }
    let mut seguidoras = Vec::new();
    for i in ia.into_iter().filter(|i| i.conta == SEGUIR_ORDEM) {
        let Some(a) = r.aba(&i.workspace, i.aba) else { continue };
        let prog = tela::programa(&a.info.command);
        let (linhas_tela, limpas) = trocar::tela_de(&i.workspace, i.aba);
        let (estado, _) = tela::situacao(&prog, &linhas_tela, &limpas);
        let limite = estado != Estado::Digitado && tela::limite_na_tela(&linhas_tela);
        if limite {
            if let Some(atual) = i.atual.as_deref().and_then(|k| contas::achar(&contas, k)) {
                let marca = format!("{}|{}|{}", historico::nascimento(&r), i.pid, tela::impressao(&linhas_tela));
                // A mesma tela antiga não prorroga o bloqueio.
                if bloqueios.get(&atual.key).is_none_or(|b| b.marca != marca) {
                    bloqueios.insert(atual.key.clone(), Bloqueio { em: t, ate: t + 300.0, marca });
                }
            }
        }
        let ociosa = a.info.last_active != 0
            && crate::contas::donos::agora_ms().saturating_sub(a.info.last_active) >= OCIOSA_MS;
        seguidoras.push(Seguidora { ia: i, estado, limite, ociosa });
    }

    let livre = |c: &Conta| -> bool {
        let medida = linhas.iter().find(|l| l.account.key == c.key);
        let bloqueada = bloqueios.get(&c.key).is_some_and(|b| {
            b.ate > t && medida.and_then(|l| l.measured_at).is_none_or(|m| m <= b.em)
        });
        c.roda && !bloqueada && medida.is_none_or(|l| l.disponivel_em(t)) && contas::ambiente(c).is_ok()
    };
    let posicao = |chave: &str| contas.iter().position(|c| c.e(chave));
    // Só contas do Claude: a aba que segue a ordem nunca vai sozinha para o
    // GPT; sem nenhuma do Claude livre, ela espera ("todas-no-limite").
    let alvo: Option<&Conta> = contas.iter().find(|c| c.engine == contas::Motor::Claude && livre(c));

    let mut alteradas = Vec::new();
    let mut pendentes = Vec::new();
    let mut esperas = BTreeMap::new();
    for s in &seguidoras {
        let i = &s.ia;
        let refe = |motivo: &str, detalhe: &str| json!({ "ws": i.workspace, "aba": i.aba, "motivo": motivo, "detalhe": detalhe });
        let atual = i.atual.as_deref().and_then(|k| contas::achar(&contas, k));
        let atual_livre = atual.is_some_and(|c| livre(c)) && !s.limite;
        let Some(alvo) = alvo else {
            if !atual_livre {
                pendentes.push(refe("todas-no-limite", ""));
            }
            continue;
        };
        if atual.is_some_and(|c| c.key == alvo.key) {
            continue;
        }
        // Precisa sair da conta em que está, ou só subir na fila?
        let abaixo = match (i.atual.as_deref().and_then(|k| posicao(k)), posicao(&alvo.order_key)) {
            (Some(a), Some(b)) => a > b,
            _ => true,
        };
        if atual_livre && !(abaixo && s.ociosa) {
            continue;
        }
        if s.estado != Estado::Livre && !(s.estado == Estado::Dialogo && s.limite) {
            pendentes.push(refe(
                match s.estado {
                    Estado::Ocupada => "ocupada",
                    Estado::Dialogo => "dialogo",
                    Estado::Digitado => "digitado",
                    Estado::Livre => "livre",
                },
                "",
            ));
            continue;
        }
        if alteradas.len() >= POR_RODADA {
            pendentes.push(refe("aguardando-rodada", ""));
            continue;
        }
        let chave_aba = historico::chave(&r, &i.workspace, i.aba);
        if let Some(e) = anterior.esperas.get(&chave_aba) {
            if e.alvo == alvo.order_key && e.ate > t {
                esperas.insert(chave_aba.clone(), e.clone());
                pendentes.push(refe(&e.motivo, &e.detalhe));
                continue;
            }
        }
        let p = Pedido {
            ws: i.workspace.clone(),
            aba: i.aba,
            para: SEGUIR_ORDEM.into(),
            conta: Some(alvo.order_key.clone()),
            cancelar_limite: s.limite,
            interromper: false,
        };
        match trocar::trocar(&p) {
            Ok(feito) => alteradas.push(json!({ "ws": i.workspace, "aba": i.aba, "feito": feito })),
            Err(e) => {
                // Trabalho em segundo plano pode durar dias: não reabrir a mesma
                // pergunta a cada rodada.
                let prazo = if e.motivo == "ocupada" && e.detalhe.contains("segundo plano") { 300.0 } else { 30.0 };
                esperas.insert(
                    chave_aba,
                    Espera { alvo: alvo.order_key.clone(), ate: t + prazo, motivo: e.motivo.clone(), detalhe: e.detalhe.clone() },
                );
                pendentes.push(refe(&e.motivo, &e.detalhe));
            }
        }
    }
    let novo = Registro { limites: bloqueios, esperas, alvo: alvo.map(|c| c.order_key.clone()) };
    if let Ok(texto) = serde_json::to_vec_pretty(&novo) {
        let tmp = arquivo().with_extension("json.tmp");
        if std::fs::write(&tmp, texto).is_ok() {
            let _ = std::fs::rename(&tmp, arquivo());
        }
    }
    let _ = trava.unlock();
    json!({
        "ok": true,
        "estado": "feito",
        "alvo": alvo.map(|c| c.order_key.clone()),
        "alteradas": alteradas,
        "pendentes": pendentes,
    })
}

/// `keep ia sincronizar`.
pub fn cli() -> i32 {
    crate::cli::responde(rodada())
}
