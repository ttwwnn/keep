//! As conversas em disco: a sessão viva de cada processo do Claude
//! (`~/.claude/sessions/<pid>.json`), a continuação de uma conversa que foi
//! para segundo plano, e se uma conversa do Claude ou do Codex existe (com
//! mensagem) para ser retomada.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;

use crate::{caminhos, processos, tela};

/// Uma sessão do Claude, como ele mesmo a anota.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessaoClaude {
    pub pid: u32,
    pub id: String,
    /// `interactive`, `bg`…
    pub tipo: String,
    /// `idle`, `busy`, `waiting`, `shell`.
    pub status: String,
    pub job: Option<String>,
    pub cwd: Option<String>,
    pub nome: Option<String>,
}

impl SessaoClaude {
    /// Trabalhando (um turno, ou um comando do shell dele).
    pub fn trabalhando(&self) -> bool {
        matches!(self.status.as_str(), "busy" | "shell")
    }

    /// Esperando uma resposta (permissão, pergunta).
    pub fn esperando(&self) -> bool {
        self.status == "waiting"
    }
}

fn pasta_sessoes() -> PathBuf {
    caminhos::claude().join("sessions")
}

/// O `procStart` que o Claude grava ("Sat Oct  3 22:57:57 2026", em UTC) é
/// o do processo dono do pid agora? Um pid reaproveitado deixa o arquivo
/// velho apontando para outro processo.
fn mesmo_processo(d: &Value, pid: u32) -> bool {
    let Some(p) = processos::um(pid) else { return false };
    if let Some(texto) = d.get("procStart").and_then(Value::as_str) {
        let normal = texto.split_whitespace().collect::<Vec<_>>().join(" ");
        if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&normal, "%a %b %d %H:%M:%S %Y") {
            let s = t.and_utc().timestamp();
            let real = (p.inicio_ms / 1000) as i64;
            // UTC no macOS; a hora local, se alguma versão gravar assim.
            let local = chrono::Local
                .from_local_datetime(&t)
                .single()
                .map(|l| l.timestamp())
                .unwrap_or(s);
            return (s - real).abs() <= 2 || (local - real).abs() <= 2;
        }
    }
    // Sem como conferir: vale se o processo é um Claude.
    tela::programa(&p.nome) == tela::Programa::Claude
        || processos::argv(pid).is_some_and(|a| a.iter().any(|x| x.contains("claude")))
}

use chrono::TimeZone;

fn de_json(d: &Value, pid: u32) -> Option<SessaoClaude> {
    let texto = |k: &str| d.get(k).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty());
    Some(SessaoClaude {
        pid,
        id: texto("sessionId")?,
        tipo: texto("kind").unwrap_or_else(|| "interactive".into()),
        status: texto("status").unwrap_or_default(),
        job: texto("jobId"),
        cwd: texto("cwd"),
        nome: texto("name"),
    })
}

/// A sessão do processo `pid`, se ele é um Claude vivo com sessão anotada.
pub fn sessao_claude(pid: u32) -> Option<SessaoClaude> {
    let texto = std::fs::read_to_string(pasta_sessoes().join(format!("{pid}.json"))).ok()?;
    let d: Value = serde_json::from_str(&texto).ok()?;
    if !mesmo_processo(&d, pid) {
        return None;
    }
    de_json(&d, pid)
}

/// Toda sessão do Claude viva, interativa ou em segundo plano.
pub fn sessoes_vivas() -> Vec<SessaoClaude> {
    let Ok(dir) = std::fs::read_dir(pasta_sessoes()) else { return Vec::new() };
    dir.filter_map(|e| {
        let nome = e.ok()?.file_name().into_string().ok()?;
        let pid: u32 = nome.strip_suffix(".json")?.parse().ok()?;
        sessao_claude(pid)
    })
    .collect()
}

/// A sessão viva que roda a conversa `id`, de qualquer processo.
pub fn sessao_viva(id: &str) -> Option<SessaoClaude> {
    sessoes_vivas().into_iter().find(|s| s.id == id)
}

/// A sessão em segundo plano do job `referencia` (id do job, da conversa
/// ou o começo dela).
pub fn job_vivo(referencia: &str) -> Option<SessaoClaude> {
    if referencia.is_empty() {
        return None;
    }
    sessoes_vivas().into_iter().find(|s| {
        s.tipo == "bg" && (s.job.as_deref() == Some(referencia) || s.id == referencia || s.id.starts_with(referencia))
    })
}

/// Os transcritos da conversa `id` do Claude (um por pasta de projeto).
pub fn transcritos_claude(id: &str) -> Vec<PathBuf> {
    let Ok(projetos) = std::fs::read_dir(caminhos::claude().join("projects")) else { return Vec::new() };
    projetos
        .filter_map(|e| {
            let p = e.ok()?.path().join(format!("{id}.jsonl"));
            p.is_file().then_some(p)
        })
        .collect()
}

