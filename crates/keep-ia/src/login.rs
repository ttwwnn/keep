//! Entrar numa conta: o "+" do rodapé abre uma aba rodando `keep ia login`,
//! que faz o login do Claude Code ou do Codex num armazém novo do Keep e,
//! quando ele termina, põe a conta na ordem — e faz a troca de quem estava
//! esperando por ele.
//!
//! O Claude: o primeiro login vai para o login global (o que o `claude`
//! usa sem nada); os outros, cada um numa pasta própria
//! (`~/.claude/contas/fixas/k-…`, `CLAUDE_SECURESTORAGE_CONFIG_DIR`), com um
//! `.claude.json` descartável, para o login não trocar a identidade das
//! abas abertas. O Codex: o primeiro no `~/.codex`; os outros em
//! `~/.codex-contas/<apelido>` (`CODEX_HOME`), com as conversas e a
//! configuração ligadas às do principal.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::contas::credencial::{self, Lido, Oauth};
use crate::contas::{self, Metadados, Motor, donos};
use crate::linha::{self, Subida};
use crate::trocar::{self, Pedido};
use crate::{caminhos, daemon, programas, tela};

// ------------------------------------------------------------ abrir abas

/// Uma aba nova em `ws`, do tamanho de uma vizinha, rodando `args` do `keep`.
pub fn abre_aba_com(ws: &str, args: &[String]) -> Result<u32, String> {
    let r = daemon::listar()?;
    if !r.abas.iter().any(|a| a.ws == ws) {
        return Err(format!("o workspace “{ws}” não existe"));
    }
    let vizinha = r.abas.iter().find(|a| a.ws == ws && !a.info.finished);
    let (cols, rows) = vizinha.map(|a| (a.info.cols, a.info.rows)).unwrap_or((120, 40));
    let casa = caminhos::casa().to_string_lossy().into_owned();
    let tab = daemon::nova_aba(ws, Some(&casa), cols, rows)?;
    // O prompt do shell à vista.
    let pronta = daemon::espera(Duration::from_secs(15), Duration::from_millis(250), || {
        let r = daemon::listar().ok()?;
        let a = r.aba(ws, tab)?.clone();
        let livre = matches!(tela::programa(&a.info.command), tela::Programa::Shell(_)) && !a.info.busy;
        let algo = daemon::tela(ws, tab).ok().is_some_and(|t| !t.trim().is_empty());
        (livre && algo).then_some(a)
    });
    let a = pronta.ok_or("a aba nova não mostrou o prompt do shell")?;
    std::thread::sleep(Duration::from_millis(400));
    let sintaxe = match tela::programa(&a.info.command) {
        tela::Programa::Shell(nome) => linha::sintaxe(&nome),
        _ => linha::Sintaxe::Posix,
    };
    let eu = std::env::current_exe().map_err(|e| e.to_string())?;
    let s = Subida { programa: eu.to_string_lossy().into_owned(), args: args.to_vec(), ..Default::default() };
    match trocar::digita(ws, tab, &linha::linha(sintaxe, &s), true) {
        Ok(true) => Ok(tab),
        Ok(false) => Err("o comando não apareceu na aba nova".into()),
        Err(r) => Err(r.detalhe),
    }
}

/// `keep ia entrar claude|gpt --ws=<W>`.
pub fn cli_entrar(a: &crate::cli::Args) -> i32 {
    let motor = a.posicoes.get(1).map(String::as_str).unwrap_or("");
    let (true, Some(ws)) = (matches!(motor, "claude" | "gpt"), a.opcao("ws")) else {
        return crate::cli::uso_errado("keep ia entrar claude|gpt --ws=<W>");
    };
    match abre_aba_com(ws, &["ia".into(), "login".into(), motor.into()]) {
        Ok(aba) => crate::cli::responde(json!({ "ok": true, "ws": ws, "aba": aba })),
        Err(e) => crate::cli::responde(crate::cli::recusa("erro", &e)),
    }
}

// -------------------------------------------- o login de uma conta pedida

/// Quem espera pelo login de uma conta: a aba dele e as trocas a fazer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Espera {
    ws: String,
    aba: u32,
    pid: u32,
    em: u64,
    depois: Vec<(String, u32, String)>,
}

fn arquivo_espera(chave: &str) -> PathBuf {
    let limpo: String = chave.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    caminhos::estado().join(format!("login-{limpo}.json"))
}

