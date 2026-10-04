//! Pergunta ao daemon do Codex se a conversa de uma aba está num turno, e
//! o interrompe.
//!
//! Desde o 0.159 o TUI do Codex é só um cliente do `codex app-server
//! --managed-daemon`: o turno roda no daemon, e o `/quit` fecha só a tela —
//! o trabalho segue sem ninguém ver. A tela é um indício; o daemon é a prova.
//!
//! Protocolo (o do TUI): WebSocket no socket unix
//! `<CODEX_HOME>/app-server-control/app-server-control.sock` (um link; a
//! conexão vai pelo alvo, que é curto), quadros de texto JSON-RPC:
//! `initialize` → `initialized` → `thread/read` → `thread/turns/list` →
//! `turn/interrupt`. Nunca sobe daemon nem conversa: sem socket ou sem
//! resposta no prazo, a resposta é `None` e vale o que a tela disser.
//! No Windows, por ora, sempre `None`.

use std::path::Path;

/// O turno da conversa no daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turno {
    /// Trabalhando.
    Ativo,
    /// Parado esperando aprovação ou resposta.
    Pergunta,
    /// Ociosa, ou não carregada: nada rodando.
    Parado,
}

#[cfg(unix)]
mod ws {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};

    pub struct Conexao {
        s: UnixStream,
        buf: Vec<u8>,
        proximo: u64,
        prazo: Duration,
    }

    fn aleatorio(n: usize) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let semente = format!("{:?}-{}", Instant::now(), std::process::id());
        Sha256::digest(semente.as_bytes()).iter().take(n).copied().collect()
    }

    impl Conexao {
        pub fn abre(home: &Path, prazo: Duration) -> std::io::Result<Conexao> {
            let link = home.join("app-server-control").join("app-server-control.sock");
            let alvo = std::fs::canonicalize(&link)?;
            let s = UnixStream::connect(alvo)?;
            s.set_read_timeout(Some(prazo))?;
            s.set_write_timeout(Some(prazo))?;
            let mut c = Conexao { s, buf: Vec::new(), proximo: 0, prazo };
            use base64::Engine;
            let chave = base64::engine::general_purpose::STANDARD.encode(aleatorio(16));
            write!(
                c.s,
                "GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {chave}\r\nSec-WebSocket-Version: 13\r\n\r\n"
            )?;
            let mut resposta = Vec::new();
            let mut pedaco = [0u8; 4096];
            while !resposta.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = c.s.read(&mut pedaco)?;
                if n == 0 {
                    return Err(std::io::Error::other("o daemon fechou no aperto de mão"));
                }
                resposta.extend_from_slice(&pedaco[..n]);
            }
            let fim = resposta.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(0) + 4;
            let primeira = String::from_utf8_lossy(&resposta[..fim]).lines().next().unwrap_or("").to_string();
            if !primeira.contains(" 101 ") {
                return Err(std::io::Error::other(format!("o daemon recusou o WebSocket: {primeira}")));
            }
            c.buf = resposta[fim..].to_vec();
            c.pede("initialize", json!({ "clientInfo": { "name": "keep", "version": "1" } }))?;
            c.envia(&json!({ "method": "initialized" }))?;
            Ok(c)
        }

        fn envia(&mut self, obj: &Value) -> std::io::Result<()> {
            let dados = serde_json::to_vec(obj)?;
            let mascara = aleatorio(4);
            let mut quadro = vec![0x81u8];
            let n = dados.len();
            if n < 126 {
                quadro.push(0x80 | n as u8);
            } else if n < 65536 {
                quadro.push(0x80 | 126);
                quadro.extend_from_slice(&(n as u16).to_be_bytes());
            } else {
                quadro.push(0x80 | 127);
                quadro.extend_from_slice(&(n as u64).to_be_bytes());
            }
            quadro.extend_from_slice(&mascara);
            quadro.extend(dados.iter().enumerate().map(|(i, b)| b ^ mascara[i % 4]));
            self.s.write_all(&quadro)
        }

        /// (opcode, carga) do próximo quadro, ou `None` no fim do prazo.
        fn quadro(&mut self, fim: Instant) -> Option<(u8, Vec<u8>)> {
            loop {
                if self.buf.len() >= 2 {
                    let mut n = (self.buf[1] & 0x7f) as usize;
                    let mut pos = 2;
                    let mut pronto = true;
                    if n == 126 {
                        if self.buf.len() >= 4 {
                            n = u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize;
                            pos = 4;
                        } else {
                            pronto = false;
                        }
                    } else if n == 127 {
                        if self.buf.len() >= 10 {
                            n = u64::from_be_bytes(self.buf[2..10].try_into().ok()?) as usize;
                            pos = 10;
                        } else {
                            pronto = false;
                        }
                    }
                    if pronto && self.buf.len() >= pos + n {
                        let op = self.buf[0] & 0x0f;
                        let carga = self.buf[pos..pos + n].to_vec();
                        self.buf.drain(..pos + n);
                        return Some((op, carga));
                    }
                }
                if Instant::now() >= fim {
                    return None;
                }
                let mut pedaco = [0u8; 65536];
                match self.s.read(&mut pedaco) {
                    Ok(0) => return None,
                    Ok(n) => self.buf.extend_from_slice(&pedaco[..n]),
                    Err(_) => return None,
                }
            }
        }

        /// O `result` da resposta; erro com a mensagem do daemon.
        pub fn pede(&mut self, metodo: &str, params: Value) -> std::io::Result<Value> {
            self.proximo += 1;
            let id = self.proximo;
            self.envia(&json!({ "id": id, "method": metodo, "params": params }))?;
            let fim = Instant::now() + self.prazo;
            loop {
                let Some((op, carga)) = self.quadro(fim) else {
                    return Err(std::io::Error::other(format!("o daemon não respondeu a {metodo}")));
                };
                if op == 8 {
                    return Err(std::io::Error::other("o daemon fechou a conexão"));
                }
                if op != 1 {
                    continue;
                }
                let Ok(msg) = serde_json::from_slice::<Value>(&carga) else { continue };
                if msg.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(e) = msg.get("error") {
                    let texto = e.get("message").and_then(Value::as_str).unwrap_or("erro do daemon");
                    return Err(std::io::Error::other(texto.to_string()));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    pub fn status(c: &mut Conexao, tid: &str) -> std::io::Result<Value> {
        match c.pede("thread/read", json!({ "threadId": tid })) {
            Ok(r) => Ok(r.get("thread").and_then(|t| t.get("status")).cloned().unwrap_or(Value::Null)),
            // Conversa que o daemon não carregou não está rodando.
            Err(e) if e.to_string().contains("not loaded") => Ok(json!({ "type": "notLoaded" })),
            Err(e) => Err(e),
        }
    }
}

/// O turno da conversa `tid` no daemon do Codex de `home`, ou `None` quando
/// não há como saber.
pub fn turno(home: &Path, tid: &str) -> Option<Turno> {
    #[cfg(unix)]
    {
        use serde_json::Value;
        let mut c = ws::Conexao::abre(home, std::time::Duration::from_secs(2)).ok()?;
        let st = ws::status(&mut c, tid).ok()?;
        match st.get("type").and_then(Value::as_str) {
            Some("active") => {
                let flags = st.get("activeFlags").and_then(Value::as_array).cloned().unwrap_or_default();
                let pergunta = flags.iter().any(|f| matches!(f.as_str(), Some("waitingOnApproval" | "waitingOnUserInput")));
                Some(if pergunta { Turno::Pergunta } else { Turno::Ativo })
            }
            Some("idle" | "notLoaded") => Some(Turno::Parado),
            _ => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (home, tid);
        None
    }
}

/// Interrompe o turno em andamento e espera o daemon dizer que parou.
/// `Some(true)` parou (ou não havia turno), `Some(false)` seguiu rodando,
/// `None` sem como falar com o daemon.
pub fn interromper(home: &Path, tid: &str) -> Option<bool> {
    #[cfg(unix)]
    {
        use serde_json::{Value, json};
        use std::time::{Duration, Instant};
        let mut c = ws::Conexao::abre(home, Duration::from_secs(2)).ok()?;
        let ativo = |c: &mut ws::Conexao| -> Option<bool> {
            Some(ws::status(c, tid).ok()?.get("type").and_then(Value::as_str) == Some("active"))
        };
        if !ativo(&mut c)? {
            return Some(true);
        }
        let turnos = c.pede("thread/turns/list", json!({ "threadId": tid, "limit": 5 })).ok()?;
        for t in turnos.get("data").and_then(Value::as_array).into_iter().flatten() {
            if t.get("status").and_then(Value::as_str) == Some("inProgress") {
                if let Some(id) = t.get("id").and_then(Value::as_str) {
                    c.pede("turn/interrupt", json!({ "threadId": tid, "turnId": id })).ok()?;
                }
            }
        }
        let fim = Instant::now() + Duration::from_secs(15);
        loop {
            if !ativo(&mut c)? {
                return Some(true);
            }
            if Instant::now() >= fim {
                return Some(false);
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (home, tid);
        None
    }
}
