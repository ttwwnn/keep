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

/// A sessão do processo `pid`, se ele é um Claude vivo com sessão anotada: na pasta de sessões de `~/.claude` ou na
/// da configuração de uma IA presa (`caminhos::pastas_de_sessoes`), esta só com o `cwd` dentro da pasta dela.
pub fn sessao_claude(pid: u32) -> Option<SessaoClaude> {
    caminhos::pastas_de_sessoes().into_iter().find_map(|(pasta, presa)| {
        let texto = caminhos::le_sessao(&pasta.join(format!("{pid}.json")))?;
        let d: Value = serde_json::from_str(&texto).ok()?;
        let cwd = d.get("cwd").and_then(Value::as_str).unwrap_or_default();
        if !mesmo_processo(&d, pid) || !caminhos::sessao_da_pasta(cwd, presa.as_deref()) {
            return None;
        }
        de_json(&d, pid)
    })
}

/// Toda sessão do Claude viva, interativa ou em segundo plano.
pub fn sessoes_vivas() -> Vec<SessaoClaude> {
    let mut pids: Vec<u32> = caminhos::pastas_de_sessoes()
        .into_iter()
        .flat_map(|(pasta, _)| std::fs::read_dir(pasta).into_iter().flatten().flatten())
        .filter_map(|e| e.file_name().into_string().ok()?.strip_suffix(".json")?.parse().ok())
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids.into_iter().filter_map(sessao_claude).collect()
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

/// O transcrito da conversa `id` do Claude, onde ela nasceu: um `--resume`
/// de outra pasta continua gravando no original (o mexido por último).
pub fn transcrito_claude(id: &str) -> Option<PathBuf> {
    transcritos_claude(id).into_iter().max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// O que se lê de trás para a frente, por vez, e no máximo.
const BLOCO: u64 = 64 << 10;
const TETO_LEITURA: u64 = 16 << 20;

/// As falas da pessoa que são comandos locais (`/effort`, `!ls`): não são da
/// conversa com o modelo.
const COMANDO_LOCAL: &[&str] = &[
    "<command-name>",
    "<command-message>",
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<local-command-caveat>",
    "<bash-input>",
    "<bash-stdout>",
    "<bash-stderr>",
];

/// O texto de uma fala do transcrito.
pub fn texto_da_fala(d: &Value) -> String {
    match d.get("message").and_then(|m| m.get("content")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(itens)) => {
            itens.iter().filter_map(|i| i.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join(" ")
        }
        _ => String::new(),
    }
}

/// Quando a fala foi gravada (ms); 0 sem carimbo.
pub fn carimbo_ms(d: &Value) -> u64 {
    d.get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp_millis().max(0) as u64)
        .unwrap_or(0)
}

/// As falas da conversa principal, da última para a primeira (`continued-in` também: o fim deste transcrito
/// deixou de ser o fim da conversa). Lê o arquivo de trás para a frente, em blocos, e para quando `f` diz.
pub fn falas_de_tras(caminho: &Path, mut f: impl FnMut(&Value) -> bool) {
    let Ok(mut fh) = std::fs::File::open(caminho) else { return };
    let Ok(tamanho) = fh.seek(SeekFrom::End(0)) else { return };
    let mut pos = tamanho;
    let mut resto: Vec<u8> = Vec::new();
    let mut lidos = 0u64;
    let mut visita = |linha: &[u8]| -> bool {
        let tem_tipo = |t: &[u8]| linha.windows(t.len()).any(|w| w == t);
        if !tem_tipo(b"\"type\":\"user\"") && !tem_tipo(b"\"type\":\"assistant\"") && !tem_tipo(b"\"type\":\"continued-in\"") {
            return true;
        }
        let Ok(d) = serde_json::from_slice::<Value>(linha) else { return true };
        let tipo = d.get("type").and_then(Value::as_str).unwrap_or("");
        if tipo == "continued-in" {
            return f(&d);
        }
        if !matches!(tipo, "user" | "assistant") {
            return true;
        }
        if d.get("isSidechain").and_then(Value::as_bool) == Some(true) || d.get("isMeta").and_then(Value::as_bool) == Some(true) {
            return true;
        }
        if tipo == "user" {
            let t = texto_da_fala(&d);
            let t = t.trim_start();
            if COMANDO_LOCAL.iter().any(|c| t.starts_with(c)) {
                return true;
            }
        }
        f(&d)
    };
    while pos > 0 && lidos < TETO_LEITURA {
        let n = BLOCO.min(pos);
        pos -= n;
        lidos += n;
        let mut bloco = vec![0u8; n as usize];
        if fh.seek(SeekFrom::Start(pos)).is_err() || fh.read_exact(&mut bloco).is_err() {
            return;
        }
        bloco.extend_from_slice(&resto);
        let mut partes: Vec<&[u8]> = bloco.split(|b| *b == b'\n').collect();
        // a primeira parte pode ser o fim de uma linha que começa no bloco anterior
        let primeira = if pos > 0 { partes.remove(0).to_vec() } else { Vec::new() };
        for linha in partes.iter().rev() {
            if !linha.is_empty() && !visita(linha) {
                return;
            }
        }
        resto = primeira;
    }
    if pos == 0 && !resto.is_empty() {
        visita(&resto);
    }
}

/// A conversa parou no limite de uso da conta: a fala de erro que o Claude
/// Code grava quando o serviço recusa o pedido (`"error": "rate_limit"`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParadaNoLimite {
    pub uuid: String,
    pub quando_ms: u64,
    /// A janela da conta que o serviço recusou (`five_hour`, `seven_day`) e
    /// quando ela recomeça, em segundos unix: o que ele disse
    /// (`quotaLimits`), sem ler a frase da tela. `None` no Claude Code que não
    /// grava isso, e no limite de um modelo só.
    pub janela: Option<String>,
    pub recomeca: Option<f64>,
    /// O limite é da conta inteira (o que tira a conta da fila): a janela
    /// recusada, ou, sem ela, a frase de sessão, semana ou uso que a tela
    /// também reconhece. O de um modelo só (Opus, Fable) não é.
    pub da_conta: bool,
}

