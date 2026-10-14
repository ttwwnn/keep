//! O Keep como o programa do esquema `claude-cli://` no Windows (`keep links instalar | remover | conferir`).
//!
//! O que o Claude Code faz lá (lido no executável 2.1.296, e registrado em docs/restauro.md): em toda sessão confere
//! `HKCU\Software\Classes\claude-cli\shell\open\command` e, se o comando não for o `claude.exe --handle-uri "%1"` dele,
//! reescreve — a não ser que o `settings.json` tenha `"disableDeepLinkRegistration": "disable"`. Quando recebe um link,
//! abre o `wt.exe`, o `pwsh.exe`, o `powershell.exe` ou o cmd; não olha `TERM_PROGRAM` nem `deepLinkTerminal`, então não
//! há como fazê-lo escolher o Keep. O desvio é registrar o Keep no lugar dele, e ele mesmo abre a aba (`deeplink.rs`).
//!
//! Por isso ligar faz DUAS coisas, ambas do usuário, em HKCU (nunca HKLM, sem administrador) e ambas desfeitas por
//! `remover`:
//!
//! 1. `HKCU\Software\Classes\claude-cli` passa a abrir `"<Keep.exe>" --open-uri "%1"`;
//! 2. o `settings.json` do Claude Code ganha `"disableDeepLinkRegistration": "disable"`, para que ele não refaça (1) na
//!    próxima sessão.
//!
//! O que havia antes de cada uma é guardado em `links-windows.json` (na pasta de estado do Keep) ANTES de mexer, e
//! `remover` devolve exatamente isso — só se o que está lá ainda é o que o Keep escreveu: se o Claude Code ou outro
//! programa mudou depois, não é mais do Keep e fica como está. O instalador não registra nada; o desinstalador chama
//! `remover`.
//!
//! Toda a lógica trabalha sobre o trait `Registro`: o de verdade (`windows-sys`) só existe no Windows, e os testes usam
//! um na memória.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::membro_json;

const SUBCHAVE: &str = r"Software\Classes\claude-cli";
const SUBCHAVE_COMANDO: &str = r"Software\Classes\claude-cli\shell\open\command";
const CHAVE_SETTINGS: &str = "disableDeepLinkRegistration";
const VALOR_SETTINGS: &str = "\"disable\"";
const ARGUMENTOS: &str = " --open-uri \"%1\"";

/// O registro do Windows, no que o Keep usa dele. O nome vazio é o valor `(Padrão)` da chave.
pub trait Registro {
    fn ler(&self, subchave: &str, nome: &str) -> Result<Option<String>, String>;
    /// Cria a chave se preciso.
    fn gravar(&mut self, subchave: &str, nome: &str, valor: &str) -> Result<(), String>;
    fn apagar_valor(&mut self, subchave: &str, nome: &str) -> Result<(), String>;
    /// A chave e tudo o que há abaixo dela.
    fn apagar_arvore(&mut self, subchave: &str) -> Result<(), String>;
    fn existe(&self, subchave: &str) -> Result<bool, String>;
}

/// O que a chave `claude-cli` tinha antes do Keep.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Antes {
    pub chave_existia: bool,
    pub padrao: Option<String>,
    pub url_protocol: Option<String>,
    pub comando: Option<String>,
}

/// O que o `settings.json` tinha antes do Keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntesSettings {
    pub caminho: String,
    /// O valor da chave como estava no arquivo (texto JSON); `None`: não havia.
    pub valor_antigo: Option<String>,
    pub arquivo_existia: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estado {
    pub versao: u32,
    /// O comando que o Keep escreveu no registro.
    pub comando: String,
    pub registro: Antes,
    /// `None`: o Keep não mexeu no `settings.json` (já tinha o valor, ou nada o impede de ser refeito).
    pub settings: Option<AntesSettings>,
}

fn arquivo_do_estado() -> PathBuf {
    keep_ia::caminhos::estado().join("links-windows.json")
}

/// O `settings.json` do usuário: na pasta do Claude Code (`CLAUDE_CONFIG_DIR`, senão `~/.claude`).
pub fn arquivo_de_settings() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(keep_ia::caminhos::claude)
        .join("settings.json")
}

/// O comando do registro para o programa `app`. Recusa o caminho que um comando de registro não carrega direito.
pub fn comando_para(app: &Path) -> Result<String, String> {
    let texto = app.to_string_lossy();
    if texto.chars().any(|c| c.is_control() || c == '"' || c == '%') {
        return Err(format!("o caminho do Keep tem aspas, % ou controle e não cabe num comando de registro: {texto}"));
    }
    let bytes = texto.as_bytes();
    let de_unidade = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
    if !(app.is_absolute() || de_unidade) {
        return Err(format!("o caminho do Keep não é absoluto: {texto}"));
    }
    Ok(format!("\"{texto}\"{ARGUMENTOS}"))
}

