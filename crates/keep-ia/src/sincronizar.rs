//! Seguir a ordem: a aba que segue a fila de prioridade e está numa conta
//! que não pode mais trabalhar (no limite, com o login recusado, parada na
//! tela de limite) vai para a primeira conta do Claude disponível da fila (o
//! GPT nunca entra sozinho: só escolhido no menu da aba) — quando está
//! livre. E a que está numa conta mais abaixo na fila que a primeira
//! disponível sobe para ela, depois de dois minutos sem uso.
//!
//! Com o Claude aberto num armazém de aba (`quente.rs`), a troca é a quente:
//! nada é digitado e a conversa segue no mesmo processo — por isso ela vale
//! com a aba trabalhando. O Claude aberto no login global (de antes da
//! troca a quente) fica com a ordem global.
//!
//! O app chama a cada 30 s. No máximo 3 abas por rodada; uma aba que
//! recusou a troca espera um pouco antes de ser tentada de novo. Com um
//! gerente externo (o kit), quem faz isso é ele, e aqui nada acontece (a
//! renovação leva as abas de armazém próprio para a conta que ele escolhe).
//!
//! Nunca sem consentimento (pedido de 25/11/2026):
//! - com o Chaveiro trancado, ou o `securityd` fora do ar, o login não se lê e
//!   a conta parece sem acesso; nenhuma aba sai da conta em que está: a
//!   rodada espera e olha de novo na seguinte;
//! - a aba que sai de uma conta que ainda trabalha, porque ela passou do teto
//!   da semanal ou porque a fila foi reordenada, é avisada à pessoa (no
//!   rodapé, por `aviso`, por uma hora, e à aba geral do workspace), com o
//!   motivo e como desfazer.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::abas::{self, SEGUIR_ORDEM};
use crate::contas::{self, Conta};
use crate::tela::{self, Estado};
use crate::trocar::{self, Pedido};
use crate::{caminhos, daemon, historico, processos, quente, sessoes, uso};

const POR_RODADA: usize = 3;
const OCIOSA_MS: u64 = 120_000;
/// Quanto tempo uma parada que já culpou uma conta é lembrada (s).
const CULPAS_POR: f64 = 12.0 * 3600.0;
/// Quanto tempo o aviso de uma troca sem pedido fica no rodapé (s).
const AVISO_POR: f64 = 3600.0;
/// Uma fila reordenada há menos que isto explica uma aba que sobe para a primeira conta (ms).
const REORDENADA_HA_MS: u64 = 24 * 3600 * 1000;

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

/// O que a pessoa precisa saber de abas que trocaram de conta sem ela pedir, e até quando dizer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Aviso {
    texto: String,
    ate: f64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Registro {
    limites: BTreeMap<String, Bloqueio>,
    esperas: BTreeMap<String, Espera>,
    alvo: Option<String>,
    aviso: Option<Aviso>,
    /// As paradas no limite (do transcrito ou da tela) que já culparam uma
    /// conta (a marca, e quando). Uma parada culpa uma vez: a mesma, ainda no
    /// fim do transcrito ou à vista, não culpa a conta nova depois de uma troca
    /// nem bloqueia de novo a mesma conta quando o bloqueio vence (aí quem diz
    /// é a medição).
    culpas: BTreeMap<String, f64>,
}

fn arquivo() -> PathBuf {
    caminhos::estado().join("ia-prioridade.json")
}

fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// O que o transcrito da conversa do Claude da aba diz do limite.
/// - `Some(Some(parada))`: a conversa parou no limite da conta (a última fala, desta abertura).
/// - `Some(None)`: a conversa não está parada no limite (andou depois, parou noutra coisa, ou a parada é de uma
///   abertura anterior) — e aí a tela, mesmo com a frase de uma parada antiga à vista, não conta.
/// - `None`: quem diz é a tela, como antes. O transcrito não pode ser lido (sem conversa ou sem arquivo), ou a
///   conversa parou num limite que ele não diz se é da conta: a frase genérica de versões antigas ("You've hit
///   your limit", que só o diálogo do limite na tela confirma), um formato novo.
fn transcrito_da_aba(i: &abas::IaAba) -> Option<Option<sessoes::ParadaNoLimite>> {
    let caminho = sessoes::transcrito_claude(i.conversa.as_deref()?)?;
    let nasceu = processos::inicio_ms(i.pid)?;
    o_que_a_fala_diz(sessoes::ultima_fala(&caminho).as_ref(), nasceu)
}

/// `transcrito_da_aba` sem o disco: a última fala da conversa e quando o processo abriu (ms). Sem fala (o
/// transcrito ainda sem a conversa, ou num formato que não se lê) ou sem carimbo na parada, a tela decide.
fn o_que_a_fala_diz(fala: Option<&Value>, nasceu: u64) -> Option<Option<sessoes::ParadaNoLimite>> {
    let fala = fala?;
    if fala.get("error").and_then(Value::as_str) != Some("rate_limit") {
        return Some(None);
    }
    match sessoes::carimbo_ms(fala) {
        0 => None,
        quando if quando < nasceu => Some(None),
        _ => sessoes::parada_no_limite(fala).filter(|p| p.da_conta).map(Some),
    }
}

/// A conta que parou no limite. A parada cuja janela já recomeçou (`agora`,
/// em segundos) acabou: de nenhuma. Depois que a conta da aba passou a valer
/// no Claude (`vale_em`), o pedido que parou saiu por ela (`atual`). Logo
/// depois de uma troca a quente (ou sem armazém de aba, quando a conta muda
/// com o login global), quem diz é o recomeço da janela recusada, se casa com
/// o medido de uma conta só; sem casar, depois da troca nenhuma leva a culpa
/// (a medição seguinte diz) e sem armazém de aba vale a da aba.
fn conta_que_parou(
    p: &sessoes::ParadaNoLimite,
    linhas: &[uso::Linha],
    atual: Option<&str>,
    vale_em: Option<u64>,
    agora: f64,
) -> Option<String> {
    if p.recomeca.is_some_and(|fim| fim <= agora) {
        return None;
    }
    if vale_em.is_some_and(|v| p.quando_ms >= v) {
        return atual.map(str::to_string);
    }
    if let (Some(janela), Some(recomeca)) = (&p.janela, p.recomeca) {
        let mut ditos = json!({});
        ditos[janela.as_str()] = json!({ "resets_at": recomeca });
        if let Some(c) = abas::conta_dos_limites(&ditos, linhas) {
            return Some(c);
        }
    }
    if vale_em.is_some() {
        return None;
    }
    atual.map(str::to_string)
}

/// Por que uma aba saiu, sem a pessoa pedir, de uma conta que ainda trabalha: (o motivo, como desfazer). A que passou
/// do teto da semanal (`reservada`), ou a que sobe para a primeira conta (`subiu`) logo depois de a fila ser
/// reordenada; `None` para a troca de sempre (a conta no limite, o login recusado) e para a volta à primeira conta
/// depois de um limite.
fn sem_pedido(de: &str, para: &str, teto: Option<f64>, subiu: bool, mudanca: Option<&contas::mudancas::Vista>) -> Option<(String, String)> {
    let quando = |v: &contas::mudancas::Vista| {
        use chrono::TimeZone;
        let hora = chrono::Local.timestamp_millis_opt(v.mudou_ms as i64).single().map(|h| h.format("%d/%m %H:%M").to_string());
        format!("{}, em {}", v.por, hora.unwrap_or_default())
    };
    if let Some(teto) = teto {
        let origem = mudanca.map(|v| format!(" (a fila de prioridade mudou por último: {})", quando(v))).unwrap_or_default();
        return Some((
            format!("a {de} passou do teto de {teto}% da semana que a fila de prioridade dá a ela{origem}"),
            format!("tire ou suba o teto7d da {de} em ~/.claude/contas/.ordem; as abas voltam sozinhas para ela em até 2 min"),
        ));
    }
    let v = mudanca.filter(|_| subiu)?;
    Some((
        format!("a fila de prioridade foi reordenada ({}) e a {para} ficou à frente da {de}", quando(v)),
        format!("suba a {de} de volta pelas setas do rodapé; as abas voltam sozinhas para ela em até 2 min"),
    ))
}

