//! O adormecer de verdade, num daemon do próprio teste, com a IA falsa do keep-ia (`cargo build -p keep-ia
//! --examples`) como `claude` em quatro abas: só a parada, livre e fora de vista sai da memória; a aba fica
//! no shell, o retrato a leva com a conversa, e o acordar a retoma com as mesmas opções, respondendo a
//! pergunta de confiança na pasta que a conversa retomada faz. Depois de um
//! reinício (outro daemon) ela volta adormecida e acorda do mesmo jeito.
#![cfg(unix)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use keep_ia::daemon;
use keep_restauro::{adormecer, restaurar, retrato};

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

fn conversa_da_aba(tab: u32) -> String {
    daemon::espera(Duration::from_secs(10), Duration::from_millis(200), || {
        let (_, ia) = keep_ia::abas::ler().ok()?;
        ia.into_iter().find(|i| i.aba == tab)?.conversa
    })
    .expect("a conversa da aba")
}

/// A última fala da conversa fica para trás (a IA falsa não põe hora nas falas: vale o arquivo).
fn envelhece(casa: &Path, conversa: &str) {
    let arq = casa.join(".claude/projects/-falso").join(format!("{conversa}.jsonl"));
    let f = std::fs::OpenOptions::new().append(true).open(&arq).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(2 * 3600)).unwrap();
}

fn situacao<'a>(r: &'a serde_json::Value, aba: u32) -> &'a str {
    let rotulo = format!("A#{aba}");
    r["abas"].as_array().unwrap().iter().find(|a| a["aba"] == rotulo.as_str()).and_then(|a| a["situacao"].as_str()).unwrap_or("(não vista)")
}