/// O programa que um comando de registro do Keep abre (`"C:\…\Keep.exe" --open-uri "%1"`); `None` se não é um.
pub fn app_do_comando(comando: &str) -> Option<PathBuf> {
    let resto = comando.strip_prefix('"')?;
    let (caminho, depois) = resto.split_once('"')?;
    (depois == ARGUMENTOS && !caminho.is_empty()).then(|| PathBuf::from(caminho))
}

fn le_antes(reg: &dyn Registro) -> Result<Antes, String> {
    Ok(Antes {
        chave_existia: reg.existe(SUBCHAVE)?,
        padrao: reg.ler(SUBCHAVE, "")?,
        url_protocol: reg.ler(SUBCHAVE, "URL Protocol")?,
        comando: reg.ler(SUBCHAVE_COMANDO, "")?,
    })
}

fn le_estado(arquivo: &Path) -> Option<Estado> {
    crate::ler::<Estado>(arquivo).filter(|e| e.versao == crate::VERSAO)
}

fn le_settings(arquivo: &Path) -> Result<Option<String>, String> {
    match std::fs::read(arquivo) {
        Ok(b) => String::from_utf8(b).map(Some).map_err(|_| format!("{} não é UTF-8", arquivo.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("não consegui ler {}: {e}", arquivo.display())),
    }
}

/// Grava o `settings.json` por um arquivo ao lado e uma troca (o Claude Code nunca lê meio arquivo), se o arquivo
/// ainda é o que foi lido; falso, sem gravar nada, se mudou nesse meio tempo.
fn grava_settings(arquivo: &Path, esperado: Option<&str>, novo: &str) -> Result<bool, String> {
    if le_settings(arquivo)?.as_deref() != esperado {
        return Ok(false);
    }
    let alvo = std::fs::canonicalize(arquivo).unwrap_or_else(|_| arquivo.to_path_buf());
    let pasta = alvo.parent().ok_or("o settings.json não tem pasta")?;
    std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
    let ao_lado = pasta.join(format!(".settings.json.keep-{}", std::process::id()));
    std::fs::write(&ao_lado, novo).map_err(|e| format!("não consegui gravar {}: {e}", ao_lado.display()))?;
    if let Err(e) = std::fs::rename(&ao_lado, &alvo) {
        let _ = std::fs::remove_file(&ao_lado);
        return Err(format!("não consegui gravar {}: {e}", alvo.display()));
    }
    Ok(true)
}

fn mesmo_valor(a: &str, b: &str) -> bool {
    match (serde_json::from_str::<serde_json::Value>(a), serde_json::from_str::<serde_json::Value>(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Liga: o Keep abre os links `claude-cli://`. Devolve o que fez, linha a linha.
pub fn ligar(reg: &mut dyn Registro, app: &Path, settings: &Path, arquivo_estado: &Path) -> Result<Vec<String>, String> {
    let nosso = comando_para(app)?;
    let atual = le_antes(reg)?;
    let anterior = le_estado(arquivo_estado);
    let mut feito = Vec::new();

    // O que havia antes é o que estava lá quando o Keep não era o dono; religar sobre o próprio registro não o apaga.
    let ja_do_keep = atual.comando.as_deref().and_then(app_do_comando).is_some();
    let registro = match (&anterior, ja_do_keep) {
        (Some(e), true) => e.registro.clone(),
        // O registro é de um Keep e o estado se perdeu: do que havia antes nada se sabe, e a chave sai inteira ao remover.
        (None, true) => Antes::default(),
        (_, false) => atual,
    };

    // O settings.json: só se preciso, e lendo antes de qualquer coisa ser gravada (arquivo estranho recusa tudo).
    let texto = le_settings(settings)?;
    let valor_agora = match &texto {
        Some(t) => membro_json::valor_bruto(t, CHAVE_SETTINGS).map_err(|e| format!("{}: {e}", settings.display()))?,
        None => None,
    };
    let ja_disable = valor_agora.as_deref().is_some_and(|v| mesmo_valor(v, VALOR_SETTINGS));
    let registrado = anterior.as_ref().and_then(|e| e.settings.clone());
    let (antes_settings, novo_texto) = if ja_disable {
        // Já está como o Keep quer: se foi o Keep que pôs, o registro dele continua; senão, a pessoa já o tinha.
        (registrado, None)
    } else {
        let novo = membro_json::define(texto.as_deref(), CHAVE_SETTINGS, VALOR_SETTINGS).map_err(|e| format!("{}: {e}", settings.display()))?;
        (
            Some(AntesSettings { caminho: settings.display().to_string(), valor_antigo: valor_agora, arquivo_existia: texto.is_some() }),
            Some(novo),
        )
    };

    let estado = Estado { versao: crate::VERSAO, comando: nosso.clone(), registro, settings: antes_settings };
    // O estado primeiro: uma queda no meio ainda deixa o caminho de volta.
    crate::gravar(arquivo_estado, &estado).map_err(|e| format!("não consegui gravar {}: {e}", arquivo_estado.display()))?;

    if let Some(novo) = novo_texto {
        if !grava_settings(settings, texto.as_deref(), &novo)? {
            return Err(format!("{} mudou enquanto o Keep o lia; tente de novo", settings.display()));
        }
        feito.push(format!("{}: {CHAVE_SETTINGS} = disable (o Claude Code não refaz o registro)", settings.display()));
    }
    reg.gravar(SUBCHAVE, "", "URL:claude-cli")?;
    reg.gravar(SUBCHAVE, "URL Protocol", "")?;
    reg.gravar(SUBCHAVE_COMANDO, "", &nosso)?;
    feito.push(format!("HKCU\\{SUBCHAVE}: {nosso}"));
    Ok(feito)
}

/// Desliga: devolve o registro e o `settings.json` ao que eram, sem tocar no que deixou de ser do Keep.
pub fn desligar(reg: &mut dyn Registro, arquivo_estado: &Path) -> Result<Vec<String>, String> {
    let mut feito = Vec::new();
    let atual = reg.ler(SUBCHAVE_COMANDO, "")?;
    let Some(estado) = le_estado(arquivo_estado) else {
        // Sem o registro do que havia: se o comando é de um Keep, sai a chave inteira (o Claude Code a refaz quando
        // quiser); se é de outro, fica.
        if atual.as_deref().and_then(app_do_comando).is_some() {
            reg.apagar_arvore(SUBCHAVE)?;
            feito.push(format!("HKCU\\{SUBCHAVE}: removida (o Keep não guardou o que havia antes)"));
        } else {
            feito.push("não havia nada do Keep para desfazer".into());
        }
        return Ok(feito);
    };

    if atual.as_deref() == Some(estado.comando.as_str()) {
        let a = &estado.registro;
        if !a.chave_existia {
            reg.apagar_arvore(SUBCHAVE)?;
            feito.push(format!("HKCU\\{SUBCHAVE}: removida (não existia antes)"));
        } else {
            for (subchave, nome, valor) in [(SUBCHAVE, "", &a.padrao), (SUBCHAVE, "URL Protocol", &a.url_protocol), (SUBCHAVE_COMANDO, "", &a.comando)] {
                match valor {
                    Some(v) => reg.gravar(subchave, nome, v)?,
                    None => reg.apagar_valor(subchave, nome)?,
                }
            }
            // A chave existia sem programa nenhum: o `shell` que o Keep criou sai também.
            if a.comando.is_none() {
                reg.apagar_arvore(&format!("{SUBCHAVE}\\shell"))?;
            }
            feito.push(format!("HKCU\\{SUBCHAVE}: devolvida ao que era ({})", a.comando.as_deref().unwrap_or("sem comando")));
        }
    } else {
        feito.push(format!("HKCU\\{SUBCHAVE}: já não é do Keep ({}); deixada como está", atual.as_deref().unwrap_or("sem comando")));
    }

    if let Some(s) = &estado.settings {
        let arquivo = PathBuf::from(&s.caminho);
        match le_settings(&arquivo)? {
            Some(texto) => {
                let agora = membro_json::valor_bruto(&texto, CHAVE_SETTINGS).map_err(|e| format!("{}: {e}", arquivo.display()))?;
                if agora.as_deref().is_some_and(|v| mesmo_valor(v, VALOR_SETTINGS)) {
                    let novo = match &s.valor_antigo {
                        Some(v) => membro_json::define(Some(&texto), CHAVE_SETTINGS, v),
                        None => membro_json::tira(&texto, CHAVE_SETTINGS),
                    }
                    .map_err(|e| format!("{}: {e}", arquivo.display()))?;
                    if !grava_settings(&arquivo, Some(&texto), &novo)? {
                        return Err(format!("{} mudou enquanto o Keep o lia; tente de novo", arquivo.display()));
                    }
                    feito.push(format!("{}: {CHAVE_SETTINGS} devolvido ao que era", arquivo.display()));
                } else {
                    feito.push(format!("{}: {CHAVE_SETTINGS} já não é o que o Keep pôs; deixado como está", arquivo.display()));
                }
            }
            None => feito.push(format!("{}: não existe mais", arquivo.display())),
        }
    }
    let _ = std::fs::remove_file(arquivo_estado);
    Ok(feito)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub ok: bool,
    pub descricao: String,
}

/// Como está agora: `ligado` é o registro ser de um Keep que existe; os itens dizem o que falta para os links
/// abrirem de ponta a ponta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Situacao {
    pub ligado: bool,
    pub app: Option<PathBuf>,
    pub itens: Vec<Item>,
}

pub fn conferir(reg: &dyn Registro, settings: &Path) -> Result<Situacao, String> {
    let comando = reg.ler(SUBCHAVE_COMANDO, "")?;
    let app = comando.as_deref().and_then(app_do_comando);
    let app_existe = app.as_ref().is_some_and(|a| a.is_file());
    let protocolo = reg.ler(SUBCHAVE, "URL Protocol")?.is_some();
    let texto = le_settings(settings)?;
    let desligado_no_claude = texto
        .as_deref()
        .and_then(|t| membro_json::valor_bruto(t, CHAVE_SETTINGS).ok().flatten())
        .is_some_and(|v| mesmo_valor(&v, VALOR_SETTINGS));
    let itens = vec![
        Item {
            ok: app.is_some(),
            descricao: match &comando {
                Some(c) if app.is_none() => format!("o esquema claude-cli:// abre outro programa: {c}"),
                Some(c) => format!("o esquema claude-cli:// abre o Keep ({c})"),
                None => "o esquema claude-cli:// não está registrado".into(),
            },
        },
        Item { ok: app_existe, descricao: "o Keep registrado existe no disco".into() },
        Item { ok: protocolo, descricao: "o esquema está marcado como protocolo (URL Protocol)".into() },
        Item { ok: desligado_no_claude, descricao: format!("o Claude Code não refaz o registro ({CHAVE_SETTINGS} no settings.json)") },
    ];
    Ok(Situacao { ligado: app.is_some() && app_existe && protocolo, app, itens })
}

/// O programa do Keep que o registro deve abrir: `--app`, senão `KEEP_APP_EXE` (o app o põe no ambiente do `keep`),
/// senão o `Keep.exe` ao lado deste `keep.exe` (instalado em `<pasta>\bin\keep.exe`, com o app em `<pasta>`).
pub fn acha_o_app(pedido: Option<&str>) -> Result<PathBuf, String> {
    let candidato = pedido
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("KEEP_APP_EXE").filter(|v| !v.is_empty()).map(PathBuf::from))
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let pasta = exe.parent()?;
            [pasta.join("Keep.exe"), pasta.parent()?.join("Keep.exe")].into_iter().find(|p| p.is_file())
        })
        .ok_or("não achei o Keep.exe; diga com --app CAMINHO")?;
    if !candidato.is_file() {
        return Err(format!("{} não existe", candidato.display()));
    }
    let candidato = candidato.canonicalize().map(keep_ia::caminhos::sem_prefixo).map_err(|e| e.to_string())?;
    Ok(candidato)
}

