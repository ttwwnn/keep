//! Um Claude Code ou Codex de mentira, para os testes da troca de IA numa
//! aba: copiado com o nome `claude` ou `codex`, ele mostra a caixa de texto
//! como os de verdade, anota a sessão e a conversa onde eles anotam
//! (debaixo de `KEEP_IA_HOME`), diz numa linha com que ambiente e argumentos
//! subiu, e sai pelo `/exit` (Claude) ou `/quit` (Codex) imprimindo o id da
//! conversa como eles imprimem. `/trabalhar` finge um turno em andamento;
//! um Esc o interrompe.

#[cfg(unix)]
fn main() {
    falsa::main();
}

#[cfg(not(unix))]
fn main() {}

#[cfg(unix)]
mod falsa {
    use std::io::{Read, Write};
    use std::path::PathBuf;

    fn casa() -> PathBuf {
        PathBuf::from(std::env::var("KEEP_IA_HOME").or_else(|_| std::env::var("HOME")).unwrap_or_default())
    }

    fn agora_ms() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
    }

    fn uuid_novo() -> String {
        let n = agora_ms() ^ ((std::process::id() as u64) << 20);
        let h = format!("{:016x}{:016x}", n.wrapping_mul(0x9e37_79b9_7f4a_7c15), n.rotate_left(17));
        format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
    }

    fn inicio_utc() -> String {
        // O procStart que o Claude grava: o nascimento do processo, em UTC.
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
        let argv: Vec<String> = std::env::args().collect();
        let papel = PathBuf::from(&argv[0]).file_name().unwrap().to_string_lossy().into_owned();
        let codex = papel.contains("codex");
        let var = |n: &str| std::env::var(n).unwrap_or_default();
        let retomada = if codex {
            argv.windows(2).find(|w| w[0] == "resume").map(|w| w[1].clone())
        } else {
            argv.windows(2).find(|w| w[0] == "--resume").map(|w| w[1].clone())
        };
        let id = retomada.clone().unwrap_or_else(uuid_novo);
        let pid = std::process::id();
        let sessao = casa().join(".claude/sessions").join(format!("{pid}.json"));
        let anota = |status: &str| {
            if codex {
                return;
            }
            let _ = std::fs::create_dir_all(sessao.parent().unwrap());
            let cwd = std::env::current_dir().unwrap_or_default();
            let _ = std::fs::write(
                &sessao,
                format!(
                    r#"{{"pid":{pid},"sessionId":"{id}","cwd":"{}","startedAt":{},"procStart":"{}","kind":"interactive","status":"{status}"}}"#,
                    cwd.display(),
                    agora_ms(),
                    inicio_utc()
                ),
            );
        };
        anota("idle");
        // A conversa, com um pedido, onde cada um a guarda.
        if codex {
            let home = std::env::var("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|_| casa().join(".codex"));
            let dir = home.join("sessions/2026/10/04");
            let _ = std::fs::create_dir_all(&dir);
            let arq = dir.join(format!("rollout-2026-10-04T00-00-00-{id}.jsonl"));
            if !arq.exists() {
                let _ = std::fs::write(
                    &arq,
                    "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"pedido feito no codex\"}]}}\n",
                );
            }
        } else {
            let dir = casa().join(".claude/projects/-falso");
            let _ = std::fs::create_dir_all(&dir);
            let arq = dir.join(format!("{id}.jsonl"));
            if !arq.exists() {
                let _ = std::fs::write(
                    &arq,
                    "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"pedido feito no claude\"}}\n",
                );
            }
        }
        let _bruto = Bruto::liga();
        let mut out = std::io::stdout();
        let resto: Vec<&String> = argv.iter().skip(1).collect();
        let _ = write!(
            out,
            "FALSO {papel} args={resto:?} escolha={} fixa={} home={} id={id}\r\n",
            var("KEEP_IA_ESCOLHA"),
            var("CLAUDE_SECURESTORAGE_CONFIG_DIR"),
            var("CODEX_HOME")
        );
        let marca = if codex { "›" } else { "❯" };
        let caixa = |out: &mut std::io::Stdout, acima: &str| {
            let linha = "─".repeat(30);
            let _ = write!(out, "{acima}\r\n{linha}\r\n{marca} \x1b7\r\n{linha}\r\n  ? for shortcuts\x1b8");
            let _ = out.flush();
        };
        caixa(&mut out, "● Pronto.");
        let mut buf = String::new();
        let mut byte = [0u8; 1];
        let mut trabalhando = false;
        while std::io::stdin().read(&mut byte).unwrap_or(0) == 1 {
            match byte[0] {
                0x1b => {
                    if trabalhando {
                        trabalhando = false;
                        anota("idle");
                        caixa(&mut out, "\r\n⎿ Interrompido.");
                    }
                }
                b'\r' | b'\n' => {
                    let cmd = std::mem::take(&mut buf);
                    match cmd.trim() {
                        "/exit" if !codex => {
                            let _ = std::fs::remove_file(&sessao);
                            let _ = write!(out, "\r\n\r\nResume this session with:\r\nclaude --resume {id}\r\n");
                            let _ = out.flush();
                            return;
                        }
                        "/quit" if codex => {
                            let _ = write!(out, "\r\n\r\nTo continue this session, run codex resume {id}\r\n");
                            let _ = out.flush();
                            return;
                        }
                        "/trabalhar" => {
                            trabalhando = true;
                            anota("busy");
                            caixa(&mut out, "\r\n✻ Pensando… (esc to interrupt)");
                        }
                        outro => caixa(&mut out, &format!("\r\n> {outro}\r\n● Feito.")),
                    }
                }
                b => {
                    buf.push(b as char);
                    let _ = out.write_all(&[b]);
                    let _ = out.flush();
                }
            }
        }
    }
}
