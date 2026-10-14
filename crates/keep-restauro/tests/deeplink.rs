//! Um link `claude-cli://` de verdade (`deeplink::abrir`): um daemon do próprio teste, a IA falsa do keep-ia
//! (`cargo build -p keep-ia --examples`) como `claude`, e a aba que o link abre. O texto do link, mesmo cheio de aspas
//! e de metacaracteres de shell, chega ao `claude` só em base64; um link recusado não abre aba nenhuma.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use keep_ia::daemon;
use keep_restauro::deeplink;

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

fn codifica(texto: &str) -> String {
    texto.bytes().map(|b| format!("%{b:02X}")).collect()
}

fn abas_vivas() -> usize {
    daemon::listar().unwrap().abas.iter().filter(|a| !a.info.finished).count()
}

#[test]
fn o_link_abre_uma_aba_com_o_texto_so_em_base64() {
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
    keep_restauro::daemon::nova_aba("W", Some(&proj.to_string_lossy()), 120, 30, 0, 0).unwrap();
    let antes = abas_vivas();

    // Um texto que um shell leria como comandos: chega só em base64.
    let perigoso = "oi\"; touch /tmp/invadido & $(id) `x` | rm -rf /\nsegunda linha";
    let url = format!("claude-cli://open?q={}&cwd={}&repo=ttwwnn%2Fkeep&x=--dangerously-skip-permissions", codifica(perigoso), codifica(&proj.to_string_lossy()));
    let (ws, tab) = deeplink::abrir(&url, 0).unwrap();
    assert_eq!(ws, "W");
    assert_eq!(abas_vivas(), antes + 1);
    let b64_pedido = deeplink::base64_url(perigoso.as_bytes());
    let b64_pasta = deeplink::base64_url(proj.to_string_lossy().as_bytes());
    let esperado = format!(
        r#"args=["--deep-link-origin", "--deep-link-repo=ttwwnn/keep", "--deep-link-cwd-b64={b64_pasta}", "--prefill-b64={b64_pedido}"]"#
    );
    assert!(tela_tem("W", tab, &esperado), "{}", daemon::tela("W", tab).unwrap_or_default());
    let tela = daemon::tela("W", tab).unwrap();
    assert!(!tela.contains("dangerously") && !tela.contains("invadido"), "nada do texto nem do parâmetro de fora na linha: {tela}");
    assert!(!Path::new("/tmp/invadido").exists());

    // Sem pasta: abre na pasta pessoal e não avisa o Claude de pasta nenhuma.
    let (_, tab2) = deeplink::abrir("claude-cli://open?q=oi", 0).unwrap();
    assert!(tela_tem("W", tab2, r#"args=["--deep-link-origin", "--prefill-b64=b2k"]"#));
    // Uma pasta que não existe: o mesmo.
    let inexistente = proj.join("nao-existe");
    let (_, tab3) = deeplink::abrir(&format!("claude-cli://open?q=oi&cwd={}", codifica(&inexistente.to_string_lossy())), 0).unwrap();
    assert!(tela_tem("W", tab3, r#"args=["--deep-link-origin", "--prefill-b64=b2k"]"#));

    // Recusados: nenhuma aba nasce.
    let antes = abas_vivas();
    for ruim in [
        "http://open?q=x".to_string(),
        "claude-cli://install-plugin?plugin=x".to_string(),
        "claude-cli://open?cwd=%2Fa%2F..%2Fb".to_string(),
        "claude-cli://open?q=a%00b".to_string(),
        format!("claude-cli://open?q={}", "a".repeat(5001)),
    ] {
        assert!(deeplink::abrir(&ruim, 0).is_err(), "{ruim}");
    }
    assert_eq!(abas_vivas(), antes);

    // O log diz o que houve, sem o texto do pedido.
    let log = std::fs::read_to_string(raiz.join("ia/links.log")).unwrap();
    assert!(log.contains("aberto em W") && log.contains("recusado"), "{log}");
    assert!(!log.contains("segunda linha"), "o texto do pedido não vai ao log");
}
