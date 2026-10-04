//! O índice dos transcritos do Claude Code: de cada chamada executora, quando
//! começou e quando acabou, e onde estão as linhas dela — lido aos pedaços,
//! só o que cada arquivo cresceu desde a última vez.
//!
//! A janela de uma chamada é [tool_use, tool_result]. O Bash em segundo
//! plano vai até a notificação de tarefa; uma chamada interrompida (mensagem
//! da pessoa sem resultado) acaba nessa mensagem; um resultado que chega
//! depois reabre e estica a janela.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ambiente::Ambiente;
use super::prazo::{Estourou, Prazo};
use super::texto::{
    cita, contem, e_uuid, fala_de_worktree, molde, saida_em_segundo_plano, texto_do_resultado, textos_juntos,
    tool_use_ids_notificados, ts, ts_da_linha,
};
use super::{BLOCO, EXECUTORAS, FOLGA, RETENCAO, RODANDO_MAX, VERSAO_DO_ESTADO};
use super::pastas::{identidade, local, mtime, outra_maquina};

/// Uma chamada executora (Bash, EnterWorktree, Agent…).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chamada {
    pub id: String,
    /// A ferramenta.
    pub n: String,
    /// Início e fim, em segundos unix.
    pub i: Option<f64>,
    pub f: Option<f64>,
    /// Em segundo plano: o fim de verdade é a notificação.
    pub bg: bool,
    /// Quando o resultado de segundo plano chegou.
    pub tr: Option<f64>,
    /// O arquivo de saída do segundo plano.
    pub sd: Option<String>,
    /// Onde está a linha do `tool_use`.
    pub ou: u64,
    /// Onde está a linha do resultado.
    #[serde(rename = "or")]
    pub or_: Option<u64>,
    /// Feita nesta máquina (a pasta dela não é de outra).
    pub loc: bool,
}

/// O que se sabe de um transcrito.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Arquivo {
    pub ino: u64,
    /// Até onde foi lido.
    pub off: u64,
    pub sid: String,
    /// O primeiro e o último horário.
    pub pri: Option<f64>,
    pub ult: Option<f64>,
    /// As chamadas abertas e as fechadas.
    pub ab: BTreeMap<String, Chamada>,
    pub fe: BTreeMap<String, Chamada>,
    /// As pendentes interrompidas por uma mensagem sem horário: acabam na
    /// próxima linha datada.
    pub orf: Vec<String>,
}

impl Arquivo {
    fn novo(sid: &str, ino: u64) -> Arquivo {
        Arquivo { ino, sid: sid.to_string(), ..Arquivo::default() }
    }
}

/// O índice: um `Arquivo` por transcrito.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Indice {
    pub versao: u32,
    pub arquivos: BTreeMap<String, Arquivo>,
    pub criado: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atualizado: Option<f64>,
}

/// Por que um transcrito não foi lido até o fim.
#[derive(Debug)]
pub enum Parou {
    Estourou,
    Io(std::io::Error),
}

impl From<Estourou> for Parou {
    fn from(_: Estourou) -> Parou {
        Parou::Estourou
    }
}

/// Os transcritos: `<projetos>/<pasta>/<sid>.jsonl` e os dos subagentes,
/// `<projetos>/<pasta>/<sid>/subagents/**.jsonl`, cada um com a sessão.
pub fn arquivos_transcritos(amb: &Ambiente) -> Vec<(String, String)> {
    let mut saida = Vec::new();
    let Ok(dir) = std::fs::read_dir(&amb.projetos) else { return saida };
    let mut pastas: Vec<_> = dir.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
    pastas.sort();
    for d in pastas {
        let Ok(ents) = std::fs::read_dir(&d) else { continue };
        let mut ents: Vec<_> = ents.flatten().collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            let nome = e.file_name().to_string_lossy().into_owned();
            if let Some(sid) = nome.strip_suffix(".jsonl").filter(|s| e_uuid(s)) {
                saida.push((e.path().to_string_lossy().into_owned(), sid.to_string()));
            } else if e_uuid(&nome) && e.file_type().is_ok_and(|t| t.is_dir()) {
                let sub = e.path().join("subagents");
                if sub.is_dir() {
                    let mut achados = Vec::new();
                    anda(&sub, &mut achados);
                    achados.sort();
                    saida.extend(achados.into_iter().map(|p| (p, nome.clone())));
                }
            }
        }
    }
    saida
}