/// As janelas da conta inteira: as que tiram a conta da fila
/// (`uso::Linha::disponivel`).
const JANELAS_DA_CONTA: &[&str] = &["five_hour", "seven_day"];

/// A frase das versões que não gravam a janela recusada.
fn frase_de_limite(t: &str) -> bool {
    t.trim().starts_with("You've hit your ") && t.contains("limit")
}

/// Se a fala é a parada no limite de uso. Vale primeiro o que o serviço disse
/// (a recusa de uma janela da conta, gravada junto da fala), depois a frase
/// das versões antigas ("You've hit your session limit · resets 5:10pm").
pub fn parada_no_limite(d: &Value) -> Option<ParadaNoLimite> {
    if d.get("type").and_then(Value::as_str) != Some("assistant")
        || d.get("error").and_then(Value::as_str) != Some("rate_limit")
    {
        return None;
    }
    let recusa = d
        .get("quotaLimits")
        .or_else(|| d.get("apiErrorParams").and_then(|p| p.get("rate_limit_info")))
        .filter(|i| i.get("status").and_then(Value::as_str) == Some("rejected"));
    let janela = recusa.and_then(|i| i.get("rateLimitType")).and_then(Value::as_str).filter(|t| JANELAS_DA_CONTA.contains(t));
    let texto = texto_da_fala(d);
    if janela.is_none() && !frase_de_limite(&texto) {
        return None;
    }
    let quando_ms = carimbo_ms(d);
    Some(ParadaNoLimite {
        uuid: d.get("uuid").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| quando_ms.to_string()),
        quando_ms,
        janela: janela.map(str::to_string),
        recomeca: janela.and(recusa).and_then(|i| i.get("resetsAt")).and_then(Value::as_f64),
        da_conta: janela.is_some() || tela::limite_na_tela(&[texto.trim().to_string()]),
    })
}

