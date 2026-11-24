//! A aba geral de cada workspace (pedido de 24/11/2026): a do topo da lista, com o nome do workspace, por onde a
//! pessoa conduz as demais abas dele, sem deixar de poder falar direto com qualquer uma.
//!
//! A rodada do núcleo (`keep_restauro::geral`, a cada 15 s com o `keep continuar`) cria a aba geral, guarda qual é
//! (`geral.json`, ao lado das adormecidas e no mesmo formato, para o app pô-la no topo com destaque) e anota em
//! `geral-eventos.jsonl` o que a pessoa pediu em cada aba e o que cada uma terminou. Aqui moram esses arquivos e o que
//! o gancho `UserPromptSubmit` do Keep (`keep ia conta-da-aba --gancho`) entrega à aba geral: o papel dela e as
//! novidades das outras abas desde a última mensagem.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::caminhos;

pub const ARQUIVO: &str = "geral.json";
pub const EVENTOS: &str = "geral-eventos.jsonl";
/// Passando disto, o arquivo de eventos fica só com o fim.
const TETO_EVENTOS: u64 = 1 << 20;
const MANTIDOS: usize = 2000;
/// O que vai à aba geral de uma vez, no máximo.
const NOVIDADES: usize = 40;
/// Na primeira mensagem de uma conversa, as novidades deste tanto para trás.
const PRIMEIRA_JANELA_MS: u64 = 24 * 3600 * 1000;

/// A aba geral de um workspace. `aba` 0: guardada de outro keepd, à espera de a conversa voltar.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geral {
    pub workspace: String,
    pub aba: u32,
    /// A conversa do Claude nela; vazia enquanto a aba recém-criada não a disse.
    #[serde(default)]
    pub conversa: String,
    #[serde(default)]
    pub desde_ms: u64,
    /// O nome com que a pessoa vê o workspace no app, quando deu outro; o da aba geral é ele.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nome: String,
}

/// Uma novidade de uma aba do workspace: `pedido` (o que a pessoa escreveu nela) ou `fim` (o que ela respondeu ao
/// terminar).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evento {
    pub quando_ms: u64,
    pub workspace: String,
    pub aba: u32,
    #[serde(default)]
    pub nome: String,
    pub tipo: String,
    pub texto: String,
}

#[derive(Deserialize)]
struct Lido {
    #[serde(default)]
    abas: Vec<Geral>,
}

fn agora_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// As abas gerais que a rodada gravou.
pub fn gerais() -> Vec<Geral> {
    gerais_em(&caminhos::restauro())
}

fn gerais_em(pasta: &Path) -> Vec<Geral> {
    std::fs::read(pasta.join(ARQUIVO))
        .ok()
        .and_then(|b| serde_json::from_slice::<Lido>(&b).ok())
        .map(|l| l.abas)
        .unwrap_or_default()
}

/// Anota as novidades; o arquivo grande demais fica com as últimas.
pub fn anotar(eventos: &[Evento]) {
    anotar_em(&caminhos::restauro(), eventos)
}

fn anotar_em(pasta: &Path, eventos: &[Evento]) {
    if eventos.is_empty() {
        return;
    }
    let caminho = pasta.join(EVENTOS);
    let _ = std::fs::create_dir_all(pasta);
    if std::fs::metadata(&caminho).is_ok_and(|m| m.len() > TETO_EVENTOS) {
        let texto = std::fs::read_to_string(&caminho).unwrap_or_default();
        let linhas: Vec<&str> = texto.lines().collect();
        let fim = linhas[linhas.len().saturating_sub(MANTIDOS)..].join("\n") + "\n";
        let _ = caminhos::grava_privado(&caminho, fim.as_bytes());
    }
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&caminho) else { return };
    let texto: String = eventos.iter().filter_map(|e| serde_json::to_string(e).ok()).map(|l| l + "\n").collect();
    let _ = f.write_all(texto.as_bytes());
}

/// As novidades do workspace `ws` depois de `desde_ms`, as mais novas no fim (até `NOVIDADES`).
pub fn eventos(ws: &str, desde_ms: u64) -> Vec<Evento> {
    eventos_em(&caminhos::restauro(), ws, desde_ms)
}

fn eventos_em(pasta: &Path, ws: &str, desde_ms: u64) -> Vec<Evento> {
    let texto = std::fs::read_to_string(pasta.join(EVENTOS)).unwrap_or_default();
    let todos: Vec<Evento> = texto
        .lines()
        .filter_map(|l| serde_json::from_str::<Evento>(l).ok())
        .filter(|e| e.workspace == ws && e.quando_ms > desde_ms)
        .collect();
    todos[todos.len().saturating_sub(NOVIDADES)..].to_vec()
}

/// Como a aba geral chama o workspace: o nome que a pessoa vê e, quando é outro, o do Keep, que os comandos usam.
fn chamado(ws: &str, nome: &str) -> String {
    if nome.is_empty() || nome == ws { format!("«{ws}»") } else { format!("«{nome}» (no Keep, \"{ws}\")") }
}

