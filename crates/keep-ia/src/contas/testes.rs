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
            "KEEP_IA_GERENTE_EXTERNO",
            "KEEP_IA_CLAUDE_BIN",
            "KEEP_IA_CODEX_BIN",
            "KEEP_IA_KEEP_BIN",
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
        "#!/bin/sh\nOP=\"$1\"; S=\nwhile [ $# -gt 0 ]; do [ \"$1\" = -s ] && { shift; S=\"$1\"; }; shift; done\n\
         F=\"$KEEP_IA_TESTE_CHAVEIRO/$S\"\n[ -f \"$F\" ] || exit 44\n\
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
        if cfg!(target_os = "macos") {
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
            if cfg!(target_os = "macos") {
                let servico = credencial::servico_de(&pasta.to_string_lossy());
                std::fs::write(self.raiz.join("chaveiro").join(servico), json).unwrap();
            } else {
                std::fs::write(pasta.join(".credentials.json"), json).unwrap();
            }
        }
        if let Some((apelido, email, uuid)) = meta {
            gravar_metadados(
                &pasta,
                &Metadados { versao: 1, apelido: apelido.into(), email: email.into(), uuid: uuid.into(), criada_em: 1 },
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
    perfis(&[("G", "4f35c383-0000", "ds@x.com"), ("F", "4f35c383-0000", "ds@x.com")]);
    c.slot("principal", "d32da84e-0000", "assinaturas@y.com", "S1");
    c.slot("reserva", "4f35c383-0000", "ds@x.com", "S2");
    std::fs::write(c.raiz.join(".claude/contas/.ativa"), "reserva\n").unwrap();
    c.global("G", "r");
    // A pasta do kit, com o nome dos 8 primeiros do uuid e sem keep.json.
    c.fixa("4f35c383", Some(("F", "r")), None);
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
    assert_eq!(mover("gpt:principal", true).unwrap(), ["claude:ana", "gpt:principal", "claude:bia"]);
    assert_eq!(ler_ordem(), ["claude:ana", "gpt:principal", "claude:bia"]);
    assert_eq!(mover("claude:ana", true).unwrap(), ["claude:ana", "gpt:principal", "claude:bia"], "a 1ª não sobe");
    assert_eq!(mover("claude:bia", false).unwrap(), ["claude:ana", "gpt:principal", "claude:bia"], "a última não desce");
    assert!(mover("claude:ninguem", true).is_err());
    let contas = listar(false);
    assert!(contas[0].is_preferred);
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
