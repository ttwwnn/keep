//! A aba geral só hiberna sozinha quando as outras abas do workspace com conversa já hibernaram (pedido de
//! 27/11/2026), num daemon do próprio teste, com a IA falsa do keep-ia (`cargo build -p keep-ia --examples`) como
//! `claude` e como `codex`:
//! - pelas 12 h (aqui, 1 h): a geral parada fica enquanto outra aba tem conversa viva, do Claude ou do Codex; a aba só
//!   com terminal não a segura; parada a outra também, as duas saem na mesma rodada, a geral por último;
//! - pela memória (a rodada e o lugar para uma aba nova): o mesmo;
//! - o zzz da pessoa vale para a geral como para as outras.
#![cfg(unix)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use keep_ia::daemon;
use keep_restauro::{adormecer, retrato};

const FOLGADA: &str = r#"{"totalMb":16384,"disponivelMb":12000}"#;
const APERTADA: &str = r#"{"totalMb":8192,"disponivelMb":300,"pressao":2}"#;

fn falsa() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.parent()?.join("examples").join("ia_falsa");
    p.is_file().then_some(p)
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

fn conversa_da_aba(tab: u32) -> String {
    daemon::espera(Duration::from_secs(10), Duration::from_millis(200), || {
        let (_, ia) = keep_ia::abas::ler().ok()?;
        ia.into_iter().find(|i| i.aba == tab)?.conversa
    })
    .expect("a conversa da aba")
}

/// A última fala da conversa fica `min` minutos para trás (a IA falsa não põe hora nas falas: vale o arquivo).
fn envelhece(casa: &Path, conversa: &str, min: u64) {
    let arq = casa.join(".claude/projects/-falso").join(format!("{conversa}.jsonl"));
    let f = std::fs::OpenOptions::new().append(true).open(&arq).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(min * 60)).unwrap();
}

/// O acordar de cada aba fica `min` minutos para trás: a parada conta dele.
fn envelhece_acordar(restauro: &Path, min: u64) {
    let arq = restauro.join("acordadas.json");
    let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(&arq).unwrap()).unwrap();
    for x in m["abas"].as_array_mut().unwrap() {
        x["em_ms"] = (x["em_ms"].as_u64().unwrap() - min * 60_000).into();
    }
    std::fs::write(&arq, m.to_string()).unwrap();
}

fn situacao(r: &serde_json::Value, aba: u32) -> String {
    let rotulo = format!("A#{aba}");
    r["abas"].as_array().unwrap().iter().find(|a| a["aba"] == rotulo.as_str()).and_then(|a| a["situacao"].as_str()).unwrap_or("(não vista)").into()
}

fn dormindo(aba: u32) -> bool {
    let l = daemon::listar().unwrap();
    let k = retrato::keepd_da_lista(&l, &keep_ia::worktrees::ambiente::Ambiente::do_sistema());
    adormecer::do_keepd(&k).iter().any(|a| a.aba == aba)
}