/// O papel da aba geral, na primeira mensagem da conversa.
pub fn papel(ws: &str, nome: &str) -> String {
    let chamado = chamado(ws, nome);
    format!(
        "Você é a aba geral do workspace {chamado} no Keep: a do topo da lista, com o nome do workspace. A pessoa conduz \
         por aqui o trabalho das demais abas deste workspace e pode também falar direto com qualquer uma; o Keep conta \
         a você, a cada mensagem, o que ela pediu nas outras abas e o que cada uma terminou.\n\
         - As abas do workspace: `keep geral abas --workspace \"{ws}\"`. A tela de uma: `keep ia aba tela --ws=\"{ws}\" --aba=N`.\n\
         - Trabalho novo vai para uma aba nova, aberta por você: `keep nova --workspace \"{ws}\" --pasta <pasta do projeto> \
         --nome \"<rótulo curto>\" --pedido \"<o pedido completo>\"`.\n\
         - Mensagem a uma aba: `keep geral mandar --workspace \"{ws}\" --aba N --texto \"…\"` (entra só com a aba parada e a caixa vazia).\n\
         - Aba resolvida e sem pendências: feche com `keep geral fechar --workspace \"{ws}\" --aba N`. Antes, confira que nada \
         ficou por fazer nem espera volta (resposta de terceiro, teste ou conferência marcada): a que só espera terceiro \
         fica aberta ou hiberna (`keep restauro adormecer --agora \"{ws}\" --aba N`). Na dúvida, não feche.\n\
         - Quando o Keep avisar que abas terminaram, verifique cada uma e feche as resolvidas."
    )
}

fn lembrete(ws: &str, nome: &str) -> String {
    format!(
        "Aba geral do workspace {} no Keep (comandos: keep geral abas | mandar | fechar --workspace \"{ws}\"; keep nova --workspace \"{ws}\").",
        chamado(ws, nome)
    )
}

fn hora(ms: u64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|t| t.format("%d/%m %H:%M").to_string()).unwrap_or_default()
}

/// Uma linha por novidade.
pub fn novidades(eventos: &[Evento]) -> String {
    eventos
        .iter()
        .map(|e| {
            let aba = if e.nome.is_empty() { format!("aba {}", e.aba) } else { format!("aba {} «{}»", e.aba, e.nome) };
            let oque = if e.tipo == "fim" { "terminou" } else { "a pessoa pediu" };
            format!("- {} {aba}, {oque}: {}", hora(e.quando_ms), e.texto)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn arquivo_lido(estado: &Path, sessao: &str) -> Option<PathBuf> {
    let limpo: String = sessao.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    (!limpo.is_empty()).then(|| estado.join("geral-lido").join(limpo))
}

/// O que a aba geral do workspace `ws` recebe na mensagem da conversa `sessao`; `None` em qualquer outra aba.
pub fn contexto(ws: &str, sessao: &str) -> Option<String> {
    contexto_em(&caminhos::restauro(), &caminhos::estado(), ws, sessao)
}

fn contexto_em(pasta: &Path, estado: &Path, ws: &str, sessao: &str) -> Option<String> {
    if ws.is_empty() || sessao.is_empty() {
        return None;
    }
    let g = gerais_em(pasta).into_iter().find(|g| g.workspace == ws && g.conversa == sessao)?;
    let marca = arquivo_lido(estado, sessao)?;
    let lido = std::fs::read_to_string(&marca).ok().and_then(|t| t.trim().parse::<u64>().ok());
    let agora = agora_ms();
    let novas = eventos_em(pasta, ws, lido.unwrap_or(agora.saturating_sub(PRIMEIRA_JANELA_MS)));
    if let Some(d) = marca.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&marca, novas.last().map_or(lido.unwrap_or(agora), |e| e.quando_ms).to_string());
    let mut texto = if lido.is_none() { papel(ws, &g.nome) } else { lembrete(ws, &g.nome) };
    if !novas.is_empty() {
        texto.push_str("\nNas outras abas, desde a sua última mensagem:\n");
        texto.push_str(&novidades(&novas));
    }
    Some(texto)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ev(ms: u64, ws: &str, tipo: &str, texto: &str) -> Evento {
        Evento { quando_ms: ms, workspace: ws.into(), aba: 7, nome: "Compras".into(), tipo: tipo.into(), texto: texto.into() }
    }

    #[test]
    fn a_aba_geral_recebe_o_papel_uma_vez_e_depois_so_as_novidades() {
        let d = tempfile::tempdir().unwrap();
        let (r, ia) = (d.path().join("r"), d.path().join("ia"));
        let contexto = |ws: &str, s: &str| contexto_em(&r, &ia, ws, s);
        let anotar = |e: &[Evento]| anotar_em(&r, e);
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(
            r.join(ARQUIVO),
            r#"{"keepd":{"pid":1,"inicio_ms":2},"abas":[{"workspace":"W","aba":1,"conversa":"c-1"}]}"#,
        )
        .unwrap();
        let agora = agora_ms();
        anotar(&[ev(agora - 5000, "W", "pedido", "faça X"), ev(agora - 4000, "Outro", "pedido", "pedido do outro workspace"), ev(agora - 3000, "W", "fim", "feito")]);

        // outra aba, outro workspace: nada
        assert_eq!(contexto("W", "c-2"), None);
        assert_eq!(contexto("Outro", "c-1"), None);

        let primeira = contexto("W", "c-1").unwrap();
        assert!(primeira.starts_with("Você é a aba geral do workspace «W»"), "{primeira}");
        assert!(primeira.contains("aba 7 «Compras», a pessoa pediu: faça X"), "{primeira}");
        assert!(primeira.contains("aba 7 «Compras», terminou: feito"), "{primeira}");
        assert!(!primeira.contains("pedido do outro workspace"), "{primeira}");

        // a seguinte: o lembrete, sem repetir o que já foi
        let segunda = contexto("W", "c-1").unwrap();
        assert!(segunda.starts_with("Aba geral do workspace «W»") && !segunda.contains("faça X"), "{segunda}");
        anotar(&[ev(agora_ms() + 10, "W", "pedido", "agora Y")]);
        let terceira = contexto("W", "c-1").unwrap();
        assert!(terceira.contains("agora Y") && !terceira.contains("faça X"), "{terceira}");
    }
}
