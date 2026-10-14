//! Os links `claude-cli://` numa aba do Keep (e não no Terminal do macOS).
//!
//! O Claude Code abre um link `claude-cli://` (o "abrir no Claude Code" do
//! claude.ai) no terminal da preferência `deepLinkTerminal`, que ele tira do
//! `TERM_PROGRAM` — e o Keep diz `ghostty` (é feito com o libghostty). Então o
//! Claude Code roda `open -na Ghostty --args --window-save-state=never
//! [--working-directory=D] -e <claude> <args do link>`.
//!
//! `keep links instalar` monta `~/Applications/Ghostty.app`: um app de nome
//! "Ghostty", sem Dock, cujo executável é uma cópia deste `keep`. Chamado com o
//! nome "Ghostty", o `keep` pega a pasta e os argumentos do link e abre a
//! conversa com `keep nova` (aba nova no workspace da aba usada por último, com o
//! `claude` do shell da aba). Com o Keep fechado, abre o Keep e tenta de novo.
//! Log: `~/Library/Logs/desvio-deep-link.log`. Com o Ghostty de verdade em
//! `/Applications`, nada é instalado (os dois disputariam o nome).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::nova::{self, Conteudo, Pedido};

pub const IDENTIDADE: &str = "br.ttwwnn.keep.links";
/// O desvio que o kit-mac montava antes (`criar-desvio-deep-link.sh`): este o substitui.
const IDENTIDADE_DO_KIT: &str = "br.ttwwnn.kit.desvio-deep-link";
const VERSAO: u32 = 1;

fn app() -> PathBuf {
    keep_ia::caminhos::casa().join("Applications/Ghostty.app")
}

/// Este processo é o desvio (o executável do Ghostty.app)?
pub fn e_o_desvio() -> bool {
    std::env::current_exe().ok().and_then(|e| e.file_name().map(|n| n == "Ghostty")).unwrap_or(false)
}

/// A pasta e os argumentos do `claude` de uma chamada do Claude Code (o binário depois do `-e` não conta: a aba
/// usa o `claude` do shell dela).
pub fn ler_chamada(args: &[String]) -> (Option<String>, Vec<String>) {
    let mut pasta = None;
    let mut depois = None;
    for (i, a) in args.iter().enumerate() {
        if let Some(p) = a.strip_prefix("--working-directory=") {
            pasta = Some(p.to_string());
        } else if a == "-e" {
            depois = Some(i + 2);
            break;
        }
    }
    let resto = depois.map(|i| args.get(i..).unwrap_or(&[]).to_vec()).unwrap_or_default();
    (pasta, resto)
}

fn registra(texto: &str) {
    let arq = keep_ia::caminhos::casa().join("Library/Logs/desvio-deep-link.log");
    if let Some(p) = arq.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(arq) {
        let _ = writeln!(f, "{} {texto}", crate::agora_ms());
    }
}

fn abre_o_keep() {
    let _ = Command::new("/usr/bin/open").args(["-a", "Keep"]).status();
}

/// O desvio: abre a conversa do link numa aba do Keep.
pub fn desvio(args: &[String]) -> i32 {
    let (pasta, resto) = ler_chamada(args);
    let pasta = pasta.filter(|p| Path::new(p).is_dir()).map(PathBuf::from).unwrap_or_else(keep_ia::caminhos::casa);
    registra(&format!("link: pasta={} args={resto:?}", pasta.display()));
    if resto.is_empty() {
        abre_o_keep();
        return 0;
    }
    let pedido = Pedido { workspace: None, pasta, conteudo: Conteudo::Argumentos(resto), nome: None, sem_enter: false };
    let mut ultima = String::new();
    for tentativa in 0..12 {
        match nova::abrir(&pedido) {
            Ok((ws, tab, _)) => {
                registra(&format!("aberto em {ws} aba {tab}"));
                return 0;
            }
            Err(e) => ultima = e,
        }
        if tentativa == 0 {
            abre_o_keep();
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    registra(&format!("falhou: {ultima}"));
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", "display notification \"Não consegui abrir o link numa aba do Keep (log: ~/Library/Logs/desvio-deep-link.log)\" with title \"Claude Code\""])
        .status();
    1
}

/// O `keep` de onde a cópia saiu: tamanho e data. A cópia assinada difere do original byte a byte; é por isto que se
/// sabe se ela está em dia.
fn marca_da_origem(exe: &Path) -> String {
    std::fs::metadata(exe)
        .map(|m| {
            let quando = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            format!("{}:{quando}", m.len())
        })
        .unwrap_or_default()
}

fn info_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key>
	<string>Ghostty</string>
	<key>CFBundleIdentifier</key>
	<string>{IDENTIDADE}</string>
	<key>CFBundleName</key>
	<string>Ghostty</string>
	<key>CFBundleDisplayName</key>
	<string>Ghostty</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{VERSAO}</string>
	<key>CFBundleVersion</key>
	<string>{VERSAO}</string>
	<key>LSUIElement</key>
	<true/>
	<key>KeepLinks</key>
	<string>{VERSAO}</string>
</dict>
</plist>
"#
    )
}

/// De quem é o Ghostty.app que está lá (`None`: não há).
fn identidade_instalada(app: &Path) -> Option<String> {
    let texto = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
    let i = texto.find("<key>CFBundleIdentifier</key>")?;
    let resto = &texto[i..];
    let a = resto.find("<string>")? + "<string>".len();
    let b = resto[a..].find("</string>")? + a;
    Some(resto[a..b].trim().to_string())
}

