//! A purga: as worktrees que o próprio Keep mandou para a Lixeira saem dela
//! de vez depois de `PURGA_APOS` (3 dias). Cada uma é uma cópia inteira do
//! projeto (vendor, node_modules), e a Lixeira cheia de worktrees encheu o
//! disco. Nada se perde: o `preparar` guardou em `refs/keep-lixeira/<id>`
//! a HEAD e em `<id>-sujo` a pasta inteira como o git a guardaria
//! (alterações e arquivos novos não ignorados); depois dos 3 dias, é lá que
//! a worktree continua.
//!
//! Só sai o que o Keep mandou, e só se ainda for ele: o item tem de estar no
//! `lixeira.log` com o destino exato, e no disco ele tem de ter o mesmo
//! número no volume (`ino`; no Windows, o índice do NTFS) e o mesmo
//! nascimento gravados na hora da ida, e o `.git` de worktree (`gitdir:`).
//! Item que voltou ("Colocar de volta"), item da pessoa com o mesmo nome no
//! mesmo lugar, pasta mexida: ficam. Nada fora da Lixeira, e nenhuma
//! configuração do Finder ou do Windows é tocada. Cada purga é uma linha
//! no `lixeira.log` (`"evento": "purga"`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::ambiente::Ambiente;
use super::estado;
use super::lixeira::identidade_da_pasta;
use super::listar::na_lixeira;
use super::texto::{agora, iso, ts};
use super::{LOG_LIXEIRA, PURGA_APOS};

/// Uma worktree que saiu da Lixeira de vez.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Purgada {
    pub destino: String,
    /// De onde ela veio.
    pub caminho: String,
    pub id: String,
    pub r#ref: Option<String>,
    pub ref_sujo: Option<String>,
}

/// Um item que era para sair e fica, com o motivo.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ignorada {
    pub destino: String,
    pub motivo: String,
}

/// A resposta da purga.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Purga {
    pub versao: u32,
    /// Outra rodada (do `indexar`, da varredura ou da purga) estava em curso.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ocupado: bool,
    pub purgadas: Vec<Purgada>,
    pub ignoradas: Vec<Ignorada>,
    pub avisos: Vec<String>,
}

/// O `$I…` que, na Lixeira do Windows, guarda de onde veio o `$R…`.
pub fn ficha_do_windows(p: &Path) -> Option<PathBuf> {
    let nome = p.file_name()?.to_str()?;
    let resto = nome.strip_prefix("$R").filter(|r| !r.is_empty())?;
    Some(p.with_file_name(format!("$I{resto}")))
}

/// O `.git` da pasta é o de uma worktree (`gitdir: …`).
fn tem_git_de_worktree(p: &Path) -> bool {
    let g = p.join(".git");
    std::fs::symlink_metadata(&g).is_ok_and(|m| m.is_file())
        && std::fs::read(&g).is_ok_and(|b| b.starts_with(b"gitdir:"))
}

fn tira_arquivo(p: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(p) {
        Err(e) if cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied => {
            // No Windows um arquivo só de leitura não sai sem tirar o atributo.
            let mut perm = std::fs::symlink_metadata(p)?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            std::fs::set_permissions(p, perm)?;
            std::fs::remove_file(p)
        }
        r => r,
    }
}

/// Apaga a pasta de vez, com o `.git` por último: interrompida no meio, ela
/// ainda se reconhece na rodada seguinte, que termina o serviço.
fn apaga_de_vez(p: &Path) -> std::io::Result<()> {
    for e in std::fs::read_dir(p)? {
        let e = e?;
        if e.file_name() == ".git" {
            continue;
        }
        if e.file_type()?.is_dir() {
            std::fs::remove_dir_all(e.path())?;
        } else {
            tira_arquivo(&e.path())?;
        }
    }
    let g = p.join(".git");
    if std::fs::symlink_metadata(&g).is_ok() {
        tira_arquivo(&g)?;
    }
    std::fs::remove_dir(p)
}

