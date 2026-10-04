//! Os repositórios e as worktrees ligadas deles, lidos dos arquivos do
//! próprio git (`<comum>/worktrees/<id>/…`), sem rodar o git.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

use super::ambiente::Ambiente;
use super::pastas::{
    base, canon, e_absoluto, e_pasta, fora_do_disco, junta, nascimento, nascimento_ns, normaliza,
    outra_maquina, pai,
};

/// Uma worktree ligada (a principal nunca é uma destas).
#[derive(Clone, Debug, PartialEq)]
pub struct Worktree {
    /// O diretório comum do repositório (`…/.git`).
    pub comum: String,
    /// A pasta do repositório.
    pub repo: String,
    /// O nome do registro dela em `<comum>/worktrees/`.
    pub id: String,
    /// `<comum>/worktrees/<id>`.
    pub adm: String,
    pub caminho: String,
    pub existe: bool,
    /// Quando o registro nasceu (o `git worktree add`), em segundos unix.
    pub nasceu: f64,
    pub nasceu_ns: i64,
    /// Quando o `.git` da pasta nasceu: longe do registro, a pasta foi
    /// restaurada de uma cópia.
    pub nasceu_git: Option<f64>,
    /// A razão da trava (`""` = travada sem razão); `None` = destravada.
    pub travada: Option<String>,
    pub ramo: Option<String>,
    /// A conversa do Codex que criou a worktree (`codex --worktree` grava
    /// `codex-thread.json` no registro).
    pub codex_dono: Option<String>,
}

fn le_texto(p: &str, limite: u64) -> std::io::Result<String> {
    use std::io::Read;
    let mut s = String::new();
    std::fs::File::open(p)?.take(limite).read_to_string(&mut s)?;
    Ok(s)
}

/// O diretório comum (canônico) do checkout ou da worktree em `d`, sem
/// chamar o git; `None` se não é repositório.
pub fn comum_de(d: &str) -> Option<String> {
    let g = junta(d, ".git");
    let m = std::fs::metadata(&g).ok()?;
    let c = if m.is_dir() {
        g
    } else {
        let txt = le_texto(&g, 4096).ok()?;
        let adm = txt.strip_prefix("gitdir:")?.trim();
        let adm = if e_absoluto(adm) { normaliza(adm) } else { normaliza(&junta(d, adm)) };
        if outra_maquina(&adm) {
            return None;
        }
        let cd = le_texto(&junta(&adm, "commondir"), 4096).ok()?;
        let cd = cd.trim();
        if e_absoluto(cd) { normaliza(cd) } else { normaliza(&junta(&adm, cd)) }
    };
    if outra_maquina(&c) || !e_pasta(&c) {
        return None;
    }
    Some(canon(&c))
}

/// A pasta do repositório de um diretório comum.
pub fn repo_de(comum: &str) -> String {
    if base(comum) == ".git" { pai(comum) } else { comum.to_string() }
}

/// Os diretórios comuns dos checkouts nas raízes (cada uma e um nível
/// abaixo) e acima de cada pasta de `extras`.
pub fn descobrir(amb: &Ambiente, extras: &[String]) -> BTreeSet<String> {
    let mut comuns = BTreeSet::new();
    for r in &amb.raizes {
        let r = r.to_string_lossy().into_owned();
        if fora_do_disco(&r) {
            continue;
        }
        let mut cands = vec![r.clone()];
        if let Ok(dir) = std::fs::read_dir(&r) {
            for e in dir.flatten() {
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    let p = e.path().to_string_lossy().into_owned();
                    if !fora_do_disco(&p) {
                        cands.push(p);
                    }
                }
            }
        }
        comuns.extend(cands.iter().filter_map(|d| comum_de(d)));
    }
    let teto = pai(&amb.casa.to_string_lossy());
    for d in extras {
        if d.is_empty() || !e_absoluto(d) || fora_do_disco(d) {
            continue;
        }
        let mut d = normaliza(d);
        for _ in 0..40 {
            if let Some(c) = comum_de(&d) {
                comuns.insert(c);
                break;
            }
            let p = pai(&d);
            if p == d || d == teto {
                break;
            }
            d = p;
        }
    }
    comuns
}