fn anda(d: &Path, saida: &mut Vec<String>) {
    let Ok(dir) = std::fs::read_dir(d) else { return };
    for e in dir.flatten() {
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            anda(&e.path(), saida);
        } else if t.is_file() {
            let n = e.file_name();
            let n = n.to_string_lossy();
            if n.ends_with(".jsonl") && n != "journal.jsonl" {
                saida.push(e.path().to_string_lossy().into_owned());
            }
        }
    }
}

const VAZIO: &[Value] = &[];

/// Lê as linhas completas de `dados` (que começam no deslocamento `base` do
/// arquivo) no estado `e`. Devolve quantos bytes consumiu.
pub fn consumir(e: &mut Arquivo, dados: &[u8], base: u64, prazo: &Prazo) -> Result<usize, Estourou> {
    let fim = memchr::memrchr(b'\n', dados).map(|k| k + 1).unwrap_or(0);
    let mut orf: BTreeSet<String> = e.orf.iter().cloned().collect();
    let mut pos = 0usize;
    let mut n = 0u64;
    let mut ultima: Option<(usize, usize)> = None;
    while pos < fim {
        let nl = pos + memchr::memchr(b'\n', &dados[pos..fim]).unwrap_or(fim - pos);
        let (ini, linha) = (pos, &dados[pos..nl]);
        let off = base + pos as u64;
        pos = nl + 1;
        n += 1;
        if n & 2047 == 0 {
            prazo.checa()?;
        }
        if linha.is_empty() {
            continue;
        }
        let tem_ts = contem(linha, b"\"timestamp\":\"");
        if tem_ts {
            ultima = Some((ini, nl));
        }
        let tem_uso = contem(linha, b"\"tool_use\"");
        let tem_res = contem(linha, b"\"tool_result\"");
        let tem_not = contem(linha, b"task-notification");
        // Mensagem da pessoa com chamada pendente: interrompe a pendente.
        let tem_usuario = !e.ab.is_empty() && contem(linha, b"\"type\":\"user\"");
        if !(tem_uso || tem_res || tem_not || tem_usuario) {
            if tem_ts && (!orf.is_empty() || e.pri.is_none()) {
                if let Some(t) = ts_da_linha(linha) {
                    e.pri.get_or_insert(t);
                    fecha_orfas(e, &mut orf, t);
                }
            }
            continue;
        }
        let Ok(o) = serde_json::from_slice::<Value>(linha) else { continue };
        let Some(obj) = o.as_object() else { continue };
        let t = obj.get("timestamp").and_then(Value::as_str).and_then(ts);
        if let Some(t) = t {
            e.pri.get_or_insert(t);
            fecha_orfas(e, &mut orf, t);
        }
        if let (true, Some(t)) = (tem_not, t) {
            let txt = match obj.get("content") {
                Some(Value::String(s)) => s.clone(),
                Some(c) if !c.is_null() => c.to_string(),
                _ => match obj.get("message") {
                    Some(m) if !m.is_null() => m.to_string(),
                    _ => "\"\"".into(),
                },
            };
            for tid in tool_use_ids_notificados(&txt) {
                if e.ab.get(tid).is_some_and(|u| u.bg) {
                    let mut u = e.ab.remove(tid).unwrap();
                    u.f = Some(t);
                    e.fe.insert(tid.to_string(), u);
                }
            }
        }
        let tipo = obj.get("type").and_then(Value::as_str);
        let c: &[Value] = match obj.get("message").and_then(|m| m.get("content")) {
            Some(Value::Array(a)) => a,
            // Mensagem digitada (texto puro): interrompe o que estava pendente.
            Some(Value::String(_)) if tipo == Some("user") => VAZIO,
            _ => continue,
        };
        if tipo == Some("assistant") {
            let pasta = obj.get("cwd").and_then(Value::as_str).unwrap_or("");
            for b in c {
                if b.get("type").and_then(Value::as_str) != Some("tool_use") {
                    continue;
                }
                let nome = b.get("name").and_then(Value::as_str).unwrap_or("");
                let id = b.get("id").and_then(Value::as_str).unwrap_or("");
                if EXECUTORAS.contains(&nome) && !id.is_empty() {
                    e.ab.insert(
                        id.to_string(),
                        Chamada {
                            id: id.to_string(),
                            n: nome.to_string(),
                            i: t,
                            f: None,
                            bg: false,
                            tr: None,
                            sd: None,
                            ou: off,
                            or_: None,
                            loc: local(pasta),
                        },
                    );
                }
            }
        } else if tipo == Some("user") {
            let mut achou = false;
            for b in c {
                if b.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                achou = true;
                let Some(i) = b.get("tool_use_id").and_then(Value::as_str) else { continue };
                let aberta = e.ab.contains_key(i);
                if !aberta && !e.fe.contains_key(i) {
                    continue;
                }
                let u = if aberta { e.ab.get_mut(i) } else { e.fe.get_mut(i) }.unwrap();
                u.or_ = Some(off);
                let tur = obj.get("toolUseResult");
                let fundo = tur
                    .and_then(|r| r.get("backgroundTaskId"))
                    .is_some_and(|v| !v.is_null() && v != &Value::Bool(false) && v.as_str() != Some(""));
                if fundo {
                    // O fim de verdade é a notificação.
                    u.bg = true;
                    u.tr = t;
                    u.sd = saida_em_segundo_plano(&texto_do_resultado(b)).map(str::to_string);
                    if !aberta {
                        let mut u = e.fe.remove(i).unwrap();
                        u.f = None;
                        e.ab.insert(i.to_string(), u);
                    }
                } else {
                    if t.is_some() {
                        u.f = t;
                    }
                    if aberta {
                        let u = e.ab.remove(i).unwrap();
                        e.fe.insert(i.to_string(), u);
                    }
                }
            }
            if !achou {
                // Mensagem da pessoa sem resultado: o que estava pendente foi
                // interrompido (Esc) e acaba nela; sem horário na linha,
                // acaba na próxima linha datada.
                let pend: Vec<String> =
                    e.ab.iter().filter(|(_, u)| !u.bg && u.f.is_none()).map(|(k, _)| k.clone()).collect();
                match t {
                    Some(t) => {
                        for k in pend {
                            let mut u = e.ab.remove(&k).unwrap();
                            u.f = Some(t);
                            e.fe.insert(k, u);
                        }
                    }
                    None => orf = pend.into_iter().collect(),
                }
            }
        }
    }
    if let Some(t) = ultima.and_then(|(a, b)| ts_da_linha(&dados[a..b])) {
        e.ult = Some(t);
    }
    e.orf = orf.into_iter().collect();
    Ok(fim)
}

