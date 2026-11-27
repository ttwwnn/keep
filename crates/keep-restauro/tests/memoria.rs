//! O controle da memória de verdade, num daemon do próprio teste, com a IA falsa do keep-ia (`cargo build -p keep-ia
//! --examples`) como `claude` e a leitura da memória num arquivo (`KEEP_MEMORIA_TESTE`):
//! - com a memória folgada, as paradas há pouco ficam; apertada, a rodada as hiberna, a parada há mais tempo primeiro
//!   e a aba geral por último (só depois das outras: `geral_hiberna`), uma por vez, medindo de novo entre elas, até
//!   aliviar;
//! - a aba acordada não volta a dormir antes da mensagem que a acordou; depois dela, sim;
//! - `keep nova` sem memória, e sem parada a hibernar, põe o pedido na fila e diz por quê; a fila abre a aba quando
//!   houver memória, e a aba geral fica sabendo; no teto de conversas vivas da RAM, o mesmo.
#![cfg(unix)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use keep_ia::daemon;
use keep_restauro::{adormecer, fila, nova, retrato};

const FOLGADA: &str = r#"{"totalMb":8192,"disponivelMb":4000}"#;
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

/// As próximas leituras da memória, uma por vez (a última fica).
fn memoria(arq: &Path, leituras: &[&str]) {
    std::fs::write(arq, format!("[{}]", leituras.join(","))).unwrap();
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
fn a_memoria_apertada_hiberna_as_paradas_segura_as_abas_novas_e_poupa_a_recem_acordada() {
    let Some(falsa) = falsa() else {
        eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin, app, restauro) = (raiz.join("casa"), raiz.join("bin"), raiz.join("app"), raiz.join("restauro"));
    for d in [&casa.join(".claude"), &bin, &app, &restauro] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&falsa, bin.join("claude")).unwrap();
    // o Chaveiro vazio: nada de login de verdade
    let security = raiz.join("security");
    std::fs::write(&security, "#!/bin/sh\nexit 44\n").unwrap();
    std::fs::set_permissions(&security, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let leituras = raiz.join("memoria.json");
    memoria(&leituras, &[FOLGADA]);
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

    let claude = bin.join("claude").display().to_string();
    let abre = || {
        let aba = daemon::nova_aba("A", Some(&raiz.to_string_lossy()), 120, 30).unwrap();
        assert!(tela_tem("A", aba, "$", 15), "o prompt");
        assert!(keep_ia::trocar::digita("A", aba, &claude, true).unwrap());
        assert!(tela_tem("A", aba, "Pronto.", 15));
        (aba, conversa_da_aba(aba))
    };
    let (a, id_a) = abre();
    let (b, id_b) = abre();
    let (g, id_g) = abre();
    let (v, _) = abre();
    envelhece(&casa, &id_a, 50);
    envelhece(&casa, &id_b, 30);
    envelhece(&casa, &id_g, 55);
    // A `g` é a aba geral do workspace: com a memória apertada, sai por último.
    let l = daemon::listar().unwrap();
    let keepd = retrato::keepd_da_lista(&l, &keep_ia::worktrees::ambiente::Ambiente::do_sistema());
    let geral = serde_json::json!({ "keepd": keepd, "abas": [{ "workspace": "A", "aba": g, "conversa": id_g }] });
    std::fs::write(restauro.join("geral.json"), geral.to_string()).unwrap();
    let vistas: HashSet<(String, u32)> = [("A".to_string(), v)].into();

    // ---- folgada: as paradas há menos de 1 h ficam
    let r = adormecer::adormecer(&vistas, false).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert_eq!(situacao(&r, a), "parada há 50 min", "{r:#}");
    assert_eq!(situacao(&r, v), "à vista", "{r:#}");
    assert!(r["memoria"]["aperto"].is_null(), "{r:#}");

    // ---- apertada: a parada há mais tempo primeiro (a geral por último), uma por vez, medindo de novo, até aliviar
    memoria(&leituras, &[APERTADA, APERTADA, FOLGADA]);
    let r = adormecer::adormecer(&vistas, false).unwrap();
    assert_eq!(r["adormecidas"], 2, "{r:#}");
    // Saem pela memória sem ter terminado, até onde o Keep sabe: voltam pela fila (tests/fila_acorda.rs).
    assert_eq!(situacao(&r, a), "adormecida pela memória (pressão de memória em alerta; parada há 50 min; volta pela fila de memória)", "{r:#}");
    assert_eq!(situacao(&r, b), "adormecida pela memória (pressão de memória em alerta; parada há 30 min; volta pela fila de memória)", "{r:#}");
    assert_eq!(situacao(&r, g), "parada há 55 min", "a geral fica: aliviou antes dela: {r:#}");
    assert_eq!(situacao(&r, v), "à vista", "{r:#}");
    assert!(tela_tem("A", a, &format!("claude --resume {id_a}"), 10), "saiu pelo /exit");
    assert!(dormindo(a) && dormindo(b) && !dormindo(g));

    // ---- a acordada não volta a dormir antes da mensagem que a acordou
    assert_eq!(adormecer::acordar("A", a).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", a, &format!(r#""--resume", "{id_a}""#), 15));
    assert!(tela_tem("A", a, "Pronto.", 15));
    std::thread::sleep(Duration::from_secs(3));
    memoria(&leituras, &[APERTADA]);
    let r = adormecer::adormecer(&vistas, false).unwrap();
    assert!(situacao(&r, a).starts_with("acordada há 0 min, à espera da mensagem que a acordou"), "{r:#}");
    assert!(!dormindo(a), "{r:#}");
    // A geral, a única parada que sobrou, fica: a acordada e a que está à vista têm conversa viva.
    assert!(situacao(&r, g).starts_with(&format!("geral: as abas {a}, {v} ainda têm conversa viva")), "{r:#}");
    assert!(!dormindo(g));
    // A mensagem chega: a parada conta dela, e passada a parada a aba volta a poder dormir.
    assert!(keep_ia::trocar::digita("A", a, "oi de novo", true).unwrap());
    assert!(tela_tem("A", a, "Feito: oi de novo", 10));
    std::thread::sleep(Duration::from_secs(3));
    let r = adormecer::adormecer(&vistas, false).unwrap();
    assert!(situacao(&r, a).starts_with("adormecida pela memória (pressão de memória em alerta; parada há 0 min"), "{r:#}");
    assert!(dormindo(a));

    // ---- `keep nova` sem memória, e sem parada a hibernar (só a que está à vista): o pedido vai para a fila
    let pedido = |nome: &str| nova::Pedido {
        workspace: Some("A".into()),
        pasta: raiz.clone(),
        conteudo: nova::Conteudo::Pedido(format!("faça {nome}")),
        nome: Some(nome.into()),
        sem_enter: false,
    };
    // Aqui, só os pedidos novos: as abas hibernadas pela memória acima, que esperam na fila para voltar, saem dela.
    std::fs::remove_file(restauro.join("fila.json")).unwrap();
    let antes = daemon::listar().unwrap().abas.len();
    match fila::nova(&pedido("Tarefa Y")).unwrap() {
        fila::Desfecho::NaFila { porque, resumo, posicao } => {
            assert_eq!(porque, "pressão de memória em alerta");
            assert_eq!(resumo, "0,3 GB livres, 2 conversas vivas", "a que está à vista e a geral, que ela segura");
            assert_eq!(posicao, 1);
        }
        fila::Desfecho::Aberta(..) => panic!("abriu sem memória"),
    }
    match fila::nova(&pedido("Tarefa W")).unwrap() {
        fila::Desfecho::NaFila { posicao, .. } => assert_eq!(posicao, 2, "atrás do que já espera"),
        fila::Desfecho::Aberta(..) => panic!("abriu sem memória"),
    }
    assert_eq!(daemon::listar().unwrap().abas.len(), antes, "nenhuma aba nova");
    assert_eq!(fila::listar().unwrap()["fila"].as_array().map(Vec::len), Some(2));
    assert!(tela_tem("A", v, "Pronto.", 1) && !dormindo(v), "a que está à vista fica");

    // ---- a fila anda quando houver memória, e a aba geral fica sabendo; o pedido parado há 13 h abre do mesmo jeito
    let arq = restauro.join("fila.json");
    let mut f: serde_json::Value = serde_json::from_slice(&std::fs::read(&arq).unwrap()).unwrap();
    f["itens"][0]["desde_ms"] = (f["itens"][0]["desde_ms"].as_u64().unwrap() - 13 * 3600 * 1000).into();
    std::fs::write(&arq, f.to_string()).unwrap();
    memoria(&leituras, &[FOLGADA]);
    let r = fila::andar();
    assert_eq!(r["abertas"].as_array().map(Vec::len), Some(1), "{r:#}");
    let nova_aba = r["abertas"][0]["aba"].as_u64().unwrap() as u32;
    assert!(tela_tem("A", nova_aba, "FALSO claude", 15), "a conversa subiu");
    assert!(tela_tem("A", nova_aba, "Leia o arquivo", 5), "com o pedido");
    // Uma por rodada: a conversa que acabou de subir ainda não pesa na medição; a seguinte abre na próxima rodada.
    assert_eq!(fila::listar().unwrap()["fila"].as_array().map(Vec::len), Some(1));
    let r = fila::andar();
    assert_eq!(r["abertas"].as_array().map(Vec::len), Some(1), "{r:#}");
    assert_eq!(fila::listar().unwrap()["fila"].as_array().map(Vec::len), Some(0));
    let eventos = std::fs::read_to_string(restauro.join(keep_ia::geral::EVENTOS)).unwrap_or_default();
    assert!(eventos.contains("abriu da fila de memória o pedido das") && eventos.contains("Tarefa Y") && eventos.contains("Tarefa W"), "{eventos}");

    // ---- no teto de conversas vivas da RAM (2 GB: uma só), o mesmo, sem a memória apertada
    memoria(&leituras, &[r#"{"totalMb":2048,"disponivelMb":1800}"#]);
    match fila::nova(&pedido("Tarefa Z")).unwrap() {
        fila::Desfecho::NaFila { porque, .. } => assert_eq!(porque, "teto de 1 conversa viva para 2,0 GB de RAM"),
        fila::Desfecho::Aberta(..) => panic!("abriu acima do teto"),
    }
}
