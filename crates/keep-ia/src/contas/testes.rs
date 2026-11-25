//! As contas numa casa falsa: cofre do kit, login global, pastas fixas e
//! logins do Codex escritos à mão, um Chaveiro falso no macOS e um servidor
//! de perfil falso nesta máquina. Nenhum teste toca em conta de verdade.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::*;

static TRAVA: Mutex<()> = Mutex::new(());

/// Uma casa falsa, com as variáveis apontadas para ela enquanto vive.
pub struct Casa {
    pub raiz: PathBuf,
    _trava: MutexGuard<'static, ()>,
}

impl Drop for Casa {
    fn drop(&mut self) {
        for v in [
            "KEEP_IA_HOME",
            "KEEP_IA_ESTADO",
            "KEEP_IA_SECURITY",
            "KEEP_IA_TESTE_CHAVEIRO",
            "KEEP_IA_PERFIL_URL",
            "KEEP_IA_CLAUDE_URL",
            "KEEP_IA_CODEX_URL",
            "KEEP_IA_OPENROUTER_URL",
            "KEEP_IA_OPENROUTER_AUTH_URL",
            "KEEP_IA_NAVEGADOR",
            "KEEP_IA_LOGIN_PRAZO",
            "OPENROUTER_API_KEY",
            "KEEP_IA_TOKEN_URL",
            "KEEP_IA_TESTE_CHAVEIRO_FALHA",
            "KEEP_IA_TESTE_CHAVEIRO_TRAVA",
            "KEEP_IA_GERENTE_EXTERNO",
            "KEEP_IA_CLAUDE_BIN",
            "KEEP_IA_CODEX_BIN",
            "KEEP_IA_KEEP_BIN",
            "KEEP_IA_TESTE_ARMAZEM",
            "KEEP_SOCKET",
            "USER",
        ] {
            // SAFETY: a trava garante um teste por vez mexendo no ambiente.
            unsafe { std::env::remove_var(v) };
        }
        let _ = std::fs::remove_dir_all(&self.raiz);
    }
}