/// Monta (ou atualiza) o desvio. Devolve o que fez.
pub fn instalar() -> Result<String, String> {
    if Path::new("/Applications/Ghostty.app").exists() {
        return Err("o Ghostty de verdade está em /Applications: o desvio não é instalado (os dois disputariam o nome)".into());
    }
    let app = app();
    if let Some(id) = identidade_instalada(&app) {
        if id != IDENTIDADE && id != IDENTIDADE_DO_KIT {
            return Err(format!("{} é de outro app ({id}); não mexo nele", app.display()));
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let destino = app.join("Contents/MacOS/Ghostty");
    let plist = info_plist();
    let marca = marca_da_origem(&exe);
    let arq_marca = app.join("Contents/Resources/origem");
    let plist_igual = std::fs::read_to_string(app.join("Contents/Info.plist")).ok().as_deref() == Some(plist.as_str());
    let em_dia = std::fs::read_to_string(&arq_marca).ok().as_deref() == Some(marca.as_str());
    if plist_igual && em_dia && destino.is_file() && !app.join("Contents/Resources/desvio.sh").exists() {
        return Ok("em dia".into());
    }
    // O do kit (lançador em C + desvio.sh) sai inteiro.
    if identidade_instalada(&app).as_deref() == Some(IDENTIDADE_DO_KIT) {
        let _ = std::fs::remove_dir_all(&app);
    }
    std::fs::create_dir_all(app.join("Contents/MacOS")).map_err(|e| e.to_string())?;
    std::fs::write(app.join("Contents/Info.plist"), plist).map_err(|e| e.to_string())?;
    let tmp = destino.with_extension("novo");
    std::fs::copy(&exe, &tmp).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &destino).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(app.join("Contents/Resources")).map_err(|e| e.to_string())?;
    std::fs::write(&arq_marca, &marca).map_err(|e| e.to_string())?;
    let assinou = Command::new("/usr/bin/codesign").args(["--force", "--sign", "-"]).arg(&app).output();
    if !assinou.as_ref().is_ok_and(|o| o.status.success()) {
        return Err("não consegui assinar o desvio (codesign)".into());
    }
    let _ = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
        .args(["-f"])
        .arg(&app)
        .output();
    Ok(format!("instalado em {}", app.display()))
}

/// Cada item: (ok, descrição).
pub fn conferir() -> Vec<(bool, String)> {
    let app = app();
    let exe = std::env::current_exe().unwrap_or_default();
    let mesmo = std::fs::read_to_string(app.join("Contents/Resources/origem")).ok() == Some(marca_da_origem(&exe))
        && app.join("Contents/MacOS/Ghostty").is_file();
    vec![
        (identidade_instalada(&app).as_deref() == Some(IDENTIDADE), format!("desvio do Keep em {}", app.display())),
        (mesmo, "executável copiado deste keep".into()),
        (
            Command::new("/usr/bin/codesign").args(["--verify"]).arg(&app).output().is_ok_and(|o| o.status.success()),
            "assinatura válida".into(),
        ),
        (!Path::new("/Applications/Ghostty.app").exists(), "sem o Ghostty de verdade em /Applications".into()),
    ]
}

pub fn cli(args: &[String]) -> i32 {
    // `abrir` é igual em todo lugar: é o que o Keep do Windows chama quando o navegador abre um link, e serve para
    // experimentar um link à mão (deeplink.rs).
    if args.first().map(String::as_str) == Some("abrir") {
        return crate::deeplink::cli_abrir(&args[1..]);
    }
    // No Windows o Keep se registra como o programa do esquema (protocolo.rs). O desvio daqui é um app do macOS
    // (Ghostty.app, codesign, lsregister): fora dele nada é montado, para não deixar uma pasta de app e uma cópia do
    // `keep` onde ninguém vai usá-los.
    if cfg!(windows) {
        return crate::protocolo::cli(args);
    }
    if !cfg!(target_os = "macos") {
        eprintln!("keep links: só existe no macOS e no Windows (docs/restauro.md)");
        return 2;
    }
    match args.first().map(String::as_str) {
        Some("instalar") => match instalar() {
            Ok(feito) => {
                println!("{feito}");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        Some("conferir") => {
            let itens = conferir();
            for (ok, d) in &itens {
                println!("{} {d}", if *ok { "ok    " } else { "falta " });
            }
            itens.iter().filter(|(ok, _)| !ok).count() as i32
        }
        _ => {
            eprintln!("uso: keep links instalar | conferir | abrir [--esperar S] LINK");
            2
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_chamada_do_claude_code() {
        let (p, a) = ler_chamada(&v(&["--window-save-state=never", "--working-directory=/p q", "-e", "/x/claude", "--deep-link-origin", "--prefill-b64=eA"]));
        assert_eq!(p.as_deref(), Some("/p q"));
        assert_eq!(a, v(&["--deep-link-origin", "--prefill-b64=eA"]));
        let (p, a) = ler_chamada(&v(&["--window-save-state=never"]));
        assert_eq!((p, a), (None, vec![]));
    }

    #[test]
    fn identidade_do_plist() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("Contents")).unwrap();
        std::fs::write(d.path().join("Contents/Info.plist"), info_plist()).unwrap();
        assert_eq!(identidade_instalada(d.path()).as_deref(), Some(IDENTIDADE));
    }
}
