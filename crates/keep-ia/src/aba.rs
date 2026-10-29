//! `keep ia aba …`: as abas do Keep para quem automatiza uma IA de fora (o
//! Jev, que escolhe modelo e esforço a cada mensagem e os digita na aba): a
//! lista, a tela, digitar e abrir uma aba, pelo próprio Keep. Quem chama não
//! fala o protocolo do daemon nem sabe onde fica o socket.
//!
//! Nunca sobe daemon: sem o Keep no ar a resposta é `sem-keepd`. O socket é
//! o de `KEEP_SOCKET`, senão o padrão (`keep ia aba socket` diz qual).
//! Digitar entra na aba com o tamanho que ela já tem, para não mexer no
//! dela, e recusa a aba que acabou.
//!
//! ```text
//! keep ia aba lista                                  → {"socket","exato","daemon","abas":[…]}
//! keep ia aba tela --ws=<W> --aba=<n> [--cores]      → {"tela"}
//! keep ia aba digitar --ws=<W> --aba=<n> < bytes     → {"bytes"}
//! keep ia aba nova --ws=<W> [--pasta=<P>] [--colunas=<c> --linhas=<l>] → {"aba"}
//! keep ia aba socket                                 → {"socket"}
//! ```

use std::io::Read;
use std::time::Duration;

use serde_json::{Value, json};

use crate::cli::{Args, recusa, responde, uso_errado};
use crate::daemon;

const USO: &str = "keep ia aba lista | tela --ws=<W> --aba=<n> [--cores] | digitar --ws=<W> --aba=<n> (bytes na entrada) | nova --ws=<W> [--pasta=<P>] [--colunas=<c> --linhas=<l>] | socket";

/// Quanto o teclado fica na aba depois de mandar: o daemon lê o que veio
/// antes de ver a conexão fechar.
const DEMORA_AO_SAIR: Duration = Duration::from_millis(150);

/// Uma aba, com os nomes do protocolo (`TabInfo`).
fn aba_json(a: &daemon::Aba) -> Value {
    let i = &a.info;
    json!({
        "ws": a.ws,
        "id": i.id,
        "title": i.title,
        "cols": i.cols,
        "rows": i.rows,
        "clients": i.clients,
        "finished": i.finished,
        "busy": i.busy,
        "splitOf": i.split_of,
        "splitDir": i.split_dir,
        "cwd": i.cwd,
        "lastActive": i.last_active,
        "command": i.command,
        "shellPid": i.shell_pid,
        "pid": i.pid,
    })
}

pub fn lista() -> Value {
    match daemon::listar() {
        Ok(r) => json!({
            "ok": true,
            "socket": daemon::socket().to_string_lossy(),
            "exato": r.exato,
            "daemon": { "pid": r.daemon.pid, "inicioMs": r.daemon.started_ms },
            "abas": r.abas.iter().map(aba_json).collect::<Vec<_>>(),
        }),
        Err(e) => recusa("sem-keepd", &e),
    }
}

/// A aba viva `ws`:`id`, ou a recusa que diz por que não.
fn viva(ws: &str, id: u32) -> Result<daemon::Aba, Value> {
    let r = daemon::listar().map_err(|e| recusa("sem-keepd", &e))?;
    r.aba(ws, id).cloned().ok_or_else(|| recusa("sem-aba", &format!("A aba {ws}:{id} não existe ou acabou.")))
}

pub fn tela(ws: &str, id: u32, cores: bool) -> Value {
    if let Err(r) = viva(ws, id) {
        return r;
    }
    let lida = if cores { daemon::tela_vt(ws, id) } else { daemon::tela(ws, id) };
    match lida {
        Ok(t) => json!({ "ok": true, "tela": t }),
        Err(e) => recusa("sem-aba", &e),
    }
}

pub fn digitar(ws: &str, id: u32, dados: &[u8]) -> Value {
    let aba = match viva(ws, id) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let enviado = daemon::Teclado::liga(&aba).and_then(|mut t| {
        t.manda(dados)?;
        std::thread::sleep(DEMORA_AO_SAIR);
        Ok(())
    });
    match enviado {
        Ok(()) => json!({ "ok": true, "bytes": dados.len() }),
        Err(e) => recusa("sem-aba", &e),
    }
}

pub fn nova(ws: &str, pasta: Option<&str>, colunas: u16, linhas: u16) -> Value {
    match daemon::nova_aba(ws, pasta, colunas, linhas) {
        Ok(id) => json!({ "ok": true, "aba": id }),
        Err(e) => recusa("sem-keepd", &e),
    }
}

fn numero<T: std::str::FromStr>(a: &Args, nome: &str) -> Option<T> {
    a.opcao(nome).and_then(|v| v.parse().ok())
}