pub fn casa(nome: &str) -> Casa {
    let trava = TRAVA.lock().unwrap_or_else(|e| e.into_inner());
    let raiz = std::env::temp_dir().join(format!("keep-ia-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    std::fs::create_dir_all(raiz.join(".claude/contas/fixas")).unwrap();
    let chaveiro = raiz.join("chaveiro");
    std::fs::create_dir_all(&chaveiro).unwrap();
    let security = raiz.join("security");
    std::fs::write(
        &security,
        // Lê, apaga e grava (`add-generic-password -U … -X <hex>`, por argv
        // ou pela entrada do `security -i`, como o CLI e a renovação gravam).
        "#!/bin/sh\nOP=\"$1\"; S=; X=\n\
         if [ \"$OP\" = -i ]; then IFS= read -r L; OP=${L%% *}; \
         S=$(printf %s \"$L\" | sed -n 's/.* -s \"\\([^\"]*\\)\".*/\\1/p'); \
         X=$(printf %s \"$L\" | sed -n 's/.* -X \"\\([0-9a-fA-F]*\\)\".*/\\1/p'); \
         else while [ $# -gt 0 ]; do case \"$1\" in -s) shift; S=\"$1\";; -X) shift; X=\"$1\";; esac; shift; done; fi\n\
         F=\"$KEEP_IA_TESTE_CHAVEIRO/$S\"\n\
         if [ \"$OP\" = add-generic-password ]; then [ -n \"$KEEP_IA_TESTE_CHAVEIRO_FALHA\" ] && exit 1; \
         printf %s \"$X\" | xxd -r -p > \"$F\"; exit 0; fi\n\
         [ -f \"$F\" ] || exit 44\n\
         [ \"$OP\" = delete-generic-password ] && { rm -f \"$F\"; exit 0; }\ncat \"$F\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&security, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // SAFETY: a trava garante um teste por vez mexendo no ambiente.
    unsafe {
        std::env::set_var("KEEP_IA_HOME", &raiz);
        std::env::set_var("KEEP_IA_ESTADO", raiz.join("estado"));
        std::env::set_var("KEEP_IA_SECURITY", &security);
        std::env::set_var("KEEP_IA_TESTE_CHAVEIRO", &chaveiro);
        std::env::set_var("KEEP_IA_GERENTE_EXTERNO", "0");
        std::env::set_var("USER", "teste");
        // Um daemon que não existe: nada aqui fala com o Keep de verdade (o
        // teste que precisa de um sobe o dele e aponta para ele).
        std::env::set_var("KEEP_SOCKET", raiz.join("sem-keepd.sock"));
        // Nem o PR-Agent instalado nesta máquina: o teste que precisa de um
        // aponta para o falso dele.
        std::env::set_var("KEEP_IA_PR_AGENT_BIN", raiz.join("sem-pr-agent"));
        // Os serviços, num lugar desta máquina onde ninguém ouve: um teste
        // que esquecer o dele falha na hora, sem mandar nada para fora. (A
        // página do login do OpenRouter não: ela só vai para o navegador.)
        for v in ["KEEP_IA_TOKEN_URL", "KEEP_IA_PERFIL_URL", "KEEP_IA_CLAUDE_URL", "KEEP_IA_CODEX_URL", "KEEP_IA_OPENROUTER_URL"] {
            std::env::set_var(v, "http://127.0.0.1:9/nada");
        }
        // O ambiente de quem roda os testes (uma aba do Keep com o Claude
        // numa conta) não passa para as abas e IAs falsas dos testes.
        for v in ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "CLAUDE_CONFIG_DIR", "KEEP_IA_ESCOLHA", "CLAUDE_KIT_CONTA", "CODEX_HOME"] {
            std::env::remove_var(v);
        }
    }
    Casa { raiz, _trava: trava }
}

impl Casa {
    fn login(acesso: &str, refresh: &str) -> String {
        Self::login_ate(acesso, refresh, donos::agora_ms() + 3_600_000)
    }

    fn login_ate(acesso: &str, refresh: &str, expira_ms: u64) -> String {
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{acesso}","refreshToken":"{refresh}","expiresAt":{expira_ms},"rateLimitTier":"default_claude_max_20x","subscriptionType":"max"}}}}"#
        )
    }

    /// O login global com este token.
    pub fn global(&self, acesso: &str, refresh: &str) {
        self.global_ate(acesso, refresh, donos::agora_ms() + 3_600_000);
    }

    /// O login global com este token, que vence em `expira_ms` (unix ms).
    pub fn global_ate(&self, acesso: &str, refresh: &str, expira_ms: u64) {
        let json = Self::login_ate(acesso, refresh, expira_ms);
        if credencial::usa_chaveiro() {
            std::fs::write(self.raiz.join("chaveiro").join(credencial::SERVICO_GLOBAL), json).unwrap();
        } else {
            std::fs::write(self.raiz.join(".claude/.credentials.json"), json).unwrap();
        }
    }

    /// Uma pasta fixa, com login (ou sem) e metadados do Keep (ou sem).
    pub fn fixa(&self, nome: &str, login: Option<(&str, &str)>, meta: Option<(&str, &str, &str)>) -> PathBuf {
        let pasta = crate::caminhos::fixas().join(nome);
        std::fs::create_dir_all(&pasta).unwrap();
        if let Some((acesso, refresh)) = login {
            let json = Self::login(acesso, refresh);
            if credencial::usa_chaveiro() {
                let servico = credencial::servico_de(&pasta.to_string_lossy());
                std::fs::write(self.raiz.join("chaveiro").join(servico), json).unwrap();
            } else {
                std::fs::write(pasta.join(".credentials.json"), json).unwrap();
            }
        }
        if let Some((apelido, email, uuid)) = meta {
            gravar_metadados(
                &pasta,
                &Metadados { versao: 1, apelido: apelido.into(), email: email.into(), uuid: uuid.into(), criada_em: 1, aba: false },
            )
            .unwrap();
        }
        pasta
    }

    /// Um slot do cofre do kit.
    pub fn slot(&self, apelido: &str, uuid: &str, email: &str, acesso: &str) {
        let json = format!(
            r#"{{"apelido":"{apelido}","accountUuid":"{uuid}","email":"{email}","credenciais":{}}}"#,
            Self::login(acesso, "r")
        );
        std::fs::write(crate::caminhos::contas().join(format!("{apelido}.json")), json).unwrap();
    }

    pub fn codex(&self, pasta: &Path, email: &str, conta: &str) {
        use base64::Engine;
        let b64 = |v: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
        let declaracoes = format!(
            r#"{{"exp":{},"https://api.openai.com/profile":{{"email":"{email}"}},"https://api.openai.com/auth":{{"chatgpt_account_id":"{conta}","chatgpt_plan_type":"pro"}}}}"#,
            donos::agora_ms() / 1000 + 3600
        );
        let acesso = format!("{}.{}.x", b64("{}"), b64(&declaracoes));
        std::fs::create_dir_all(pasta).unwrap();
        std::fs::write(
            pasta.join("auth.json"),
            format!(r#"{{"tokens":{{"access_token":"{acesso}","account_id":"{conta}"}}}}"#),
        )
        .unwrap();
    }

    pub fn ordem(&self, linhas: &[&str]) {
        std::fs::write(crate::caminhos::ordem(), linhas.join("\n") + "\n").unwrap();
    }
}

/// Um servidor de perfil nesta máquina: token → (uuid, e-mail).
pub fn perfis(mapa: &[(&str, &str, &str)]) {
    let donos: HashMap<String, (String, String)> =
        mapa.iter().map(|(t, u, e)| (t.to_string(), (u.to_string(), e.to_string()))).collect();
    let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = ouvinte.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conexao in ouvinte.incoming() {
            let Ok(mut c) = conexao else { continue };
            let mut buf = [0u8; 8192];
            let n = c.read(&mut buf).unwrap_or(0);
            let pedido = String::from_utf8_lossy(&buf[..n]).to_string();
            let token = pedido
                .lines()
                .find_map(|l| l.strip_prefix("authorization: Bearer ").or_else(|| l.strip_prefix("Authorization: Bearer ")))
                .unwrap_or("")
                .trim()
                .to_string();
            let resposta = match donos.get(&token) {
                Some((u, e)) => {
                    let corpo = format!(r#"{{"account":{{"uuid":"{u}","email_address":"{e}"}}}}"#);
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}", corpo.len())
                }
                None => "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            };
            let _ = c.write_all(resposta.as_bytes());
        }
    });
    // SAFETY: chamado com a casa (e a trava) de pé.
    unsafe { std::env::set_var("KEEP_IA_PERFIL_URL", format!("http://127.0.0.1:{porta}/profile")) };
}

fn chaves(contas: &[Conta]) -> Vec<String> {
    contas.iter().map(|c| c.order_key.clone()).collect()
}

#[test]
fn sem_kit_o_global_as_fixas_e_o_codex_viram_contas() {
    let c = casa("simples");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
    c.global("G", "r");
    let trabalho = c.fixa("k-00000001", Some(("F", "r")), Some(("trabalho", "bia@y.com", "u-2")));
    c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
    c.codex(&c.raiz.join(".codex-contas/outra"), "carla@z.com", "acct-2");

    let contas = listar(true);
    assert_eq!(chaves(&contas), ["claude:ana", "claude:trabalho", "gpt:principal", "gpt:outra"]);
    let ana = &contas[0];
    assert!(ana.is_active && ana.is_preferred && ana.roda && ana.has_token);
    assert_eq!(ana.email.as_deref(), Some("ana@x.com"));
    assert_eq!(ana.plan.as_deref(), Some("Max 20x"));
    assert_eq!(ambiente(ana), Ok(vec![]), "o global não precisa de variável");
    assert_eq!(
        ambiente(&contas[1]),
        Ok(vec![("CLAUDE_SECURESTORAGE_CONFIG_DIR".into(), trabalho.to_string_lossy().into_owned())])
    );
    assert_eq!(ambiente(&contas[2]), Ok(vec![]));
    assert_eq!(
        ambiente(&contas[3]),
        Ok(vec![("CODEX_HOME".into(), crate::caminhos::codex_contas().join("outra").to_string_lossy().into_owned())])
    );
    assert_eq!(contas[2].plan.as_deref(), Some("Pro"));
    assert!(contas[2].is_active, "o GPT principal é o que as abas usam");
}

#[test]
fn com_o_kit_os_apelidos_e_a_ordem_dele_valem() {
    let c = casa("kit");
    perfis(&[("G", "aaaa1111-0000", "ds@x.com"), ("F", "aaaa1111-0000", "ds@x.com")]);
    c.slot("principal", "bbbb2222-0000", "assinaturas@y.com", "S1");
    c.slot("reserva", "aaaa1111-0000", "ds@x.com", "S2");
    std::fs::write(c.raiz.join(".claude/contas/.ativa"), "reserva\n").unwrap();
    c.global("G", "r");
    // A pasta do kit, com o nome dos 8 primeiros do uuid e sem keep.json.
    c.fixa("aaaa1111", Some(("F", "r")), None);
    c.codex(&c.raiz.join(".codex"), "ds@x.com", "acct-1");
    c.ordem(&["# fila", "claude:reserva", "gpt:principal", "claude:principal"]);

    let contas = listar(true);
    assert_eq!(chaves(&contas), ["claude:reserva", "gpt:principal", "claude:principal"]);
    let reserva = &contas[0];
    assert_eq!(reserva.armazens.len(), 3, "slot, global e fixa da mesma pessoa: {:?}", reserva.armazens);
    assert!(reserva.is_active);
    assert_eq!(ambiente(reserva), Ok(vec![]));
    let principal = &contas[2];
    assert!(!principal.roda, "um slot é cópia: nenhuma aba roda nele");
    assert!(principal.has_token, "mas ele mede o consumo");
    assert_eq!(
        ambiente(principal),
        Err(Falta::PrecisaLogin { chave: "claude:principal".into(), apelido: "principal".into() })
    );
}

#[test]
fn global_recusado_manda_para_a_fixa_da_mesma_conta() {
    let c = casa("recusado");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-1", "ana@x.com")]);
    c.global("G", "");
    let fixa = c.fixa("k-1", Some(("F", "r")), Some(("ana", "ana@x.com", "u-1")));
    let contas = listar(true);
    assert_eq!(contas.len(), 1);
    assert!(contas[0].warning.is_none(), "uma conta que roda na fixa funciona");
    assert_eq!(
        ambiente(&contas[0]),
        Ok(vec![("CLAUDE_SECURESTORAGE_CONFIG_DIR".into(), fixa.to_string_lossy().into_owned())])
    );
}

#[test]
fn global_morto_sem_dono_nao_toma_a_chave_da_conta_viva() {
    // 10/11: o global morreu (o servidor não diz de quem é um token morto), o
    // `.ativa` que o kit deixou dizia "principal", e a conta morta passou na
    // frente da viva com a mesma chave: a aba presa a ela ficou sem acesso.
    let c = casa("global-morto");
    perfis(&[("F", "u-1", "ana@x.com")]);
    c.slot("principal", "u-1", "ana@x.com", "S1");
    std::fs::write(c.raiz.join(".claude/contas/.ativa"), "principal\n").unwrap();
    c.global("MORTO", "");
    let fixa = c.fixa("k-1", Some(("F", "r")), Some(("principal", "ana@x.com", "u-1")));
    c.ordem(&["# fila", "claude:principal"]);

    let contas = listar(true);
    let principais: Vec<&Conta> = contas.iter().filter(|c| c.e("claude:principal")).collect();
    assert_eq!(principais.len(), 1, "uma chave, uma conta: {:?}", chaves(&contas));
    let achada = achar(&contas, "claude:principal").expect("a conta da chave");
    assert_eq!(achada.key, "u-1");
    assert!(achada.roda);
    assert_eq!(
        ambiente(achada),
        Ok(vec![("CLAUDE_SECURESTORAGE_CONFIG_DIR".into(), fixa.to_string_lossy().into_owned())])
    );
    assert_eq!(contas[0].key, "u-1", "a conta viva fica em primeiro, como na ordem");
}

#[test]
fn achar_prefere_a_conta_que_roda_entre_duas_com_a_mesma_chave() {
    let base = |key: &str, roda: bool| Conta {
        engine: Motor::Claude,
        key: key.into(),
        alias: "principal".into(),
        aliases: vec!["principal".into()],
        email: None,
        plan: None,
        is_active: false,
        is_preferred: false,
        warning: None,
        order_key: "claude:principal".into(),
        has_token: true,
        armazens: vec![],
        token: None,
        roda,
        weekly_cap: None,
    };
    let lista = [base("morta", false), base("viva", true)];
    assert_eq!(achar(&lista, "claude:principal").map(|c| c.key.as_str()), Some("viva"));
    let so_morta = [base("morta", false)];
    assert_eq!(achar(&so_morta, "claude:principal").map(|c| c.key.as_str()), Some("morta"), "sem outra, a que há");
}

#[test]
fn login_recusado_em_todo_lugar_vira_aviso() {
    let c = casa("aviso");
    perfis(&[("G", "u-1", "ana@x.com")]);
    c.global("G", "");
    let contas = listar(true);
    assert_eq!(contas.len(), 1);
    assert!(contas[0].warning.as_deref().is_some_and(|w| w.contains("recusado")), "{:?}", contas[0].warning);
    assert!(!contas[0].roda);
}

#[test]
fn mover_troca_com_a_vizinha_e_grava_tudo() {
    let c = casa("mover");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
    c.global("G", "r");
    c.fixa("k-2", Some(("F", "r")), Some(("bia", "bia@y.com", "u-2")));
    c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
    let _ = listar(true); // os donos ficam em cache
    assert_eq!(mover("gpt:principal", true, "keep ia ordem").unwrap(), ["claude:ana", "gpt:principal", "claude:bia"]);
    assert_eq!(ler_ordem(), ["claude:ana", "gpt:principal", "claude:bia"]);
    assert_eq!(mover("claude:ana", true, "keep ia ordem").unwrap(), ["claude:ana", "gpt:principal", "claude:bia"], "a 1ª não sobe");
    assert_eq!(mover("claude:bia", false, "keep ia ordem").unwrap(), ["claude:ana", "gpt:principal", "claude:bia"], "a última não desce");
    assert!(mover("claude:ninguem", true, "keep ia ordem").is_err());
    let contas = listar(false);
    assert!(contas[0].is_preferred);
}

#[test]
fn o_teto_da_semanal_vem_da_ordem_e_fica_quando_ela_e_regravada() {
    let c = casa("teto");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
    c.global("G", "r");
    c.fixa("k-2", Some(("F", "r")), Some(("bia", "bia@y.com", "u-2")));
    c.ordem(&["# fila", "claude:bia teto7d=80", "claude:ana"]);
    let contas = listar(true);
    assert_eq!(chaves(&contas), ["claude:bia", "claude:ana"]);
    assert_eq!(contas[0].weekly_cap, Some(80.0));
    assert_eq!(contas[1].weekly_cap, None, "a linha sem teto vale como antes");
    assert!(serde_json::to_string(&contas[0]).unwrap().contains(r#""weeklyCap":80.0"#), "o rodapé lê o teto");
    // Reordenar pela barra não perde o teto.
    assert_eq!(mover("claude:ana", true, "keep ia ordem").unwrap(), ["claude:ana", "claude:bia"]);
    assert_eq!(ler_fila(), [("claude:ana".to_string(), None), ("claude:bia".to_string(), Some(80.0))]);
    assert_eq!(ler_ordem(), ["claude:ana", "claude:bia"]);
    // Nem um login novo que entra no fim da fila.
    c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
    acrescentar_na_ordem("gpt:principal").unwrap();
    let texto = std::fs::read_to_string(crate::caminhos::ordem()).unwrap();
    assert!(texto.ends_with("\nclaude:ana\nclaude:bia teto7d=80\ngpt:principal\n"), "{texto}");
    assert_eq!(listar(false)[1].weekly_cap, Some(80.0));
}

#[test]
fn o_teto_aceita_por_cento_e_recusa_o_que_nao_e_teto() {
    assert_eq!(teto_de("teto7d=80"), Some(80.0));
    assert_eq!(teto_de("teto7d=80%"), Some(80.0));
    assert_eq!(teto_de("outra=1 teto7d=72.5"), Some(72.5));
    assert_eq!(teto_de(""), None);
    assert_eq!(teto_de("teto7d=abc"), None);
    assert_eq!(teto_de("teto7d=150"), None, "acima de 100 não é teto");
    assert_eq!(teto_de("teto7d=-1"), None);
    assert_eq!(teto_de("teto5h=80"), None, "só a semanal tem teto");
}

#[test]
fn sem_rede_o_dono_vem_do_cache() {
    let c = casa("cache");
    perfis(&[("G", "u-1", "ana@x.com")]);
    c.global("G", "r");
    assert_eq!(listar(true)[0].alias, "ana");
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::set_var("KEEP_IA_PERFIL_URL", "http://127.0.0.1:9/nada") };
    assert_eq!(listar(true)[0].alias, "ana", "o dono ficou guardado pela impressão do token");
}

#[test]
fn apelidos_de_email_sao_limpos_e_unicos() {
    assert_eq!(apelido_de_email("joão.silva+x@y.com"), "jo-o.silva-x");
    assert_eq!(apelido_de_email("@y.com"), "conta");
    let c = casa("unicos");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "ana@y.com")]);
    c.global("G", "r");
    c.fixa("k-1", Some(("F", "r")), None);
    let contas = listar(true);
    assert_eq!(chaves(&contas), ["claude:ana", "claude:ana-2"]);
}