/// A última fala da conversa principal (`continued-in` também): onde a
/// conversa parou, se parou.
pub fn ultima_fala(caminho: &Path) -> Option<Value> {
    let mut ultima = None;
    falas_de_tras(caminho, |d| {
        ultima = Some(d.clone());
        false
    });
    ultima
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

    /// A fala de erro como o Claude Code 2.1.318 a grava (04/11/2026), sem a conversa em volta.
    fn parada(tipo: &str, frase: &str, quando: &str) -> Value {
        serde_json::json!({
            "type": "assistant", "uuid": format!("u-{quando}"), "timestamp": quando, "isApiErrorMessage": true,
            "error": "rate_limit", "apiError": "usage_limit_reached",
            "apiErrorParams": {"rate_limit_info": {"status": "rejected", "resetsAt": 1793833200, "rateLimitType": tipo}},
            "quotaLimits": {"status": "rejected", "resetsAt": 1793833200, "rateLimitType": tipo, "overageStatus": "rejected"},
            "message": {"role": "assistant", "content": [{"type": "text", "text": frase}]}
        })
    }

    #[test]
    fn a_parada_no_limite_vale_pelo_que_o_servico_disse() {
        let semanal = parada_no_limite(&parada("seven_day", "You've hit your weekly limit · resets 8pm", "2026-11-04T18:54:42.688Z")).unwrap();
        assert_eq!((semanal.janela.as_deref(), semanal.recomeca, semanal.da_conta), (Some("seven_day"), Some(1_793_833_200.0), true));
        assert_eq!((semanal.uuid.as_str(), semanal.quando_ms), ("u-2026-11-04T18:54:42.688Z", 1_793_818_482_688));
        // Outra frase para a mesma recusa: vale o que o serviço disse.
        let outra = parada_no_limite(&parada("five_hour", "Your session allowance is used up", "2026-11-04T18:54:42Z")).unwrap();
        assert_eq!(outra.janela.as_deref(), Some("five_hour"));
        // O limite de um modelo só não tira a conta da fila: só a frase antiga o diz, como antes, sem janela.
        let opus = parada_no_limite(&parada("seven_day_opus", "You've hit your Opus limit · resets 8pm", "2026-11-04T18:54:42Z")).unwrap();
        assert_eq!((opus.janela, opus.recomeca, opus.da_conta), (None, None, false));
        assert!(parada_no_limite(&parada("seven_day_fable", "You've reached your Fable limit.", "2026-11-04T18:54:42Z")).is_none());
        // A versão que não grava a recusa: a frase.
        let antiga = serde_json::json!({"type": "assistant", "error": "rate_limit", "timestamp": "2026-10-13T01:00:00Z",
            "message": {"content": [{"type": "text", "text": "You've hit your session limit · resets 5:10pm"}]}});
        assert_eq!(parada_no_limite(&antiga).map(|p| (p.janela, p.da_conta)), Some((None, true)));
        // Não é parada no limite.
        let mut sem_frase = antiga.clone();
        sem_frase["message"]["content"][0]["text"] = "API Error: Request rejected (429)".into();
        assert!(parada_no_limite(&sem_frase).is_none());
        let mut servidor = parada("seven_day", "x", "2026-11-04T18:54:42Z");
        servidor["error"] = "server_error".into();
        assert!(parada_no_limite(&servidor).is_none());
        let mut aceita = parada("seven_day", "x", "2026-11-04T18:54:42Z");
        aceita["quotaLimits"]["status"] = "allowed".into();
        aceita["apiErrorParams"]["rate_limit_info"]["status"] = "allowed".into();
        assert!(parada_no_limite(&aceita).is_none());
    }

    #[test]
    fn a_ultima_fala_e_a_da_conversa_principal() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path().join("t.jsonl");
        let erro = parada("five_hour", "You've hit your session limit · resets 5pm", "2026-11-04T18:00:00Z");
        let linhas = [
            serde_json::json!({"type": "user", "timestamp": "2026-11-04T17:59:00Z", "message": {"content": "faça"}}),
            erro.clone(),
            serde_json::json!({"type": "assistant", "isSidechain": true, "timestamp": "2026-11-04T18:00:05Z", "message": {"content": [{"type": "text", "text": "subagente"}]}}),
            serde_json::json!({"type": "user", "timestamp": "2026-11-04T18:00:06Z", "message": {"content": "<command-name>/effort</command-name>"}}),
        ];
        let texto: String = linhas.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&t, &texto).unwrap();
        let parada = |t: &Path| ultima_fala(t).and_then(|d| parada_no_limite(&d));
        assert_eq!(parada(&t).map(|p| p.uuid), Some("u-2026-11-04T18:00:00Z".into()), "subagente e comando local não contam");
        std::fs::write(&t, format!("{texto}{}\n", serde_json::json!({"type": "assistant", "timestamp": "2026-11-04T18:05:00Z", "message": {"content": [{"type": "text", "text": "pronto"}]}}))).unwrap();
        assert!(parada(&t).is_none(), "o Claude respondeu depois");
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
