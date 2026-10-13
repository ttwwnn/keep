//! Um Claude Code de mentira que PAROU numa queda, para o teste do `keep continuar`: copiado com o nome
//! `claude`, anota a sessão e a conversa onde o de verdade anota (debaixo de `KEEP_IA_HOME`), grava no
//! transcrito a fala sintética de erro que o de verdade grava e a mostra acima da caixa de texto. O que se
//! digitar e confirmar com Enter vira fala da pessoa no transcrito, seguida de uma resposta sem erro.
//!
//!   FALSO_ERRO=login|rede|limite   a queda (login, se nada)
//!   FALSO_RASCUNHO=<texto>         a caixa já começa com isto digitado
//!   FALSO_OCUPADO=1                o rodapé diz "esc to interrupt"

#[cfg(unix)]
fn main() {
    parado::main();
}

#[cfg(not(unix))]
fn main() {}

#[cfg(unix)]
mod parado {
    use std::io::{Read, Write};
    use std::path::PathBuf;

    fn casa() -> PathBuf {
        PathBuf::from(std::env::var("KEEP_IA_HOME").or_else(|_| std::env::var("HOME")).unwrap_or_default())
    }

    fn agora_ms() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
    }

    fn carimbo() -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn uuid_novo() -> String {
        let n = agora_ms() ^ ((std::process::id() as u64) << 20);
        let h = format!("{:016x}{:016x}", n.wrapping_mul(0x9e37_79b9_7f4a_7c15), n.rotate_left(17));
        format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
    }

    fn inicio_utc() -> String {
        let ms = keep_ia::processos::um(std::process::id()).map(|p| p.inicio_ms).unwrap_or_else(agora_ms);
        chrono::DateTime::from_timestamp((ms / 1000) as i64, 0)
            .map(|d| d.format("%a %b %e %H:%M:%S %Y").to_string())
            .unwrap_or_default()
    }

    struct Bruto(libc::termios);

    impl Bruto {
        fn liga() -> Option<Bruto> {
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(0, &mut t) } != 0 {
                return None;
            }
            let antes = t;
            unsafe { libc::cfmakeraw(&mut t) };
            t.c_oflag |= libc::OPOST | libc::ONLCR;
            unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) };
            Some(Bruto(antes))
        }
    }

    impl Drop for Bruto {
        fn drop(&mut self) {
            unsafe { libc::tcsetattr(0, libc::TCSANOW, &self.0) };
        }
    }

    pub fn main() {
        let id = uuid_novo();
        let pid = std::process::id();
        let inicio = agora_ms();
        let sessao = casa().join(".claude/sessions").join(format!("{pid}.json"));
        let _ = std::fs::create_dir_all(sessao.parent().unwrap());
        let cwd = std::env::current_dir().unwrap_or_default();
        let _ = std::fs::write(
            &sessao,
            format!(
                r#"{{"pid":{pid},"sessionId":"{id}","cwd":"{}","startedAt":{inicio},"procStart":"{}","kind":"interactive","status":"idle"}}"#,
                cwd.display(),
                inicio_utc()
            ),
        );
        let (erro, texto) = match std::env::var("FALSO_ERRO").unwrap_or_default().as_str() {
            "rede" => ("server_error", "API Error: Connection error."),
            "limite" => ("rate_limit", "You've hit your session limit · resets 5:10pm"),
            _ => ("authentication_failed", "Not logged in · Please run /login"),
        };
        let dir = casa().join(".claude/projects/-falso");
        let _ = std::fs::create_dir_all(&dir);
        let arq = dir.join(format!("{id}.jsonl"));
        // o erro depois da abertura do processo, como no de verdade
        std::thread::sleep(std::time::Duration::from_millis(20));
        let fala = |tipo: &str, texto: &str, erro: Option<&str>| {
            let mut v = serde_json::json!({
                "type": tipo,
                "uuid": uuid_novo(),
                "timestamp": carimbo(),
                "sessionId": id,
                "message": { "role": tipo, "content": [{ "type": "text", "text": texto }] },
            });
            if let Some(e) = erro {
                v["error"] = serde_json::json!(e);
                v["isApiErrorMessage"] = serde_json::json!(true);
            }
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&arq).unwrap();
            let _ = writeln!(f, "{v}");
        };
        fala("user", "rode os testes", None);
        fala("assistant", texto, Some(erro));

        let _bruto = Bruto::liga();
        let mut out = std::io::stdout();
        let rodape = if std::env::var("FALSO_OCUPADO").is_ok() {
            "  ⏵⏵ bypass permissions on · esc to interrupt"
        } else {
            "  ⏵⏵ bypass permissions on · ← for agents"
        };
        let caixa = |out: &mut std::io::Stdout, acima: &str, digitado: &str| {
            let linha = "─".repeat(60);
            let _ = write!(out, "{acima}\r\n{linha}\r\n❯\u{a0}{digitado}\x1b7\r\n{linha}\r\n{rodape}\x1b8");
            let _ = out.flush();
        };
        let mut buf: Vec<u8> = std::env::var("FALSO_RASCUNHO").unwrap_or_default().into_bytes();
        caixa(&mut out, &format!("● Rodei os testes.\r\n  ⎿  {texto}\r\n"), &String::from_utf8_lossy(&buf));
        let mut byte = [0u8; 1];
        while std::io::stdin().read(&mut byte).unwrap_or(0) == 1 {
            match byte[0] {
                b'\r' | b'\n' => {
                    let dito = String::from_utf8_lossy(&std::mem::take(&mut buf)).into_owned();
                    fala("user", &dito, None);
                    fala("assistant", "Retomado.", None);
                    caixa(&mut out, &format!("\r\n> {dito}\r\n● Retomado."), "");
                }
                b => {
                    buf.push(b);
                    let _ = out.write_all(&[b]);
                    let _ = out.flush();
                }
            }
        }
    }
}