fn fecha_orfas(e: &mut Arquivo, orf: &mut BTreeSet<String>, t: f64) {
    for k in std::mem::take(orf) {
        if e.ab.get(&k).is_some_and(|u| u.f.is_none() && !u.bg) {
            let mut u = e.ab.remove(&k).unwrap();
            u.f = Some(t);
            e.fe.insert(k, u);
        }
    }
}

/// Lê o arquivo a partir de `e.off` (só linhas completas). `parar` diz,
/// depois de cada bloco, se o resto não interessa.
pub fn ler_arquivo(
    e: &mut Arquivo,
    caminho: &str,
    prazo: &Prazo,
    parar: Option<&dyn Fn(&Arquivo) -> bool>,
) -> Result<(), Parou> {
    let mut f = std::fs::File::open(caminho).map_err(Parou::Io)?;
    f.seek(SeekFrom::Start(e.off)).map_err(Parou::Io)?;
    let mut resto: Vec<u8> = Vec::new();
    let mut base = e.off;
    loop {
        let mut bloco = Vec::with_capacity(BLOCO);
        (&mut f).take(BLOCO as u64).read_to_end(&mut bloco).map_err(Parou::Io)?;
        if bloco.is_empty() {
            break;
        }
        let dados = if resto.is_empty() {
            bloco
        } else {
            let mut d = std::mem::take(&mut resto);
            d.extend_from_slice(&bloco);
            d
        };
        let k = consumir(e, &dados, base, prazo)?;
        base += k as u64;
        resto = dados[k..].to_vec();
        e.off = base;
        if parar.is_some_and(|p| p(e)) {
            break;
        }
    }
    e.off = base;
    Ok(())
}

