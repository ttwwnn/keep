//! Os dois botões de liga e desliga do rodapé: o Jev e o Ponytail.
//!
//! O Jev é a skill que decide, a cada mensagem do Claude e do Codex, o modelo,
//! o esforço e se o pedido leva as regras do Ponytail; os ganchos dele nada
//! fazem enquanto existir `~/.claude/jev/desligado` (o mesmo de
//! `jev auto desligar`). O Ponytail é o plugin `ponytail@ponytail`
//! (github.com/DietrichGebert/ponytail), instalado no Claude e no Codex:
//! desligado, nenhuma das IAs o tem, e o Jev nem pergunta por ele. O que diz
//! se está ligado é a própria IA — `enabledPlugins` no `~/.claude/settings.json`
//! e `[plugins."ponytail@ponytail"] enabled` no `~/.codex/config.toml` —, para
//! o botão nunca discordar do que ela carrega.
//!
//! `keep ia automacao` responde o estado dos dois; `keep ia automacao jev|ponytail
//! ligar|desligar` muda um e responde o estado novo. O que não está instalado
//! vem como `instalado: false`, e o app não mostra o botão dele.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::caminhos;
use crate::cli::{Args, recusa, responde, uso_errado};

pub const PONYTAIL: &str = "ponytail@ponytail";

fn pasta_do_jev() -> PathBuf {
    caminhos::claude().join("jev")
}

fn jev_desligado() -> PathBuf {
    pasta_do_jev().join("desligado")
}

fn jev_instalado() -> bool {
    pasta_do_jev().is_dir() || caminhos::claude().join("skills/jev/ganchos.py").is_file()
}

fn settings_do_claude() -> PathBuf {
    caminhos::claude().join("settings.json")
}

fn config_do_codex() -> PathBuf {
    caminhos::codex().join("config.toml")
}

/// No Claude: `None` sem o plugin, senão se está ligado.
fn ponytail_no_claude() -> Option<bool> {
    let texto = std::fs::read_to_string(settings_do_claude()).ok()?;
    let v: Value = serde_json::from_str(&texto).ok()?;
    v.get("enabledPlugins")?.get(PONYTAIL)?.as_bool()
}