/// Lê, muda e grava a espera do login de `chave`, sob trava (o pedido
/// repetido e o login mexem nela ao mesmo tempo).
fn com_espera<T>(chave: &str, muda: impl FnOnce(&mut Option<Espera>) -> T) -> Result<T, String> {
    let arq = arquivo_espera(chave);
    let _ = std::fs::create_dir_all(caminhos::estado());
    let trava = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(arq.with_extension("trava"))
        .map_err(|e| e.to_string())?;
    trava.lock().map_err(|e| e.to_string())?;
    let mut e: Option<Espera> = std::fs::read_to_string(&arq).ok().and_then(|t| serde_json::from_str(&t).ok());
    let saida = muda(&mut e);
    match &e {
        Some(d) => {
            let _ = std::fs::write(&arq, serde_json::to_vec_pretty(d).unwrap_or_default());
        }
        None => {
            let _ = std::fs::remove_file(&arq);
        }
    }
    let _ = trava.unlock();
    Ok(saida)
}

/// O login de pé: o processo dele vive (ou a aba acabou de abrir) e a aba existe.
fn de_pe(e: &Espera) -> bool {
    let vivo = if e.pid != 0 {
        crate::processos::vivo(e.pid)
    } else {
        donos::agora_ms().saturating_sub(e.em) < 60_000
    };
    vivo && daemon::listar().ok().is_some_and(|r| r.aba(&e.ws, e.aba).is_some())
}

/// Abre (ou acha aberta) a aba do login próprio da conta `apelido`, e anota
/// que, quando ele terminar, a aba `ws#aba` passa para `para`. Devolve onde
/// está a aba do login e se ela foi aberta agora.
pub fn abre_login_da_conta(
    apelido: &str,
    email: &str,
    chave_conta: &str,
    ws: &str,
    aba: u32,
    para: &str,
) -> Result<(String, u32, bool), String> {
    let mut abriu = false;
    let mut falha = None;
    let resultado = com_espera(chave_conta, |e| {
        let aberta = e.as_ref().filter(|d| de_pe(d)).cloned();
        let mut d = match aberta {
            Some(d) => d,
            None => {
                let mut args = vec!["ia".to_string(), "login".into(), "claude".into(), format!("--conta={apelido}")];
                if !email.is_empty() {
                    args.push(format!("--email={email}"));
                }
                match abre_aba_com(ws, &args) {
                    Ok(nova) => {
                        abriu = true;
                        Espera { ws: ws.into(), aba: nova, pid: 0, em: donos::agora_ms(), depois: Vec::new() }
                    }
                    Err(x) => {
                        falha = Some(x);
                        return None;
                    }
                }
            }
        };
        let alvo = (ws.to_string(), aba, para.to_string());
        if !d.depois.contains(&alvo) {
            d.depois.push(alvo);
        }
        *e = Some(d.clone());
        Some(d)
    })?;
    if let Some(x) = falha {
        return Err(x);
    }
    let d = resultado.ok_or("não deu para abrir a aba do login")?;
    Ok((d.ws, d.aba, abriu))
}

// ------------------------------------------------------- o login na aba

fn pergunta(texto: &str, padrao: &str) -> String {
    if padrao.is_empty() {
        print!("{texto}: ");
    } else {
        print!("{texto} [{padrao}]: ");
    }
    let _ = std::io::stdout().flush();
    let mut l = String::new();
    if std::io::stdin().lock().read_line(&mut l).unwrap_or(0) == 0 {
        return padrao.to_string();
    }
    let l = l.trim().to_string();
    if l.is_empty() { padrao.to_string() } else { l }
}

fn email_valido(e: &str) -> bool {
    let Some((a, b)) = e.split_once('@') else { return false };
    !a.is_empty() && b.contains('.') && !e.contains(char::is_whitespace)
}

