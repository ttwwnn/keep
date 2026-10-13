//! `keep nova` de verdade: um daemon do próprio teste, a IA falsa do keep-ia
//! (`cargo build -p keep-ia --examples`) como `claude` no PATH da aba, e a
//! conversa nova aberta numa aba nova, com o pedido, os argumentos de um link e o nome.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use keep_ia::daemon;
use keep_restauro::nova::{self, Conteudo, Pedido};

fn falsa() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.parent()?.join("examples").join("ia_falsa");
    p.is_file().then_some(p)
}

fn sem_espaco(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn tela_tem(ws: &str, tab: u32, texto: &str) -> bool {
    let alvo = sem_espaco(texto);
    daemon::espera(Duration::from_secs(15), Duration::from_millis(100), || {
        daemon::tela(ws, tab).ok().filter(|t| sem_espaco(t).contains(&alvo)).map(|_| ())
    })
    .is_some()
}

fn nascimento(sock: &Path) -> f64 {
    std::fs::metadata(sock).unwrap().created().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64()
}

#[test]
fn a_conversa_nova_abre_numa_aba_nova_com_o_pedido_o_link_e_o_nome() {
    let Some(falsa) = falsa() else {
        eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin, app, proj) = (raiz.join("casa"), raiz.join("bin"), raiz.join("app"), raiz.join("proj"));
    for d in [&casa, &bin, &app, &proj] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&falsa, bin.join("claude")).unwrap();
    let sock = raiz.join("k.sock");
    // SAFETY: o único teste deste arquivo.
    unsafe {
        std::env::set_var("KEEP_IA_HOME", &casa);
        std::env::set_var("KEEP_IA_ESTADO", raiz.join("ia"));
        std::env::set_var("KEEP_STATE_DIR", &app);
        std::env::set_var("KEEP_SOCKET", &sock);
        std::env::set_var("SHELL", "/bin/sh");
        // A conta da aba de quem roda os testes (o Claude dela num armazém de
        // login próprio) não passa para as abas e IAs falsas do teste.
        for v in ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "CLAUDE_CONFIG_DIR", "KEEP_IA_ESCOLHA", "CLAUDE_KIT_CONTA", "CODEX_HOME"] {
            std::env::remove_var(v);
        }
        std::env::set_var("PS1", "$ ");
        std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    }
    let server = std::sync::Arc::new(keepd::Server::bind(&sock).expect("bind"));
    let run = std::sync::Arc::clone(&server);
    std::thread::spawn(move || {
        let _ = run.run();
    });
    std::mem::forget(server);

    // Keep fechado para o workspace que não existe: recusa, sem abrir nada
    let base = keep_restauro::daemon::nova_aba("W", Some(&proj.to_string_lossy()), 120, 30, 0, 0).unwrap();
    let errado = Pedido { workspace: Some("Nao".into()), pasta: proj.clone(), conteudo: Conteudo::Pedido("x".into()), nome: None, sem_enter: false };
    assert!(nova::abrir(&errado).unwrap_err().contains("não existe"));

    // um pedido: vai para um arquivo, e a conversa o lê
    let p = Pedido {
        workspace: Some("W".into()),
        pasta: proj.clone(),
        conteudo: Conteudo::Pedido("faça a tarefa \"x\"\ncom duas linhas".into()),
        nome: Some("Tarefa X".into()),
        sem_enter: false,
    };
    let (ws, tab, linha) = nova::abrir(&p).unwrap();
    assert_eq!(ws, "W");
    assert_ne!(tab, base);
    assert!(tela_tem("W", tab, "FALSO claude"), "a conversa subiu");
    let arquivo = linha.split("Leia o arquivo ").nth(1).and_then(|r| r.split(" inteiro").next()).expect("o arquivo na linha");
    assert_eq!(std::fs::read_to_string(arquivo).unwrap(), "faça a tarefa \"x\"\ncom duas linhas\n");
    assert!(arquivo.starts_with(&casa.join(".claude/handoffs/abas").to_string_lossy().into_owned()));
    let nomes: serde_json::Value = serde_json::from_slice(&std::fs::read(app.join("names.json")).unwrap()).unwrap();
    assert_eq!(nomes["tabs"][format!("W\u{1f}{tab}")], "Tarefa X");
    assert!((nomes["daemonStart"].as_f64().unwrap() - nascimento(&sock)).abs() < 0.01);

    // um link claude-cli://: os argumentos passam inteiros, no workspace da aba usada por último
    let link = Pedido {
        workspace: None,
        pasta: proj.clone(),
        conteudo: Conteudo::Argumentos(vec!["--deep-link-origin".into(), "--prefill-b64=ZmHDp2E=".into()]),
        nome: None,
        sem_enter: false,
    };
    let (ws2, tab2, _) = nova::abrir(&link).unwrap();
    assert_eq!(ws2, "W");
    assert!(tela_tem("W", tab2, r#"args=["--deep-link-origin", "--prefill-b64=ZmHDp2E="]"#));

    // --sem-enter: digita e espera
    let espera = Pedido { workspace: Some("W".into()), pasta: proj.clone(), conteudo: Conteudo::Argumentos(vec!["--version".into()]), nome: None, sem_enter: true };
    let (_, tab3, _) = nova::abrir(&espera).unwrap();
    std::thread::sleep(Duration::from_millis(800));
    let t = daemon::tela("W", tab3).unwrap();
    assert!(t.contains("claude --version") && !t.contains("FALSO"), "{t}");
}