fn cauda(arquivo: &Path, bytes: u64) -> Option<String> {
    let mut f = std::fs::File::open(arquivo).ok()?;
    let n = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(n.saturating_sub(bytes))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// O id em que a conversa segue: ao ir para segundo plano o Claude continua
/// a conversa noutro id e grava `continuedInSessionId` no fim do transcrito.
/// Retomar o id antigo daria uma cópia paralela da conversa.
pub fn continuacao(id: &str) -> String {
    let re = Regex::new(r#""continuedInSessionId"\s*:\s*"([0-9a-f-]{36})""#).expect("regex válida");
    let mut atual = id.to_string();
    let mut vistos = vec![];
    while !atual.is_empty() && !vistos.contains(&atual) && vistos.len() < 5 {
        vistos.push(atual.clone());
        let proximo = transcritos_claude(&atual)
            .iter()
            .filter_map(|t| cauda(t, 512 * 1024))
            .find_map(|c| re.captures_iter(&c).last().map(|m| m[1].to_string()));
        match proximo {
            Some(p) => atual = p,
            None => break,
        }
    }
    atual
}

/// A conversa do Claude tem ao menos uma mensagem gravada (sem nenhuma,
/// `--resume` diz "No conversation found").
pub fn conversa_claude_existe(id: &str) -> bool {
    let re = Regex::new(r#""type"\s*:\s*"(?:user|assistant)""#).expect("regex válida");
    transcritos_claude(id).iter().any(|t| {
        let mut buf = Vec::new();
        std::fs::File::open(t).and_then(|f| f.take(4 * 1024 * 1024).read_to_end(&mut buf)).is_ok()
            && re.is_match(&String::from_utf8_lossy(&buf))
    })
}

/// Os rollouts da conversa `id` do Codex em `home` (`sessions/AAAA/MM/DD/rollout-…-<id>.jsonl`).
pub fn rollouts_codex(home: &Path, id: &str) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    let mut pilha = vec![home.join("sessions")];
    let fim = format!("{id}.jsonl");
    while let Some(dir) = pilha.pop() {
        let Ok(it) = std::fs::read_dir(&dir) else { continue };
        for e in it.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => pilha.push(p),
                Ok(_) => {
                    let nome = e.file_name().to_string_lossy().into_owned();
                    if nome.starts_with("rollout-") && nome.ends_with(&fim) {
                        saida.push(p);
                    }
                }
                Err(_) => {}
            }
        }
    }
    saida
}

/// A conversa do Codex está em disco nessa conta (sem ela, `codex resume`
/// diz "no rollout found").
pub fn conversa_codex_existe(home: &Path, id: &str) -> bool {
    !rollouts_codex(home, id).is_empty()
}

/// O id de `codex resume <id>` numa linha de comando.
pub fn id_codex_do_argv(argv: &[String]) -> Option<String> {
    argv.windows(2).find(|w| w[0] == "resume" && tela::e_uuid(&w[1])).map(|w| w[1].clone())
}

/// O id de `claude --resume <id>` numa linha de comando.
pub fn id_claude_do_argv(argv: &[String]) -> Option<String> {
    argv.windows(2)
        .find(|w| (w[0] == "--resume" || w[0] == "-r") && tela::e_uuid(&w[1]))
        .map(|w| w[1].clone())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ids_nas_linhas_de_comando() {
        let a: Vec<String> = ["codex", "resume", "01a103e9-cc76-75b1-b919-7702b666146a", "--yolo"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(id_codex_do_argv(&a).as_deref(), Some("01a103e9-cc76-75b1-b919-7702b666146a"));
        let a: Vec<String> =
            ["claude", "-r", "9e2fff2d-f449-4d5f-998f-1b3495689c61"].iter().map(|s| s.to_string()).collect();
        assert_eq!(id_claude_do_argv(&a).as_deref(), Some("9e2fff2d-f449-4d5f-998f-1b3495689c61"));
    }

    #[test]
    fn o_proc_start_confere_com_o_processo() {
        let eu = std::process::id();
        let inicio = crate::processos::um(eu).unwrap().inicio_ms;
        let utc = chrono::DateTime::from_timestamp((inicio / 1000) as i64, 0).unwrap();
        let texto = utc.format("%a %b %e %H:%M:%S %Y").to_string();
        assert!(mesmo_processo(&serde_json::json!({ "procStart": texto }), eu), "{texto}");
        let outro = (utc - chrono::Duration::hours(3)).format("%a %b %e %H:%M:%S %Y").to_string();
        let local = chrono::Local.timestamp_opt((inicio / 1000) as i64, 0).unwrap();
        if local.offset().local_minus_utc() != -3 * 3600 {
            assert!(!mesmo_processo(&serde_json::json!({ "procStart": outro }), eu), "{outro}");
        }
    }

    #[test]
    fn a_continuacao_segue_o_continued_in() {
        let c = crate::contas::testes::casa("sessoes");
        let projeto = c.raiz.join(".claude/projects/-x");
        std::fs::create_dir_all(&projeto).unwrap();
        let a = "aaaaaaaa-0000-0000-0000-000000000001";
        let b = "bbbbbbbb-0000-0000-0000-000000000002";
        std::fs::write(projeto.join(format!("{a}.jsonl")), format!("{{\"type\":\"user\"}}\n{{\"type\":\"continued-in\",\"continuedInSessionId\":\"{b}\"}}\n")).unwrap();
        std::fs::write(projeto.join(format!("{b}.jsonl")), "{\"type\":\"assistant\"}\n").unwrap();
        assert_eq!(continuacao(a), b);
        assert!(conversa_claude_existe(b));
        assert!(!conversa_claude_existe("cccccccc-0000-0000-0000-000000000003"));
    }
}