/// Tira do índice o que não pode mais estar ativo num nascimento a partir
/// de `limite`: as fechadas antes dele, e as pendentes (fora do segundo
/// plano) cuja janela acaba no máximo `RODANDO_MAX` depois do início.
pub fn podar(e: &mut Arquivo, limite: f64) {
    e.fe.retain(|_, u| !u.f.is_some_and(|f| f < limite));
    e.ab.retain(|_, u| u.bg || !u.i.is_some_and(|i| i + RODANDO_MAX < limite));
}

fn arquivo_do_indice(amb: &Ambiente) -> std::path::PathBuf {
    amb.estado.join(super::ARQ_INDICE)
}

pub fn carrega(amb: &Ambiente) -> Indice {
    std::fs::read(arquivo_do_indice(amb))
        .ok()
        .and_then(|b| serde_json::from_slice::<Indice>(&b).ok())
        .filter(|i| i.versao == VERSAO_DO_ESTADO)
        .unwrap_or(Indice { versao: VERSAO_DO_ESTADO, ..Indice::default() })
}

pub fn grava(amb: &Ambiente, ind: &Indice) -> std::io::Result<()> {
    super::estado::grava_json(&arquivo_do_indice(amb), &serde_json::to_vec(ind)?)
}

/// Lê o que cada transcrito cresceu. Arquivo novo parado há mais de
/// `RETENCAO` (e de sessão morta) não é lido: nenhuma chamada dele pode
/// estar ativa num nascimento que o índice cobre. Devolve os bytes lidos.
pub fn atualizar(
    amb: &Ambiente,
    ind: &mut Indice,
    agora: f64,
    vivas: &HashSet<String>,
    prazo: &Prazo,
) -> Result<u64, Estourou> {
    if ind.criado.is_none() {
        ind.criado = Some(agora);
    }
    let mut vistos = HashSet::new();
    let mut lidos = 0u64;
    for (p, sid) in arquivos_transcritos(amb) {
        vistos.insert(p.clone());
        let Ok(st) = std::fs::metadata(&p) else { continue };
        let (ino, tam) = (identidade(&st), st.len());
        if ind.arquivos.get(&p).is_some_and(|e| e.ino == ino && e.off == tam) {
            continue;
        }
        let novo = match ind.arquivos.get(&p) {
            None => true,
            Some(e) => e.ino != ino || tam < e.off,
        };
        if novo {
            let mut e = Arquivo::novo(&sid, ino);
            if mtime(&st) < agora - RETENCAO && !vivas.contains(&sid) {
                e.off = tam;
                ind.arquivos.insert(p, e);
                continue;
            }
            ind.arquivos.insert(p.clone(), e);
        }
        let e = ind.arquivos.get_mut(&p).unwrap();
        let antes = e.off;
        match ler_arquivo(e, &p, prazo, None) {
            Ok(()) => {}
            Err(Parou::Estourou) => return Err(Estourou),
            // Arquivo que sumiu ou não se lê agora: fica para a próxima.
            Err(Parou::Io(_)) => continue,
        }
        lidos += e.off.saturating_sub(antes);
        podar(e, agora - RETENCAO);
        prazo.checa()?;
    }
    ind.arquivos.retain(|p, _| vistos.contains(p));
    ind.atualizado = Some(agora);
    Ok(lidos)
}

/// O primeiro horário de um transcrito (das primeiras 400 linhas).
fn primeiro_ts(p: &str) -> Option<f64> {
    let f = std::fs::File::open(p).ok()?;
    let mut r = BufReader::new(f);
    let mut linha = Vec::new();
    for _ in 0..=401 {
        linha.clear();
        if r.read_until(b'\n', &mut linha).ok()? == 0 {
            return None;
        }
        if contem(&linha, b"\"timestamp\":\"") {
            if let Some(t) = ts_da_linha(&linha) {
                return Some(t);
            }
        }
    }
    None
}