#[test]
fn a_geral_fica_enquanto_outra_aba_tem_conversa_viva_e_sai_depois_delas() {
    let Some(falsa) = falsa() else {
        eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin, app, restauro) = (raiz.join("casa"), raiz.join("bin"), raiz.join("app"), raiz.join("restauro"));
    for d in [&casa.join(".claude"), &casa.join(".codex"), &bin, &app, &restauro] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&falsa, bin.join("claude")).unwrap();
    std::fs::copy(&falsa, bin.join("codex")).unwrap();
    // o Chaveiro vazio: nada de login de verdade
    let security = raiz.join("security");
    std::fs::write(&security, "#!/bin/sh\nexit 44\n").unwrap();
    std::fs::set_permissions(&security, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let leituras = raiz.join("memoria.json");
    std::fs::write(&leituras, FOLGADA).unwrap();
    // SAFETY: o único teste deste arquivo; nada mais lê o ambiente ao mesmo tempo.
    unsafe {
        std::env::set_var("KEEP_IA_SECURITY", &security);
        for v in ["KEEP_IA_TOKEN_URL", "KEEP_IA_PERFIL_URL", "KEEP_IA_CLAUDE_URL", "KEEP_IA_CODEX_URL"] {
            std::env::set_var(v, "http://127.0.0.1:9/nada");
        }
        std::env::set_var("KEEP_IA_HOME", &casa);
        std::env::set_var("KEEP_IA_ESTADO", raiz.join("ia"));
        std::env::set_var("KEEP_RESTAURO_ESTADO", &restauro);
        std::env::set_var("KEEP_STATE_DIR", &app);
        std::env::set_var("KEEP_IA_CLAUDE_BIN", bin.join("claude"));
        std::env::set_var("KEEP_MEMORIA_TESTE", &leituras);
        // As 12 h viram 1 h, e os 10 min da memória apertada, 2 s.
        std::env::set_var("KEEP_ADORMECER_PARADA_S", "3600");
        std::env::set_var("KEEP_ADORMECER_APERTO_S", "2");
        std::env::set_var("SHELL", "/bin/sh");
        for v in ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "CLAUDE_CONFIG_DIR", "KEEP_IA_ESCOLHA", "CLAUDE_KIT_CONTA", "CODEX_HOME"] {
            std::env::remove_var(v);
        }
        std::env::set_var("PS1", "$ ");
        std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    }
    let sock = raiz.join("k.sock");
    let server = std::sync::Arc::new(keepd::Server::bind(&sock).expect("bind"));
    let run = std::sync::Arc::clone(&server);
    std::thread::spawn(move || {
        let _ = run.run();
    });
    std::mem::forget(server);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock) };

    let nova = || {
        let aba = daemon::nova_aba("A", Some(&raiz.to_string_lossy()), 120, 30).unwrap();
        assert!(tela_tem("A", aba, "$", 15), "o prompt");
        aba
    };
    let abre = |programa: &str| {
        let aba = nova();
        assert!(keep_ia::trocar::digita("A", aba, &bin.join(programa).display().to_string(), true).unwrap());
        assert!(tela_tem("A", aba, "Pronto.", 15));
        aba
    };
    let g = abre("claude");
    let id_g = conversa_da_aba(g);
    let o = abre("claude");
    let id_o = conversa_da_aba(o);
    // Uma aba só com terminal, que o Keep nunca hiberna: não segura a geral.
    let _terminal = nova();
    let l = daemon::listar().unwrap();
    let keepd = retrato::keepd_da_lista(&l, &keep_ia::worktrees::ambiente::Ambiente::do_sistema());
    let geral = serde_json::json!({ "keepd": keepd, "abas": [{ "workspace": "A", "aba": g, "conversa": id_g }] });
    std::fs::write(restauro.join("geral.json"), geral.to_string()).unwrap();
    let ninguem = HashSet::new();

    // ---- pelas 12 h: a geral parada há 2 h fica, porque a outra ainda tem conversa viva
    envelhece(&casa, &id_g, 120);
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert_eq!(situacao(&r, g), format!("geral: a aba {o} ainda tem conversa viva (parada há 2 h)"), "{r:#}");
    assert!(!dormindo(g));

    // ---- uma conversa viva do Codex também a segura
    envelhece(&casa, &id_o, 120);
    let codex = abre("codex");
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 1, "só a outra: {r:#}");
    assert!(situacao(&r, o).starts_with("adormecida (parada há 2 h)"), "{r:#}");
    assert_eq!(situacao(&r, g), format!("geral: a aba {codex} ainda tem conversa viva (parada há 2 h)"), "{r:#}");
    assert!(dormindo(o) && !dormindo(g));
    daemon::fecha_aba("A", codex).unwrap();
    std::thread::sleep(Duration::from_millis(500));

    // ---- todas as outras hibernadas (a aba só com terminal não conta): a geral sai
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert!(situacao(&r, g).starts_with("adormecida (parada há 2 h)"), "{r:#}");
    assert!(dormindo(g));

    // ---- na mesma rodada: paradas as duas, a outra sai primeiro e a geral logo depois
    for aba in [g, o] {
        assert_eq!(adormecer::acordar("A", aba).unwrap()["acordadas"], 1);
        assert!(tela_tem("A", aba, "--resume", 15) && tela_tem("A", aba, "Pronto.", 15));
    }
    // Acordadas há 2 h, sem fala desde então: a parada conta do acordar.
    envelhece(&casa, &id_g, 120);
    envelhece(&casa, &id_o, 120);
    envelhece_acordar(&restauro, 120);
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 2, "{r:#}");
    assert!(situacao(&r, g).starts_with("adormecida (parada há 2 h)"), "{r:#}");
    assert!(dormindo(g) && dormindo(o));

    // ---- pela memória: a outra trabalhando segura a geral, na rodada e no lugar para uma aba nova
    for aba in [g, o] {
        assert_eq!(adormecer::acordar("A", aba).unwrap()["acordadas"], 1);
        assert!(tela_tem("A", aba, "--resume", 15) && tela_tem("A", aba, "Pronto.", 15));
        assert!(keep_ia::trocar::digita("A", aba, "de novo", true).unwrap());
        assert!(tela_tem("A", aba, "Feito: de novo", 10));
    }
    assert!(keep_ia::trocar::digita("A", o, "/trabalhar", true).unwrap());
    assert!(tela_tem("A", o, "esc to interrupt", 10));
    std::thread::sleep(Duration::from_secs(3));
    std::fs::write(&leituras, APERTADA).unwrap();
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert!(situacao(&r, o).contains("trabalhando"), "{r:#}");
    assert_eq!(situacao(&r, g), format!("geral: a aba {o} ainda tem conversa viva (parada há 0 min)"), "{r:#}");
    // O lugar para uma aba nova poupa as abas que o app mostrou há pouco (o acordar as marca): aqui, nenhuma.
    let _ = std::fs::remove_file(restauro.join("vistas.json"));
    let r = adormecer::abrir_vaga(1).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert_eq!(situacao(&r, g), format!("geral: a aba {o} ainda tem conversa viva (parada há 0 min)"), "{r:#}");
    assert!(!dormindo(g));

    // ---- o zzz da pessoa vale para a geral, com a outra viva
    let r = adormecer::adormecer_agora("A", Some(g), &ninguem).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert!(dormindo(g));

    // ---- a outra termina e para: pela memória saem as duas, a geral por último
    assert_eq!(adormecer::acordar("A", g).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", g, "Pronto.", 15));
    assert!(keep_ia::trocar::digita("A", g, "mais uma", true).unwrap());
    assert!(tela_tem("A", g, "Feito: mais uma", 10));
    keep_ia::trocar::tecla("A", o, b"\x1b").unwrap();
    assert!(tela_tem("A", o, "Interrompido.", 10));
    std::thread::sleep(Duration::from_secs(3));
    let _ = std::fs::remove_file(restauro.join("vistas.json"));
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 2, "{r:#}");
    assert!(situacao(&r, o).starts_with("adormecida pela memória"), "{r:#}");
    assert!(situacao(&r, g).starts_with("adormecida pela memória"), "{r:#}");
    assert!(dormindo(g) && dormindo(o));
}
