//! A conversa que o Keep hibernou pela memória volta pela fila de memória (pedido de 27/11/2026), num daemon do próprio
//! teste, com a IA falsa do keep-ia (`cargo build -p keep-ia --examples`) como `claude` e a leitura da memória num
//! arquivo (`KEEP_MEMORIA_TESTE`):
//! - a rodada que hiberna pela memória põe a aba na fila; o lugar para uma aba nova também, atrás dela;
//! - a fila não a acorda antes de caber mais uma conversa, e acorda pela ordem, uma por rodada, quando couber;
//! - uma volta só: a que a fila acordou e ninguém falou com ela desde então não volta de novo; a que falou, volta;
//! - a hibernação das 12 h e o zzz da pessoa ficam de fora, e a que a pessoa já acordou sai da fila calada;
//! - a pressão sozinha não hiberna ninguém para abrir lugar, e o swap usado não segura a fila.
#![cfg(unix)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use keep_ia::daemon;
use keep_restauro::{adormecer, fila, retrato};

const FOLGADA: &str = r#"{"totalMb":8192,"disponivelMb":4000}"#;
const APERTADA: &str = r#"{"totalMb":8192,"disponivelMb":300,"pressao":2}"#;
/// Só a pressão do kernel em alerta, com memória disponível de sobra: hibernar não abre lugar na hora.
const SO_PRESSAO: &str = r#"{"totalMb":8192,"disponivelMb":4000,"pressao":2}"#;
/// O swap bem acima da metade, o resto folgado (no macOS o usado não volta a baixar sozinho).
const SO_SWAP: &str = r#"{"totalMb":8192,"disponivelMb":4000,"swapUsadoMb":2500,"swapTotalMb":3072}"#;

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

