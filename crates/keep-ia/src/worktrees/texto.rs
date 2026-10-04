//! O que se procura dentro dos transcritos e dos rollouts: horários, nomes
//! de worktree com fronteira de palavra, os marcadores do Claude Code e do
//! Codex. Tudo à mão, sem expressão regular: as regras do kit usavam
//! lookbehind, que o `regex` não tem.

use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
use memchr::memmem;
use serde_json::Value;

/// `agulha` aparece em `palheiro`.
pub fn contem(palheiro: &[u8], agulha: &[u8]) -> bool {
    memmem::find(palheiro, agulha).is_some()
}

/// Agora, em segundos unix.
pub fn agora() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Um horário ISO 8601 (`2026-09-27T04:18:30.372Z`, com ou sem fração, `Z`
/// ou deslocamento) em segundos unix. Sem fuso, vale como UTC.
pub fn ts(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(segundos(d.with_timezone(&Utc)));
    }
    for formato in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, formato) {
            return Some(segundos(n.and_utc()));
        }
    }
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    Some(segundos(d.and_hms_opt(0, 0, 0)?.and_utc()))
}

fn segundos(d: DateTime<Utc>) -> f64 {
    d.timestamp() as f64 + f64::from(d.timestamp_subsec_nanos()) * 1e-9
}

/// Segundos unix em ISO 8601 com milissegundos e `Z`, como o kit gravava
/// (`datetime.isoformat(timespec="milliseconds")`: o microssegundo
/// arredondado, o milissegundo cortado).
pub fn iso(t: f64) -> String {
    let inteiro = t.floor();
    let mut seg = inteiro as i64;
    let mut us = ((t - inteiro) * 1e6).round_ties_even() as i64;
    if us >= 1_000_000 {
        seg += 1;
        us -= 1_000_000;
    }
    let ns = (us / 1000 * 1_000_000) as u32;
    DateTime::<Utc>::from_timestamp(seg, ns)
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}

/// O horário de uma linha JSON pelo texto (`"timestamp":"…Z"`), sem ler o
/// JSON: o que importa da maioria das linhas é só isso.
pub fn ts_da_linha(linha: &[u8]) -> Option<f64> {
    const MARCA: &[u8] = b"\"timestamp\":\"";
    let mut desde = 0;
    while let Some(k) = memmem::find(&linha[desde..], MARCA) {
        let ini = desde + k + MARCA.len();
        let mut fim = ini;
        while fim < linha.len() && matches!(linha[fim], b'0'..=b'9' | b'T' | b':' | b'.' | b'-') {
            fim += 1;
        }
        if fim > ini && linha.get(fim) == Some(&b'Z') && linha.get(fim + 1) == Some(&b'"') {
            return std::str::from_utf8(&linha[ini..=fim]).ok().and_then(ts);
        }
        desde = ini;
    }
    None
}

/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, em minúsculas.
pub fn e_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(&c),
        })
}

/// `rollout-<data>-<thread>.jsonl[.zst]` → a thread.
pub fn thread_do_rollout(nome: &str) -> Option<&str> {
    let resto = nome.strip_prefix("rollout-")?;
    let resto = resto.strip_suffix(".jsonl.zst").or_else(|| resto.strip_suffix(".jsonl"))?;
    if resto.len() < 38 || !resto.is_char_boundary(resto.len() - 36) {
        return None;
    }
    let (antes, tid) = resto.split_at(resto.len() - 36);
    (antes.len() >= 2 && antes.ends_with('-') && e_uuid(tid)).then_some(tid)
}