/// Apaga o login que este processo acabou de criar numa pasta nova (login
/// numa conta errada, ou sem dono confirmado). Nunca toca em outro.
fn apaga_login_da_pasta(pasta: &Path) {
    if cfg!(target_os = "macos") {
        let servico = credencial::servico_de(&pasta.to_string_lossy());
        let security = std::env::var("KEEP_IA_SECURITY").unwrap_or_else(|_| "/usr/bin/security".into());
        let _ = std::process::Command::new(security)
            .args(["delete-generic-password", "-a", &credencial::conta_do_item(), "-s", &servico])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = std::fs::remove_file(pasta.join(".credentials.json"));
}

/// O token do login lido de um armazém, se ele roda.
fn token_de(lido: Lido) -> Option<String> {
    match lido {
        Lido::Ok(v) => Oauth::de(&v).filter(|o| !o.recusado()).and_then(|o| o.acesso),
        _ => None,
    }
}

fn claude_bin() -> PathBuf {
    programas::claude().unwrap_or_else(|| PathBuf::from("claude"))
}

/// Roda `claude auth login` com o ambiente dado; `true` se saiu bem.
fn roda_login_claude(email: &str, securestorage: Option<&Path>, config: Option<&Path>) -> bool {
    let mut cmd = std::process::Command::new(claude_bin());
    cmd.args(["auth", "login", "--claudeai"]);
    if !email.is_empty() {
        cmd.args(["--email", email]);
    }
    trocar::ambiente_limpo(&mut cmd);
    if let Some(p) = securestorage {
        cmd.env("CLAUDE_SECURESTORAGE_CONFIG_DIR", p);
    }
    if let Some(c) = config {
        cmd.env("CLAUDE_CONFIG_DIR", c);
    }
    cmd.status().is_ok_and(|s| s.success())
}

fn depois_do_login(alvos: &[(String, u32, String)]) {
    for (ws, aba, para) in alvos {
        println!("\nPassando a aba {ws}#{aba} para {}…", trocar::rotulo(para));
        let p = Pedido { ws: ws.clone(), aba: *aba, para: para.clone(), ..Default::default() };
        match trocar::trocar(&p) {
            Ok(feito) => println!("Pronto: {feito}."),
            Err(r) => println!(
                "Não troquei agora: {}\nQuando quiser, escolha {} no menu da aba: o login já está pronto.",
                r.detalhe,
                trocar::rotulo(para)
            ),
        }
    }
}

/// `keep ia login claude [--conta=<apelido>] [--email=<e>]`.
fn login_claude(conta: Option<&str>, email_dado: Option<&str>) -> Result<(), String> {
    let contas_agora = contas::listar(true);
    let pedida = conta.and_then(|ap| contas_agora.iter().find(|c| c.engine == Motor::Claude && c.aliases.iter().any(|a| a == ap)));
    if conta.is_some() && pedida.is_none() {
        return Err(format!("Não há a conta “{}” no Keep.", conta.unwrap_or_default()));
    }
    let global_roda = token_de(credencial::global()).is_some();
    let no_global = pedida.is_none() && !global_roda;
    match pedida {
        Some(c) => {
            println!("Login próprio das abas na conta {} ({}).", c.alias, c.email.as_deref().unwrap_or("sem e-mail"));
            println!("É uma aprovação no navegador, uma vez só. Ctrl+C desiste sem mudar nada.\n");
        }
        None => println!("Entrar em outra conta do Claude\n"),
    }
    let email = match (email_dado, pedida) {
        (Some(e), _) if !e.is_empty() => e.to_string(),
        (_, Some(c)) => c.email.clone().unwrap_or_default(),
        _ => {
            let e = pergunta("E-mail da conta (Enter para escolher no navegador)", "");
            if !e.is_empty() && !email_valido(&e) {
                return Err(format!("E-mail inválido: {e}"));
            }
            e
        }
    };
    if no_global {
        println!("Abrindo o login do Claude no navegador. Se ele pedir um código, cole aqui.");
        if !roda_login_claude(&email, None, None) {
            return Err("Login cancelado. Nada mudou.".into());
        }
        let acesso = token_de(credencial::global()).ok_or("O login terminou sem credencial. Nada mudou.")?;
        let dono = donos::dono(&acesso, true);
        let lista = contas::listar(true);
        let c = lista
            .iter()
            .find(|c| c.engine == Motor::Claude && dono.as_ref().is_some_and(|d| d.uuid == c.key))
            .or_else(|| lista.iter().find(|c| c.engine == Motor::Claude && c.global()));
        if let Some(c) = c {
            let _ = contas::acrescentar_na_ordem(&c.order_key);
            println!("\nPronto: {} ({}) entrou no Keep.", trocar::rotulo(&c.order_key), c.email.as_deref().unwrap_or(""));
        } else {
            println!("\nPronto: o login global do Claude está feito.");
        }
        return Ok(());
    }
    let pasta = contas::pasta_fixa_nova();
    std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o700));
    }
    let descartavel = caminhos::estado().join(format!("login-config-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&descartavel);
    let desfaz = |pasta: &Path| {
        apaga_login_da_pasta(pasta);
        let _ = std::fs::remove_dir_all(pasta);
    };
    println!("Abrindo o login do Claude no navegador. Se ele pedir um código, cole aqui.");
    let saiu = roda_login_claude(&email, Some(&pasta), Some(&descartavel));
    let _ = std::fs::remove_dir_all(&descartavel);
    if !saiu {
        desfaz(&pasta);
        return Err("Login cancelado. Nada mudou.".into());
    }
    let Some(acesso) = token_de(credencial::fixa(&pasta)) else {
        desfaz(&pasta);
        return Err("O login terminou sem credencial. Nada mudou.".into());
    };
    let Some(dono) = donos::dono(&acesso, true) else {
        desfaz(&pasta);
        return Err("Não consegui confirmar de quem é o login (sem rede?). Nada mudou.".into());
    };
    if let Some(c) = pedida {
        if c.key != dono.uuid && c.email.as_deref().is_none_or(|e| !e.eq_ignore_ascii_case(&dono.email)) {
            desfaz(&pasta);
            return Err(format!("O login terminou em OUTRA conta ({}), não em {}. Nada ficou gravado.", dono.email, c.alias));
        }
    }
    // A conta de quem entrou: a pedida, ou uma que já existe com o mesmo dono.
    let existente = contas_agora.iter().find(|c| c.engine == Motor::Claude && c.key == dono.uuid);
    let apelido = match existente {
        Some(c) => c.alias.clone(),
        None => {
            let usados: Vec<String> = contas_agora.iter().flat_map(|c| c.chaves()).collect();
            let mut base = contas::apelido_de_email(&dono.email);
            let mut n = 2;
            while usados.contains(&Motor::Claude.chave(&base)) {
                base = format!("{}-{n}", contas::apelido_de_email(&dono.email));
                n += 1;
            }
            let ap = pergunta("Apelido no Keep", &base);
            if !contas::apelido_valido(&ap) || ap == "ordem" || usados.contains(&Motor::Claude.chave(&ap)) {
                desfaz(&pasta);
                return Err(format!("Apelido inválido ou já usado: {ap}"));
            }
            ap
        }
    };
    contas::gravar_metadados(
        &pasta,
        &Metadados { versao: 1, apelido: apelido.clone(), email: dono.email.clone(), uuid: dono.uuid.clone(), criada_em: donos::agora_ms() },
    )
    .map_err(|e| e.to_string())?;
    let chave = Motor::Claude.chave(&apelido);
    let _ = contas::acrescentar_na_ordem(&chave);
    let lista = contas::listar(false);
    let pos = contas::ordem_efetiva(&lista).iter().position(|k| *k == chave).map(|i| (i + 1).to_string()).unwrap_or("?".into());
    println!("\nPronto: Claude · {apelido} ({}) está no Keep, na posição {pos} da fila.", dono.email);
    Ok(())
}

/// O que as contas do GPT dividem com o `~/.codex`: conversas, configuração,
/// skills e afins. O `auth.json` nunca (cada conta tem o seu), nem os bancos
/// SQLite (ligar banco com WAL estraga).
fn liga_ao_principal(home: &Path) -> Result<(), String> {
    let base = caminhos::codex();
    for nome in ["sessions", "config.toml", "AGENTS.md", "skills", "plugins", "packages", "prompts", "rules"] {
        let origem = base.join(nome);
        let destino = home.join(nome);
        if !origem.exists() || destino.symlink_metadata().is_ok() {
            continue;
        }
        let r = if origem.is_dir() { liga_pasta(&origem, &destino) } else { liga_arquivo(&origem, &destino) };
        if let Err(e) = r {
            eprintln!("aviso: não liguei {nome} ao ~/.codex ({e})");
        }
    }
    if home.join("auth.json").symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{}/auth.json é um link: recuso (o login de uma conta apagaria o da outra)", home.display()));
    }
    Ok(())
}

