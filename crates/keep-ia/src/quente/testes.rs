//! Os armazéns das abas numa casa falsa: o Chaveiro de mentira do macOS e o
//! arquivo do Windows (`KEEP_IA_TESTE_ARMAZEM=arquivo`, o mesmo caminho que
//! o Claude Code usa lá), processos de verdade no papel do shell e do Claude
//! de uma aba, e um servidor de perfil nesta máquina. Nenhum teste toca em
//! conta de verdade.

use super::*;
use crate::contas::testes::{Casa, casa, perfis};

/// Um processo de teste que dorme (no papel do shell de uma aba, ou do
/// Claude dela), com o ambiente dado. Morre quando sai de cena.
struct Filho(std::process::Child);

impl Filho {
    fn novo(vars: &[(&str, &str)]) -> Filho {
        Filho::de(std::env::current_exe().unwrap(), vars)
    }

    /// O mesmo, com o nome `claude`: para quem procura os Claude que rodam
    /// num armazém, ele é um.
    fn claude(c: &Casa, vars: &[(&str, &str)]) -> Filho {
        let bin = c.raiz.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("claude");
        if !exe.exists() {
            std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        }
        Filho::de(exe, vars)
    }

    fn de(exe: PathBuf, vars: &[(&str, &str)]) -> Filho {
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--exact", "quente::testes::dorme", "--ignored", "--test-threads=1", "-q"])
            .env("KEEP_IA_TESTE_DORME", "1")
            .env_remove(VAR)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        for (n, v) in vars {
            cmd.env(n, v);
        }
        let f = cmd.spawn().unwrap();
        // O ambiente só aparece depois do exec.
        for _ in 0..200 {
            if processos::variavel(f.id(), "KEEP_IA_TESTE_DORME").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Filho(f)
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn mata(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Filho {
    fn drop(&mut self) {
        self.mata();
    }
}

#[test]
#[ignore]
fn dorme() {
    if std::env::var_os("KEEP_IA_TESTE_DORME").is_some() {
        std::thread::sleep(Duration::from_secs(120));
    }
}

/// O teste no Chaveiro (o macOS) e no arquivo (o Windows); fora do macOS,
/// os dois são o arquivo.
fn nos_dois(nome: &str, teste: impl Fn(&Casa)) {
    for modo in ["chaveiro", "arquivo"] {
        let c = casa(&format!("{nome}-{modo}"));
        if modo == "arquivo" {
            // SAFETY: a casa (e a trava) está de pé.
            unsafe { std::env::set_var("KEEP_IA_TESTE_ARMAZEM", "arquivo") };
        }
        teste(&c);
    }
}

/// Ana no login global, Bia numa pasta fixa: os donos.
fn duas_contas(c: &Casa) -> PathBuf {
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com"), ("G2", "u-1", "ana@x.com"), ("C", "u-3", "cara@z.com")]);
    c.global("G", "rG");
    let bia = c.fixa("k-2", Some(("F", "rF")), Some(("bia", "bia@y.com", "u-2")));
    let _ = contas::listar(true); // os donos ficam em cache
    bia
}

fn le(pasta: &Path) -> Value {
    match credencial::fixa(pasta) {
        Lido::Ok(v) => v,
        outro => panic!("o armazém {} não se lê: {outro:?}", pasta.display()),
    }
}

fn acesso(pasta: &Path) -> String {
    le(pasta)["claudeAiOauth"]["accessToken"].as_str().unwrap_or_default().to_string()
}

fn refresh_global() -> Option<String> {
    match credencial::global() {
        Lido::Ok(v) => Oauth::de(&v).and_then(|o| o.refresh),
        _ => None,
    }
}

/// Nenhum armazém de aba tem refresh: o refresh fica só nos donos.
fn sem_refresh_nas_abas() {
    for (_, r) in le_estado().abas {
        let v = le(&r.pasta);
        assert!(v["claudeAiOauth"]["refreshToken"].is_null(), "refresh no armazém {}: {v}", r.pasta.display());
        assert!(v["claudeAiOauth"].get("refreshTokenExpiresAt").is_none());
    }
}

fn pastas_fixas() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(caminhos::fixas())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn preparar_poe_so_o_acesso_da_conta_no_armazem_da_aba() {
    nos_dois("preparar", |c| {
        duas_contas(c);
        let shell = Filho::novo(&[]);
        let p = preparar("T", shell.pid(), None, None).unwrap();
        assert_eq!((p.conta.as_str(), p.escolha.as_str()), ("claude:ana", SEGUIR_ORDEM), "a aba nova segue a ordem");
        assert!(p.pasta.starts_with(caminhos::abas()), "{}", p.pasta.display());
        assert_eq!(acesso(&p.pasta), "G");
        let v = le(&p.pasta);
        assert!(v["claudeAiOauth"]["refreshToken"].is_null(), "{v}");
        assert_eq!(v["claudeAiOauth"]["rateLimitTier"], "default_claude_max_20x", "o resto do login vai junto");
        // A escolha muda a conta do armazém; sem escolha, fica a que a aba tem.
        let bia = preparar("T", shell.pid(), Some("claude:bia"), None).unwrap();
        assert_eq!(bia.pasta, p.pasta, "o mesmo shell, o mesmo armazém");
        assert_eq!(acesso(&p.pasta), "F");
        assert_eq!(preparar("T", shell.pid(), None, None).unwrap().escolha, "claude:bia");
        assert_eq!(acesso(&p.pasta), "F");
        // Os donos seguem com o refresh deles; as abas, sem nenhum.
        assert_eq!(refresh_global().as_deref(), Some("rG"));
        sem_refresh_nas_abas();
        // Outro shell, outro armazém.
        let outro = Filho::novo(&[]);
        assert_ne!(preparar("T", outro.pid(), None, None).unwrap().pasta, p.pasta);
        // Escolha de GPT ou desconhecida: recusada, nada muda.
        assert_eq!(preparar("T", shell.pid(), Some("gpt:principal"), None).unwrap_err().motivo, "uso");
        assert_eq!(preparar("T", shell.pid(), Some("claude:ninguem"), None).unwrap_err().motivo, "sem-conta");
        assert_eq!(acesso(&p.pasta), "F");
    });
}

#[test]
fn trocar_regrava_o_armazem_do_claude_aberto_sem_tocar_nos_donos() {
    nos_dois("trocar", |c| {
        let bia = duas_contas(c);
        let shell = Filho::novo(&[]);
        let p = preparar("T", shell.pid(), None, None).unwrap();
        let pasta = p.pasta.to_string_lossy().into_owned();
        let claude = Filho::novo(&[(VAR, &pasta)]);
        let lista = contas::listar(true);
        let t = |para: &str| trocar_processo(claude.pid(), shell.pid(), "T", 1, para, None, &lista);
        assert_eq!(t("claude:bia").unwrap(), Feito::Trocou("claude:bia".into()));
        let pendente = registro_da_pasta(&p.pasta).unwrap().1.chrome_pendente_desde;
        assert!(pendente.is_some(), "a ponte persistente precisa acompanhar a nova conta");
        assert_eq!(acesso(&p.pasta), "F");
        assert_eq!(
            crate::abas::conta_do_processo(claude.pid(), "claude", &lista),
            ("claude:bia".into(), Some("claude:bia".into())),
            "a conta da aba vem do armazém, não do ambiente de quando ele subiu"
        );
        assert_eq!(t("claude:bia").unwrap(), Feito::Ja("claude:bia".into()));
        assert_eq!(registro_da_pasta(&p.pasta).unwrap().1.chrome_pendente_desde, pendente);
        assert_eq!(t(SEGUIR_ORDEM).unwrap(), Feito::Trocou("claude:ana".into()));
        assert_eq!(acesso(&p.pasta), "G");
        assert_eq!(
            crate::abas::conta_do_processo(claude.pid(), "claude", &lista),
            (SEGUIR_ORDEM.into(), Some("claude:ana".into()))
        );
        // Com a ordem decidida pela sincronização.
        let lista2 = lista.clone();
        assert_eq!(
            trocar_processo(claude.pid(), shell.pid(), "T", 1, SEGUIR_ORDEM, Some("claude:bia"), &lista2).unwrap(),
            Feito::Trocou("claude:bia".into())
        );
        assert_eq!(acesso(&p.pasta), "F");
        // Os donos: intactos.
        assert_eq!(refresh_global().as_deref(), Some("rG"));
        assert_eq!(Oauth::de(&le(&bia)).and_then(|o| o.refresh).as_deref(), Some("rF"));
        sem_refresh_nas_abas();
        assert_eq!(pastas_fixas(), ["k-2"], "nenhuma pasta fixa nova");
    });
}

#[test]
fn o_claude_no_login_global_ou_num_armazem_estranho_nao_troca() {
    let c = casa("global");
    duas_contas(&c);
    let shell = Filho::novo(&[]);
    let lista = contas::listar(true);
    let global = Filho::novo(&[]);
    let e = trocar_processo(global.pid(), shell.pid(), "T", 1, "claude:bia", None, &lista).unwrap_err();
    assert_eq!(e.motivo, "claude-global", "{e:?}");
    let estranha = c.raiz.join("outra-pasta");
    std::fs::create_dir_all(&estranha).unwrap();
    let outro = Filho::novo(&[(VAR, &estranha.to_string_lossy())]);
    let e = trocar_processo(outro.pid(), shell.pid(), "T", 1, "claude:bia", None, &lista).unwrap_err();
    assert_eq!(e.motivo, "armazem-desconhecido", "{e:?}");
    assert!(le_estado().abas.is_empty(), "nada foi registrado");
    assert_eq!(refresh_global().as_deref(), Some("rG"));
}

#[test]
fn a_pasta_fixa_de_antes_vira_armazem_da_aba_quando_a_conta_tem_outro_dono() {
    nos_dois("legada", |c| {
        let bia = duas_contas(c);
        // Uma segunda pasta da Bia (outro login dela, outra família), usada
        // só pelo Claude desta aba.
        let antiga = c.fixa("k-9", Some(("F9", "rF9")), Some(("bia", "bia@y.com", "u-2")));
        let shell = Filho::novo(&[]);
        let claude = Filho::novo(&[(VAR, &antiga.to_string_lossy())]);
        let lista = contas::listar(false);
        assert!(lista.iter().any(|c| c.alias == "bia" && c.fixa().is_some()));
        assert_eq!(
            trocar_processo(claude.pid(), shell.pid(), "T", 3, "claude:ana", None, &lista).unwrap(),
            Feito::Trocou("claude:ana".into())
        );
        assert!(contas::ler_metadados(&antiga).unwrap().aba, "marcada como armazém de aba");
        assert_eq!(acesso(&antiga), "G");
        assert!(le(&antiga)["claudeAiOauth"]["refreshToken"].is_null());
        // A Bia segue com o dono dela (k-2), e a k-9 não conta mais.
        let lista = contas::listar(false);
        let b = lista.iter().find(|c| c.alias == "bia").unwrap();
        assert_eq!(b.armazens, vec![Armazem::Fixa { pasta: bia.clone() }]);
        assert!(crate::renovar::alvos().iter().all(|a| !mesma(&a.pasta, &antiga)), "a renovação não passa por ela");
        assert_eq!(pastas_fixas(), ["k-2", "k-9"], "nenhuma pasta nova: a conta tinha outro dono");
        let (chave, r) = registro_da_pasta(&antiga).unwrap();
        assert_eq!((chave.as_str(), r.legada, r.aba, r.escolha.as_str()), ("fixa-k-9", true, 3, "claude:ana"));
        // Depois disso, é uma troca a quente como as outras.
        assert_eq!(
            trocar_processo(claude.pid(), shell.pid(), "T", 3, "claude:bia", None, &lista).unwrap(),
            Feito::Trocou("claude:bia".into())
        );
        assert_eq!(acesso(&antiga), "F");
        sem_refresh_nas_abas();
    });
}

#[test]
fn a_pasta_fixa_unica_dona_muda_o_login_para_uma_pasta_nova() {
    nos_dois("promove", |c| {
        let bia = duas_contas(c);
        let shell = Filho::novo(&[]);
        let claude = Filho::novo(&[(VAR, &bia.to_string_lossy())]);
        let lista = contas::listar(false);
        assert_eq!(
            trocar_processo(claude.pid(), shell.pid(), "T", 1, "claude:ana", None, &lista).unwrap(),
            Feito::Trocou("claude:ana".into())
        );
        // A k-2 virou o armazém da aba; o login inteiro da Bia mudou para uma
        // pasta nova (o refresh num lugar só).
        assert!(contas::ler_metadados(&bia).unwrap().aba);
        assert!(le(&bia)["claudeAiOauth"]["refreshToken"].is_null());
        let novas: Vec<String> = pastas_fixas().into_iter().filter(|n| n != "k-2").collect();
        assert_eq!(novas.len(), 1, "{novas:?}");
        let nova = caminhos::fixas().join(&novas[0]);
        let o = Oauth::de(&le(&nova)).unwrap();
        assert_eq!((o.acesso.as_deref(), o.refresh.as_deref()), (Some("F"), Some("rF")));
        let m = contas::ler_metadados(&nova).unwrap();
        assert_eq!((m.apelido.as_str(), m.uuid.as_str(), m.aba), ("bia", "u-2", false));
        let lista = contas::listar(false);
        let b = lista.iter().find(|c| c.alias == "bia").unwrap();
        assert!(b.roda, "a Bia segue rodando, pela pasta nova");
        assert_eq!(contas::ambiente(b).unwrap(), vec![(VAR.to_string(), nova.to_string_lossy().into_owned())]);
        sem_refresh_nas_abas();
    });
}

#[test]
fn a_pasta_usada_por_outro_claude_nao_e_convertida() {
    let c = casa("compartilhada");
    let bia = duas_contas(&c);
    let shell = Filho::novo(&[]);
    let a = Filho::claude(&c, &[(VAR, &bia.to_string_lossy())]);
    let _b = Filho::claude(&c, &[(VAR, &bia.to_string_lossy())]);
    let lista = contas::listar(false);
    let e = trocar_processo(a.pid(), shell.pid(), "T", 1, "claude:ana", None, &lista).unwrap_err();
    assert_eq!(e.motivo, "pasta-compartilhada", "{e:?}");
    let o = Oauth::de(&le(&bia)).unwrap();
    assert_eq!((o.acesso.as_deref(), o.refresh.as_deref()), (Some("F"), Some("rF")), "intacta");
    assert!(!contas::ler_metadados(&bia).unwrap().aba);
    assert_eq!(pastas_fixas(), ["k-2"]);
}

#[test]
fn espalhar_leva_o_acesso_renovado_e_apaga_o_armazem_da_aba_que_acabou() {
    nos_dois("espalhar", |c| {
        duas_contas(c);
        let mut shell_a = Filho::novo(&[]);
        let shell_b = Filho::novo(&[]);
        let a = preparar("T", shell_a.pid(), None, None).unwrap();
        let b = preparar("T", shell_b.pid(), Some("claude:bia"), None).unwrap();
        assert_eq!(espalhar(&contas::listar(false)), Espalhado::default(), "nada a fazer");
        // A Ana renovada (o dono ganhou acesso e refresh novos).
        c.global("G2", "rG2");
        let lista = contas::listar(true);
        let r = espalhar(&lista);
        assert_eq!((r.atualizadas, r.trocadas.len(), r.apagadas.len()), (1, 0, 0), "{r:?}");
        assert_eq!(acesso(&a.pasta), "G2");
        assert!(registro_da_pasta(&a.pasta).unwrap().1.chrome_pendente_desde.is_none(), "mesma conta não reconecta");
        assert_eq!(acesso(&b.pasta), "F", "a outra conta não muda");
        sem_refresh_nas_abas();
        // O shell da aba A acabou: o armazém dela sai (item e pasta).
        shell_a.mata();
        let r = espalhar(&lista);
        assert_eq!(r.apagadas, vec![a.chave.clone()], "{r:?}");
        assert!(!a.pasta.exists());
        assert!(matches!(credencial::fixa(&a.pasta), Lido::Ausente), "o login dela saiu também");
        assert!(registro_da_pasta(&a.pasta).is_none());
        assert_eq!(acesso(&b.pasta), "F");
        assert_eq!(refresh_global().as_deref(), Some("rG2"), "o dono não foi tocado");
    });
}

#[test]
fn o_armazem_de_um_claude_que_sobreviveu_a_aba_fica() {
    let c = casa("sobrevive");
    duas_contas(&c);
    let mut shell = Filho::novo(&[]);
    let p = preparar("T", shell.pid(), None, None).unwrap();
    // Uma conversa em segundo plano, que nasceu na aba e segue rodando.
    let mut fundo = Filho::claude(&c, &[(VAR, &p.pasta.to_string_lossy())]);
    shell.mata();
    let r = espalhar(&contas::listar(false));
    assert!(r.apagadas.is_empty(), "{r:?}");
    assert_eq!(acesso(&p.pasta), "G");
    fundo.mata();
    assert_eq!(espalhar(&contas::listar(false)).apagadas, vec![p.chave]);
    assert!(!p.pasta.exists());
}

#[test]
fn com_o_kit_quem_segue_a_ordem_vai_para_a_ativa_e_a_presa_fica() {
    let c = casa("kit");
    perfis(&[("A1", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com"), ("G", "u-1", "ana@x.com")]);
    c.global("G", "rG");
    c.fixa("k-1", Some(("A1", "rA1")), Some(("ana", "ana@x.com", "u-1")));
    c.fixa("k-2", Some(("F", "rF")), Some(("bia", "bia@y.com", "u-2")));
    std::fs::write(c.raiz.join(".claude/contas/.ativa"), "ana\n").unwrap();
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_GERENTE_EXTERNO", "1") };
    let _ = contas::listar(true);
    let ordem = Filho::novo(&[]);
    let presa = Filho::novo(&[]);
    let o = preparar("T", ordem.pid(), None, None).unwrap();
    let p = preparar("T", presa.pid(), Some("claude:ana"), None).unwrap();
    // Com o kit, o dono é a pasta fixa (o global é dele, que o troca de dono).
    assert_eq!((acesso(&o.pasta), acesso(&p.pasta)), ("A1".into(), "A1".into()));
    std::fs::write(c.raiz.join(".claude/contas/.ativa"), "bia\n").unwrap();
    let r = espalhar(&contas::listar(false));
    assert_eq!(r.trocadas, vec![(o.chave.clone(), "claude:bia".to_string())], "{r:?}");
    assert_eq!(acesso(&o.pasta), "F");
    assert_eq!(acesso(&p.pasta), "A1", "a aba presa numa conta não segue a ordem");
    let (_, ro) = registro_da_pasta(&o.pasta).unwrap();
    assert_eq!((ro.escolha.as_str(), ro.conta.as_str()), (SEGUIR_ORDEM, "claude:bia"));
    assert!(ro.chrome_pendente_desde.is_some(), "a troca automática também renova a ponte");
    assert!(registro_da_pasta(&p.pasta).unwrap().1.chrome_pendente_desde.is_none());
}

#[test]
fn confirmar_uma_reconexao_nao_apaga_outra_troca() {
    let c = casa("reconectar-chrome");
    duas_contas(&c);
    let shell = Filho::novo(&[]);
    let p = preparar("T", shell.pid(), None, None).unwrap();
    let mut estado = le_estado();
    estado.abas.get_mut(&p.chave).unwrap().chrome_pendente_desde = Some(50);
    grava_estado(&estado).unwrap();
    concluir_reconexao(&p.chave, "u-2", 50).unwrap();
    assert_eq!(
        registro_da_pasta(&p.pasta).unwrap().1.chrome_pendente_desde,
        Some(50)
    );
    concluir_reconexao(&p.chave, "u-1", 40).unwrap();
    assert_eq!(
        registro_da_pasta(&p.pasta).unwrap().1.chrome_pendente_desde,
        Some(50)
    );
    concluir_reconexao(&p.chave, "u-1", 50).unwrap();
    assert!(registro_da_pasta(&p.pasta)
        .unwrap()
        .1
        .chrome_pendente_desde
        .is_none());
}

#[test]
fn comando_precisa_estar_na_caixa_sem_rascunho_junto() {
    let regua = "────────────────";
    assert!(comando_na_caixa(&format!("histórico\n❯ \n{regua}\nrodapé"), ""));
    assert!(comando_na_caixa(&format!("histórico\n❯ {RECONECTAR_CHROME}\n{regua}\nrodapé"), RECONECTAR_CHROME));
    assert!(!comando_na_caixa(&format!("❯ {RECONECTAR_CHROME}\nresposta\n❯ meu rascunho\n{regua}"), RECONECTAR_CHROME));
    assert!(!comando_na_caixa(&format!("❯ \nsegunda linha do rascunho\n{regua}"), ""));
    assert!(!comando_na_caixa("❯ meu rascunho", ""));
    assert!(!comando_na_caixa("sem prompt", ""));
    assert!(!comando_na_caixa("histórico de uma caixa antiga: \n❯ ", ""));
}

#[test]
fn um_login_feito_na_aba_vira_a_escolha_dela_e_o_dono_da_conta_nova() {
    nos_dois("login-na-aba", |c| {
        duas_contas(c);
        let shell = Filho::novo(&[]);
        let p = preparar("T", shell.pid(), None, None).unwrap();
        // /login no Claude da aba, numa conta que o Keep não tinha.
        let local = credencial::local_fixa(&p.pasta);
        let login: Value = serde_json::from_str(
            r#"{"claudeAiOauth":{"accessToken":"C","refreshToken":"rC","expiresAt":9999999999999,"subscriptionType":"max"}}"#,
        )
        .unwrap();
        credencial::gravar(&local, &login).unwrap();
        let r = espalhar(&contas::listar(false));
        assert!(r.falhas.is_empty(), "{r:?}");
        // O login mudou para uma pasta fixa nova, dona da conta nova...
        let novas: Vec<String> = pastas_fixas().into_iter().filter(|n| n != "k-2").collect();
        assert_eq!(novas.len(), 1, "{novas:?}");
        let o = Oauth::de(&le(&caminhos::fixas().join(&novas[0]))).unwrap();
        assert_eq!((o.acesso.as_deref(), o.refresh.as_deref()), (Some("C"), Some("rC")));
        assert!(contas::ler_ordem().contains(&"claude:cara".to_string()), "{:?}", contas::ler_ordem());
        // ... e a aba fica nela, só com o acesso.
        assert_eq!(acesso(&p.pasta), "C");
        sem_refresh_nas_abas();
        let (_, r) = registro_da_pasta(&p.pasta).unwrap();
        assert_eq!((r.escolha.as_str(), r.conta.as_str(), r.dono.as_str()), ("claude:cara", "claude:cara", "u-3"));
        // Um /login de novo, na conta que já tem dono: o refresh a mais sai.
        let outro: Value = serde_json::from_str(
            r#"{"claudeAiOauth":{"accessToken":"F","refreshToken":"rF-outro","expiresAt":9999999999999,"subscriptionType":"max"}}"#,
        )
        .unwrap();
        credencial::gravar(&local, &outro).unwrap();
        let antes = pastas_fixas();
        let r = espalhar(&contas::listar(false));
        assert!(r.falhas.is_empty(), "{r:?}");
        assert_eq!(pastas_fixas(), antes, "nenhuma pasta nova: a Bia tem dono");
        assert_eq!(acesso(&p.pasta), "F");
        assert_eq!(registro_da_pasta(&p.pasta).unwrap().1.escolha, "claude:bia");
        sem_refresh_nas_abas();
    });
}

#[test]
fn a_renovacao_espalha_o_acesso_novo_nas_abas_da_conta() {
    let c = casa("renova-espalha");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
    c.global("G", "rG");
    // A Bia vence em 10 min: a renovação a renova e espalha.
    let bia = c.fixa("k-2", None, Some(("bia", "bia@y.com", "u-2")));
    let quase: Value = serde_json::from_str(&format!(
        r#"{{"claudeAiOauth":{{"accessToken":"F","refreshToken":"rF","expiresAt":{},"scopes":["user:inference"]}}}}"#,
        agora() + 10 * 60_000
    ))
    .unwrap();
    credencial::gravar(&credencial::local_fixa(&bia), &quase).unwrap();
    let _ = contas::listar(true);
    let servidor = crate::renovar::testes::Servidor::sobe();
    servidor.gira("rF", "F-nova", "rF-nova");
    let shell = Filho::novo(&[]);
    let p = preparar("T", shell.pid(), Some("claude:bia"), None).unwrap();
    assert_eq!(acesso(&p.pasta), "F");
    let r = crate::renovar::rodada(false);
    assert_eq!(r.renovados.len(), 1, "{r:?}");
    assert_eq!(r.abas.atualizadas, 1, "{r:?}");
    assert_eq!(acesso(&p.pasta), "F-nova");
    assert_eq!(Oauth::de(&le(&bia)).and_then(|o| o.refresh).as_deref(), Some("rF-nova"));
    sem_refresh_nas_abas();
    // O dono do acesso novo é sabido sem perguntar à rede.
    assert_eq!(registro_da_pasta(&p.pasta).unwrap().1.conta, "claude:bia");
    assert!(contas::listar(false).iter().any(|c| c.alias == "bia" && c.roda));
}

#[test]
fn o_problema_de_um_armazem_aparece_uma_vez_e_some_quando_passa() {
    let c = casa("falha-uma-vez");
    let bia = duas_contas(&c);
    let shell = Filho::novo(&[]);
    let p = preparar("T", shell.pid(), Some("claude:bia"), None).unwrap();
    // O acesso da Bia venceu e não renovou (sem rede): a aba fica onde está.
    c.fixa("k-2", None, None);
    let vencida: Value = serde_json::from_str(&format!(
        r#"{{"claudeAiOauth":{{"accessToken":"F","refreshToken":"rF","expiresAt":{}}}}}"#,
        agora() - 60_000
    ))
    .unwrap();
    credencial::gravar(&credencial::local_fixa(&bia), &vencida).unwrap();
    let r = espalhar(&contas::listar(false));
    assert_eq!(r.falhas.len(), 1, "{r:?}");
    assert!(r.falhas[0].1.contains("sem login em dia"), "{r:?}");
    assert_eq!(espalhar(&contas::listar(false)).falhas, vec![], "o mesmo problema não se repete a cada rodada");
    assert_eq!(acesso(&p.pasta), "F");
    // Renovado o dono, passa.
    c.fixa("k-2", Some(("F", "rF")), None);
    let r = espalhar(&contas::listar(false));
    assert!(r.falhas.is_empty(), "{r:?}");
    assert!(registro_da_pasta(&p.pasta).unwrap().1.falha.is_none());
}

#[test]
fn um_processo_que_nao_e_claude_nao_segura_o_armazem_nem_a_pasta() {
    let c = casa("nao-claude");
    let bia = duas_contas(&c);
    // Um servidor MCP que ficou órfão, com a variável da aba: não lê o login.
    let mut shell = Filho::novo(&[]);
    let p = preparar("T", shell.pid(), None, None).unwrap();
    let _orfao = Filho::novo(&[(VAR, &p.pasta.to_string_lossy())]);
    shell.mata();
    assert_eq!(espalhar(&contas::listar(false)).apagadas, vec![p.chave.clone()]);
    // E a pasta fixa de antes, com um órfão desses ao lado, ainda vira a da aba.
    let shell = Filho::novo(&[]);
    let claude = Filho::claude(&c, &[(VAR, &bia.to_string_lossy())]);
    let _outro = Filho::novo(&[(VAR, &bia.to_string_lossy())]);
    let lista = contas::listar(false);
    assert_eq!(
        trocar_processo(claude.pid(), shell.pid(), "T", 1, "claude:ana", None, &lista).unwrap(),
        Feito::Trocou("claude:ana".into())
    );
    assert!(contas::ler_metadados(&bia).unwrap().aba);
}

#[test]
fn com_o_kit_o_login_global_nao_conta_como_outro_dono() {
    let c = casa("kit-dono");
    perfis(&[("G", "u-2", "bia@y.com"), ("F", "u-2", "bia@y.com")]);
    // O kit pôs a Bia no global; a única pasta fixa dela é a da aba antiga.
    c.global("G", "rG");
    let bia = c.fixa("k-2", Some(("F", "rF")), Some(("bia", "bia@y.com", "u-2")));
    std::fs::write(c.raiz.join(".claude/contas/.ativa"), "bia\n").unwrap();
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_GERENTE_EXTERNO", "1") };
    let _ = contas::listar(true);
    let shell = Filho::novo(&[]);
    let claude = Filho::claude(&c, &[(VAR, &bia.to_string_lossy())]);
    let lista = contas::listar(false);
    assert_eq!(
        trocar_processo(claude.pid(), shell.pid(), "T", 1, "claude:bia", None, &lista).unwrap(),
        Feito::Trocou("claude:bia".into())
    );
    // O login da pasta mudou para uma pasta nova (a Bia continua com dono
    // quando o kit tirar o global dela), em vez de ser descartado.
    let novas: Vec<String> = pastas_fixas().into_iter().filter(|n| n != "k-2").collect();
    assert_eq!(novas.len(), 1, "{novas:?}");
    let o = Oauth::de(&le(&caminhos::fixas().join(&novas[0]))).unwrap();
    assert_eq!(o.refresh.as_deref(), Some("rF"));
    assert_eq!(refresh_global().as_deref(), Some("rG"), "o global é do kit: intocado");
}
