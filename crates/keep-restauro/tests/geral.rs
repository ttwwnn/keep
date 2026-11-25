//! A aba geral de verdade, num daemon do próprio teste, com a IA falsa do keep-ia (`cargo build -p keep-ia
//! --examples`) como `claude`: a rodada abre a aba geral no workspace com conversa viva e lhe dá o nome dele; a
//! aba aberta depois dela que terminou é levada a ela (digitada na caixa), a que já existia só entra nas
//! novidades; o gancho entrega o papel e as novidades só à aba geral; fechar recusa a aba geral e fecha a outra;
//! fechada a aba geral, outra é aberta.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use keep_ia::daemon;
use keep_restauro::{geral, nova};

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

fn gerais(restauro: &Path) -> Vec<keep_ia::geral::Geral> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(restauro.join("geral.json")).unwrap()).unwrap();
    serde_json::from_value(v["abas"].clone()).unwrap()
}

#[test]
fn a_aba_geral_abre_sabe_das_outras_confere_as_novas_e_fecha_as_resolvidas() {
    let Some(falsa) = falsa() else {
        eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let raiz = dir.path().canonicalize().unwrap();
    let (casa, bin, app, restauro) = (raiz.join("casa"), raiz.join("bin"), raiz.join("app"), raiz.join("restauro"));
    for d in [&casa.join(".claude"), &bin, &app] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::copy(&falsa, bin.join("claude")).unwrap();
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
        std::env::set_var("KEEP_RESTAURO_ESTADO", &restauro);
        std::env::set_var("KEEP_STATE_DIR", &app);
        std::env::set_var("KEEP_IA_CLAUDE_BIN", bin.join("claude"));
        // A memória folgada (a da máquina que roda o teste não decide se a aba geral abre).
        let memoria = raiz.join("memoria.json");
        std::fs::write(&memoria, r#"{"totalMb":16384,"disponivelMb":12000}"#).unwrap();
        std::env::set_var("KEEP_MEMORIA_TESTE", &memoria);
        // A resposta vai à aba geral sem os 10 min de espera.
        std::env::set_var("KEEP_GERAL_QUIETO_S", "0");
        // Conversa nova demais: ainda sem aba geral (o teste muda para 0 depois de ver).
        std::env::set_var("KEEP_GERAL_IDADE_S", "3600");
        // A pessoa longe do teclado: o aviso pode ser digitado.
        std::env::set_var("KEEP_CONTINUAR_OCIOSO_S", "999");
        std::env::set_var("SHELL", "/bin/sh");
        for v in ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "CLAUDE_CONFIG_DIR", "KEEP_IA_ESCOLHA", "CLAUDE_KIT_CONTA", "CODEX_HOME"] {
            std::env::remove_var(v);
        }
        std::env::set_var("PS1", "$ ");
        std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    }
    let sock = raiz.join("k.sock");
    sobe_daemon(&sock);
    unsafe { std::env::set_var("KEEP_SOCKET", &sock) };

    // ---- um workspace só de shell não ganha aba geral; um com conversa viva, sim
    let shell = daemon::nova_aba("S", Some(&raiz.to_string_lossy()), 120, 30).unwrap();
    assert!(tela_tem("S", shell, "$", 15));
    // Larga: o aviso cabe numa linha da caixa do Claude falso, que não a redesenha ao quebrar a linha como o de
    // verdade (as abas que a rodada abre tomam este tamanho).
    let antiga = daemon::nova_aba("A", Some(&raiz.to_string_lossy()), 220, 30).unwrap();
    assert!(tela_tem("A", antiga, "$", 15));
    assert!(keep_ia::trocar::digita("A", antiga, "claude", true).unwrap());
    assert!(tela_tem("A", antiga, "Pronto.", 15));

    // O nome que a pessoa deu ao workspace no app é o da aba geral.
    std::fs::write(app.join("names.json"), r#"{"workspaces":{"A":"Projeto A"}}"#).unwrap();

    let cedo = geral::rodada(true).unwrap();
    assert_eq!(cedo["relato"][0]["situacao"], "conversas novas demais; espero", "{cedo:#}");
    unsafe { std::env::set_var("KEEP_GERAL_IDADE_S", "0") };

    let seco = geral::rodada(true).unwrap();
    assert_eq!(seco["relato"][0]["situacao"], "abriria a aba geral", "{seco:#}");
    assert!(!restauro.join("geral.json").exists() && !restauro.join("geral.log").exists(), "o seco não grava nem registra");

    let r = geral::rodada(false).unwrap();
    let lista = r["gerais"].as_array().unwrap();
    assert_eq!(lista.len(), 1, "{r:#}");
    assert_eq!(lista[0]["workspace"], "A", "{r:#}");
    let g = lista[0]["aba"].as_u64().unwrap() as u32;
    assert_ne!(g, antiga);
    assert!(tela_tem("A", g, "Pronto.", 15), "a aba geral roda o Claude");
    let nomes: serde_json::Value = serde_json::from_slice(&std::fs::read(app.join("names.json")).unwrap()).unwrap();
    assert_eq!(nomes["tabs"][format!("A\u{1f}{g}")], "Projeto A", "a aba geral leva o nome do workspace: {nomes:#}");
    assert_eq!(geral::listar().unwrap()["abas"], serde_json::json!([{ "workspace": "A", "aba": g }]));

    // ---- a rodada seguinte sabe a conversa dela; outra não abre outra
    let r = geral::rodada(false).unwrap();
    assert_eq!(r["gerais"].as_array().unwrap().len(), 1, "{r:#}");
    let conversa_g = gerais(&restauro)[0].conversa.clone();
    assert!(!conversa_g.is_empty());

    // ---- uma aba aberta depois (como a aba geral abre), e a pessoa falando nas duas
    let pedido = nova::Pedido {
        workspace: Some("A".into()),
        pasta: casa.clone(),
        conteudo: nova::Conteudo::Argumentos(Vec::new()),
        nome: Some("Trabalho".into()),
        sem_enter: false,
    };
    let (_, nova_aba, _) = nova::abrir(&pedido).unwrap();
    assert!(tela_tem("A", nova_aba, "Pronto.", 15));
    let _ = geral::rodada(false).unwrap();
    assert!(keep_ia::trocar::digita("A", nova_aba, "faca o relatorio", true).unwrap());
    assert!(tela_tem("A", nova_aba, "Feito: faca o relatorio", 10));
    assert!(keep_ia::trocar::digita("A", antiga, "outra coisa", true).unwrap());
    assert!(tela_tem("A", antiga, "Feito: outra coisa", 10));
    std::thread::sleep(Duration::from_millis(300));

    let r = geral::rodada(false).unwrap();
    assert_eq!(r["eventos"], 4, "pedido e fim de cada uma: {r:#}");
    assert_eq!(r["avisadas"], serde_json::json!([{ "workspace": "A", "abas": [nova_aba] }]), "só a nova vai conferir: {r:#}");
    assert!(tela_tem("A", g, "[Keep] Pedem sua ação", 10), "digitado na aba geral");
    assert!(tela_tem("A", g, "terminou (confira e feche se resolvida)", 2));
    assert!(tela_tem("A", g, &format!("aba {nova_aba}"), 2));
    // a mesma resposta não volta
    let r = geral::rodada(false).unwrap();
    assert_eq!((r["eventos"].as_u64(), r["avisadas"].as_array().map(Vec::len)), (Some(0), Some(0)), "{r:#}");

    // ---- o gancho: o papel e as novidades só na aba geral
    let ctx = keep_ia::geral::contexto("A", &conversa_g).expect("a aba geral recebe");
    assert!(ctx.starts_with("Você é a aba geral do workspace «Projeto A» (no Keep, \"A\")"), "{ctx}");
    assert!(ctx.contains("keep geral fechar --workspace \"A\""), "os comandos com o nome do Keep: {ctx}");
    assert!(ctx.contains("«Trabalho», a pessoa pediu: faca o relatorio"), "{ctx}");
    assert!(ctx.contains("terminou: Feito: faca o relatorio"), "{ctx}");
    assert!(ctx.contains("a pessoa pediu: outra coisa"), "a antiga também entra nas novidades: {ctx}");
    let outra = gerais(&restauro); // a conversa da nova não é a geral
    assert!(keep_ia::geral::contexto("A", "outra-conversa").is_none() && outra.len() == 1);

    // ---- a aba geral que ainda não falou não hiberna: o Claude de verdade só grava o transcrito na 1ª mensagem
    let transcrito = keep_ia::sessoes::transcrito_claude(&conversa_g).unwrap();
    let guardado = std::fs::read(&transcrito).unwrap();
    std::fs::remove_file(&transcrito).unwrap();
    unsafe { std::env::set_var("KEEP_ADORMECER_PARADA_S", "0") };
    let r = keep_restauro::adormecer::adormecer_agora("A", Some(g), &Default::default()).unwrap();
    assert_eq!(r["adormecidas"], 0, "{r:#}");
    assert!(r.to_string().contains("conversa sem nenhuma fala"), "{r:#}");
    std::fs::write(&transcrito, guardado).unwrap();

    // ---- fechar: nunca a aba geral; a resolvida, sim
    assert!(geral::fechar("A", g).unwrap_err().contains("aba geral"));
    let abas = geral::abas("A").unwrap();
    assert!(abas["abas"].as_array().unwrap().iter().any(|a| a["aba"] == nova_aba && a["situacao"] == "parada"), "{abas:#}");
    geral::fechar("A", nova_aba).unwrap();
    assert!(daemon::listar().unwrap().aba("A", nova_aba).is_none(), "a aba fechou");

    // ---- mandar: entra na aba parada de caixa vazia
    geral::mandar("A", antiga, "e agora isto").unwrap();
    assert!(tela_tem("A", antiga, "Feito: e agora isto", 10));

    // ---- a aba parada num erro da API vai à aba geral, para retomá-la; com a sessão "busy" também (um workflow
    // em segundo plano), quando a tela diz que o turno parou
    let pid = keep_ia::abas::ler().unwrap().1.into_iter().find(|i| i.workspace == "A" && i.aba == antiga).unwrap().pid;
    let sessao = casa.join(".claude/sessions").join(format!("{pid}.json"));
    let idle = std::fs::read_to_string(&sessao).unwrap();
    std::fs::write(&sessao, idle.replace("\"status\":\"idle\"", "\"status\":\"busy\"")).unwrap();
    unsafe { std::env::set_var("KEEP_GERAL_ERRO_S", "0") };
    assert!(keep_ia::trocar::digita("A", antiga, "/limite 9999999999 You've hit your limit", true).unwrap());
    assert!(tela_tem("A", antiga, "You've hit your limit", 10));
    std::thread::sleep(Duration::from_millis(300));
    let r = geral::rodada(false).unwrap();
    assert_eq!(r["avisadas"], serde_json::json!([{ "workspace": "A", "abas": [antiga] }]), "{r:#}");
    assert!(tela_tem("A", g, "parou com erro (retome-a)", 10));
    let ctx = keep_ia::geral::contexto("A", &conversa_g).unwrap();
    assert!(ctx.contains("parou com erro (retome-a): You've hit your limit"), "{ctx}");
    std::fs::write(&sessao, &idle).unwrap();

    // ---- a pergunta de uma aba à pessoa vai à aba geral, que a responde na aba
    assert!(geral::responder("A", antiga, Some(1), None).unwrap_err().contains("não espera resposta"));
    std::fs::write(&sessao, idle.replace("\"status\":\"idle\"", "\"status\":\"waiting\"")).unwrap();
    let r = geral::rodada(false).unwrap();
    assert_eq!(r["avisadas"], serde_json::json!([{ "workspace": "A", "abas": [antiga] }]), "{r:#}");
    assert!(tela_tem("A", g, "pergunta (responda se souber", 10));
    let ctx = keep_ia::geral::contexto("A", &conversa_g).unwrap();
    assert!(ctx.contains("pergunta à pessoa") && ctx.contains("You've hit your limit"), "a tela da aba vai junto: {ctx}");
    geral::responder("A", antiga, Some(1), None).unwrap();
    if !tela_tem("A", antiga, "Feito: 1", 10) {
        panic!("o número e o Enter chegaram à aba:\n{}", daemon::tela("A", antiga).unwrap_or_default());
    }
    std::fs::write(&sessao, idle).unwrap();

    // ---- workspace novo: a única aba vira a geral, já no Claude; não a de um workspace com mais abas
    let unica = daemon::nova_aba("N", Some(&raiz.to_string_lossy()), 220, 30).unwrap();
    assert!(tela_tem("N", unica, "$", 15));
    geral::iniciar("N", unica).unwrap();
    assert!(tela_tem("N", unica, "Pronto.", 15), "o Claude subiu na aba");
    assert!(geral::listar().unwrap()["abas"].as_array().unwrap().iter().any(|g| g["workspace"] == "N" && g["aba"] == unica));
    assert!(geral::iniciar("N", unica).unwrap_err().contains("já tem aba geral"));
    assert!(geral::iniciar("A", antiga).unwrap_err().contains("não é a única aba"));

    // ---- fechada a aba geral, outra é aberta (sem os 10 min entre tentativas)
    daemon::fecha_aba("A", g).unwrap();
    unsafe { std::env::set_var("KEEP_GERAL_NOVA_TENTATIVA_S", "0") };
    let r = geral::rodada(false).unwrap();
    let nova_g = r["gerais"].as_array().unwrap().iter().find(|g| g["workspace"] == "A").unwrap()["aba"].as_u64().unwrap() as u32;
    assert!(nova_g != g && nova_g != antiga, "{r:#}");
    assert!(tela_tem("A", nova_g, "Pronto.", 15));
}