/// A seção do plugin no `config.toml`: as linhas do cabeçalho até antes da
/// próxima seção.
fn secao(linhas: &[&str]) -> Option<(usize, usize)> {
    let cabecalho = format!("[plugins.\"{PONYTAIL}\"]");
    let inicio = linhas.iter().position(|l| l.trim() == cabecalho)?;
    let fim = linhas[inicio + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(linhas.len(), |i| inicio + 1 + i);
    Some((inicio, fim))
}

fn chave_enabled(linha: &str) -> Option<bool> {
    let (chave, valor) = linha.split_once('=')?;
    if chave.trim() != "enabled" {
        return None;
    }
    match valor.split('#').next()?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// No Codex: `None` sem o plugin; a seção sem `enabled` vale ligado, como o
/// Codex a lê.
fn ponytail_no_codex() -> Option<bool> {
    let texto = std::fs::read_to_string(config_do_codex()).ok()?;
    let linhas: Vec<&str> = texto.lines().collect();
    let (inicio, fim) = secao(&linhas)?;
    Some(linhas[inicio + 1..fim].iter().find_map(|l| chave_enabled(l)).unwrap_or(true))
}

/// Liga ou desliga o plugin no `config.toml` do Codex, mexendo só na linha
/// `enabled` da seção dele.
fn grava_no_codex(ligar: bool) -> Result<(), String> {
    let arquivo = config_do_codex();
    let texto = std::fs::read_to_string(&arquivo).map_err(|e| format!("não li {}: {e}", arquivo.display()))?;
    let mut linhas: Vec<String> = texto.lines().map(str::to_string).collect();
    let vistas: Vec<&str> = texto.lines().collect();
    let Some((inicio, fim)) = secao(&vistas) else {
        return Ok(());
    };
    let nova = format!("enabled = {ligar}");
    match (inicio + 1..fim).find(|&i| chave_enabled(&linhas[i]).is_some()) {
        Some(i) => linhas[i] = nova,
        None => linhas.insert(inicio + 1, nova),
    }
    let mut saida = linhas.join("\n");
    if texto.ends_with('\n') {
        saida.push('\n');
    }
    let tmp = arquivo.with_extension("toml.keep-tmp");
    std::fs::write(&tmp, saida).map_err(|e| format!("não gravei {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &arquivo).map_err(|e| format!("não gravei {}: {e}", arquivo.display()))
}

/// O próprio Claude Code muda o `settings.json` (`claude plugin enable|disable`):
/// é ele quem conhece o arquivo, e o resto dele fica como estava.
fn grava_no_claude(ligar: bool) -> Result<(), String> {
    let claude = crate::programas::claude().ok_or("não achei o Claude Code para ligar o plugin")?;
    let acao = if ligar { "enable" } else { "disable" };
    let saida = Command::new(&claude)
        .args(["plugin", acao, PONYTAIL])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("o Claude Code não abriu: {e}"))?;
    if ponytail_no_claude() == Some(ligar) {
        return Ok(());
    }
    let erro = String::from_utf8_lossy(&saida.stderr);
    let texto = String::from_utf8_lossy(&saida.stdout);
    let disse = erro.trim().lines().chain(texto.trim().lines()).next().unwrap_or("sem resposta");
    Err(format!("claude plugin {acao} não mudou o plugin: {disse}"))
}

pub fn estado() -> Value {
    let claude = ponytail_no_claude();
    let codex = ponytail_no_codex();
    json!({
        "jev": { "instalado": jev_instalado(), "ligado": jev_instalado() && !jev_desligado().exists() },
        "ponytail": {
            "instalado": claude.is_some() || codex.is_some(),
            "ligado": claude.or(codex).unwrap_or(false),
            "claude": claude,
            "codex": codex,
        },
    })
}

pub fn ligar_jev(ligar: bool) -> Result<(), String> {
    if !jev_instalado() {
        return Err("o Jev não está instalado nesta máquina".into());
    }
    if ligar {
        match std::fs::remove_file(jev_desligado()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("não religuei o Jev: {e}")),
            _ => Ok(()),
        }
    } else {
        std::fs::create_dir_all(pasta_do_jev()).map_err(|e| format!("não desliguei o Jev: {e}"))?;
        let quando = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        std::fs::write(jev_desligado(), quando + " (Keep)\n").map_err(|e| format!("não desliguei o Jev: {e}"))
    }
}

pub fn ligar_ponytail(ligar: bool) -> Result<(), String> {
    let (claude, codex) = (ponytail_no_claude(), ponytail_no_codex());
    if claude.is_none() && codex.is_none() {
        return Err("o Ponytail não está instalado no Claude nem no Codex".into());
    }
    let mut erros = Vec::new();
    if claude.is_some_and(|c| c != ligar) {
        erros.extend(grava_no_claude(ligar).err());
    }
    if codex.is_some_and(|c| c != ligar) {
        erros.extend(grava_no_codex(ligar).err());
    }
    if erros.is_empty() { Ok(()) } else { Err(erros.join("; ")) }
}

/// `keep ia automacao [jev|ponytail ligar|desligar]`.
pub fn cli(a: &Args) -> i32 {
    let alvo = a.posicoes.get(1).map(String::as_str);
    let acao = a.posicoes.get(2).map(String::as_str);
    let ligar = match acao {
        None => None,
        Some("ligar") => Some(true),
        Some("desligar") => Some(false),
        Some(_) => return uso_errado("keep ia automacao [jev|ponytail ligar|desligar]"),
    };
    let feito = match (alvo, ligar) {
        (None, None) => Ok(()),
        (Some("jev"), Some(l)) => ligar_jev(l),
        (Some("ponytail"), Some(l)) => ligar_ponytail(l),
        _ => return uso_errado("keep ia automacao [jev|ponytail ligar|desligar]"),
    };
    match feito {
        Ok(()) => {
            let mut v = estado();
            v["ok"] = json!(true);
            responde(v)
        }
        Err(e) => responde(recusa("automacao", &e)),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::contas::testes::casa;

    const CONFIG: &str = "model = \"gpt-6-astra\"\n\n[plugins.\"ponytail@ponytail\"]\nenabled = true\n\n[projects.\"/x\"]\ntrust_level = \"trusted\"\n";

    /// Um programa falso: um script do sh, ou um `.cmd` no Windows (o `claude`
    /// do npm também é um `.cmd`).
    fn programa(c: &crate::contas::testes::Casa, sh: &str, cmd: &str) -> std::path::PathBuf {
        let (bin, texto) = if cfg!(windows) {
            (c.raiz.join("claude.cmd"), cmd.replace('\n', "\r\n"))
        } else {
            (c.raiz.join("claude"), sh.to_string())
        };
        std::fs::write(&bin, texto).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // SAFETY: a casa (e a trava) de pé.
        unsafe { std::env::set_var("KEEP_IA_CLAUDE_BIN", &bin) };
        bin
    }

    /// Um `claude` falso que faz o que o de verdade faz com o plugin: muda
    /// `enabledPlugins` e deixa o resto do arquivo.
    fn claude_falso(c: &crate::contas::testes::Casa) {
        programa(
            c,
            "#!/bin/sh\necho \"$@\" >> \"$KEEP_IA_HOME/claude.log\"\nF=\"$KEEP_IA_HOME/.claude/settings.json\"\n\
             [ \"$2\" = enable ] && V=true || V=false\n\
             sed -i.bak \"s/\\\"ponytail@ponytail\\\": *[a-z]*/\\\"ponytail@ponytail\\\": $V/\" \"$F\"\n",
            "@echo off\n>>\"%KEEP_IA_HOME%\\claude.log\" echo %*\n\
             if \"%2\"==\"enable\" (set V=true) else (set V=false)\n\
             >\"%KEEP_IA_HOME%\\.claude\\settings.json\" echo {\"model\": \"opus[1m]\", \"enabledPlugins\": {\"ponytail@ponytail\": %V%}}\n",
        );
    }

    fn com_ponytail(c: &crate::contas::testes::Casa) {
        std::fs::write(
            c.raiz.join(".claude/settings.json"),
            "{\n  \"model\": \"opus[1m]\",\n  \"enabledPlugins\": {\n    \"ponytail@ponytail\": true\n  }\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(c.raiz.join(".codex")).unwrap();
        std::fs::write(c.raiz.join(".codex/config.toml"), CONFIG).unwrap();
        claude_falso(c);
    }

    #[test]
    fn sem_nada_instalado_nao_ha_botao() {
        let _c = casa("automacao-nada");
        let e = estado();
        assert_eq!(e["jev"]["instalado"], false);
        assert_eq!(e["ponytail"]["instalado"], false);
        assert!(ligar_ponytail(true).is_err());
        assert!(ligar_jev(false).is_err());
    }

    #[test]
    fn o_jev_liga_e_desliga_pelo_arquivo_dos_ganchos() {
        let c = casa("automacao-jev");
        std::fs::create_dir_all(c.raiz.join(".claude/jev")).unwrap();
        assert_eq!(estado()["jev"], json!({ "instalado": true, "ligado": true }));
        ligar_jev(false).unwrap();
        assert!(c.raiz.join(".claude/jev/desligado").exists(), "o mesmo arquivo de `jev auto desligar`");
        assert_eq!(estado()["jev"]["ligado"], false);
        ligar_jev(true).unwrap();
        ligar_jev(true).unwrap();
        assert!(!c.raiz.join(".claude/jev/desligado").exists());
        assert_eq!(estado()["jev"]["ligado"], true);
    }

    #[test]
    fn o_ponytail_desliga_nas_duas_ias_e_o_resto_fica() {
        let c = casa("automacao-ponytail");
        com_ponytail(&c);
        assert_eq!(estado()["ponytail"], json!({ "instalado": true, "ligado": true, "claude": true, "codex": true }));
        ligar_ponytail(false).unwrap();
        assert_eq!(estado()["ponytail"], json!({ "instalado": true, "ligado": false, "claude": false, "codex": false }));
        let toml = std::fs::read_to_string(c.raiz.join(".codex/config.toml")).unwrap();
        assert_eq!(toml, CONFIG.replace("enabled = true", "enabled = false"), "só a linha do plugin muda");
        let log = std::fs::read_to_string(c.raiz.join("claude.log")).unwrap();
        assert_eq!(log.trim_end(), "plugin disable ponytail@ponytail");
        ligar_ponytail(false).unwrap();
        assert_eq!(std::fs::read_to_string(c.raiz.join("claude.log")).unwrap(), log, "já desligado: nada roda");
        ligar_ponytail(true).unwrap();
        assert_eq!(estado()["ponytail"]["ligado"], true);
        assert_eq!(std::fs::read_to_string(c.raiz.join(".codex/config.toml")).unwrap(), CONFIG);
    }

    #[test]
    fn so_no_codex_e_secao_sem_enabled() {
        let c = casa("automacao-codex");
        std::fs::create_dir_all(c.raiz.join(".codex")).unwrap();
        std::fs::write(c.raiz.join(".codex/config.toml"), "[plugins.\"ponytail@ponytail\"]\n[outra]\nx = 1\n").unwrap();
        assert_eq!(estado()["ponytail"]["ligado"], true, "sem `enabled` o Codex o carrega");
        assert_eq!(estado()["ponytail"]["claude"], Value::Null);
        ligar_ponytail(false).unwrap();
        assert_eq!(
            std::fs::read_to_string(c.raiz.join(".codex/config.toml")).unwrap(),
            "[plugins.\"ponytail@ponytail\"]\nenabled = false\n[outra]\nx = 1\n"
        );
    }

    #[test]
    fn claude_que_nao_muda_o_arquivo_e_erro() {
        let c = casa("automacao-claude-falha");
        com_ponytail(&c);
        programa(&c, "#!/bin/sh\necho falhou >&2\nexit 1\n", "@echo off\necho falhou 1>&2\nexit /b 1\n");
        let e = ligar_ponytail(false).unwrap_err();
        assert!(e.contains("falhou"), "{e}");
        assert_eq!(estado()["ponytail"]["claude"], true);
        assert_eq!(estado()["ponytail"]["codex"], false, "o Codex desliga mesmo assim");
    }
}
