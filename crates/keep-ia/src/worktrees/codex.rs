//! A conversa do Codex é dona das worktrees que o rollout dela prova ter
//! criado, pela mesma regra do nascimento e na mesma exclusividade que as do
//! Claude.
//!
//! Conferido com sessões reais do codex 0.157.0 (fixture em
//! `tests/worktrees_dados/`): no code mode a chamada é um `custom_tool_call`
//! "exec" (célula JS com `tools.exec_command`) gravado QUANDO COMEÇA, a saída
//! (`custom_tool_call_output`, "Wall time 0.2 seconds") quando termina, e o
//! nascimento da worktree cai entre as duas linhas. As outras formas também
//! valem: `function_call`/`exec_command`, o "Wall time: X seconds" que puxa o
//! início para trás quando a chamada só é gravada no fim, o processo que
//! segue vivo entre chamadas (`session_id` + `write_stdin`) e o `.jsonl.zst`
//! dos rollouts antigos. Rollout que não se lê (corrompido, ou uma thread
//! "paginated", cujo histórico só está no SQLite) é dúvida: impede o dono de
//! qualquer outra conversa naquele nascimento e nunca é dono.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, Read};
use std::path::Path;

use serde_json::{Value, json};

use super::ambiente::Ambiente;
use super::pastas::{fora_do_disco, local, mtime};
use super::prazo::{Estourou, Prazo};
use super::repos::{Worktree, chave, nomes};
use super::texto::{
    como_python, escreve_em_sessao, exec_vivo, iso, pai_da_thread, textos_juntos,
    thread_do_rollout, ts_da_linha, wall_time,
};
use super::{FOLGA, RODANDO_MAX};
use super::indice::nivel;

const CHAMADAS: [&str; 3] = ["function_call", "custom_tool_call", "local_shell_call"];
const SAIDAS: [&str; 2] = ["function_call_output", "custom_tool_call_output"];

/// Os rollouts: `<codex>/sessions/AAAA/MM/DD/rollout-<data>-<thread>.jsonl[.zst]`.
pub fn rollouts(amb: &Ambiente) -> Vec<(String, String)> {
    let base = amb.codex.join("sessions");
    let mut saida = Vec::new();
    if fora_do_disco(&base.to_string_lossy()) || !base.is_dir() {
        return saida;
    }
    fn anda(d: &Path, saida: &mut Vec<(String, String)>) {
        let Ok(dir) = std::fs::read_dir(d) else { return };
        let mut ents: Vec<_> = dir.flatten().collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                anda(&e.path(), saida);
            } else if let Some(tid) = thread_do_rollout(&e.file_name().to_string_lossy()) {
                saida.push((e.path().to_string_lossy().into_owned(), tid.to_string()));
            }
        }
    }
    anda(&base, &mut saida);
    saida
}

/// O conteúdo de um rollout (o `.zst`, descomprimido; vários quadros em
/// sequência valem).
fn abre(p: &str) -> std::io::Result<Box<dyn BufRead>> {
    let bytes = std::fs::read(p)?;
    if !p.ends_with(".zst") {
        return Ok(Box::new(std::io::Cursor::new(bytes)));
    }
    let mut fonte: &[u8] = &bytes;
    let mut saida = Vec::new();
    while !fonte.is_empty() {
        let antes = fonte.len();
        let mut d = ruzstd::decoding::StreamingDecoder::new(&mut fonte)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        d.read_to_end(&mut saida)?;
        drop(d);
        if fonte.len() >= antes {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "quadro zstd sem fim"));
        }
    }
    Ok(Box::new(std::io::Cursor::new(saida)))
}

/// As threads com rollout que se abre (o nome do arquivo diz a thread).
pub fn threads_legiveis(amb: &Ambiente) -> HashSet<String> {
    rollouts(amb)
        .into_iter()
        .filter(|(p, _)| abre(p).is_ok())
        .map(|(_, tid)| tid)
        .collect()
}