#[cfg(windows)]
pub mod real {
    //! O registro de verdade, em `HKEY_CURRENT_USER`.
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW,
        RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };

    use super::Registro;

    pub struct Hkcu;

    fn largo(texto: &str) -> Vec<u16> {
        std::ffi::OsStr::new(texto).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn nao_existe(status: u32) -> bool {
        status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND
    }

    /// Uma chave aberta, fechada ao sair de escopo.
    struct Aberta(HKEY);

    impl Drop for Aberta {
        fn drop(&mut self) {
            // SAFETY: aberta por `abre`/`cria`, fechada uma vez só.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn abre(subchave: &str, acesso: u32) -> Result<Option<Aberta>, String> {
        let nome = largo(subchave);
        let mut chave: HKEY = std::ptr::null_mut();
        // SAFETY: `nome` termina em NUL e `chave` é onde a chamada põe a alça que abre.
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, nome.as_ptr(), 0, acesso, &mut chave) };
        if nao_existe(status) {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("o registro recusou abrir HKCU\\{subchave} (erro {status})"));
        }
        Ok(Some(Aberta(chave)))
    }

    fn cria(subchave: &str) -> Result<Aberta, String> {
        let nome = largo(subchave);
        let mut chave: HKEY = std::ptr::null_mut();
        // SAFETY: `nome` termina em NUL, os ponteiros opcionais são nulos e `chave` recebe a alça.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                nome.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_QUERY_VALUE,
                std::ptr::null(),
                &mut chave,
                std::ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!("o registro recusou criar HKCU\\{subchave} (erro {status})"));
        }
        Ok(Aberta(chave))
    }

    impl Registro for Hkcu {
        fn ler(&self, subchave: &str, nome: &str) -> Result<Option<String>, String> {
            let Some(chave) = abre(subchave, KEY_QUERY_VALUE)? else { return Ok(None) };
            let valor = largo(nome);
            // Um comando de registro cabe em 32 KiB; um valor maior não é um que o Keep escreveu.
            let mut dados = vec![0u8; 64 * 1024];
            let mut tamanho = dados.len() as u32;
            let mut tipo = 0u32;
            // SAFETY: a chave está aberta, `valor` termina em NUL e o buffer tem o tamanho que se diz junto dele.
            let status = unsafe { RegQueryValueExW(chave.0, valor.as_ptr(), std::ptr::null(), &mut tipo, dados.as_mut_ptr(), &mut tamanho) };
            if nao_existe(status) {
                return Ok(None);
            }
            if status != ERROR_SUCCESS {
                return Err(format!("o registro recusou ler HKCU\\{subchave} (erro {status})"));
            }
            if tipo != REG_SZ && tipo != REG_EXPAND_SZ {
                return Ok(None);
            }
            let unidades: Vec<u16> = dados[..tamanho as usize]
                .chunks_exact(2)
                .map(|par| u16::from_le_bytes([par[0], par[1]]))
                .take_while(|&u| u != 0)
                .collect();
            Ok(Some(String::from_utf16_lossy(&unidades)))
        }

        fn gravar(&mut self, subchave: &str, nome: &str, valor: &str) -> Result<(), String> {
            let chave = cria(subchave)?;
            let nome_largo = largo(nome);
            let dados = largo(valor);
            // SAFETY: a chave está aberta, o nome termina em NUL e `dados` é o texto largo com o NUL, `len * 2` bytes.
            let status = unsafe {
                RegSetValueExW(chave.0, nome_largo.as_ptr(), 0, REG_SZ, dados.as_ptr().cast::<u8>(), (dados.len() * 2) as u32)
            };
            if status != ERROR_SUCCESS {
                return Err(format!("o registro recusou gravar HKCU\\{subchave} (erro {status})"));
            }
            Ok(())
        }

        fn apagar_valor(&mut self, subchave: &str, nome: &str) -> Result<(), String> {
            let Some(chave) = abre(subchave, KEY_SET_VALUE)? else { return Ok(()) };
            let valor = largo(nome);
            // SAFETY: a chave está aberta e `valor` termina em NUL.
            let status = unsafe { RegDeleteValueW(chave.0, valor.as_ptr()) };
            if status != ERROR_SUCCESS && !nao_existe(status) {
                return Err(format!("o registro recusou apagar um valor de HKCU\\{subchave} (erro {status})"));
            }
            Ok(())
        }

        fn apagar_arvore(&mut self, subchave: &str) -> Result<(), String> {
            let nome = largo(subchave);
            // SAFETY: `nome` termina em NUL.
            let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, nome.as_ptr()) };
            if status != ERROR_SUCCESS && !nao_existe(status) {
                return Err(format!("o registro recusou apagar HKCU\\{subchave} (erro {status})"));
            }
            Ok(())
        }

        fn existe(&self, subchave: &str) -> Result<bool, String> {
            Ok(abre(subchave, KEY_QUERY_VALUE)?.is_some())
        }
    }
}