/// O aviso de uma rodada: as abas que trocaram sem pedido, por conta de origem e destino.
fn texto_do_aviso(trocas: &[(String, String, String, String)]) -> String {
    let mut grupos: Vec<(&(String, String, String, String), usize)> = Vec::new();
    for t in trocas {
        match grupos.iter_mut().find(|(g, _)| *g == t) {
            Some((_, n)) => *n += 1,
            None => grupos.push((t, 1)),
        }
    }
    grupos
        .iter()
        .map(|((de, para, motivo, desfazer), n)| {
            let abas = if *n == 1 { "1 aba saiu".to_string() } else { format!("{n} abas saíram") };
            format!("{abas} da {de} para a {para} sem pedido seu: {motivo}. Para desfazer: {desfazer}.")
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    let anterior: Registro = std::fs::read_to_string(arquivo()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let aviso_vigente = anterior.aviso.as_ref().filter(|a| a.ate > agora()).map(|a| a.texto.clone());
    // O login que não se lê com o Chaveiro trancado (ou o securityd fora do ar) não é conta que acabou: nenhuma aba sai
    // da conta em que está; a próxima rodada olha de novo.
    if let Some(por) = crate::contas::credencial::chaveiro_indisponivel() {
        return json!({ "ok": true, "estado": "chaveiro-trancado", "detalhe": format!("{por}: nenhuma aba troca de conta até ele voltar"), "alteradas": [], "pendentes": [], "aviso": aviso_vigente });
    }
    let Ok(r) = daemon::listar() else {
        return json!({ "ok": true, "estado": "sem-keepd", "alteradas": [], "pendentes": [], "aviso": aviso_vigente });
    };
    let contas = contas::listar(true);
    let linhas = uso::medir(false, None);
    // Depois da medição: um bloqueio criado nesta rodada não pode ser mais velho que a medição que ela acabou de fazer
    // (uma medição mais nova que o bloqueio o desfaz).
    let t = agora();
    let mut bloqueios = anterior.limites.clone();
    bloqueios.retain(|_, b| b.ate > t);
    let mut culpas = anterior.culpas.clone();
    culpas.retain(|_, em| t - *em < CULPAS_POR);
    let ia = abas::das_abas(&r, &contas);

    // As abas que seguem a ordem, e o que as telas delas dizem.
    struct Seguidora {
        ia: abas::IaAba,
        estado: Estado,
        /// A tela mostra o limite.
        limite: bool,
        /// Quem parou no limite foi a conta em que a aba está (e não a de
        /// antes de uma troca, cuja tela ainda está à vista).
        limite_da_atual: bool,
        ociosa: bool,
        claude_aberto: bool,
        /// O Claude roda num armazém de aba: troca a quente.
        armazem_proprio: bool,
    }
    let mut seguidoras = Vec::new();
    // E as presas numa conta desligada: desligar a conta vale para todas as conversas (elas passam a seguir a ordem).
    let presa_desligada = |i: &abas::IaAba| contas::achar(&contas, &i.conta).is_some_and(|c| c.off);
    for i in ia.into_iter().filter(|i| i.conta == SEGUIR_ORDEM || presa_desligada(i)) {
        let Some(a) = r.aba(&i.workspace, i.aba) else { continue };
        let prog = tela::programa(&a.info.command);
        let (linhas_tela, limpas) = trocar::tela_de(&i.workspace, i.aba);
        let (estado, _) = tela::situacao(&prog, &linhas_tela, &limpas);
        let limite = estado != Estado::Digitado && tela::limite_na_tela(&linhas_tela);
        let claude_aberto = prog == tela::Programa::Claude;
        let vale_em = if claude_aberto { quente::conta_vale_em(i.pid) } else { None };
        let atual = i.atual.as_deref().and_then(|k| contas::achar(&contas, k));
        // Quem parou no limite. O transcrito, quando há, vale antes da tela: a
        // frase muda de uma versão do Claude Code para outra, e a tela da conta
        // que parou segue à vista depois da troca a quente para a próxima,
        // mesmo depois que a conversa andou nela.
        let leitura = match claude_aberto.then(|| transcrito_da_aba(&i)).flatten() {
            // O diálogo do limite e a espera dele ("continuing automatically") só ficam à vista enquanto o Claude
            // espera, ao contrário da frase de uma parada antiga: valem mesmo com o transcrito sem a parada.
            Some(None) if tela::espera_limite(&linhas_tela) => None,
            outra => outra,
        };
        let (culpada, marca, recomeca) = match leitura {
            Some(Some(p)) => {
                let marca = format!("parada|{}", p.uuid);
                let culpada = (!culpas.contains_key(&marca))
                    .then(|| conta_que_parou(&p, &linhas, i.atual.as_deref(), vale_em, t))
                    .flatten()
                    .and_then(|k| contas::achar(&contas, &k));
                (culpada, marca, p.recomeca)
            }
            // A conversa andou depois da parada (o "continue", a resposta da conta nova) ou não parou no limite.
            Some(None) => (None, String::new(), None),
            // Sem o transcrito, a tela.
            None if limite => {
                let marca = format!("{}|{}|{}", historico::nascimento(&r), i.pid, tela::impressao(&linhas_tela));
                // A tela que já culpou uma conta é a de antes da troca: não culpa a conta nova, nem depois
                // que o bloqueio da antiga venceu (a tela segue à vista enquanto ninguém digita).
                let nova = !culpas.contains_key(&marca) && !bloqueios.values().any(|b| b.marca == marca);
                (atual.filter(|_| nova), marca, None)
            }
            None => (None, String::new(), None),
        };
        if let Some(c) = culpada {
            // Lembrada só quando culpou alguém: a conta da aba ainda não reconhecida culpa depois.
            culpas.insert(marca.clone(), t);
            // A mesma parada não prorroga o bloqueio, que acaba no máximo quando a janela recusada recomeça.
            if bloqueios.get(&c.key).is_none_or(|b| b.marca != marca) {
                let ate = recomeca.map_or(t + 300.0, |fim| fim.min(t + 300.0));
                bloqueios.insert(c.key.clone(), Bloqueio { em: t, ate, marca });
            }
        }
        let limite_da_atual = culpada.is_some_and(|c| atual.is_some_and(|a| a.key == c.key));
        let ociosa = a.info.last_active != 0
            && crate::contas::donos::agora_ms().saturating_sub(a.info.last_active) >= OCIOSA_MS;
        let armazem_proprio = vale_em.is_some();
        seguidoras.push(Seguidora { ia: i, estado, limite, limite_da_atual, ociosa, claude_aberto, armazem_proprio });
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
    // Sem nenhuma livre, a aba numa conta acima do teto da semanal não espera
    // nela: vai para a primeira do Claude que roda sem passar do teto (no
    // limite, a aba espera ali), e o fim da semana fica com quem divide a conta.
    let reservada =
        |c: &Conta| linhas.iter().find(|l| l.account.key == c.key).is_some_and(|l| l.reservada_em(t));
    let refugio: Option<&Conta> = contas
        .iter()
        .find(|c| c.engine == contas::Motor::Claude && c.roda && !reservada(c) && contas::ambiente(c).is_ok());

    let mut alteradas = Vec::new();
    let mut pendentes = Vec::new();
    let mut esperas = BTreeMap::new();
    // As trocas sem pedido desta rodada: (de, para, motivo, como desfazer), e as novidades para as abas gerais.
    let mut sem_pedir: Vec<(String, String, String, String)> = Vec::new();
    let mut novidades = Vec::new();
    let mudanca = contas::mudancas::vista()
        .filter(|v| v.mudou_ms > 0 && crate::contas::donos::agora_ms().saturating_sub(v.mudou_ms) < REORDENADA_HA_MS);
    for s in &seguidoras {
        let i = &s.ia;
        let refe = |motivo: &str, detalhe: &str| json!({ "ws": i.workspace, "aba": i.aba, "motivo": motivo, "detalhe": detalhe });
        let atual = i.atual.as_deref().and_then(|k| contas::achar(&contas, k));
        let atual_livre = atual.is_some_and(|c| livre(c)) && !s.limite_da_atual;
        let Some(alvo) = alvo.or(refugio.filter(|_| atual.is_some_and(|c| reservada(c)))) else {
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
        // Com o Claude aberto no login global, nunca: trocar a conta dele
        // seria sair e voltar. Com o kit, a ordem global é que o leva; sem
        // ele, só a pessoa, saindo e retomando a conversa na aba.
        if s.claude_aberto && !s.armazem_proprio {
            if !atual_livre {
                pendentes.push(refe(
                    "claude-global",
                    if contas::gerente_externo() {
                        "O Claude da aba roda no login global; a conta dele muda com a ordem global."
                    } else {
                        "O Claude da aba roda no login global, sem o login próprio da aba: a conta dele só troca saindo dele (/exit) e retomando a conversa na aba (claude --resume)."
                    },
                ));
            }
            continue;
        }
        // A quente, a aba pode estar trabalhando: nada é digitado nela.
        if !s.armazem_proprio && s.estado != Estado::Livre && !(s.estado == Estado::Dialogo && s.limite) {
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
            cancelar_limite: s.limite && !s.armazem_proprio,
            interromper: false,
        };
        match trocar::trocar(&p) {
            Ok(feito) => {
                alteradas.push(json!({ "ws": i.workspace, "aba": i.aba, "feito": feito }));
                if let Some(de) = atual {
                    let teto = de.weekly_cap.filter(|_| reservada(de));
                    let (rotulo_de, rotulo_para) = (trocar::rotulo_em(&contas, &de.order_key), trocar::rotulo_em(&contas, &alvo.order_key));
                    if let Some((motivo, desfazer)) = sem_pedido(&rotulo_de, &rotulo_para, teto, atual_livre && abaixo, mudanca.as_ref()) {
                        novidades.push(crate::geral::Evento {
                            quando_ms: crate::contas::donos::agora_ms(),
                            workspace: i.workspace.clone(),
                            aba: i.aba,
                            nome: String::new(),
                            tipo: "conta".into(),
                            texto: format!("saiu da {rotulo_de} para a {rotulo_para} sem pedido seu: {motivo}. Para desfazer: {desfazer}."),
                        });
                        sem_pedir.push((rotulo_de, rotulo_para, motivo, desfazer));
                    }
                }
            }
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
    let aviso = if sem_pedir.is_empty() {
        anterior.aviso.clone().filter(|a| a.ate > t)
    } else {
        crate::geral::anotar(&novidades);
        Some(Aviso { texto: texto_do_aviso(&sem_pedir), ate: t + AVISO_POR })
    };
    let novo = Registro { limites: bloqueios, esperas, alvo: alvo.map(|c| c.order_key.clone()), aviso: aviso.clone(), culpas };
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
        "aviso": aviso.map(|a| a.texto),
    })
}

/// `keep ia sincronizar`.
pub fn cli() -> i32 {
    crate::cli::responde(rodada())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::abas::testes::linha;
    use crate::contas::Motor;

    #[test]
    fn a_troca_sem_pedido_diz_o_motivo_e_como_desfazer() {
        let mudou = contas::mudancas::Vista { texto: String::new(), mudou_ms: 1_795_000_000_000, por: "arquivo editado por fora".into() };
        // O teto da semanal: com a mudança da fila, de onde ela veio.
        let (motivo, desfazer) = sem_pedido("IA·1", "IA·3", Some(75.0), false, Some(&mudou)).unwrap();
        assert!(motivo.starts_with("a IA·1 passou do teto de 75% da semana que a fila de prioridade dá a ela (a fila de prioridade mudou por último: arquivo editado por fora, em "), "{motivo}");
        assert!(desfazer.starts_with("tire ou suba o teto7d da IA·1"), "{desfazer}");
        assert!(sem_pedido("IA·1", "IA·3", Some(75.0), false, None).unwrap().0.ends_with("dá a ela"), "sem mudança recente, só o teto");
        // A fila reordenada: a aba sobe para a nova primeira.
        let (motivo, desfazer) = sem_pedido("IA·2", "IA·1", None, true, Some(&mudou)).unwrap();
        assert!(motivo.starts_with("a fila de prioridade foi reordenada (arquivo editado por fora, em ") && motivo.ends_with("e a IA·1 ficou à frente da IA·2"), "{motivo}");
        assert!(desfazer.starts_with("suba a IA·2 de volta pelas setas do rodapé"), "{desfazer}");
        // A volta à primeira conta depois de um limite, sem a fila mudar, e a troca de quem está no limite: as de sempre.
        assert_eq!(sem_pedido("IA·2", "IA·1", None, true, None), None);
        assert_eq!(sem_pedido("IA·1", "IA·2", None, false, Some(&mudou)), None);
        // Várias abas da mesma troca, numa frase só.
        let t = |de: &str| (de.to_string(), "IA·3".to_string(), "m".to_string(), "d".to_string());
        assert_eq!(
            texto_do_aviso(&[t("IA·1"), t("IA·1"), t("IA·2")]),
            "2 abas saíram da IA·1 para a IA·3 sem pedido seu: m. Para desfazer: d. 1 aba saiu da IA·2 para a IA·3 sem pedido seu: m. Para desfazer: d."
        );
    }

    #[test]
    fn o_transcrito_cala_a_tela_so_quando_a_conversa_nao_esta_parada() {
        let fala = |erro: Option<&str>, texto: &str, cota: Option<&str>| {
            let mut v = json!({ "type": "assistant", "uuid": "u", "timestamp": "2026-11-04T18:00:00Z",
                "message": { "content": [{ "type": "text", "text": texto }] } });
            if let Some(e) = erro {
                v["error"] = json!(e);
            }
            if let Some(tipo) = cota {
                v["quotaLimits"] = json!({ "status": "rejected", "resetsAt": 1_793_833_200, "rateLimitType": tipo });
            }
            v
        };
        let abriu = sessoes::carimbo_ms(&json!({ "timestamp": "2026-11-04T17:00:00Z" }));
        let diz = |v: &Value| o_que_a_fala_diz(Some(v), abriu).map(|p| p.map(|p| p.janela));
        // Parada no limite da conta: o transcrito diz.
        assert_eq!(diz(&fala(Some("rate_limit"), "x", Some("five_hour"))), Some(Some(Some("five_hour".into()))));
        assert_eq!(diz(&fala(Some("rate_limit"), "You've hit your weekly limit · resets 8pm", None)), Some(Some(None)));
        // A conversa não está parada no limite: a tela não conta.
        assert_eq!(diz(&fala(None, "Feito.", None)), Some(None));
        assert_eq!(diz(&fala(Some("server_error"), "API Error: Connection error.", None)), Some(None));
        let depois = sessoes::carimbo_ms(&json!({ "timestamp": "2026-11-04T19:00:00Z" }));
        assert_eq!(o_que_a_fala_diz(Some(&fala(Some("rate_limit"), "x", Some("five_hour"))), depois), Some(None), "de outra abertura");
        // Parada num limite que o transcrito não diz se é da conta: a tela decide, como antes.
        assert_eq!(diz(&fala(Some("rate_limit"), "You've hit your limit · resets 5pm", None)), None);
        assert_eq!(diz(&fala(Some("rate_limit"), "Usage limit reached · resets 5pm", None)), None);
        assert_eq!(diz(&fala(Some("rate_limit"), "You've hit your Opus limit · resets 8pm", Some("seven_day_opus"))), None);
        // Sem fala, ou a parada sem carimbo (não se sabe de que abertura): a tela decide.
        assert_eq!(o_que_a_fala_diz(None, abriu), None);
        let mut sem_carimbo = fala(Some("rate_limit"), "x", Some("five_hour"));
        sem_carimbo.as_object_mut().unwrap().remove("timestamp");
        assert_eq!(o_que_a_fala_diz(Some(&sem_carimbo), abriu), None);
    }

    #[test]
    fn a_culpa_e_da_conta_que_parou() {
        let linhas = vec![
            linha(Motor::Claude, "claude:ana", &[("5h", 1_900_000_000.0)]),
            linha(Motor::Claude, "claude:bia", &[("5h", 1_900_003_600.0)]),
        ];
        let parada = |recomeca: Option<f64>, quando_ms: u64| sessoes::ParadaNoLimite {
            uuid: "u".into(),
            quando_ms,
            janela: Some("five_hour".into()),
            recomeca,
            da_conta: true,
        };
        const ANTES: f64 = 1_700_000_000.0; // antes de todos os recomeços
        const DA_ANA: Option<f64> = Some(1_900_000_060.0);
        // A aba está na Bia; a conta dela vale no Claude desde 50_000 ms.
        let culpa = |p: sessoes::ParadaNoLimite, vale_em: Option<u64>, agora: f64| {
            conta_que_parou(&p, &linhas, Some("claude:bia"), vale_em, agora)
        };
        // Depois que a conta da aba vale, a parada é dela, diga o recomeço o que disser.
        assert_eq!(culpa(parada(DA_ANA, 90_000), Some(50_000), ANTES).as_deref(), Some("claude:bia"));
        assert_eq!(culpa(parada(None, 50_000), Some(50_000), ANTES).as_deref(), Some("claude:bia"));
        // Logo depois da troca: o recomeço dito casa com o medido de uma conta, e é ela.
        assert_eq!(culpa(parada(DA_ANA, 10_000), Some(50_000), ANTES).as_deref(), Some("claude:ana"));
        // ...e sem casar, nenhuma: o pedido pode ter saído pela conta de antes.
        assert_eq!(culpa(parada(None, 49_999), Some(50_000), ANTES), None);
        assert_eq!(culpa(parada(Some(1_800_000_000.0), 10_000), Some(50_000), ANTES), None);
        // Sem armazém de aba (a conta muda com o login global): o recomeço, senão a da aba.
        assert_eq!(culpa(parada(DA_ANA, 90_000), None, ANTES).as_deref(), Some("claude:ana"));
        assert_eq!(culpa(parada(None, 90_000), None, ANTES).as_deref(), Some("claude:bia"));
        // A janela recusada já recomeçou: a parada acabou, de nenhuma.
        assert_eq!(culpa(parada(DA_ANA, 90_000), None, 1_900_000_060.0), None);
        assert_eq!(culpa(parada(DA_ANA, 90_000), Some(50_000), 1_900_000_061.0), None);
        assert_eq!(culpa(parada(DA_ANA, 90_000), None, 1_900_000_059.0).as_deref(), Some("claude:ana"));
    }
}