/// Um índice só com as chamadas que podem estar ativas em algum dos
/// nascimentos `ts` (os que o índice incremental não cobre): filtro pela
/// data do arquivo e pelo primeiro horário (sessão viva não se filtra pela
/// data), e parada antecipada.
pub fn varredura_fria(
    amb: &Ambiente,
    nascimentos: &[f64],
    vivas: &HashSet<String>,
    prazo: &Prazo,
    so_sessoes: Option<&HashSet<String>>,
) -> Result<Indice, Estourou> {
    let lo = nascimentos.iter().copied().fold(f64::INFINITY, f64::min) - FOLGA;
    let hi = nascimentos.iter().copied().fold(f64::NEG_INFINITY, f64::max) + FOLGA;
    let mut arquivos = BTreeMap::new();
    for (p, sid) in arquivos_transcritos(amb) {
        if so_sessoes.is_some_and(|s| !s.contains(&sid)) {
            continue;
        }
        prazo.checa()?;
        let Ok(st) = std::fs::metadata(&p) else { continue };
        if !vivas.contains(&sid) && mtime(&st) < lo {
            continue;
        }
        if primeiro_ts(&p).is_some_and(|pri| pri > hi) {
            continue;
        }
        let mut e = Arquivo::novo(&sid, identidade(&st));
        let parar = |e: &Arquivo| {
            if e.ult.is_none_or(|u| u <= hi) || !e.orf.is_empty() {
                return false;
            }
            !e.ab.values().any(|u| u.i.is_some_and(|i| i <= hi))
        };
        match ler_arquivo(&mut e, &p, prazo, Some(&parar)) {
            Ok(()) => {}
            Err(Parou::Estourou) => return Err(Estourou),
            Err(Parou::Io(_)) => continue,
        }
        podar(&mut e, lo);
        arquivos.insert(p, e);
    }
    Ok(Indice { versao: VERSAO_DO_ESTADO, arquivos, criado: None, atualizado: None })
}

/// O fim efetivo da janela de uma chamada, e se ela pode estar rodando agora.
///
/// Sem resultado: numa sessão viva, a chamada ainda roda se começou há menos
/// de `RODANDO_MAX` e o arquivo não andou mais que isso depois dela (o laço
/// do agente espera o resultado) → vale até agora, e a decisão fica
/// provisória. Fora disso foi abandonada (sessão morta, cópia de outra
/// máquina): vale até a última linha datada, no máximo `RODANDO_MAX` depois
/// do início.
pub fn fim_da_janela(u: &Chamada, e: &Arquivo, viva: bool, agora: f64) -> (Option<f64>, bool) {
    if let Some(f) = u.f {
        return (Some(f), false);
    }
    let Some(i) = u.i else { return (None, false) };
    let ult = e.ult.unwrap_or(i);
    if u.bg {
        let mut f = u.tr.unwrap_or(i);
        if let Some(sd) = u.sd.as_deref().filter(|s| !outra_maquina(s)) {
            if let Ok(m) = std::fs::metadata(sd) {
                f = f.max(mtime(&m));
            }
        }
        if viva && agora - f <= RODANDO_MAX {
            // Segundo plano de sessão viva escrevendo: até agora.
            return (Some(agora), false);
        }
        return (Some(f), false);
    }
    if viva && agora - i <= RODANDO_MAX && ult - i <= RODANDO_MAX {
        return (Some(agora), true);
    }
    (Some(i.max(ult.min(i + RODANDO_MAX))), false)
}

/// Uma chamada em andamento num instante.
#[derive(Clone, Debug)]
pub struct Ativa {
    pub sid: String,
    pub arquivo: String,
    pub chamada: Chamada,
    pub fim: f64,
    pub aberta: bool,
}