fn texto_do_item(i: &Item) -> String {
    format!("{} {}", if i.ok { "ok    " } else { "falta " }, i.descricao)
}

fn json_da_situacao(s: &Situacao) -> serde_json::Value {
    json!({
        "versao": crate::VERSAO,
        "ok": true,
        "ligado": s.ligado,
        "completo": s.itens.iter().all(|i| i.ok),
        "app": s.app.as_ref().map(|a| a.display().to_string()),
        "itens": s.itens.iter().map(|i| json!({ "ok": i.ok, "descricao": i.descricao })).collect::<Vec<_>>(),
    })
}

/// `keep links instalar [--app CAMINHO] [--json]`, `remover [--json]` e `conferir [--json]`, no Windows.
#[cfg(windows)]
pub fn cli(args: &[String]) -> i32 {
    run(&mut real::Hkcu, args)
}

/// Fora do Windows não há registro a mexer.
#[cfg(not(windows))]
pub fn cli(_args: &[String]) -> i32 {
    eprintln!("keep links: o registro do esquema claude-cli:// só existe no Windows (no macOS, `keep links instalar` monta o Ghostty.app)");
    2
}

/// O comando de linha de comando sobre um registro qualquer (o de verdade, ou o dos testes).
pub fn run(reg: &mut dyn Registro, args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let app_pedido = args.iter().position(|a| a == "--app").and_then(|i| args.get(i + 1)).cloned();
    let falha = |motivo: &str| {
        if json {
            println!("{}", json!({ "versao": crate::VERSAO, "ok": false, "motivo": "erro", "detalhe": motivo }));
        } else {
            eprintln!("{motivo}");
        }
        1
    };
    let settings = arquivo_de_settings();
    let estado = arquivo_do_estado();
    let situacao = |reg: &mut dyn Registro| conferir(reg, &settings);
    match args.first().map(String::as_str) {
        Some("instalar") => {
            let app = match acha_o_app(app_pedido.as_deref()) {
                Ok(a) => a,
                Err(e) => return falha(&e),
            };
            let feito = match ligar(reg, &app, &settings, &estado) {
                Ok(f) => f,
                Err(e) => return falha(&e),
            };
            match situacao(reg) {
                Ok(s) if json => println!("{}", json_da_situacao(&s)),
                Ok(_) => feito.iter().for_each(|l| println!("{l}")),
                Err(e) => return falha(&e),
            }
            0
        }
        Some("remover") => {
            let feito = match desligar(reg, &estado) {
                Ok(f) => f,
                Err(e) => return falha(&e),
            };
            match situacao(reg) {
                Ok(s) if json => println!("{}", json_da_situacao(&s)),
                Ok(_) => feito.iter().for_each(|l| println!("{l}")),
                Err(e) => return falha(&e),
            }
            0
        }
        Some("conferir") => match situacao(reg) {
            Ok(s) => {
                if json {
                    println!("{}", json_da_situacao(&s));
                } else {
                    s.itens.iter().for_each(|i| println!("{}", texto_do_item(i)));
                }
                s.itens.iter().filter(|i| !i.ok).count().min(100) as i32
            }
            Err(e) => falha(&e),
        },
        _ => {
            eprintln!("uso: keep links instalar [--app CAMINHO] [--json] | remover [--json] | conferir [--json] | abrir [--esperar S] LINK");
            2
        }
    }
}

