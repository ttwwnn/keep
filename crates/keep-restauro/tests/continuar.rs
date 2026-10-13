//! O `keep continuar` de verdade, num daemon do próprio teste, com o Claude parado de mentira
//! (`cargo build -p keep-restauro --examples`) em quatro abas: duas paradas que podem receber o
//! "continue" (login, rede), uma com rascunho na caixa e uma ocupada. Enquanto o login e a rede não
//! voltam, nada é digitado; quando voltam, só as duas recebem, a mensagem entra no transcrito, e a
//! rodada seguinte não digita de novo.
#![cfg(unix)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use keep_ia::daemon;
use keep_restauro::continuar::{self, MSG_LOGIN, MSG_REDE};
use serde_json::Value;

fn parado() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.parent()?.join("examples").join("claude_parado");
    p.is_file().then_some(p)
}

fn sobe_daemon(sock: &Path) {
    let server = std::sync::Arc::new(keepd::Server::bind(sock).expect("bind"));
    let run = std::sync::Arc::clone(&server);
    std::thread::spawn(move || {
        let _ = run.run();
    });
    std::mem::forget(server);
}

fn sem_espaco(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn tela_tem(ws: &str, tab: u32, texto: &str, segundos: u64) -> bool {
    let alvo = sem_espaco(texto);
    daemon::espera(Duration::from_secs(segundos), Duration::from_millis(100), || {
        daemon::tela(ws, tab).ok().filter(|t| sem_espaco(t).contains(&alvo)).map(|_| ())
    })
    .is_some()
}

/// Uma API de mentira: responde 200 a quem chamar.
fn api_falsa() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let mut c = c;
            let mut buf = [0u8; 2048];
            let _ = c.read(&mut buf);
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        }
    });
    format!("http://127.0.0.1:{porta}/api/hello")
}

fn situacao<'a>(r: &'a Value, aba: &str) -> &'a str {
    r["conversas"].as_array().unwrap().iter().find(|c| c["aba"] == aba).and_then(|c| c["situacao"].as_str()).unwrap_or("(não vista)")
}

#[test]
fn so_a_aba_parada_e_segura_recebe_o_continue_quando_a_causa_volta() {
    let Some(parado) = parado() else {
        eprintln!("sem o Claude parado (cargo build -p keep-restauro --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin) = (raiz.join("casa"), raiz.join("bin"));
    for d in [&casa.join(".claude"), &bin] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&parado, bin.join("claude")).unwrap();
    // o Chaveiro vazio: o login global sai do .credentials.json da casa falsa
    let security = raiz.join("security");
    std::fs::write(&security, "#!/bin/sh\nexit 44\n").unwrap();
    std::fs::set_permissions(&security, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    // SAFETY: o único teste deste arquivo; nada mais lê o ambiente ao mesmo tempo.
    unsafe {
        std::env::set_var("KEEP_IA_HOME", &casa);
        std::env::set_var("KEEP_IA_ESTADO", raiz.join("ia"));
        std::env::set_var("KEEP_IA_SECURITY", &security);
        std::env::set_var("KEEP_RESTAURO_ESTADO", raiz.join("restauro"));
        std::env::set_var("KEEP_STATE_DIR", raiz.join("app"));
        std::env::set_var("KEEP_CONTINUAR_OCIOSO_S", "0");
        // a rede "caída": nada escuta nesta porta
        std::env::set_var("KEEP_CONTINUAR_SONDA", "http://127.0.0.1:9/api/hello");
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("PS1", "$ ");
    }
    let sock = raiz.join("k.sock");
    sobe_daemon(&sock);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock) };

    let claude = bin.join("claude").display().to_string();
    let abre = |ambiente: &str| {
        let aba = daemon::nova_aba("C", Some(&raiz.to_string_lossy()), 120, 30).unwrap();
        assert!(tela_tem("C", aba, "$", 15), "o prompt");
        assert!(keep_ia::trocar::digita("C", aba, &format!("{ambiente} {claude}"), true).unwrap());
        assert!(tela_tem("C", aba, "Rodei os testes", 15), "o Claude parado na aba {aba}");
        aba
    };
    let login = abre("FALSO_ERRO=login");
    let rede = abre("FALSO_ERRO=rede");
    let rascunho = abre("FALSO_RASCUNHO=meu-rascunho");
    let ocupada = abre("FALSO_OCUPADO=1");
    let rot = |a: u32| format!("C#{a}");
    // o erro precisa de 5 s de idade
    std::thread::sleep(Duration::from_millis(5500));

    // ---- o login e a rede ainda não voltaram
    let r = continuar::rodada(false, false).unwrap();
    assert_eq!(r["enviadas"], 0, "{r:#}");
    assert!(situacao(&r, &rot(login)).contains("o login ainda não voltou"), "{r:#}");
    assert!(situacao(&r, &rot(rede)).contains("a rede ainda não voltou"), "{r:#}");

    // ---- voltaram: no seco, diz o que faria e não digita
    std::fs::write(
        casa.join(".claude/.credentials.json"),
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"a","refreshToken":"r","expiresAt":{},"subscriptionType":"max"}}}}"#,
            keep_restauro::agora_ms() + 3_600_000
        ),
    )
    .unwrap();
    unsafe { std::env::set_var("KEEP_CONTINUAR_SONDA", api_falsa()) };
    let seco = continuar::rodada(true, false).unwrap();
    assert!(situacao(&seco, &rot(login)).starts_with("mandaria continuar"), "{seco:#}");
    assert!(!tela_tem("C", login, "Retomado", 1), "o seco não digita");

    // ---- de verdade: só as duas paradas e seguras
    let r = continuar::rodada(false, false).unwrap();
    assert_eq!((r["enviadas"].as_u64(), r["confirmadas"].as_u64()), (Some(2), Some(2)), "{r:#}");
    assert!(tela_tem("C", login, &format!("> {MSG_LOGIN}"), 5));
    assert!(tela_tem("C", rede, &format!("> {MSG_REDE}"), 5));
    assert!(situacao(&r, &rot(rascunho)).contains("caixa com texto"), "{r:#}");
    assert!(situacao(&r, &rot(ocupada)).contains("ocupada"), "{r:#}");
    assert!(!tela_tem("C", rascunho, "Retomado", 1));
    assert!(!tela_tem("C", ocupada, "Retomado", 1));

    // ---- a conversa andou: nada em dobro
    let r = continuar::rodada(false, false).unwrap();
    assert_eq!(r["enviadas"], 0, "{r:#}");
    assert_eq!(situacao(&r, &rot(login)), "última fala: assistant", "{r:#}");
    let log = std::fs::read_to_string(raiz.join("restauro/continuar.log")).unwrap();
    assert!(log.contains(&format!("{}: o login de volta -> mandei continuar (1ª) -- a mensagem entrou", rot(login))), "{log}");
}