/// As strings de `arguments`/`input`/`output`/`action` (string com JSON
/// dentro vira as strings de dentro, uma por linha).
fn texto_codex(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.starts_with('{') || t.starts_with('[') {
                if let Ok(j) = serde_json::from_str::<Value>(t) {
                    return textos_juntos(&j);
                }
            }
            s.clone()
        }
        Some(outro) => textos_juntos(outro),
    }
}

/// O processo do exec ainda vivo (a sessão dele) e se ele saiu, lidos SÓ do
/// envelope do exec, nunca do que o comando imprimiu: um comando que imprima
/// "Process running with session ID 5" não pode deixar a janela dele aberta.
/// Clássico: o cabeçalho antes de "Output:". Code mode: o item de texto que é
/// o JSON do exec (`session_id` enquanto vive, `exit_code` ao sair).
pub fn estado_do_exec(output: Option<&Value>) -> (Option<String>, bool) {
    let (mut viva, mut saiu) = (None, false);
    match output {
        Some(Value::String(s)) => {
            let cab = s.split("\nOutput:").next().unwrap_or("");
            viva = exec_vivo(cab);
            saiu = cab.contains("Process exited with code");
        }
        Some(Value::Array(itens)) => {
            for it in itens {
                let Some(tx) = it.get("text").and_then(Value::as_str) else { continue };
                if !tx.trim_start().starts_with('{') {
                    continue;
                }
                let Ok(env) = serde_json::from_str::<Value>(tx) else { continue };
                let Some(env) = env.as_object() else { continue };
                if !env.contains_key("wall_time_seconds") {
                    continue;
                }
                if env.contains_key("exit_code") {
                    saiu = true;
                } else if let Some(n) = env.get("session_id").filter(|v| v.is_i64() || v.is_u64()) {
                    viva = Some(n.to_string());
                }
            }
        }
        _ => {}
    }
    (viva, saiu)
}

/// Uma chamada de ferramenta do Codex e a janela dela.
#[derive(Clone, Debug)]
pub struct Janela {
    pub id: String,
    pub nome: String,
    pub i: f64,
    pub fim: f64,
    pub entrada: String,
    pub saida: String,
    pub aberta: bool,
}

/// O que um rollout diz.
pub struct Rollout {
    pub thread: Option<String>,
    pub pai: Option<String>,
    pub pri: Option<f64>,
    pub local: bool,
    pub janelas: Vec<Janela>,
}

pub enum Falha {
    Estourou,
    Ilegivel,
}

struct Pendente {
    id: String,
    nome: String,
    i: f64,
    f: Option<f64>,
    entrada: String,
    saida: String,
    escreve: Option<String>,
    vivo: bool,
}

