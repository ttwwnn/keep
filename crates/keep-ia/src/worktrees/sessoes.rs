//! As conversas do Claude Code vivas nesta máquina e o que os transcritos
//! dizem delas: o título, a última pasta, a worktree em que entraram.
//!
//! Uma sessão viva é um `~/.claude/sessions/<pid>.json` cujo processo é o
//! que o gravou: o Claude Code escreve ali quando o processo nasceu
//! (`procStart`, o `ps -o lstart` em UTC; no Windows, `procStartFt`, o
//! FILETIME), e um pid reaproveitado nasceu em outra hora.

use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::texto::{contem, titulo_limpo};

/// Uma sessão do Claude viva.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessaoViva {
    pub pid: u32,
    /// O `sessionId`: a conversa.
    pub id: String,
    /// `interactive`, `bg`, `daemon`… (ausente nas versões antigas).
    pub kind: Option<String>,
    pub cwd: String,
    /// O trabalho em segundo plano que esta sessão é (`jobId`).
    pub job: Option<String>,
    /// O trabalho para onde esta sessão foi estacionada (`parkedJobId`).
    pub parked: Option<String>,
}

/// A pasta das sessões do Claude Code deste usuário.
pub fn pasta_padrao() -> PathBuf {
    crate::caminhos::claude().join("sessions")
}