pub fn cli(args: &[String]) -> i32 {
    let a = Args::de(args);
    let ws = a.opcao("ws").filter(|w| !w.is_empty());
    let id = numero::<u32>(&a, "aba");
    match a.posicoes.first().map(String::as_str) {
        Some("lista") => responde(lista()),
        Some("socket") => responde(json!({ "ok": true, "socket": daemon::socket().to_string_lossy() })),
        Some("tela") => match (ws, id) {
            (Some(ws), Some(id)) => responde(tela(ws, id, a.tem("cores"))),
            _ => uso_errado(USO),
        },
        Some("digitar") => match (ws, id) {
            (Some(ws), Some(id)) => {
                let mut dados = Vec::new();
                if let Err(e) = std::io::stdin().read_to_end(&mut dados) {
                    return responde(recusa("uso", &format!("não deu para ler a entrada: {e}")));
                }
                if dados.is_empty() {
                    return uso_errado("keep ia aba digitar: nada na entrada para digitar");
                }
                responde(digitar(ws, id, &dados))
            }
            _ => uso_errado(USO),
        },
        Some("nova") => match ws {
            Some(ws) => responde(nova(
                ws,
                a.opcao("pasta").filter(|p| !p.is_empty()),
                numero(&a, "colunas").unwrap_or(80),
                numero(&a, "linhas").unwrap_or(24),
            )),
            None => uso_errado(USO),
        },
        _ => uso_errado(USO),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contas::testes::casa;

    fn daemon_de_teste(dir: &std::path::Path) -> std::path::PathBuf {
        let sock = dir.join("k.sock");
        let server = std::sync::Arc::new(keepd::Server::bind(&sock).expect("bind"));
        let run = std::sync::Arc::clone(&server);
        std::thread::spawn(move || {
            let _ = run.run();
        });
        std::mem::forget(server);
        sock
    }

    fn sem_espacos(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn lista_tela_digitar_e_nova_pelo_keep() {
        let c = casa("aba");
        // Sem daemon: nada sobe, a resposta diz.
        // SAFETY: a casa (e a trava) está de pé.
        unsafe { std::env::set_var("KEEP_SOCKET", c.raiz.join("nada.sock")) };
        assert_eq!(lista()["motivo"], "sem-keepd");
        assert!(!c.raiz.join("nada.sock").exists(), "o keep ia aba não sobe daemon");

        let sock = daemon_de_teste(&c.raiz);
        let shell_antes = std::env::var_os("SHELL");
        unsafe {
            std::env::set_var("KEEP_SOCKET", &sock);
            std::env::set_var("SHELL", "/bin/sh");
            std::env::set_var("PS1", "$ ");
        }
        let aberta = nova("T", Some(&c.raiz.to_string_lossy()), 100, 30);
        let id = aberta["aba"].as_u64().expect("o número da aba nova") as u32;
        let l = lista();
        assert_eq!(l["socket"], sock.to_string_lossy().as_ref());
        let aba = l["abas"].as_array().unwrap().iter().find(|a| a["ws"] == "T" && a["id"] == id).cloned().expect("a aba na lista");
        assert_eq!((aba["cols"].as_u64(), aba["rows"].as_u64(), aba["finished"].as_bool()), (Some(100), Some(30), Some(false)));

        assert_eq!(digitar("T", id, b"echo jev-$((40+2))\r")["ok"], true);
        let viu = daemon::espera(Duration::from_secs(15), Duration::from_millis(100), || {
            let t = tela("T", id, false);
            t["tela"].as_str().filter(|t| sem_espacos(t).contains("jev-42")).map(str::to_string)
        });
        assert!(viu.is_some(), "o que foi digitado rodou na aba: {}", tela("T", id, false));
        let com_cores = tela("T", id, true);
        assert!(com_cores["tela"].as_str().is_some_and(|t| sem_espacos(t).contains("jev-42")), "{com_cores}");
        // O tamanho da aba é o dela: digitar não o mudou.
        let depois = lista()["abas"].as_array().unwrap().iter().find(|a| a["id"] == id).cloned().unwrap();
        assert_eq!((depois["cols"].as_u64(), depois["rows"].as_u64()), (Some(100), Some(30)));

        for r in [tela("T", 999, false), digitar("T", 999, b"x"), tela("Outro", id, false)] {
            assert_eq!(r["motivo"], "sem-aba", "{r}");
        }
        unsafe {
            match shell_antes {
                Some(s) => std::env::set_var("SHELL", s),
                None => std::env::remove_var("SHELL"),
            }
            std::env::remove_var("PS1");
        }
    }
}
