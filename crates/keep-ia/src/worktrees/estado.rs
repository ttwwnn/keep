//! O que este módulo guarda: as decisões (uma por nascimento), as conversas
//! que já rodaram em cada shell, os repositórios vistos e o git de cada
//! worktree candidata — num arquivo só (`worktrees.json`, 0600), gravado
//! inteiro de uma vez.
//!
//! Quem usava o kit-mac tem as decisões dele em
//! `~/.local/state/kit-keep/worktrees.json`: na primeira vez, sem estado
//! próprio, elas vêm de lá (as chaves são as mesmas: repositório, id e
//! nascimento). O histórico de conversas por shell só vale para o daemon em
//! que foi visto, e só vem se for o daemon que está rodando.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use keep_proto::DaemonInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ambiente::Ambiente;
use super::dono::Decisao;
use super::texto::{agora, iso};
use super::{ARQ_ESTADO, LOG, VERSAO_DO_ESTADO};

/// As conversas que já rodaram num shell (do Claude e, à parte, as do
/// Codex), e quando o shell nasceu: um pid reaproveitado é outro shell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Historia {
    pub ids: Vec<String>,
    pub nasceu: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codex: Vec<String>,
}

/// O que o diálogo mostra de uma worktree, como o `indexar` o contou.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DetalhesGit {
    pub ramo: Option<String>,
    pub head: Option<String>,
    /// Alterações rastreadas (`git status -uno`).
    pub rastreados: u64,
    /// Commits só nesta HEAD (fora de ramo e de remoto).
    pub commits_so_aqui: Option<u64>,
    pub submodulos: bool,
    #[serde(default)]
    pub nao_rastreados: u64,
    #[serde(default)]
    pub alteracoes: u64,
    /// Quando foi contado.
    #[serde(default)]
    pub em: f64,
}

/// O estado inteiro.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Estado {
    pub versao: u32,
    #[serde(default)]
    pub nascimentos: BTreeMap<String, Decisao>,
    /// `{daemon: {pid do shell: história}}`.
    #[serde(default)]
    pub historico: BTreeMap<String, BTreeMap<String, Historia>>,
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub git: BTreeMap<String, DetalhesGit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atualizado_em: Option<String>,
    /// O histórico que veio do kit, até se saber se é do daemon que roda.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub historico_do_kit: Option<BTreeMap<String, BTreeMap<String, Historia>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trazido_do_kit: Option<String>,
}

impl Estado {
    pub fn vazio() -> Estado {
        Estado { versao: VERSAO_DO_ESTADO, ..Estado::default() }
    }
}

pub fn arquivo(amb: &Ambiente) -> PathBuf {
    amb.estado.join(ARQ_ESTADO)
}

fn le_estado(p: &Path) -> Result<Option<Estado>, std::io::Error> {
    let bytes = std::fs::read(p)?;
    Ok(serde_json::from_slice::<Estado>(&bytes).ok().filter(|e| e.versao == VERSAO_DO_ESTADO))
}

/// O estado gravado; sem estado próprio, o do kit (sem gravar).
pub fn carrega(amb: &Ambiente) -> Option<Estado> {
    match le_estado(&arquivo(amb)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => do_kit(amb),
        Err(_) => None,
    }
}

/// As decisões, os repositórios e o git que o kit-mac já registrou.
pub fn do_kit(amb: &Ambiente) -> Option<Estado> {
    let p = amb.kit.as_ref()?.join("worktrees.json");
    let v: Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
    if v.get("versao").and_then(Value::as_u64) != Some(u64::from(VERSAO_DO_ESTADO)) {
        return None;
    }
    let mapa = |campo: &str| v.get(campo).and_then(Value::as_object).cloned().unwrap_or_default();
    let nascimentos = mapa("nascimentos")
        .into_iter()
        .filter_map(|(k, d)| Some((k, serde_json::from_value::<Decisao>(d).ok()?)))
        .collect();
    let git = mapa("git")
        .into_iter()
        .filter_map(|(k, d)| Some((k, serde_json::from_value::<DetalhesGit>(d).ok()?)))
        .collect();
    let historico: BTreeMap<String, BTreeMap<String, Historia>> = mapa("historico")
        .into_iter()
        .filter_map(|(k, d)| Some((k, serde_json::from_value(d).ok()?)))
        .collect();
    let repos = v
        .get("repos")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    Some(Estado {
        versao: VERSAO_DO_ESTADO,
        nascimentos,
        historico: BTreeMap::new(),
        repos,
        git,
        atualizado_em: None,
        historico_do_kit: (!historico.is_empty()).then_some(historico),
        trazido_do_kit: Some(iso(agora())),
    })
}