#[cfg(unix)]
fn liga_pasta(origem: &Path, destino: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(origem, destino)
}

#[cfg(unix)]
fn liga_arquivo(origem: &Path, destino: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(origem, destino)
}

#[cfg(windows)]
fn liga_pasta(origem: &Path, destino: &Path) -> std::io::Result<()> {
    // Junção: não pede o modo de desenvolvedor, como o link simbólico pede.
    use std::os::windows::process::CommandExt;
    let st = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(destino)
        .arg(origem)
        .stdout(std::process::Stdio::null())
        .creation_flags(0x0800_0000)
        .status()?;
    if st.success() { Ok(()) } else { Err(std::io::Error::other("mklink /J falhou")) }
}

#[cfg(windows)]
fn liga_arquivo(origem: &Path, destino: &Path) -> std::io::Result<()> {
    std::fs::hard_link(origem, destino).or_else(|_| std::fs::copy(origem, destino).map(|_| ()))
}

/// `keep ia login gpt [--apelido=<A>]`.
fn login_gpt(apelido_dado: Option<&str>) -> Result<(), String> {
    println!("Entrar em outra conta do GPT (ChatGPT)\n");
    let contas_agora = contas::listar(false);
    let principal_tem = contas_agora.iter().any(|c| matches!(c.codex_home(), Some((_, true))));
    let codex = programas::codex().unwrap_or_else(|| PathBuf::from("codex"));
    let (home, apelido, nova) = if !principal_tem {
        (caminhos::codex(), "principal".to_string(), false)
    } else {
        let usados: Vec<String> = contas_agora.iter().filter(|c| c.engine == Motor::Codex).flat_map(|c| c.aliases.clone()).collect();
        let mut sugestao = "gpt-2".to_string();
        let mut n = 3;
        while usados.contains(&sugestao) || caminhos::codex_contas().join(&sugestao).exists() {
            sugestao = format!("gpt-{n}");
            n += 1;
        }
        let ap = apelido_dado.map(str::to_string).unwrap_or_else(|| pergunta("Apelido no Keep", &sugestao));
        if !contas::apelido_valido(&ap) || ap == "principal" || ap == "ordem" || usados.contains(&ap) {
            return Err(format!("Apelido inválido ou já usado: {ap}"));
        }
        let home = caminhos::codex_contas().join(&ap);
        let nova = !home.exists();
        std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700));
        }
        liga_ao_principal(&home)?;
        (home, ap, nova)
    };
    println!("Abrindo o login do Codex no navegador.");
    let mut cmd = std::process::Command::new(codex);
    cmd.arg("login");
    for (n, _) in std::env::vars_os() {
        if n.to_string_lossy().starts_with("CODEX") {
            cmd.env_remove(n);
        }
    }
    if apelido != "principal" {
        cmd.env("CODEX_HOME", &home);
    }
    let ok = cmd.status().is_ok_and(|s| s.success());
    let lista = contas::listar(false);
    let entrou = lista
        .iter()
        .find(|c| c.engine == Motor::Codex && c.codex_home().is_some_and(|(h, _)| h == &home) && c.has_token);
    let Some(c) = entrou.filter(|_| ok) else {
        if nova {
            let _ = std::fs::remove_dir_all(&home);
        }
        return Err("Login cancelado ou sem credencial. Nada mudou.".into());
    };
    // Outra conta GPT com o mesmo dono: é a mesma conta.
    if c.armazens.len() > 1 && nova {
        let _ = std::fs::remove_dir_all(&home);
        return Err(format!("Essa conta do GPT já está no Keep como “{}”.", c.alias));
    }
    let chave = Motor::Codex.chave(&apelido);
    let _ = contas::acrescentar_na_ordem(&chave);
    println!("\nPronto: GPT · {apelido} ({}) está na fila de prioridade.", c.email.as_deref().unwrap_or(""));
    Ok(())
}