#[test]
fn uma_casa_de_teste_sem_o_chaveiro_de_teste_nunca_chega_ao_de_verdade() {
    let _c = casa("sem-chaveiro");
    // SAFETY: a casa (e a trava) está de pé.
    unsafe { std::env::remove_var("KEEP_IA_SECURITY") };
    let item = credencial::Local::Chaveiro(credencial::SERVICO_GLOBAL.into());
    assert!(matches!(credencial::ler(&item), credencial::Lido::Ausente));
    assert!(matches!(credencial::global(), credencial::Lido::Ausente | credencial::Lido::Ilegivel(_)));
    // (Só o apagar prova a recusa: sem ela, ele não acharia o item de uma
    // conta "teste" e nada mudaria; um gravar sem ela gravaria de verdade.)
    assert!(credencial::apagar(&item).unwrap_err().contains("casa de teste"));
}

#[test]
fn toda_mudanca_da_ordem_fica_registrada_com_o_caminho() {
    let c = casa("registro-da-ordem");
    perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
    c.global("G", "r");
    c.fixa("k-2", Some(("F", "r")), Some(("bia", "bia@y.com", "u-2")));
    c.ordem(&["# fila", "claude:ana", "claude:bia"]);
    let registro = || -> Vec<Value> {
        std::fs::read_to_string(crate::caminhos::estado().join("ordem.log"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    };
    let _ = listar(true);
    assert!(registro().is_empty(), "a primeira leitura só conhece a fila");
    // As setas do rodapé.
    mover("claude:bia", true, "setas do rodapé").unwrap();
    // Por fora: a segunda conta ganha um teto que não tinha.
    c.ordem(&["# fila", "claude:bia", "claude:ana teto7d=75"]);
    let _ = ler_fila();
    let _ = ler_fila();
    // Vista já na leitura (o Keep pode nem voltar a gravar a fila), e uma vez só.
    let lido = registro();
    assert_eq!(lido.len(), 2, "{lido:#?}");
    assert_eq!(lido[1]["por"], mudancas::POR_FORA);
    // Só um comentário a mais por fora não é mudança da fila.
    c.ordem(&["# fila", "# comentário novo", "claude:bia", "claude:ana teto7d=75"]);
    let _ = listar(false);
    // O login de uma conta nova entra no fim.
    c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
    acrescentar_na_ordem("gpt:principal").unwrap();
    let r = registro();
    assert_eq!(r.len(), 3, "{r:#?}");
    assert_eq!(r[0]["por"], "setas do rodapé");
    assert_eq!((&r[0]["antes"], &r[0]["depois"]), (&serde_json::json!(["claude:ana", "claude:bia"]), &serde_json::json!(["claude:bia", "claude:ana"])));
    assert_eq!(r[1]["por"], mudancas::POR_FORA);
    assert_eq!(r[1]["antes"], serde_json::json!(["claude:bia", "claude:ana"]), "o antes é o que o Keep tinha gravado");
    assert_eq!(r[1]["depois"], serde_json::json!(["claude:bia", "claude:ana teto7d=75"]));
    assert_eq!(r[2]["por"], "login de conta nova");
    assert_eq!(r[2]["depois"], serde_json::json!(["claude:bia", "claude:ana teto7d=75", "gpt:principal"]));
    assert!(r.iter().all(|l| l["quando"].as_str().is_some_and(|q| q.len() == 19)), "{r:#?}");
    // A última mudança e de onde veio, para quem avisa de uma troca de conta.
    let v = mudancas::vista().unwrap();
    assert_eq!(v.por, "login de conta nova");
    assert!(v.mudou_ms > 0);
}