fn de_palavra(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// O nome inteiro aparece no texto: não colado a letra, dígito, `.`, `_` ou
/// `-` antes, nem a letra, dígito, `_`, `-` ou `.<letra>` depois
/// (`wt-falhas-e` não é `wt-falhas-e2`, nem `wt-x.git`).
pub fn cita(nome: &str, texto: &str) -> bool {
    if nome.is_empty() {
        return false;
    }
    let t = texto.as_bytes();
    let n = nome.as_bytes();
    let mut desde = 0;
    while let Some(k) = memmem::find(&t[desde..], n) {
        let i = desde + k;
        let fim = i + n.len();
        let antes_ok = i == 0 || !(de_palavra(t[i - 1]) || t[i - 1] == b'.');
        let depois_ok = match t.get(fim) {
            None => true,
            Some(&c) if de_palavra(c) => false,
            Some(b'.') => !t.get(fim + 1).is_some_and(|c| c.is_ascii_alphanumeric()),
            Some(_) => true,
        };
        if antes_ok && depois_ok {
            return true;
        }
        desde = i + 1;
    }
    false
}

/// Nome gerado por variável: um prefixo do nome (de 4 letras até uma a
/// menos que ele) colado em `$` (`wt-falhas-$g`, `wt-falhas-s$i`).
pub fn molde(nome: &str, texto: &str) -> bool {
    let fronteiras: Vec<usize> = nome.char_indices().map(|(i, _)| i).collect();
    let n = fronteiras.len();
    (4..n).rev().any(|k| {
        let prefixo = &nome[..fronteiras[k]];
        texto.contains(&format!("{prefixo}$"))
    })
}

/// A entrada fala de worktree (sem diferença de caixa).
pub fn fala_de_worktree(texto: &str) -> bool {
    texto.to_ascii_lowercase().contains("worktree")
}

/// Todas as strings dentro de um valor JSON (valores dos objetos, itens das
/// listas), na ordem em que aparecem.
pub fn textos<'a>(v: &'a Value, saida: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => saida.push(s),
        Value::Object(m) => m.values().for_each(|x| textos(x, saida)),
        Value::Array(a) => a.iter().for_each(|x| textos(x, saida)),
        _ => {}
    }
}

/// As strings de um valor, uma por linha.
pub fn textos_juntos(v: &Value) -> String {
    let mut s = Vec::new();
    textos(v, &mut s);
    s.join("\n")
}

/// O texto de um `tool_result`: a string, ou os `text` da lista.
pub fn texto_do_resultado(b: &Value) -> String {
    match b.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Os `<tool-use-id>toolu_…</tool-use-id>` de uma notificação de tarefa.
pub fn tool_use_ids_notificados(txt: &str) -> Vec<&str> {
    const ABRE: &str = "<tool-use-id>";
    const FECHA: &str = "</tool-use-id>";
    let mut saida = Vec::new();
    let mut resto = txt;
    while let Some(i) = resto.find(ABRE) {
        let depois = &resto[i + ABRE.len()..];
        let n = depois.bytes().take_while(|&b| b.is_ascii_alphanumeric() || b == b'_').count();
        let id = &depois[..n];
        if id.starts_with("toolu_") && id.len() > "toolu_".len() && depois[n..].starts_with(FECHA) {
            saida.push(id);
        }
        resto = depois;
    }
    saida
}

/// `Output is being written to: <arquivo>.output` (o Bash em segundo plano).
pub fn saida_em_segundo_plano(txt: &str) -> Option<&str> {
    const MARCA: &str = "Output is being written to: ";
    let mut resto = txt;
    while let Some(i) = resto.find(MARCA) {
        let depois = &resto[i + MARCA.len()..];
        let palavra = depois.split(char::is_whitespace).next().unwrap_or("");
        // O menor trecho sem espaço que termina em `.output`.
        if let Some(k) = palavra.char_indices().skip(1).map(|(k, _)| k).find(|&k| palavra[k..].starts_with(".output"))
        {
            return Some(&palavra[..k + ".output".len()]);
        }
        resto = depois;
    }
    None
}

/// `Wall time: 8.2 seconds` (ou `Wall time 8.2 seconds`, do code mode).
pub fn wall_time(txt: &str) -> Option<f64> {
    const MARCA: &str = "Wall time";
    let mut resto = txt;
    while let Some(i) = resto.find(MARCA) {
        let mut d = &resto[i + MARCA.len()..];
        resto = d;
        d = d.strip_prefix(':').unwrap_or(d);
        let Some(d) = d.strip_prefix(' ') else { continue };
        let n = d.bytes().take_while(|b| b.is_ascii_digit()).count();
        if n == 0 {
            continue;
        }
        let mut fim = n;
        if d[n..].starts_with('.') {
            let f = d[n + 1..].bytes().take_while(|b| b.is_ascii_digit()).count();
            if f > 0 {
                fim = n + 1 + f;
            }
        }
        if d[fim..].starts_with(" seconds") {
            return d[..fim].parse().ok();
        }
    }
    None
}

/// Os dígitos logo depois de `marca`, onde houver.
fn numero_depois<'a>(txt: &'a str, marca: &str) -> Option<&'a str> {
    let mut resto = txt;
    while let Some(i) = resto.find(marca) {
        let d = &resto[i + marca.len()..];
        let n = d.bytes().take_while(|b| b.is_ascii_digit()).count();
        if n > 0 {
            return Some(&d[..n]);
        }
        resto = d;
    }
    None
}