/// `keep ia login claude|gpt …` (interativo, numa aba).
pub fn cli_login(a: &crate::cli::Args) -> i32 {
    let motor = a.posicoes.get(1).map(String::as_str).unwrap_or("");
    let resultado = match motor {
        "claude" => {
            let conta = a.opcao("conta");
            // O processo do login passa a ser o dono da espera dela.
            let chave_espera = conta.and_then(|ap| {
                contas::listar(false).into_iter().find(|c| c.engine == Motor::Claude && c.aliases.iter().any(|x| x == ap)).map(|c| c.key)
            });
            if let Some(k) = &chave_espera {
                let _ = com_espera(k, |e| {
                    if let Some(d) = e.as_mut() {
                        d.pid = std::process::id();
                    }
                });
            }
            let r = login_claude(conta, a.opcao("email"));
            let alvos = chave_espera
                .and_then(|k| {
                    com_espera(&k, |e| {
                        let alvos = e.as_ref().map(|d| d.depois.clone()).unwrap_or_default();
                        *e = None;
                        alvos
                    })
                    .ok()
                })
                .unwrap_or_default();
            if r.is_ok() {
                depois_do_login(&alvos);
            }
            r
        }
        "gpt" => login_gpt(a.opcao("apelido")),
        _ => return crate::cli::uso_errado("keep ia login claude [--conta=<apelido>] | gpt [--apelido=<A>]"),
    };
    match resultado {
        Ok(()) => {
            println!("\nPode fechar esta aba.");
            0
        }
        Err(e) => {
            println!("\n{e}");
            1
        }
    }
}