#[cfg(test)]
mod testes {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// O registro na memória: valores por (chave, nome), mais as chaves que existem.
    #[derive(Default, Clone)]
    struct Falso {
        valores: BTreeMap<(String, String), String>,
        chaves: BTreeSet<String>,
    }

    impl Falso {
        fn com(mut self, subchave: &str, nome: &str, valor: &str) -> Self {
            self.gravar(subchave, nome, valor).unwrap();
            self
        }
    }

    impl Registro for Falso {
        fn ler(&self, subchave: &str, nome: &str) -> Result<Option<String>, String> {
            Ok(self.valores.get(&(subchave.to_string(), nome.to_string())).cloned())
        }
        fn gravar(&mut self, subchave: &str, nome: &str, valor: &str) -> Result<(), String> {
            // Cria a chave e as de cima, como o Windows.
            let mut caminho = String::new();
            for parte in subchave.split('\\') {
                if !caminho.is_empty() {
                    caminho.push('\\');
                }
                caminho.push_str(parte);
                self.chaves.insert(caminho.clone());
            }
            self.valores.insert((subchave.to_string(), nome.to_string()), valor.to_string());
            Ok(())
        }
        fn apagar_valor(&mut self, subchave: &str, nome: &str) -> Result<(), String> {
            self.valores.remove(&(subchave.to_string(), nome.to_string()));
            Ok(())
        }
        fn apagar_arvore(&mut self, subchave: &str) -> Result<(), String> {
            let prefixo = format!("{subchave}\\");
            self.chaves.retain(|c| c != subchave && !c.starts_with(&prefixo));
            self.valores.retain(|(c, _), _| c != subchave && !c.starts_with(&prefixo));
            Ok(())
        }
        fn existe(&self, subchave: &str) -> Result<bool, String> {
            Ok(self.chaves.contains(subchave))
        }
    }