/// `Process running with session ID N`: o processo do exec segue vivo.
pub fn exec_vivo(cab: &str) -> Option<String> {
    numero_depois(cab, "Process running with session ID ").map(str::to_string)
}

/// `write_stdin({session_id: N` dentro de uma célula do code mode.
pub fn escreve_em_sessao(entrada: &str) -> Option<String> {
    let mut resto = entrada;
    while let Some(i) = resto.find("write_stdin(") {
        let d = resto[i + "write_stdin(".len()..].trim_start();
        resto = &resto[i + "write_stdin(".len()..];
        let Some(d) = d.strip_prefix('{') else { continue };
        let Some(d) = d.trim_start().strip_prefix("session_id") else { continue };
        let Some(d) = d.trim_start().strip_prefix(':') else { continue };
        let d = d.trim_start();
        let n = d.bytes().take_while(|b| b.is_ascii_digit()).count();
        if n > 0 {
            return Some(d[..n].to_string());
        }
    }
    None
}

/// O `parent_thread_id` dentro de um valor (o `source` de um subagente).
pub fn pai_da_thread(v: &Value) -> Option<String> {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(s)) = m.get("parent_thread_id") {
                if !s.is_empty() {
                    return Some(s.clone());
                }
            }
            m.values().find_map(pai_da_thread)
        }
        Value::Array(a) => a.iter().find_map(pai_da_thread),
        _ => None,
    }
}

/// `"✳ Nome da conversa"` → `"Nome da conversa"`: o glifo do começo é o
/// estado do Claude, não o nome. Espaços juntados, no máximo 80 letras.
pub fn titulo_limpo(t: &str) -> String {
    let junto = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let glifo = |c: char| !(c.is_alphanumeric() || c == '_' || c.is_whitespace());
    let mut cs = junto.chars();
    let sem = match (cs.next(), cs.next(), cs.next()) {
        (Some(a), Some(b), Some(' ')) if glifo(a) && glifo(b) => cs.as_str().to_string(),
        (Some(a), Some(' '), _) if glifo(a) => junto.chars().skip(2).collect(),
        _ => junto.clone(),
    };
    sem.chars().take(80).collect()
}

