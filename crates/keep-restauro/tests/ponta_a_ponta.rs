//! A restauração de verdade, com dois daemons do próprio teste (o "antes" e o
//! "depois" do reinício) e a IA falsa do keep-ia (`cargo build -p keep-ia
//! --examples`) no lugar do Claude: o retrato do primeiro, as abas, o painel,
//! o nome e a conversa de volta no segundo, sem nada digitado à mão.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use keep_ia::daemon;
use keep_restauro::{restaurar, retrato};

fn falsa() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.parent()?.join("examples").join("ia_falsa");
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

fn nascimento(sock: &Path) -> f64 {
    let m = std::fs::metadata(sock).unwrap();
    let t = m.created().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap();
    t.as_secs_f64()
}

#[test]
fn as_abas_o_painel_o_nome_e_a_conversa_voltam_depois_do_reinicio() {
    let Some(falsa) = falsa() else {
        eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin, app) = (raiz.join("casa"), raiz.join("bin"), raiz.join("app"));
    for d in [&casa, &bin, &app, &raiz.join("p1"), &raiz.join("p2"), &raiz.join("p3")] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&falsa, bin.join("claude")).unwrap();
    // SAFETY: o único teste deste arquivo; nada mais lê o ambiente ao mesmo tempo.
    unsafe {
        std::env::set_var("KEEP_IA_HOME", &casa);
        std::env::set_var("KEEP_IA_ESTADO", raiz.join("ia"));
        std::env::set_var("KEEP_RESTAURO_ESTADO", raiz.join("restauro"));
        std::env::set_var("KEEP_STATE_DIR", &app);
        std::env::set_var("KEEP_IA_CLAUDE_BIN", bin.join("claude"));
        std::env::set_var("SHELL", "/bin/sh");
        std::env::set_var("PS1", "$ ");
    }

    // ---- antes do reinício
    let sock1 = raiz.join("k1.sock");
    sobe_daemon(&sock1);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock1) };
    let p = |n: &str| raiz.join(n).to_string_lossy().into_owned();
    let conversa_aba = daemon::nova_aba("A", Some(&p("p1")), 120, 30).unwrap();
    assert!(tela_tem("A", conversa_aba, "$", 15), "o prompt");
    let mao = format!("{} --effort max", bin.join("claude").display());
    assert!(keep_ia::trocar::digita("A", conversa_aba, &mao, true).unwrap());
    assert!(tela_tem("A", conversa_aba, "FALSO claude", 15));
    let painel = keep_restauro::daemon::nova_aba("A", Some(&p("p2")), 120, 30, conversa_aba, 2).unwrap();
    let solta = daemon::nova_aba("B", Some(&p("p3")), 100, 25).unwrap();
    assert!(tela_tem("B", solta, "$", 15));
    std::fs::write(
        app.join("names.json"),
        // como o app grava: o separador escapado no JSON
        format!(r#"{{"daemonStart":{},"tabs":{{"A\u001f{conversa_aba}":"Minha conversa"}},"workspaces":{{}}}}"#, nascimento(&sock1)),
    )
    .unwrap();
    let id = daemon::espera(Duration::from_secs(10), Duration::from_millis(200), || {
        let (_, ia) = keep_ia::abas::ler().ok()?;
        ia.into_iter().find(|i| i.aba == conversa_aba)?.conversa
    })
    .expect("a conversa da aba");

    let (r1, _) = retrato::gravar().unwrap();
    assert_eq!(r1.abas.len(), 3, "{r1:?}");
    let a = r1.abas.iter().find(|a| a.aba == conversa_aba).unwrap();
    let ia = a.ia.as_ref().expect("a IA da aba");
    assert_eq!((ia.agente.as_str(), ia.conversa.as_str()), ("claude", id.as_str()));
    assert_eq!(ia.parametros, vec!["--effort", "max"]);
    assert_eq!(a.nome.as_deref(), Some("Minha conversa"));
    let pa = r1.abas.iter().find(|a| a.aba == painel).unwrap();
    assert_eq!((pa.split_of, pa.split_dir, pa.cwd.as_str()), (conversa_aba, 2, p("p2").as_str()));

    // ---- o reinício: outro daemon, vazio
    let sock2 = raiz.join("k2.sock");
    sobe_daemon(&sock2);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock2) };

    let seco = restaurar::restaurar(false, true).unwrap();
    assert_eq!(seco.falhas.len(), 3, "{seco:?}");
    assert!(daemon::listar().unwrap().abas.is_empty(), "o seco não cria nada");

    // fase 1: as abas, na mesma pasta, o painel ao lado da mãe, o nome
    let f1 = restaurar::restaurar(true, false).unwrap();
    assert_eq!((f1.abas, f1.conversas), (3, 0), "{f1:?}");
    let l = daemon::listar().unwrap();
    let a_ws: Vec<_> = l.abas.iter().filter(|a| a.ws == "A").collect();
    let mae = a_ws.iter().find(|a| a.info.split_of == 0).expect("a aba-mãe");
    let filho = a_ws.iter().find(|a| a.info.split_of != 0).expect("o painel");
    assert_eq!((filho.info.split_of, filho.info.split_dir), (mae.info.id, 2));
    assert!(tela_tem("A", mae.info.id, "$", 15));
    assert_eq!(l.abas.iter().filter(|a| a.ws == "B").count(), 1);
    let nomes: serde_json::Value = serde_json::from_slice(&std::fs::read(app.join("names.json")).unwrap()).unwrap();
    assert!((nomes["daemonStart"].as_f64().unwrap() - nascimento(&sock2)).abs() < 0.01, "{nomes}");
    assert_eq!(nomes["tabs"][format!("A\u{1f}{}", mae.info.id)], "Minha conversa", "{nomes}");

    // fase 2: a conversa, retomada direto, com os parâmetros
    let f2 = restaurar::restaurar(false, false).unwrap();
    assert_eq!((f2.abas, f2.conversas, f2.falhas.len()), (0, 1, 0), "{f2:?}");
    assert!(tela_tem("A", mae.info.id, &format!(r#""--effort", "max", "--resume", "{id}""#), 15));
    assert!(daemon::tela("A", filho.info.id).unwrap().contains('$'), "o painel fica no shell");

    // de novo: nada em dobro
    let f3 = restaurar::restaurar(false, false).unwrap();
    assert_eq!(f3.motivo, "já restaurado neste keepd");
    assert_eq!(daemon::listar().unwrap().abas.len(), 3);

    // o retrato do keepd novo leva o do anterior para anterior.json
    let (r2, g) = retrato::gravar().unwrap();
    assert_eq!(g, retrato::Gravacao::GiraEGrava);
    assert_eq!(r2.abas.len(), 3);
    assert!(raiz.join("restauro/anterior.json").is_file());
}