/// Janela de cada chamada = da linha dela até a da saída. "Wall time" na
/// saída puxa o início para trás; saída com o processo vivo deixa a chamada
/// aberta até um `write_stdin` daquela sessão dizer que ele saiu (a saída do
/// processo entra no texto dela); sem fim, vai até a última linha do arquivo,
/// no máximo `RODANDO_MAX` depois do início (processo ainda vivo: até a
/// última linha).
pub fn janelas(p: &str, agora: f64, prazo: &Prazo) -> Result<Rollout, Falha> {
    let leitor = abre(p).map_err(|_| Falha::Ilegivel)?;
    let (mut thread, mut pai, mut pri, mut ult) = (None::<String>, None::<String>, None::<f64>, None::<f64>);
    let mut loc = true;
    let mut chamadas: HashMap<String, Pendente> = HashMap::new();
    let mut ordem: Vec<String> = Vec::new();
    let mut sessoes: HashMap<String, String> = HashMap::new();
    for (n, linha) in leitor.split(b'\n').enumerate() {
        if n & 2047 == 0 {
            prazo.checa().map_err(|Estourou| Falha::Estourou)?;
        }
        let linha = linha.map_err(|_| Falha::Ilegivel)?;
        let t = if super::texto::contem(&linha, b"\"timestamp\":\"") { ts_da_linha(&linha) } else { None };
        if let Some(t) = t {
            ult = Some(ult.map_or(t, |u| u.max(t)));
            pri.get_or_insert(t);
        }
        if thread.is_none() && super::texto::contem(&linha, b"\"session_meta\"") {
            let pl = serde_json::from_slice::<Value>(&linha)
                .ok()
                .and_then(|o| o.get("payload").cloned())
                .unwrap_or(Value::Null);
            if pl.is_object() {
                thread = pl.get("id").and_then(Value::as_str).map(str::to_string);
                pai = pl.get("source").and_then(pai_da_thread);
                loc = local(pl.get("cwd").and_then(Value::as_str).unwrap_or(""));
            }
            continue;
        }
        if !super::texto::contem(&linha, b"\"response_item\"") || !super::texto::contem(&linha, b"call") {
            continue;
        }
        let Ok(o) = serde_json::from_slice::<Value>(&linha) else { continue };
        let Some(pl) = o.get("payload").filter(|p| p.is_object()) else { continue };
        let ty = pl.get("type").and_then(Value::as_str).unwrap_or("");
        let Some(t) = t else { continue };
        if CHAMADAS.contains(&ty) {
            let Some(cid) = pl.get("call_id").or_else(|| pl.get("id")).and_then(Value::as_str) else { continue };
            let entrada: Vec<String> =
                ["arguments", "input", "action"].iter().map(|k| texto_codex(pl.get(*k))).filter(|x| !x.is_empty()).collect();
            let entrada = entrada.join("\n");
            let nome = pl.get("name").and_then(Value::as_str);
            let escreve = if nome == Some("write_stdin") {
                // `str(json.loads(arguments or "{}").get("session_id"))`, como
                // o kit: sem o campo, a sessão "None", que nada casa.
                let args = match pl.get("arguments") {
                    None | Some(Value::Null) => Some("{}"),
                    Some(Value::String(a)) if a.is_empty() => Some("{}"),
                    Some(Value::String(a)) => Some(a.as_str()),
                    Some(Value::Object(m)) if m.is_empty() => Some("{}"),
                    Some(_) => None,
                };
                args.and_then(|a| serde_json::from_str::<Value>(a).ok())
                    .filter(Value::is_object)
                    .map(|a| como_python(a.get("session_id")))
            } else {
                escreve_em_sessao(&entrada)
            };
            if !chamadas.contains_key(cid) {
                ordem.push(cid.to_string());
            }
            chamadas.insert(
                cid.to_string(),
                Pendente {
                    id: cid.to_string(),
                    nome: nome.unwrap_or(ty).to_string(),
                    i: t,
                    f: None,
                    entrada,
                    saida: String::new(),
                    escreve,
                    vivo: false,
                },
            );
        } else if SAIDAS.contains(&ty) {
            let Some(cid) = pl.get("call_id").and_then(Value::as_str) else { continue };
            let Some(c) = chamadas.get_mut(cid) else { continue };
            let txt = texto_codex(pl.get("output"));
            c.saida.push('\n');
            c.saida.push_str(&txt);
            if let Some(w) = wall_time(&txt) {
                c.i = c.i.min(t - w);
            }
            c.f = Some(t);
            let (viva, saiu) = estado_do_exec(pl.get("output"));
            if let Some(escreve) = c.escreve.clone() {
                // A saída é do processo que o exec_command subiu.
                if let Some(alvo) = sessoes.get(&escreve).cloned().and_then(|a| chamadas.get_mut(&a)) {
                    alvo.saida.push('\n');
                    alvo.saida.push_str(&txt);
                    if saiu {
                        alvo.f = Some(t);
                        alvo.vivo = false;
                    }
                }
            } else if let Some(viva) = viva {
                // O processo continua: janela aberta.
                c.f = None;
                c.vivo = true;
                sessoes.insert(viva, c.id.clone());
            }
        }
    }
    let janelas = ordem
        .into_iter()
        .filter_map(|cid| chamadas.remove(&cid))
        .map(|c| {
            let (fim, aberta) = match c.f {
                Some(f) => (f, false),
                None if c.vivo => (c.i.max(ult.unwrap_or(c.i)), false),
                None => (c.i.max(ult.unwrap_or(c.i).min(c.i + RODANDO_MAX)), agora - c.i <= RODANDO_MAX),
            };
            Janela { id: c.id, nome: c.nome, i: c.i, fim, entrada: c.entrada, saida: c.saida, aberta }
        })
        .collect();
    Ok(Rollout { thread, pai, pri, local: loc, janelas })
}