/// As worktrees ligadas do repositório, lidas de `<comum>/worktrees/<id>`.
pub fn worktrees_do_repo(comum: &str) -> Vec<Worktree> {
    let base_adm = junta(comum, "worktrees");
    let Ok(dir) = std::fs::read_dir(&base_adm) else { return Vec::new() };
    let mut ids: Vec<String> = dir.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    ids.sort();
    let repo = repo_de(comum);
    let mut saida = Vec::new();
    for id in ids {
        let adm = junta(&base_adm, &id);
        let Ok(m) = std::fs::metadata(&adm) else { continue };
        if !m.is_dir() {
            continue;
        }
        let Ok(gd) = le_texto(&junta(&adm, "gitdir"), 1 << 16) else { continue };
        let gd = gd.trim();
        if gd.is_empty() {
            continue;
        }
        let gd = if e_absoluto(gd) { normaliza(gd) } else { normaliza(&junta(&adm, gd)) };
        let mut caminho = pai(&gd);
        let mut existe = false;
        let mut nasceu_git = None;
        if !outra_maquina(&caminho) {
            existe = e_pasta(&caminho);
            if existe {
                caminho = canon(&caminho);
                nasceu_git = std::fs::metadata(junta(&caminho, ".git")).ok().map(|m| nascimento(&m));
            }
        }
        let travada = match le_texto(&junta(&adm, "locked"), 1 << 16) {
            Ok(r) => Some(r.trim().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => Some(String::new()),
        };
        let ramo = le_texto(&junta(&adm, "HEAD"), 4096)
            .ok()
            .and_then(|h| h.trim().strip_prefix("ref: refs/heads/").map(str::to_string));
        let nasceu = nascimento_do_registro(&adm, &m);
        let codex_dono = std::fs::read(junta(&adm, "codex-thread.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.get("ownerThreadId").and_then(Value::as_str).map(str::to_string))
            .filter(|s| !s.is_empty());
        saida.push(Worktree {
            comum: comum.to_string(),
            repo: repo.clone(),
            id,
            adm,
            caminho,
            existe,
            nasceu,
            nasceu_ns: nascimento_ns(nasceu),
            nasceu_git,
            travada,
            ramo,
            codex_dono,
        });
    }
    saida
}

/// Quando o registro de uma worktree (`<comum>/worktrees/<id>`) nasceu: o
/// instante do `git worktree add`, a hora T de quem é dono.
///
/// No macOS, o nascimento da pasta do registro (o mesmo número que o kit
/// gravava). No Windows, o do `commondir` de dentro dela: o NTFS devolve a
/// uma pasta apagada e recriada com o mesmo nome em até 15 s a hora de
/// criação da anterior ("tunneling"), e um registro novo nunca herda o que
/// estava dentro do velho. No Linux sem o nascimento (`statx`), a última
/// mudança do `commondir`, que o git escreve uma vez só — a da pasta muda a
/// cada trava que o git cria lá dentro.
pub fn nascimento_do_registro(adm: &str, m: &std::fs::Metadata) -> f64 {
    let dentro = || std::fs::metadata(junta(adm, "commondir")).ok();
    if cfg!(windows) {
        if let Some(c) = dentro() {
            return nascimento(&c);
        }
        return nascimento(m);
    }
    if m.created().is_ok() {
        return nascimento(m);
    }
    nascimento(&dentro().unwrap_or_else(|| m.clone()))
}

/// O nascimento do registro em `adm`, se ele existe.
pub fn nascimento_do_registro_em(adm: &str) -> Option<f64> {
    std::fs::metadata(adm).ok().map(|m| nascimento_do_registro(adm, &m))
}

/// A chave de uma worktree no registro: o repositório, o id e o nascimento
/// (um nome reaproveitado é outra worktree).
pub fn chave(w: &Worktree) -> String {
    format!("{}|{}|{}", w.comum, w.id, w.nasceu_ns)
}

/// Os nomes pelos quais uma conversa cita a worktree: o da pasta e o id do
/// registro (diferentes depois de um `git worktree move`).
pub fn nomes(w: &Worktree) -> Vec<String> {
    let mut s: BTreeSet<String> = BTreeSet::new();
    s.insert(base(&w.caminho));
    s.insert(w.id.clone());
    s.into_iter().filter(|n| !n.is_empty()).collect()
}

/// Todas as worktrees dos repositórios dados.
pub fn todas(comuns: &BTreeSet<String>) -> Vec<Worktree> {
    comuns.iter().flat_map(|c| worktrees_do_repo(c)).collect()
}

/// O nascimento como o `listar` o mostra, de um registro.
pub fn nascimento_ns_do_registro(adm: &Path) -> Option<i64> {
    nascimento_do_registro_em(&adm.to_string_lossy()).map(nascimento_ns)
}