fn texto_de(v: &Value, campo: &str) -> Option<String> {
    v.get(campo).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

fn pid_de(v: &Value) -> Option<u32> {
    match v.get("pid")? {
        Value::Number(n) => n.as_u64().and_then(|p| u32::try_from(p).ok()),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|&p| p > 0)
}

/// Toda sessão do Claude viva, de qualquer tipo.
pub fn vivas_em(pasta: &Path) -> Vec<SessaoViva> {
    let Ok(dir) = std::fs::read_dir(pasta) else { return Vec::new() };
    let mut nomes: Vec<PathBuf> = dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    nomes.sort();
    nomes
        .into_iter()
        .filter_map(|p| {
            let v: Value = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            let pid = pid_de(&v)?;
            let id = texto_de(&v, "sessionId")?;
            confere(pid, &v).then(|| SessaoViva {
                pid,
                id,
                kind: texto_de(&v, "kind"),
                cwd: texto_de(&v, "cwd").unwrap_or_default(),
                job: texto_de(&v, "jobId"),
                parked: texto_de(&v, "parkedJobId"),
            })
        })
        .collect()
}

/// A sessão gravada para o processo `pid`, se ele é mesmo quem a gravou.
pub fn do_pid_em(pasta: &Path, pid: u32) -> Option<SessaoViva> {
    let v: Value = serde_json::from_slice(&std::fs::read(pasta.join(format!("{pid}.json"))).ok()?).ok()?;
    if pid_de(&v)? != pid {
        return None;
    }
    let id = texto_de(&v, "sessionId")?;
    confere(pid, &v).then(|| SessaoViva {
        pid,
        id,
        kind: texto_de(&v, "kind"),
        cwd: texto_de(&v, "cwd").unwrap_or_default(),
        job: texto_de(&v, "jobId"),
        parked: texto_de(&v, "parkedJobId"),
    })
}

/// `Sat Oct  3 20:55:10 2026` (UTC) em segundos unix.
pub fn lstart(s: &str) -> Option<f64> {
    const MESES: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let p: Vec<&str> = s.split_whitespace().collect();
    if p.len() != 5 {
        return None;
    }
    let mes = MESES.iter().position(|m| *m == p[1])? as u32 + 1;
    let data = NaiveDate::from_ymd_opt(p[4].parse().ok()?, mes, p[2].parse().ok()?)?;
    let hora = NaiveTime::parse_from_str(p[3], "%H:%M:%S").ok()?;
    Some(data.and_time(hora).and_utc().timestamp() as f64)
}

/// O processo `pid` é o que gravou o registro: o nascimento dele bate com o
/// `procStart`/`procStartFt` (1 s de folga: o `lstart` não tem fração).
/// Registro sem nenhum dos dois: vale o programa ser o Claude.
pub fn confere(pid: u32, reg: &Value) -> bool {
    let Some(nasceu) = crate::processos::um(pid).map(|p| p.inicio_ms).filter(|&t| t > 0) else {
        return false;
    };
    let nasceu = nasceu as f64 / 1000.0;
    if let Some(s) = reg.get("procStart").and_then(Value::as_str) {
        return lstart(s).is_some_and(|t| (t - nasceu).abs() <= 1.0);
    }
    if let Some(s) = reg.get("procStartFt").and_then(Value::as_str) {
        return s
            .trim()
            .parse::<u64>()
            .ok()
            .map(|ft| ft as f64 / 1e7 - 11_644_473_600.0)
            .is_some_and(|t| (t - nasceu).abs() <= 1.0);
    }
    e_claude(pid)
}

/// O processo é o Claude Code (pela linha de comando ou pelo nome).
pub fn e_claude(pid: u32) -> bool {
    use crate::tela::{Programa, programa};
    crate::processos::programa(pid).is_some_and(|p| programa(&p) == Programa::Claude)
        || crate::processos::um(pid).is_some_and(|p| programa(&p.nome) == Programa::Claude)
}

/// O transcrito principal de uma conversa (`<projetos>/<pasta>/<id>.jsonl`;
/// havendo mais de um, o mais recente).
pub fn transcrito_de(sid: &str, projetos: &Path) -> Option<PathBuf> {
    let dir = std::fs::read_dir(projetos).ok()?;
    dir.filter_map(|e| {
        let p = e.ok()?.path().join(format!("{sid}.jsonl"));
        let m = std::fs::metadata(&p).ok().filter(|m| m.is_file())?;
        Some((m.modified().ok()?, p))
    })
    .max()
    .map(|(_, p)| p)
}

/// Lê o arquivo de trás para a frente em blocos, entregando cada bloco já
/// sem a primeira linha cortada (ela vai junto com o bloco de trás).
/// `parar` diz se já achou tudo.
fn de_tras_para_frente(caminho: &Path, teto: u64, mut bloco_fn: impl FnMut(&[u8]) -> bool) {
    let Ok(mut f) = std::fs::File::open(caminho) else { return };
    let Ok(mut pos) = f.seek(SeekFrom::End(0)) else { return };
    let mut lidos = 0u64;
    let mut resto: Vec<u8> = Vec::new();
    while pos > 0 && lidos < teto {
        let n = (256u64 << 10).min(pos);
        pos -= n;
        lidos += n;
        if f.seek(SeekFrom::Start(pos)).is_err() {
            return;
        }
        let mut bloco = vec![0u8; n as usize];
        if f.read_exact(&mut bloco).is_err() {
            return;
        }
        bloco.extend_from_slice(&resto);
        resto.clear();
        let corpo = if pos > 0 {
            match bloco.iter().position(|&b| b == b'\n') {
                None => {
                    resto = bloco;
                    continue;
                }
                Some(k) => {
                    resto = bloco[..k].to_vec();
                    bloco[k + 1..].to_vec()
                }
            }
        } else {
            bloco
        };
        if bloco_fn(&corpo) {
            return;
        }
    }
}

/// O valor da ÚLTIMA linha JSON do bloco que tem `marca` e para a qual
/// `valor` dá algo.
fn ultima_linha<T>(bloco: &[u8], marca: &[u8], valor: impl Fn(&Value) -> Option<T>) -> Option<T> {
    let mut fim = bloco.len();
    loop {
        let k = memchr::memmem::rfind(&bloco[..fim], marca)?;
        let ini = bloco[..k].iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
        let sai = bloco[k..].iter().position(|&b| b == b'\n').map(|i| k + i).unwrap_or(bloco.len());
        if let Ok(o) = serde_json::from_slice::<Value>(&bloco[ini..sai]) {
            if o.is_object() {
                if let Some(v) = valor(&o) {
                    return Some(v);
                }
            }
        }
        if ini == 0 {
            return None;
        }
        fim = ini;
    }
}

/// O último título que o Claude deu à conversa (`ai-title`) e o que a
/// pessoa deu (`custom-title`), limpos.
pub fn titulos_do_transcrito(caminho: &Path) -> BTreeSet<String> {
    let mut ai: Option<String> = None;
    let mut pessoa: Option<String> = None;
    de_tras_para_frente(caminho, 2 << 20, |bloco| {
        for (tipo, campo, achado) in
            [("ai-title", "aiTitle", &mut ai), ("custom-title", "customTitle", &mut pessoa)]
        {
            if achado.is_some() {
                continue;
            }
            *achado = ultima_linha(bloco, format!("\"{tipo}\"").as_bytes(), |o| {
                (o.get("type").and_then(Value::as_str) == Some(tipo))
                    .then(|| o.get(campo).and_then(Value::as_str).filter(|s| !s.trim().is_empty()))
                    .flatten()
                    .map(titulo_limpo)
            });
        }
        ai.is_some() && pessoa.is_some()
    });
    ai.into_iter().chain(pessoa).collect()
}

/// A última pasta da conversa e a worktree em que ela entrou e não saiu,
/// lidas do fim do transcrito principal (no máximo `teto` bytes).
pub fn estado_final(sid: &str, projetos: &Path) -> (Option<String>, Option<String>) {
    let Some(tr) = transcrito_de(sid, projetos) else { return (None, None) };
    let mut cwd: Option<String> = None;
    // Some("") = saiu da worktree (ou nunca entrou).
    let mut wt: Option<String> = None;
    de_tras_para_frente(&tr, 4 << 20, |bloco| {
        if wt.is_none() {
            wt = ultima_linha(bloco, b"\"worktree-state\"", |o| {
                if o.get("type").and_then(Value::as_str) != Some("worktree-state") {
                    return None;
                }
                Some(
                    o.get("worktreeSession")
                        .and_then(|w| w.get("worktreePath"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            });
        }
        if cwd.is_none() && contem(bloco, b"\"cwd\":\"") {
            cwd = ultima_linha(bloco, b"\"cwd\":\"", |o| texto_de(o, "cwd"));
        }
        cwd.is_some() && wt.is_some()
    });
    (cwd, wt.filter(|w| !w.is_empty()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_lstart_do_ps() {
        let t = lstart("Sat Oct  3 20:55:10 2026").unwrap();
        assert_eq!(t, 1791060910.0);
        assert_eq!(lstart("Sat Oct 3 20:55:10"), None);
    }

    #[test]
    fn o_proprio_processo_confere_pelo_inicio() {
        let eu = std::process::id();
        let nasceu = crate::processos::um(eu).unwrap().inicio_ms;
        let ft = (nasceu / 1000 + 11_644_473_600) * 10_000_000;
        assert!(confere(eu, &serde_json::json!({ "procStartFt": ft.to_string() })));
        let longe = ft + 50_000_000;
        assert!(!confere(eu, &serde_json::json!({ "procStartFt": longe.to_string() })));
        let s = chrono::DateTime::from_timestamp((nasceu / 1000) as i64, 0)
            .unwrap()
            .format("%a %b %e %H:%M:%S %Y")
            .to_string();
        assert!(confere(eu, &serde_json::json!({ "procStart": s })), "{s}");
    }

    #[test]
    fn titulo_e_estado_do_fim_do_transcrito() {
        let d = tempfile::tempdir().unwrap();
        let proj = d.path().join("-p");
        std::fs::create_dir_all(&proj).unwrap();
        let sid = "11111111-2222-3333-4444-555555555555";
        let mut linhas = vec![
            r#"{"type":"ai-title","aiTitle":"✳ Primeiro","sessionId":"x"}"#.to_string(),
            r#"{"type":"worktree-state","worktreeSession":{"worktreePath":"/w/um"}}"#.to_string(),
        ];
        for i in 0..3000 {
            linhas.push(format!(r#"{{"type":"assistant","cwd":"/pasta/{i}","x":"{}"}}"#, "y".repeat(300)));
        }
        linhas.push(r#"{"type":"ai-title","aiTitle":"Segundo  título","sessionId":"x"}"#.into());
        std::fs::write(proj.join(format!("{sid}.jsonl")), linhas.join("\n") + "\n").unwrap();
        let tr = transcrito_de(sid, d.path()).unwrap();
        assert_eq!(titulos_do_transcrito(&tr), BTreeSet::from(["Segundo título".to_string()]));
        assert_eq!(estado_final(sid, d.path()), (Some("/pasta/2999".into()), Some("/w/um".into())));
    }
}