/// As chamadas cuja janela (com folga) cobre `t`.
pub fn chamadas_em(ind: &Indice, t: f64, vivas: &HashSet<String>, agora: f64) -> Vec<Ativa> {
    let mut saida = Vec::new();
    for (p, e) in &ind.arquivos {
        if e.pri.is_some_and(|pri| pri > t + FOLGA) {
            continue;
        }
        let viva = vivas.contains(&e.sid);
        for u in e.ab.values().chain(e.fe.values()) {
            let Some(i) = u.i else { continue };
            if i - FOLGA > t {
                continue;
            }
            let (f, aberta) = fim_da_janela(u, e, viva, agora);
            let Some(f) = f else { continue };
            if f + FOLGA < t {
                continue;
            }
            saida.push(Ativa { sid: e.sid.clone(), arquivo: p.clone(), chamada: u.clone(), fim: f, aberta });
        }
    }
    saida
}

/// Relê linhas dos transcritos pelo deslocamento (o índice guarda só
/// horários e posições).
#[derive(Default)]
pub struct Linhas {
    cache: HashMap<(String, u64), Value>,
}

impl Linhas {
    fn linha(&mut self, p: &str, off: u64) -> &Value {
        self.cache.entry((p.to_string(), off)).or_insert_with(|| {
            (|| {
                let f = std::fs::File::open(p).ok()?;
                let mut r = BufReader::new(f);
                r.seek(SeekFrom::Start(off)).ok()?;
                let mut l = Vec::new();
                r.read_until(b'\n', &mut l).ok()?;
                serde_json::from_slice(&l).ok()
            })()
            .unwrap_or(Value::Null)
        })
    }

    /// A entrada, o resultado e se é um `EnterWorktree` com `path` (entrar
    /// numa worktree que já existe, não criar).
    pub fn textos(&mut self, p: &str, u: &Chamada) -> (String, String, bool) {
        let (mut entrada, mut resultado, mut entrando) = (String::new(), String::new(), false);
        let conteudo = |o: &Value| -> Vec<Value> {
            o.get("message").and_then(|m| m.get("content")).and_then(Value::as_array).cloned().unwrap_or_default()
        };
        let o = self.linha(p, u.ou).clone();
        for b in conteudo(&o) {
            if b.get("type").and_then(Value::as_str) == Some("tool_use") && b.get("id").and_then(Value::as_str) == Some(&u.id)
            {
                let inp = b.get("input").cloned().unwrap_or(Value::Null);
                entrada = textos_juntos(&inp);
                if b.get("name").and_then(Value::as_str) == Some("EnterWorktree")
                    && inp.get("path").is_some_and(|v| match v {
                        Value::String(s) => !s.is_empty(),
                        Value::Null | Value::Bool(false) => false,
                        _ => true,
                    })
                {
                    entrando = true;
                }
                break;
            }
        }
        if let Some(or_) = u.or_ {
            let o = self.linha(p, or_).clone();
            for b in conteudo(&o) {
                if b.get("type").and_then(Value::as_str) == Some("tool_result")
                    && b.get("tool_use_id").and_then(Value::as_str) == Some(&u.id)
                {
                    resultado = texto_do_resultado(&b);
                    break;
                }
            }
            if let Some(w) = o.get("toolUseResult").and_then(|r| r.get("worktreePath")).and_then(Value::as_str) {
                resultado.push('\n');
                resultado.push_str(w);
            }
        }
        (entrada, resultado, entrando)
    }
}

/// O nível da prova que uma chamada dá de ter criado a worktree:
/// `A` cita o nome ou o caminho; `B` é um comando de worktree com o nome em
/// molde (`wt-falhas-$g`); `C` só fala de worktree.
pub fn nivel(entrada: &str, resultado: &str, nomes: &[String], caminho: &str, entrando: bool) -> Option<char> {
    if entrando {
        return None;
    }
    let txt = format!("{entrada}\n{resultado}");
    if nomes.iter().any(|n| cita(n, &txt)) || cita(caminho, &txt) {
        return Some('A');
    }
    let fala = fala_de_worktree(entrada);
    if fala && nomes.iter().any(|n| molde(n, entrada)) {
        return Some('B');
    }
    fala.then_some('C')
}