/// As marcas de `arquivo` (acordadas, acordadas pela fila) ficam `min` minutos para trás.
fn envelhece_marcas(restauro: &Path, arquivo: &str, min: u64) {
    let arq = restauro.join(arquivo);
    let Ok(texto) = std::fs::read(&arq) else { return };
    let mut m: serde_json::Value = serde_json::from_slice(&texto).unwrap();
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

/// O que espera na fila, pelo nome.
fn na_fila() -> Vec<String> {
    fila::listar().unwrap()["fila"].as_array().unwrap().iter().map(|f| f["nome"].as_str().unwrap().to_string()).collect()
}

fn abertas(r: &serde_json::Value) -> Vec<u64> {
    r["abertas"].as_array().map(|x| x.iter().filter_map(|a| a["aba"].as_u64()).collect()).unwrap_or_default()
}

#[test]
fn a_hibernada_pela_memoria_volta_pela_fila_quando_chega_a_vez_dela() {
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
    let memoria = |l: &str| std::fs::write(&leituras, l).unwrap();
    memoria(FOLGADA);
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
    envelhece(&casa, &id_a, 50);
    envelhece(&casa, &id_b, 30);
    let ninguem = HashSet::new();
    let nome = |aba: u32| format!("a aba {aba}, hibernada pela memória");
    // A fila só tenta hibernar para andar uma vez por minuto: o teste a deixa tentar já.
    let tentar_ja = || {
        let arq = restauro.join("fila.json");
        let mut f: serde_json::Value = serde_json::from_slice(&std::fs::read(&arq).unwrap()).unwrap();
        f["aliviou_ms"] = 0.into();
        std::fs::write(&arq, f.to_string()).unwrap();
    };

    // ---- a rodada que hiberna pela memória põe a aba na fila
    std::fs::write(&leituras, format!("[{APERTADA},{FOLGADA}]")).unwrap();
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert!(situacao(&r, a).ends_with("; volta pela fila de memória)"), "{r:#}");
    assert!(dormindo(a) && !dormindo(b));
    assert_eq!(na_fila(), vec![nome(a)]);

    // ---- sem lugar, não acorda; a fila hiberna a outra para abrir lugar, e ela entra atrás
    memoria(APERTADA);
    tentar_ja();
    let r = fila::andar();
    assert!(abertas(&r).is_empty(), "{r:#}");
    assert!(dormindo(a), "não acorda antes da vez");
    assert!(dormindo(b), "a fila hibernou a outra para abrir lugar: {r:#}");
    assert_eq!(na_fila(), vec![nome(a), nome(b)]);

    // ---- com lugar, acorda pela ordem, uma por rodada, como os pedidos novos
    memoria(FOLGADA);
    let r = fila::andar();
    assert_eq!(abertas(&r), vec![a as u64], "{r:#}");
    assert!(tela_tem("A", a, &format!(r#""--resume", "{id_a}""#), 15), "a conversa voltou na mesma aba");
    assert!(!dormindo(a) && dormindo(b));
    assert_eq!(na_fila(), vec![nome(b)]);
    let r = fila::andar();
    assert_eq!(abertas(&r), vec![b as u64], "{r:#}");
    assert!(tela_tem("A", b, &format!(r#""--resume", "{id_b}""#), 15));
    assert!(na_fila().is_empty());
    let eventos = std::fs::read_to_string(restauro.join(keep_ia::geral::EVENTOS)).unwrap_or_default();
    assert!(eventos.contains("a conversa hibernada pela memória às") && eventos.contains("voltou da fila de memória"), "{eventos}");

    // ---- uma volta só: acordadas pela fila e sem fala desde então, hibernam de novo e não voltam
    assert!(tela_tem("A", a, "Pronto.", 15) && tela_tem("A", b, "Pronto.", 15));
    // Paradas há meia hora (menos que as 12 h, aqui 1 h): saem pela memória.
    envelhece(&casa, &id_a, 30);
    envelhece(&casa, &id_b, 30);
    envelhece_marcas(&restauro, "acordadas.json", 40);
    memoria(APERTADA);
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 2, "{r:#}");
    assert!(!situacao(&r, a).contains("volta pela fila"), "{r:#}");
    assert!(dormindo(a) && dormindo(b));
    assert!(na_fila().is_empty(), "{:?}", na_fila());

    // ---- a que falou depois de a fila a acordar volta de novo
    assert_eq!(adormecer::acordar("A", a).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", a, "Pronto.", 15));
    assert!(keep_ia::trocar::digita("A", a, "mais um pedido", true).unwrap());
    assert!(tela_tem("A", a, "Feito: mais um pedido", 10));
    std::thread::sleep(Duration::from_secs(3));
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert!(situacao(&r, a).ends_with("; volta pela fila de memória)"), "{r:#}");
    assert_eq!(na_fila(), vec![nome(a)]);

    // ---- a que a pessoa já acordou sai da fila calada, sem pedir memória
    assert_eq!(adormecer::acordar("A", a).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", a, "Pronto.", 15));
    let r = fila::andar();
    assert!(abertas(&r).is_empty() && r["sairam"].as_array().is_some_and(Vec::is_empty), "{r:#}");
    assert!(na_fila().is_empty());

    // ---- o zzz da pessoa e as 12 h ficam de fora
    let r = adormecer::adormecer_agora("A", Some(a), &ninguem).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert!(na_fila().is_empty(), "o zzz não põe na fila");
    assert_eq!(adormecer::acordar("A", b).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", b, "Pronto.", 15));
    envelhece(&casa, &id_b, 120);
    envelhece_marcas(&restauro, "acordadas.json", 120);
    memoria(FOLGADA);
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert!(situacao(&r, b).starts_with("adormecida (parada há 2 h)"), "{r:#}");
    assert!(na_fila().is_empty(), "as 12 h não põem na fila");

    // ---- só a pressão: a fila espera, sem hibernar ninguém para abrir lugar
    assert_eq!(adormecer::acordar("A", b).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", b, "Pronto.", 15));
    assert!(keep_ia::trocar::digita("A", b, "outro pedido", true).unwrap());
    assert!(tela_tem("A", b, "Feito: outro pedido", 10));
    std::thread::sleep(Duration::from_secs(3));
    // A `a` dormia pelo zzz: acordada pela pessoa, ela é parada há pouco e conta; a fila recebe o pedido dela outra vez.
    assert_eq!(adormecer::acordar("A", a).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", a, "Pronto.", 15));
    assert!(keep_ia::trocar::digita("A", a, "e este", true).unwrap());
    assert!(tela_tem("A", a, "Feito: e este", 10));
    std::thread::sleep(Duration::from_secs(3));
    std::fs::write(&leituras, format!("[{APERTADA},{FOLGADA}]")).unwrap();
    let r = adormecer::adormecer(&ninguem, false).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert_eq!(na_fila().len(), 1, "{r:#}");
    memoria(SO_PRESSAO);
    tentar_ja();
    // As que a pessoa acordou contam como vistas por 5 min: aqui, nenhuma, para a regra da pressão decidir sozinha.
    let _ = std::fs::remove_file(restauro.join("vistas.json"));
    let r = fila::andar();
    assert!(abertas(&r).is_empty(), "{r:#}");
    assert_eq!(r["espera"].as_str().map(|e| e.starts_with("pressão de memória em alerta")), Some(true), "{r:#}");
    let vivas = [a, b].into_iter().filter(|x| !dormindo(*x)).count();
    assert_eq!(vivas, 1, "a pressão sozinha não hibernou a outra para abrir lugar");

    // ---- o swap usado acima da metade não segura a fila: a que espera acorda
    memoria(SO_SWAP);
    let r = fila::andar();
    assert_eq!(abertas(&r).len(), 1, "{r:#}");
    assert!(na_fila().is_empty());
    assert!(!dormindo(a) && !dormindo(b));
}