/// A chave de um daemon no histórico.
pub fn chave_keepd(d: &DaemonInfo) -> String {
    format!("{}:{}", d.pid, d.started_ms)
}

/// O histórico que veio do kit passa a valer se for do daemon que está
/// rodando: o kit o guardava pelo nascimento do socket.
pub fn adota_historico_do_kit(est: &mut Estado, daemon: &DaemonInfo, nascimento_do_socket: Option<f64>) {
    let Some(kit) = est.historico_do_kit.take() else { return };
    if daemon.pid == 0 {
        return;
    }
    for (k, ents) in kit {
        let Ok(inicio) = k.parse::<f64>() else { continue };
        let mesmo = nascimento_do_socket.is_some_and(|n| (n - inicio).abs() <= 0.001)
            || (daemon.started_ms > 0 && (inicio * 1000.0 - daemon.started_ms as f64).abs() <= 2000.0);
        if mesmo {
            let cur = est.historico.entry(chave_keepd(daemon)).or_default();
            for (pid, h) in ents {
                cur.entry(pid).or_insert(h);
            }
        }
    }
}

/// As conversas que já rodaram no shell, se é o MESMO shell (`campo`:
/// `ids` ou `codex`).
pub fn historico_da_concha(est: Option<&Estado>, daemon: Option<&str>, concha: u32, codex: bool) -> Vec<String> {
    let (Some(est), Some(daemon)) = (est, daemon) else { return Vec::new() };
    let Some(h) = est.historico.get(daemon).and_then(|cur| cur.get(&concha.to_string())) else {
        return Vec::new();
    };
    let Some(nasceu) = nascimento_do_processo(concha) else { return Vec::new() };
    if (h.nasceu - nasceu).abs() > 0.01 {
        return Vec::new();
    }
    if codex { h.codex.clone() } else { h.ids.clone() }
}

/// Quando o processo nasceu, em segundos.
pub fn nascimento_do_processo(pid: u32) -> Option<f64> {
    crate::processos::um(pid).map(|p| p.inicio_ms).filter(|&t| t > 0).map(|t| t as f64 / 1000.0)
}

/// Grava o estado inteiro (0600).
pub fn grava(amb: &Ambiente, est: &mut Estado) -> std::io::Result<()> {
    est.versao = VERSAO_DO_ESTADO;
    est.atualizado_em = Some(iso(agora()));
    let mut bytes = serde_json::to_vec_pretty(est)?;
    bytes.push(b'\n');
    grava_json(&arquivo(amb), &bytes)
}

/// A pasta, só para o dono.
pub fn cria_pasta(p: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(p)
}

/// Grava num temporário da mesma pasta (0600), sincroniza e troca: quem lê
/// vê o arquivo velho inteiro ou o novo inteiro.
pub fn grava_json(caminho: &Path, dados: &[u8]) -> std::io::Result<()> {
    static N: AtomicU64 = AtomicU64::new(0);
    let pasta = caminho.parent().unwrap_or(Path::new("."));
    cria_pasta(pasta)?;
    let mut tmp = caminho.as_os_str().to_owned();
    tmp.push(format!(".tmp.{}.{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let tmp = PathBuf::from(tmp);
    let mut op = std::fs::OpenOptions::new();
    op.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        op.mode(0o600);
    }
    let r = (|| {
        let mut f = op.open(&tmp)?;
        f.write_all(dados)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, caminho)
    })();
    if r.is_err() {
        std::fs::remove_file(&tmp).ok();
        return r;
    }
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(pasta) {
        d.sync_all().ok();
    }
    Ok(())
}

/// A trava do `indexar`: `None` se outra execução está com ela.
pub fn trava(amb: &Ambiente) -> Option<std::fs::File> {
    cria_pasta(&amb.estado).ok()?;
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(amb.estado.join("worktrees.lock")).ok()?;
    f.try_lock().ok()?;
    Some(f)
}

/// Uma linha no `worktrees.log`.
pub fn registra(amb: &Ambiente, msg: &str) {
    if cria_pasta(&amb.estado).is_err() {
        return;
    }
    let linha = format!("{}  {msg}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(amb.estado.join(LOG)) {
        f.write_all(linha.as_bytes()).ok();
    }
}

/// Uma linha JSON acrescentada a um arquivo do estado (0600).
pub fn acrescenta(amb: &Ambiente, nome: &str, linha: &Value) -> std::io::Result<()> {
    cria_pasta(&amb.estado)?;
    let mut op = std::fs::OpenOptions::new();
    op.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        op.mode(0o600);
    }
    let mut f = op.open(amb.estado.join(nome))?;
    let mut s = serde_json::to_string(linha)?;
    s.push('\n');
    f.write_all(s.as_bytes())
}