    const DO_CLAUDE: &str = r#""C:\Users\ana\.local\bin\claude.exe" --handle-uri "%1""#;

    struct Cenario {
        dir: tempfile::TempDir,
        app: PathBuf,
        settings: PathBuf,
        estado: PathBuf,
    }

    fn cenario() -> Cenario {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Keep.exe");
        std::fs::write(&app, "x").unwrap();
        let app = app.canonicalize().unwrap();
        let settings = dir.path().join("claude").join("settings.json");
        let estado = dir.path().join("ia").join("links-windows.json");
        Cenario { dir, app, settings, estado }
    }

    fn do_claude() -> Falso {
        Falso::default()
            .com(SUBCHAVE, "", "URL:claude-cli")
            .com(SUBCHAVE, "URL Protocol", "")
            .com(SUBCHAVE_COMANDO, "", DO_CLAUDE)
    }

    #[test]
    fn o_comando_do_registro() {
        let c = comando_para(Path::new(r"C:\Users\ana\AppData\Local\Keep\Keep.exe")).unwrap();
        assert_eq!(c, r#""C:\Users\ana\AppData\Local\Keep\Keep.exe" --open-uri "%1""#);
        assert_eq!(app_do_comando(&c), Some(PathBuf::from(r"C:\Users\ana\AppData\Local\Keep\Keep.exe")));
        assert_eq!(app_do_comando(DO_CLAUDE), None, "o do Claude não é do Keep");
        assert_eq!(app_do_comando(r#""C:\x.exe" --open-uri "%1" --mais"#), None);
        assert_eq!(app_do_comando(r#"C:\x.exe --open-uri "%1""#), None);
        assert_eq!(app_do_comando(""), None);
        // O que não cabe num comando de registro.
        for ruim in [r#"C:\a"b\Keep.exe"#, r"C:\100%\Keep.exe", "C:\\a\nb\\Keep.exe", r"Keep.exe", r"relativo\Keep.exe", ""] {
            assert!(comando_para(Path::new(ruim)).is_err(), "{ruim}");
        }
    }

    #[test]
    fn ligar_e_remover_numa_maquina_sem_nada() {
        let c = cenario();
        let mut reg = Falso::default();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        let nosso = comando_para(&c.app).unwrap();
        assert_eq!(reg.ler(SUBCHAVE_COMANDO, "").unwrap().as_deref(), Some(nosso.as_str()));
        assert_eq!(reg.ler(SUBCHAVE, "").unwrap().as_deref(), Some("URL:claude-cli"));
        assert_eq!(reg.ler(SUBCHAVE, "URL Protocol").unwrap().as_deref(), Some(""));
        let s = std::fs::read_to_string(&c.settings).unwrap();
        assert_eq!(s, "{\n  \"disableDeepLinkRegistration\": \"disable\"\n}\n");
        assert!(conferir(&reg, &c.settings).unwrap().ligado);
        desligar(&mut reg, &c.estado).unwrap();
        assert!(!reg.existe(SUBCHAVE).unwrap(), "a chave não existia: sai inteira");
        assert!(reg.valores.is_empty());
        // O arquivo foi criado pelo Keep e fica só sem o membro.
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), "{}\n");
        assert!(!c.estado.exists());
        assert!(!conferir(&reg, &c.settings).unwrap().ligado);
    }

    #[test]
    fn ligar_sobre_o_registro_do_claude_e_devolver_a_ele() {
        let c = cenario();
        let original = "{\n  \"theme\": \"dark\",\n  \"permissions\": {\n    \"allow\": [\"Bash(git status)\"]\n  }\n}\n";
        std::fs::create_dir_all(c.settings.parent().unwrap()).unwrap();
        std::fs::write(&c.settings, original).unwrap();
        let mut reg = do_claude();
        let antes = reg.clone();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        let depois = std::fs::read_to_string(&c.settings).unwrap();
        assert!(depois.starts_with("{\n  \"theme\": \"dark\",\n  \"permissions\": {"), "{depois}");
        assert!(depois.contains("\"disableDeepLinkRegistration\": \"disable\""));
        desligar(&mut reg, &c.estado).unwrap();
        assert_eq!(reg.valores, antes.valores, "o comando do Claude de volta");
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), original, "o settings byte a byte");
    }

    #[test]
    fn um_valor_que_a_pessoa_ja_tinha_volta_como_era() {
        let c = cenario();
        let original = "{\n  \"disableDeepLinkRegistration\": \"enable\",\n  \"x\": 1\n}\n";
        std::fs::create_dir_all(c.settings.parent().unwrap()).unwrap();
        std::fs::write(&c.settings, original).unwrap();
        let mut reg = Falso::default();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        assert!(std::fs::read_to_string(&c.settings).unwrap().contains("\"disable\""));
        desligar(&mut reg, &c.estado).unwrap();
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), original);
    }

    #[test]
    fn quem_ja_tinha_o_disable_o_mantem_ao_remover() {
        let c = cenario();
        let original = "{\n  \"disableDeepLinkRegistration\": \"disable\"\n}\n";
        std::fs::create_dir_all(c.settings.parent().unwrap()).unwrap();
        std::fs::write(&c.settings, original).unwrap();
        let mut reg = do_claude();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), original, "nada a mudar");
        desligar(&mut reg, &c.estado).unwrap();
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), original, "a escolha dela não é desfeita");
    }

    #[test]
    fn ligar_duas_vezes_nao_perde_o_que_havia() {
        let c = cenario();
        let mut reg = do_claude();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        // E com o app noutro lugar (uma atualização que mudou a pasta).
        let outro = c.dir.path().join("novo").join("Keep.exe");
        std::fs::create_dir_all(outro.parent().unwrap()).unwrap();
        std::fs::write(&outro, "x").unwrap();
        ligar(&mut reg, &outro.canonicalize().unwrap(), &c.settings, &c.estado).unwrap();
        desligar(&mut reg, &c.estado).unwrap();
        assert_eq!(reg.ler(SUBCHAVE_COMANDO, "").unwrap().as_deref(), Some(DO_CLAUDE));
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), "{}\n");
    }

    #[test]
    fn o_que_mudou_depois_nao_e_desfeito() {
        let c = cenario();
        let mut reg = do_claude();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        // O Claude Code reescreveu o registro e a pessoa tirou o disable.
        reg.gravar(SUBCHAVE_COMANDO, "", r#""D:\outro\claude.exe" --handle-uri "%1""#).unwrap();
        std::fs::write(&c.settings, "{\n  \"disableDeepLinkRegistration\": \"enable\"\n}\n").unwrap();
        let feito = desligar(&mut reg, &c.estado).unwrap().join("\n");
        assert!(feito.contains("já não é do Keep"), "{feito}");
        assert_eq!(reg.ler(SUBCHAVE_COMANDO, "").unwrap().as_deref(), Some(r#""D:\outro\claude.exe" --handle-uri "%1""#));
        assert!(std::fs::read_to_string(&c.settings).unwrap().contains("\"enable\""));
        assert!(!c.estado.exists());
    }

    #[test]
    fn sem_o_estado_so_sai_o_que_e_de_um_keep() {
        let c = cenario();
        let mut do_keep = Falso::default();
        do_keep.gravar(SUBCHAVE_COMANDO, "", &comando_para(&c.app).unwrap()).unwrap();
        desligar(&mut do_keep, &c.estado).unwrap();
        assert!(!do_keep.existe(SUBCHAVE).unwrap());
        let mut claude = do_claude();
        let feito = desligar(&mut claude, &c.estado).unwrap().join("\n");
        assert!(feito.contains("nada do Keep"), "{feito}");
        assert_eq!(claude.ler(SUBCHAVE_COMANDO, "").unwrap().as_deref(), Some(DO_CLAUDE));
    }

    #[test]
    fn settings_ilegivel_recusa_tudo_e_nada_e_gravado() {
        let c = cenario();
        std::fs::create_dir_all(c.settings.parent().unwrap()).unwrap();
        std::fs::write(&c.settings, "{ // comentário\n}").unwrap();
        let mut reg = do_claude();
        let antes = reg.valores.clone();
        assert!(ligar(&mut reg, &c.app, &c.settings, &c.estado).is_err());
        assert_eq!(reg.valores, antes, "o registro não foi tocado");
        assert!(!c.estado.exists(), "nem o estado");
        assert_eq!(std::fs::read_to_string(&c.settings).unwrap(), "{ // comentário\n}");
    }

    #[test]
    fn a_situacao_diz_o_que_falta() {
        let c = cenario();
        let mut reg = Falso::default();
        let s = conferir(&reg, &c.settings).unwrap();
        assert!(!s.ligado && s.itens.iter().all(|i| !i.ok));
        reg = do_claude();
        let s = conferir(&reg, &c.settings).unwrap();
        assert!(!s.ligado);
        assert!(s.itens[0].descricao.contains("outro programa"), "{:?}", s.itens);
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        let s = conferir(&reg, &c.settings).unwrap();
        assert!(s.ligado && s.itens.iter().all(|i| i.ok), "{:?}", s.itens);
        assert_eq!(s.app.as_deref(), Some(c.app.as_path()));
        // O Claude Code refez o registro: desligado, e o item do settings continua certo.
        reg.gravar(SUBCHAVE_COMANDO, "", DO_CLAUDE).unwrap();
        assert!(!conferir(&reg, &c.settings).unwrap().ligado);
        // O app sumiu do disco.
        let mut reg = Falso::default();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        std::fs::remove_file(&c.app).unwrap();
        let s = conferir(&reg, &c.settings).unwrap();
        assert!(!s.ligado && !s.itens[1].ok);
    }

    #[test]
    fn a_resposta_para_o_app() {
        let c = cenario();
        let mut reg = Falso::default();
        ligar(&mut reg, &c.app, &c.settings, &c.estado).unwrap();
        let v = json_da_situacao(&conferir(&reg, &c.settings).unwrap());
        assert_eq!(v["versao"], 1);
        assert_eq!(v["ok"], true);
        assert_eq!(v["ligado"], true);
        assert_eq!(v["completo"], true);
        assert_eq!(v["itens"].as_array().unwrap().len(), 4);
    }
}