#[test]
fn so_a_aba_parada_e_livre_fora_de_vista_adormece_e_volta_ao_ser_mostrada() {
    let Some(falsa) = falsa() else {
        eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin, app) = (raiz.join("casa"), raiz.join("bin"), raiz.join("app"));
    for d in [&casa.join(".claude"), &bin, &app] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&falsa, bin.join("claude")).unwrap();
    // o Chaveiro vazio: nada de login de verdade
    let security = raiz.join("security");
    std::fs::write(&security, "#!/bin/sh\nexit 44\n").unwrap();
    std::fs::set_permissions(&security, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    // SAFETY: o único teste deste arquivo; nada mais lê o ambiente ao mesmo tempo.
    unsafe {
        std::env::set_var("KEEP_IA_SECURITY", &security);
        for v in ["KEEP_IA_TOKEN_URL", "KEEP_IA_PERFIL_URL", "KEEP_IA_CLAUDE_URL", "KEEP_IA_CODEX_URL"] {
            std::env::set_var(v, "http://127.0.0.1:9/nada");
        }
        std::env::set_var("KEEP_IA_HOME", &casa);
        std::env::set_var("KEEP_IA_ESTADO", raiz.join("ia"));
        std::env::set_var("KEEP_RESTAURO_ESTADO", raiz.join("restauro"));
        std::env::set_var("KEEP_STATE_DIR", &app);
        std::env::set_var("KEEP_IA_CLAUDE_BIN", bin.join("claude"));
        std::env::set_var("KEEP_ADORMECER_PARADA_S", "3600");
        // A conversa retomada pergunta se confia na pasta, como o Claude 2.1.30x numa pasta nova.
        std::env::set_var("FALSA_CONFIANCA", "1");
        std::env::set_var("SHELL", "/bin/sh");
        for v in ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "CLAUDE_CONFIG_DIR", "KEEP_IA_ESCOLHA", "CLAUDE_KIT_CONTA", "CODEX_HOME"] {
            std::env::remove_var(v);
        }
        std::env::set_var("PS1", "$ ");
    }
    let sock1 = raiz.join("k1.sock");
    sobe_daemon(&sock1);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock1) };

    let claude = format!("{} --effort max", bin.join("claude").display());
    // O título que o Claude dá à aba (OSC 0), posto pelo shell antes da IA falsa, que não diz nenhum.
    let abre = || {
        let aba = daemon::nova_aba("A", Some(&raiz.to_string_lossy()), 120, 30).unwrap();
        assert!(tela_tem("A", aba, "$", 15), "o prompt");
        let linha = format!(r"printf '\033]0;✳ Conversa {aba}\007'; {claude}");
        assert!(keep_ia::trocar::digita("A", aba, &linha, true).unwrap());
        assert!(tela_tem("A", aba, "FALSO claude", 15));
        assert!(tela_tem("A", aba, "Pronto.", 15));
        (aba, conversa_da_aba(aba))
    };
    let (parada, id) = abre();
    let (vista, id_vista) = abre();
    let (recente, _) = abre();
    let (ocupada, id_ocupada) = abre();
    for c in [&id, &id_vista, &id_ocupada] {
        envelhece(&casa, c);
    }
    assert!(keep_ia::trocar::digita("A", ocupada, "/trabalhar", true).unwrap());
    assert!(tela_tem("A", ocupada, "esc to interrupt", 10));
    let vistas: HashSet<(String, u32)> = [("A".to_string(), vista)].into();
    let nome_no_app = |aba: u32| -> Option<String> {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(app.join("names.json")).ok()?).ok()?;
        v["tabs"][format!("A\u{1f}{aba}")].as_str().map(str::to_string)
    };

    // ---- no seco, só diz
    let seco = adormecer::adormecer(&vistas, true).unwrap();
    assert!(situacao(&seco, parada).starts_with("adormeceria"), "{seco:#}");
    assert!(!tela_tem("A", parada, "Resume this session", 1), "o seco não sai do Claude");

    // ---- de verdade: só a parada, livre e fora de vista
    let r = adormecer::adormecer(&vistas, false).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert_eq!(situacao(&r, vista), "à vista", "{r:#}");
    assert!(situacao(&r, recente).starts_with("parada há 0 min"), "{r:#}");
    assert!(situacao(&r, ocupada).contains("trabalhando"), "{r:#}");
    assert!(tela_tem("A", parada, &format!("claude --resume {id}"), 10), "saiu pelo /exit");
    let l = daemon::listar().unwrap();
    assert!(matches!(keep_ia::tela::programa(&l.aba("A", parada).unwrap().info.command), keep_ia::tela::Programa::Shell(_)));
    let k1 = retrato::keepd_da_lista(&l, &keep_ia::worktrees::ambiente::Ambiente::do_sistema());
    let guardadas = adormecer::do_keepd(&k1);
    assert_eq!(guardadas.len(), 1, "{guardadas:?}");
    assert_eq!((guardadas[0].aba, guardadas[0].ia.conversa.as_str()), (parada, id.as_str()));
    assert_eq!(guardadas[0].ia.parametros, vec!["--effort", "max"]);
    // Sem o Claude a aba perde o título: fica com o da conversa como nome.
    assert_eq!(guardadas[0].nome_posto.as_deref(), Some(format!("Conversa {parada}").as_str()));
    assert_eq!(nome_no_app(parada), Some(format!("Conversa {parada}")));

    // ---- o retrato leva a conversa da adormecida
    let foto = retrato::fotografar().unwrap();
    let a = foto.abas.iter().find(|a| a.aba == parada).unwrap();
    assert!(a.adormecida, "{a:?}");
    assert_eq!(a.ia.as_ref().map(|i| i.conversa.as_str()), Some(id.as_str()));

    // ---- outra rodada não a perde
    let _ = adormecer::adormecer(&vistas, false).unwrap();
    assert_eq!(adormecer::do_keepd(&k1).len(), 1);

    // ---- agora: o workspace pedido, sem esperar as 12 h; o que trabalha e o que está à vista ficam
    let r = adormecer::adormecer_agora("B", None, &vistas).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    // a aba pedida, não as outras do workspace; sem o filtro da aba a `recente` também sairia
    let r = adormecer::adormecer_agora("A", Some(ocupada), &HashSet::new()).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert!(situacao(&r, ocupada).contains("trabalhando"), "{r:#}");
    assert_eq!(situacao(&r, recente), "(não vista)", "{r:#}");
    // pedida pela pessoa, a recusa vai em palavras, sem o "parada há…" da rodada; aba sem Claude também diz por quê
    assert!(!situacao(&r, ocupada).contains("parada há"), "{r:#}");
    let r = adormecer::adormecer_agora("A", Some(9999), &vistas).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert_eq!(situacao(&r, 9999), "o Keep não achou o Claude nesta aba", "{r:#}");
    let r = adormecer::adormecer_agora("A", Some(recente), &vistas).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert_eq!(adormecer::do_keepd(&k1).len(), 2);
    // pedir de novo a aba que já dorme diz isso, não "não achei o Claude"
    let r = adormecer::adormecer_agora("A", Some(recente), &vistas).unwrap();
    assert_eq!(situacao(&r, recente), "a conversa já está hibernada", "{r:#}");
    let ia_recente = adormecer::do_keepd(&k1).into_iter().find(|a| a.aba == recente).unwrap().ia;
    // a lista que o app do Windows pergunta: as duas adormecidas, e só elas
    let mut dormem: Vec<u64> = adormecer::listar().unwrap()["abas"].as_array().unwrap().iter().map(|a| a["aba"].as_u64().unwrap()).collect();
    dormem.sort();
    assert_eq!(dormem, vec![parada as u64, recente as u64]);
    assert_eq!(adormecer::acordar("A", recente).unwrap()["acordadas"], 1);
    assert_eq!(adormecer::listar().unwrap()["abas"].as_array().unwrap().len(), 1, "a que acordou sai da lista");
    assert!(tela_tem("A", recente, "Pronto.", 15));
    // a corrida: mostrar a aba enquanto o Claude ainda sai dela (a entrada já gravada, o Claude vivo) não perde a conversa
    adormecer::registrar(&k1, "A", recente, ia_recente, None);
    let r = adormecer::acordar("A", recente).unwrap();
    assert_eq!(r["acordadas"], 0, "{r:#}");
    assert!(r["falhas"][0].as_str().unwrap().contains("ainda está saindo"), "{r:#}");
    assert!(adormecer::do_keepd(&k1).iter().any(|a| a.aba == recente), "a entrada volta: a conversa não fica sem dono");
    // e o que a aba faz depois segue valendo: dorme de verdade, sem entrada em dobro, e acorda
    let r = adormecer::adormecer_agora("A", Some(recente), &vistas).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert_eq!(adormecer::do_keepd(&k1).iter().filter(|a| a.aba == recente).count(), 1);
    assert_eq!(adormecer::acordar("A", recente).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", recente, "Pronto.", 15));
    // sem aba, as livres do workspace; o que trabalha e o que está à vista ficam
    let r = adormecer::adormecer_agora("A", None, &vistas).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert!(situacao(&r, ocupada).contains("trabalhando"), "{r:#}");
    assert_eq!(situacao(&r, vista), "à vista", "{r:#}");
    assert!(tela_tem("A", recente, "Resume this session", 10), "a recente saiu pelo /exit");
    assert_eq!(adormecer::do_keepd(&k1).len(), 2);
    assert_eq!(adormecer::acordar("A", recente).unwrap()["acordadas"], 1);
    assert!(tela_tem("A", recente, "Pronto.", 15));

    // ---- outra aba mostrada: nada acorda; a dela, acorda com as mesmas opções
    assert_eq!(adormecer::acordar("A", vista).unwrap()["acordadas"], 0);
    let r = adormecer::acordar("A", parada).unwrap();
    assert_eq!(r["acordadas"], 1, "{r:#}");
    assert!(tela_tem("A", parada, &format!(r#""--effort", "max", "--resume", "{id}""#), 15));
    assert!(adormecer::do_keepd(&k1).is_empty());
    assert_eq!(nome_no_app(parada), None, "o nome posto sai: o Claude diz o título de novo");
    assert_eq!(adormecer::acordar("A", parada).unwrap()["acordadas"], 0, "nada em dobro");

    // ---- adormece de novo e o computador reinicia
    assert!(tela_tem("A", parada, "Pronto.", 15));
    for t in [vista, recente, ocupada] {
        daemon::fecha_aba("A", t).unwrap();
    }
    // O nome que a pessoa deu fica, e não é trocado pelo título.
    let nasceu = std::fs::metadata(&sock1).unwrap().created().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    std::fs::write(
        app.join("names.json"),
        format!(r#"{{"daemonStart":{nasceu},"tabs":{{"A\u001f{parada}":"Meu nome"}},"workspaces":{{}}}}"#),
    )
    .unwrap();
    let r = adormecer::adormecer(&HashSet::new(), false).unwrap();
    assert_eq!(r["adormecidas"], 1, "{r:#}");
    assert!(tela_tem("A", parada, "Resume this session", 10));
    assert_eq!(nome_no_app(parada).as_deref(), Some("Meu nome"));
    assert_eq!(adormecer::do_keepd(&k1)[0].nome_posto, None);
    retrato::gravar().unwrap();
    let sock2 = raiz.join("k2.sock");
    sobe_daemon(&sock2);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock2) };
    let f1 = restaurar::restaurar(true, false).unwrap();
    assert_eq!(f1.abas, 1, "{f1:?}");
    let f2 = restaurar::restaurar(false, false).unwrap();
    assert_eq!((f2.conversas, f2.falhas.len()), (0, 0), "volta adormecida: {f2:?}");
    let nova = f1.criadas[0].aba;
    assert!(tela_tem("A", nova, "$", 15));
    assert!(!tela_tem("A", nova, "FALSO claude", 2), "não subiu sozinha");
    assert_eq!(nome_no_app(nova).as_deref(), Some("Meu nome"), "o nome volta com a aba");
    let r = adormecer::acordar("A", nova).unwrap();
    assert_eq!(r["acordadas"], 1, "{r:#}");
    assert!(tela_tem("A", nova, &format!(r#""--effort", "max", "--resume", "{id}""#), 15));
    assert_eq!(nome_no_app(nova).as_deref(), Some("Meu nome"), "o nome dado pela pessoa fica ao acordar");
}