/// A prova de uma conversa do Codex num nascimento: o nível, a prova e se a
/// chamada pode estar rodando agora.
pub type Evidencia = (Option<char>, Value, bool);

/// `{chave da worktree: {thread de cima: evidência}}`: as chamadas do Codex
/// em volta de cada nascimento. Subagente conta pela conversa de cima.
/// Rollout ilegível que pode ter estado ativo no nascimento conta como prova
/// A (não dá para descartar).
pub fn evidencias(
    amb: &Ambiente,
    wts: &[&Worktree],
    agora: f64,
    prazo: &Prazo,
) -> Result<HashMap<String, BTreeMap<String, Evidencia>>, Estourou> {
    if wts.is_empty() {
        return Ok(HashMap::new());
    }
    let lo = wts.iter().map(|w| w.nasceu).fold(f64::INFINITY, f64::min) - FOLGA;
    let hi = wts.iter().map(|w| w.nasceu).fold(f64::NEG_INFINITY, f64::max) + FOLGA;
    let mut lidos: Vec<(String, Option<f64>, Option<Vec<Janela>>)> = Vec::new();
    let mut pais: HashMap<String, String> = HashMap::new();
    for (p, tid_nome) in rollouts(amb) {
        prazo.checa()?;
        match std::fs::metadata(&p) {
            // Nada escrito depois do primeiro nascimento: não estava ativo.
            Ok(m) if mtime(&m) < lo => continue,
            Err(_) => continue,
            Ok(_) => {}
        }
        let r = match janelas(&p, agora, prazo) {
            Ok(r) => r,
            Err(Falha::Estourou) => return Err(Estourou),
            Err(Falha::Ilegivel) => {
                lidos.push((tid_nome, None, None));
                continue;
            }
        };
        if !r.local || r.pri.is_some_and(|pri| pri > hi) {
            continue;
        }
        let tid = r.thread.unwrap_or(tid_nome);
        if let Some(pai) = r.pai {
            pais.insert(tid.clone(), pai);
        }
        lidos.push((tid, r.pri, Some(r.janelas)));
    }
    if lidos.is_empty() {
        return Ok(HashMap::new());
    }
    let raiz = |t: &str| {
        let mut t = t.to_string();
        for _ in 0..20 {
            match pais.get(&t) {
                Some(p) => t = p.clone(),
                None => break,
            }
        }
        t
    };
    let mut saida = HashMap::new();
    for w in wts {
        let t_nasc = w.nasceu;
        let ns = nomes(w);
        let mut ev: BTreeMap<String, Evidencia> = BTreeMap::new();
        for (tid, pri, jan) in &lidos {
            let r = raiz(tid);
            let Some(jan) = jan else {
                ev.insert(r, (Some('A'), json!({"ferramenta": "codex", "ilegivel": true, "thread": tid}), false));
                continue;
            };
            if pri.is_some_and(|p| p > t_nasc + FOLGA) {
                continue;
            }
            for c in jan {
                if c.i - FOLGA > t_nasc || c.fim + FOLGA < t_nasc {
                    continue;
                }
                let nv = nivel(&c.entrada, &c.saida, &ns, &w.caminho, false);
                let (mut nv0, mut pv0, ab0) = ev.get(&r).cloned().unwrap_or((None, Value::Null, false));
                if let Some(nv) = nv {
                    if nv0.is_none_or(|v| nv < v) {
                        nv0 = Some(nv);
                        pv0 = json!({
                            "ferramenta": format!("codex:{}", c.nome),
                            "call_id": c.id,
                            "thread": tid,
                            "inicio": iso(c.i),
                            "fim": iso(c.fim),
                        });
                    }
                }
                ev.insert(r.clone(), (nv0, pv0, ab0 || c.aberta));
            }
        }
        ev.retain(|_, v| v.0.is_some() || v.2);
        if !ev.is_empty() {
            saida.insert(chave(w), ev);
        }
    }
    Ok(saida)
}