/// `int(str(valor))` do Python para o que vem de um `session_id`.
pub fn como_python(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(true)) => "True".into(),
        Some(Value::Bool(false)) => "False".into(),
        Some(outro) => outro.to_string(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn horarios() {
        let t = ts("2026-09-27T04:18:30.372Z").unwrap();
        assert!((t - 1790482710.372).abs() < 1e-6, "{t}");
        assert_eq!(iso(t), "2026-09-27T04:18:30.372Z");
        assert_eq!(iso(1790482710.9999996), "2026-09-27T04:18:31.000Z");
        assert_eq!(ts("2026-09-27T01:18:30-03:00"), ts("2026-09-27T04:18:30Z"));
        assert_eq!(ts_da_linha(br#"{"a":1,"timestamp":"2026-09-27T04:18:30.372Z","b":2}"#), Some(t));
        assert_eq!(ts_da_linha(br#"{"timestamp":"x","t":{"timestamp":"2026-09-27T04:18:30.372Z"}}"#), Some(t));
        assert_eq!(ts_da_linha(br#"{"timestamp":"2026-09-27"}"#), None);
    }

    #[test]
    fn nome_com_fronteira() {
        assert!(cita("wt-f-e", "ls /x/wt-f-e/"));
        assert!(!cita("wt-f-e", "git worktree add /x/wt-f-e2 HEAD"));
        assert!(!cita("wt-f-e", "x.wt-f-e"));
        assert!(cita("wt-f-e", "(wt-f-e)"));
        assert!(cita("wt-f-e", "wt-f-e. fim"));
        assert!(!cita("wt-f-e", "wt-f-e.git"));
        assert!(!cita("wt-f-e", "wt-f-e_x"));
        assert!(cita("aa", "aaa aa"));
        assert!(!cita("", "x"));
    }

    #[test]
    fn molde_com_variavel() {
        assert!(molde("wt-lote-b", r#"for g in a b; do git worktree add "$R/wt-lote-$g""#));
        assert!(molde("wt-falhas-s1", "wt-falhas-s$i"));
        assert!(!molde("wt-x", "wt-$x"), "prefixo de menos de 4 letras não vale");
        assert!(!molde("wt-lote-b", "wt-lote-b"));
    }

    #[test]
    fn marcadores() {
        assert_eq!(
            tool_use_ids_notificados("<task-notification>\n<tool-use-id>toolu_ab_1</tool-use-id>\n"),
            vec!["toolu_ab_1"]
        );
        assert_eq!(
            saida_em_segundo_plano("Command running. Output is being written to: /tmp/x/b1.output"),
            Some("/tmp/x/b1.output")
        );
        assert_eq!(wall_time("Chunk\nWall time: 8.2000 seconds\n"), Some(8.2));
        assert_eq!(wall_time("Script completed\nWall time 10.3 seconds\n"), Some(10.3));
        assert_eq!(wall_time("Wall time: x seconds\nWall time 3 seconds"), Some(3.0));
        assert_eq!(exec_vivo("Wall time: 1\nProcess running with session ID 75847\n").as_deref(), Some("75847"));
        assert_eq!(
            escreve_em_sessao("text(await tools.write_stdin({session_id:75847,chars:\"\"}));").as_deref(),
            Some("75847")
        );
        assert_eq!(escreve_em_sessao("write_stdin( { session_id : 7 })").as_deref(), Some("7"));
        assert_eq!(thread_do_rollout("rollout-2026-09-27T04-18-30-01a0e115-feab-74c2-9f34-c4dbad52a3a5.jsonl"),
                   Some("01a0e115-feab-74c2-9f34-c4dbad52a3a5"));
        assert!(thread_do_rollout("rollout-x-01a0e115-feab-74c2-9f34-c4dbad52a3a5.jsonl.zst").is_some());
        assert_eq!(thread_do_rollout("rollout-01a0e115-feab-74c2-9f34-c4dbad52a3a5.jsonl"), None);
    }

    #[test]
    fn titulos() {
        assert_eq!(titulo_limpo("✳ Minha conversa"), "Minha conversa");
        assert_eq!(titulo_limpo("  ⠂   Outra   aba "), "Outra aba");
        assert_eq!(titulo_limpo("!! x"), "x");
        assert_eq!(titulo_limpo("!!! x"), "!!! x");
        assert_eq!(titulo_limpo("Sem glifo"), "Sem glifo");
        assert_eq!(titulo_limpo(&"a".repeat(100)).len(), 80);
    }
}