/// A purga: apaga de vez o que o Keep mandou para a Lixeira há mais de
/// `PURGA_APOS` e ainda é ele. `agora` (testes): o instante em que ela finge
/// rodar.
pub fn purgar(amb: &Ambiente, agora_dado: Option<f64>) -> Purga {
    let mut r = Purga { versao: crate::VERSAO, ..Purga::default() };
    let Some(_trava) = estado::trava(amb) else {
        r.ocupado = true;
        return r;
    };
    let agora = agora_dado.unwrap_or_else(agora);
    let texto = match std::fs::read_to_string(amb.estado.join(LOG_LIXEIRA)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return r,
        Err(e) => {
            r.avisos.push(format!("não consegui ler o lixeira.log ({e})"));
            return r;
        }
    };
    let linhas: Vec<Value> =
        texto.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(Value::is_object).collect();
    // Vale a última linha de cada destino: um nome que a purga liberou e
    // que outra worktree tomou depois é outro item.
    let mut vistos: HashSet<String> = HashSet::new();
    for l in linhas.iter().rev() {
        let Some(destino) = l["destino"].as_str().filter(|d| !d.is_empty()) else { continue };
        if !vistos.insert(destino.to_string()) || l.get("evento").is_some() {
            continue;
        }
        // Linha de antes desta versão (sem a identidade da pasta): não há
        // como saber se ainda é o mesmo item.
        let (Some(ino), Some(nasceu), Some(foi)) =
            (l["ino"].as_u64(), l["pasta_nasceu"].as_f64(), l["ts"].as_str().and_then(ts))
        else {
            continue;
        };
        if agora - foi < PURGA_APOS {
            continue;
        }
        let p = Path::new(destino);
        let ignora = |motivo: &str, r: &mut Purga| {
            r.ignoradas.push(Ignorada { destino: destino.to_string(), motivo: motivo.to_string() })
        };
        if std::fs::symlink_metadata(p).is_err() {
            continue; // já saiu da Lixeira (voltou para o lugar, ou a Lixeira foi esvaziada)
        }
        if !na_lixeira(amb, destino) {
            ignora("fora da Lixeira", &mut r);
            continue;
        }
        match identidade_da_pasta(p) {
            Some((i, n)) if i == ino && (n - nasceu).abs() < 1e-3 => {}
            _ => {
                ignora("outro item no mesmo lugar: não é a pasta que o Keep mandou", &mut r);
                continue;
            }
        }
        if !tem_git_de_worktree(p) {
            ignora("sem o .git de worktree: não é a pasta que o Keep mandou", &mut r);
            continue;
        }
        if let Err(e) = apaga_de_vez(p) {
            r.avisos.push(format!("não consegui apagar {destino} ({e})"));
            continue;
        }
        if let Some(ficha) = ficha_do_windows(p) {
            std::fs::remove_file(ficha).ok();
        }
        let texto_de = |k: &str| l[k].as_str().map(str::to_string);
        let feita = Purgada {
            destino: destino.to_string(),
            caminho: texto_de("caminho").unwrap_or_default(),
            id: texto_de("id").unwrap_or_default(),
            r#ref: texto_de("ref"),
            ref_sujo: texto_de("ref_sujo"),
        };
        let linha = json!({
            "ts": iso(agora), "evento": "purga", "destino": feita.destino, "caminho": feita.caminho,
            "id": feita.id, "ref": feita.r#ref, "ref_sujo": feita.ref_sujo, "foi_para_a_lixeira": iso(foi),
        });
        estado::acrescenta(amb, LOG_LIXEIRA, &linha).ok();
        estado::registra(amb, &format!("purga: {destino} saiu da Lixeira de vez (os commits seguem em {})", feita.r#ref.clone().unwrap_or_else(|| "refs/keep-lixeira".into())));
        r.purgadas.push(feita);
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_ficha_do_item_na_lixeira_do_windows() {
        let lixeira = Path::new("C:").join("$Recycle.Bin").join("S-1-5-21");
        let item = lixeira.join("$RAB12CD");
        assert_eq!(ficha_do_windows(&item), Some(lixeira.join("$IAB12CD")));
        assert_eq!(ficha_do_windows(&Path::new("x").join(".Trash").join("wt-a")), None);
        assert_eq!(ficha_do_windows(&lixeira.join("$R")), None);
    }
}
