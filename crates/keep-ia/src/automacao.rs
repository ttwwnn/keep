//! Os três botões de liga e desliga do rodapé: o Jev, o Ponytail e o PR-Agent.
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
//! O PR-Agent é a skill `pr-agent` que o próprio Keep grava no Claude e no Codex
//! (`skills/pr-agent` de cada um, a partir de `skills/pr-agent` deste crate):
//! ela revisa um PR pelo PR-Agent (github.com/ttwwnn/pr-agent-claude, o
//! `pr-agent` do `uv tool install`), que chama o modelo por `claude -p`, na
//! assinatura de quem roda. Ligado = a skill está lá; desligado, nenhuma das
//! IAs a tem. Uma skill `pr-agent` da pessoa, sem a marca do Keep, não é tocada.
//!
//! `keep ia automacao` responde o estado dos três; `keep ia automacao
//! jev|ponytail|pr-agent ligar|desligar` muda um e responde o estado novo. O que
//! não está instalado vem como `instalado: false`, e o app não mostra o botão
//! dele.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::caminhos;
use crate::cli::{Args, recusa, responde, uso_errado};

pub const PONYTAIL: &str = "ponytail@ponytail";
pub const PR_AGENT: &str = "pr-agent";

/// Os arquivos da skill do PR-Agent, na ordem em que são gravados: o script
/// antes do `SKILL.md`, que é o que faz a IA ver a skill.
const SKILL: [(&str, &str); 2] = [
    ("pr-agent.sh", include_str!("../skills/pr-agent/pr-agent.sh")),
    ("SKILL.md", include_str!("../skills/pr-agent/SKILL.md")),
];
/// O que diz, no `SKILL.md`, que a skill é a do Keep.
const MARCA: &str = "keep ia automacao pr-agent";
const INSTALAR: &str = "uv tool install --python 3.12 'pr-agent @ git+https://github.com/ttwwnn/pr-agent-claude@main'";

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

/// Um arquivo da skill como vai para o disco: sempre com `\n`, mesmo vindo de
/// um checkout do Windows (o `sh` não lê `\r\n`).
fn conteudo(texto: &str) -> String {
    texto.replace("\r\n", "\n")
}

fn pasta_da_skill(ia: &Path) -> PathBuf {
    ia.join("skills").join(PR_AGENT)
}

/// As IAs que recebem a skill: as contas extras do GPT ligam a `skills` do
/// `~/.codex` (`login.rs`), e a do Claude é uma só para todas as abas.
fn ias() -> [(&'static str, PathBuf); 2] {
    [("Claude", caminhos::claude()), ("Codex", caminhos::codex())]
}

#[derive(Debug, PartialEq)]
enum Skill {
    Nenhuma,
    /// A do Keep; `atual` quando é a deste `keep`, arquivo por arquivo.
    DoKeep { atual: bool },
    /// Uma `pr-agent` sem a marca do Keep: é da pessoa.
    DaPessoa,
}

/// A skill numa IA; `None` quando a IA nunca rodou aqui (sem a pasta dela).
fn skill_em(ia: &Path) -> Option<Skill> {
    if !ia.is_dir() {
        return None;
    }
    let pasta = pasta_da_skill(ia);
    let Ok(md) = std::fs::read_to_string(pasta.join("SKILL.md")) else {
        return Some(Skill::Nenhuma);
    };
    if !md.contains(MARCA) {
        return Some(Skill::DaPessoa);
    }
    let atual = SKILL
        .iter()
        .all(|(nome, texto)| std::fs::read_to_string(pasta.join(nome)).is_ok_and(|t| t == conteudo(texto)));
    Some(Skill::DoKeep { atual })
}

/// Grava a skill (cada arquivo inteiro, por um temporário), o script executável.
fn grava_skill(ia: &Path) -> Result<(), String> {
    let pasta = pasta_da_skill(ia);
    std::fs::create_dir_all(&pasta).map_err(|e| format!("não criei {}: {e}", pasta.display()))?;
    for (nome, texto) in SKILL {
        let arquivo = pasta.join(nome);
        let tmp = pasta.join(format!(".{nome}.keep-tmp"));
        std::fs::write(&tmp, conteudo(texto)).map_err(|e| format!("não gravei {}: {e}", tmp.display()))?;
        #[cfg(unix)]
        if nome.ends_with(".sh") {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("não gravei {}: {e}", tmp.display()))?;
        }
        std::fs::rename(&tmp, &arquivo).map_err(|e| format!("não gravei {}: {e}", arquivo.display()))?;
    }
    Ok(())
}

/// Apaga a skill do Keep, o `SKILL.md` primeiro; a pasta só sai vazia.
fn apaga_skill(ia: &Path) -> Result<(), String> {
    let pasta = pasta_da_skill(ia);
    for (nome, _) in SKILL.iter().rev() {
        let arquivo = pasta.join(nome);
        match std::fs::remove_file(&arquivo) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("não apaguei {}: {e}", arquivo.display()));
            }
            _ => {}
        }
    }
    let _ = std::fs::remove_dir(&pasta);
    Ok(())
}

/// A skill ligada acompanha o Keep: a cópia que outra versão dele gravou é
/// regravada com a deste. Só a do Keep e só quando difere, e por isso ler de
/// novo não grava nada.
fn renova_pr_agent() {
    for (_, ia) in ias() {
        if skill_em(&ia) == Some(Skill::DoKeep { atual: false }) {
            let _ = grava_skill(&ia);
        }
    }
}

/// Instalado = há o PR-Agent para a skill rodar, ou a skill está em alguma IA.
/// Ligado = a do Keep em alguma delas: o botão desliga o que estiver lá.
fn pr_agent_estado() -> Value {
    renova_pr_agent();
    let lado = |ia: &Path| skill_em(ia).map(|s| matches!(s, Skill::DoKeep { .. }));
    let (claude, codex) = (lado(&caminhos::claude()), lado(&caminhos::codex()));
    let ligado = claude == Some(true) || codex == Some(true);
    json!({
        "instalado": crate::programas::pr_agent().is_some() || ligado,
        "ligado": ligado,
        "claude": claude,
        "codex": codex,
    })
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
        "pr-agent": pr_agent_estado(),
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

/// Liga: grava a skill no Claude e no Codex, os que já rodaram aqui; tudo ou
/// nada, e por isso a skill da pessoa numa delas recusa o ligar sem gravar em
/// nenhuma. Desliga: tira a do Keep de cada uma; a da pessoa fica.
pub fn ligar_pr_agent(ligar: bool) -> Result<(), String> {
    if ligar && crate::programas::pr_agent().is_none() {
        return Err(format!("o PR-Agent não está instalado nesta máquina: {INSTALAR}"));
    }
    let ias: Vec<_> = ias().into_iter().filter_map(|(nome, ia)| skill_em(&ia).map(|s| (nome, ia, s))).collect();
    if ligar {
        if ias.is_empty() {
            return Err("nem o Claude Code nem o Codex rodaram nesta máquina".into());
        }
        let delas: Vec<String> = ias
            .iter()
            .filter(|(_, _, s)| *s == Skill::DaPessoa)
            .map(|(nome, ia, _)| {
                format!("o {nome} já tem uma skill pr-agent que não é a do Keep ({})", pasta_da_skill(ia).display())
            })
            .collect();
        if !delas.is_empty() {
            return Err(delas.join("; "));
        }
    }
    let mut erros = Vec::new();
    for (_, ia, skill) in ias {
        let feito = match (skill, ligar) {
            (_, true) => grava_skill(&ia),
            (Skill::DoKeep { .. }, false) => apaga_skill(&ia),
            (_, false) => Ok(()),
        };
        erros.extend(feito.err());
    }
    if erros.is_empty() { Ok(()) } else { Err(erros.join("; ")) }
}

const USO: &str = "keep ia automacao [jev|ponytail|pr-agent ligar|desligar]";

/// `keep ia automacao [jev|ponytail|pr-agent ligar|desligar]`.
pub fn cli(a: &Args) -> i32 {
    let alvo = a.posicoes.get(1).map(String::as_str);
    let acao = a.posicoes.get(2).map(String::as_str);
    let ligar = match acao {
        None => None,
        Some("ligar") => Some(true),
        Some("desligar") => Some(false),
        Some(_) => return uso_errado(USO),
    };
    let feito = match (alvo, ligar) {
        (None, None) => Ok(()),
        (Some("jev"), Some(l)) => ligar_jev(l),
        (Some("ponytail"), Some(l)) => ligar_ponytail(l),
        (Some(PR_AGENT), Some(l)) => ligar_pr_agent(l),
        _ => return uso_errado(USO),
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
        assert_eq!(e["pr-agent"]["instalado"], false);
        assert!(ligar_ponytail(true).is_err());
        assert!(ligar_jev(false).is_err());
        let e = ligar_pr_agent(true).unwrap_err();
        assert!(e.contains("uv tool install"), "diz como instalar: {e}");
    }

    /// Um `pr-agent` falso, só para existir.
    fn pr_agent_falso(c: &crate::contas::testes::Casa) -> PathBuf {
        let bin = c.raiz.join(if cfg!(windows) { "pr-agent.exe" } else { "pr-agent" });
        std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // SAFETY: a casa (e a trava) de pé.
        unsafe { std::env::set_var("KEEP_IA_PR_AGENT_BIN", &bin) };
        bin
    }

    fn arquivos_da_skill(ia: &Path) -> Vec<(String, String)> {
        let pasta = pasta_da_skill(ia);
        let mut v: Vec<_> = std::fs::read_dir(&pasta)
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v.into_iter().map(|n| (n.clone(), std::fs::read_to_string(pasta.join(&n)).unwrap_or_default())).collect()
    }

    #[test]
    fn o_pr_agent_grava_e_apaga_a_skill_nas_duas_ias() {
        let c = casa("automacao-pr-agent");
        std::fs::create_dir_all(c.raiz.join(".codex")).unwrap();
        pr_agent_falso(&c);
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": false, "claude": false, "codex": false }));
        ligar_pr_agent(true).unwrap();
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": true, "claude": true, "codex": true }));
        for ia in [c.raiz.join(".claude"), c.raiz.join(".codex")] {
            let esperado: Vec<(String, String)> = {
                let mut v: Vec<_> = SKILL.iter().map(|(n, t)| (n.to_string(), conteudo(t))).collect();
                v.sort();
                v
            };
            assert_eq!(arquivos_da_skill(&ia), esperado, "só a skill, sem temporário: {}", ia.display());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let modo = std::fs::metadata(pasta_da_skill(&ia).join("pr-agent.sh")).unwrap().permissions().mode();
                assert_eq!(modo & 0o777, 0o755);
            }
        }
        let md = std::fs::read_to_string(pasta_da_skill(&c.raiz.join(".claude")).join("SKILL.md")).unwrap();
        assert!(md.starts_with("---\nname: pr-agent\ndescription: "), "o cabeçalho que o Claude e o Codex leem");
        ligar_pr_agent(true).unwrap();
        ligar_pr_agent(false).unwrap();
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": false, "claude": false, "codex": false }));
        assert!(!pasta_da_skill(&c.raiz.join(".claude")).exists(), "a pasta da skill sai junto");
        assert!(!pasta_da_skill(&c.raiz.join(".codex")).exists());
        assert!(c.raiz.join(".claude/skills").is_dir(), "a pasta das skills fica");
        ligar_pr_agent(false).unwrap();
    }

    #[test]
    fn a_skill_pr_agent_da_pessoa_nao_e_tocada() {
        let c = casa("automacao-pr-agent-pessoa");
        std::fs::create_dir_all(c.raiz.join(".codex")).unwrap();
        pr_agent_falso(&c);
        let dela = pasta_da_skill(&c.raiz.join(".claude"));
        std::fs::create_dir_all(&dela).unwrap();
        std::fs::write(dela.join("SKILL.md"), "---\nname: pr-agent\ndescription: a minha\n---\n").unwrap();
        assert_eq!(estado()["pr-agent"]["ligado"], false, "não é a do Keep");
        let e = ligar_pr_agent(true).unwrap_err();
        assert!(e.contains("o Claude já tem uma skill pr-agent que não é a do Keep"), "{e}");
        assert!(!pasta_da_skill(&c.raiz.join(".codex")).exists(), "tudo ou nada: nem o Codex a recebe");
        ligar_pr_agent(false).unwrap();
        renova_pr_agent();
        assert_eq!(std::fs::read_to_string(dela.join("SKILL.md")).unwrap(), "---\nname: pr-agent\ndescription: a minha\n---\n");
        assert_eq!(arquivos_da_skill(&c.raiz.join(".claude")).len(), 1, "nada gravado ao lado dela");
    }

    #[test]
    fn a_skill_de_outro_keep_e_regravada_ao_ler_e_so_ela() {
        let c = casa("automacao-pr-agent-renova");
        pr_agent_falso(&c);
        ligar_pr_agent(true).unwrap();
        let pasta = pasta_da_skill(&c.raiz.join(".claude"));
        std::fs::write(pasta.join("pr-agent.sh"), "#!/bin/sh\necho velho\n").unwrap();
        assert_eq!(skill_em(&c.raiz.join(".claude")), Some(Skill::DoKeep { atual: false }));
        assert_eq!(estado()["pr-agent"]["ligado"], true);
        assert_eq!(std::fs::read_to_string(pasta.join("pr-agent.sh")).unwrap(), conteudo(SKILL[0].1));
        let antes = std::fs::metadata(pasta.join("SKILL.md")).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        estado();
        assert_eq!(std::fs::metadata(pasta.join("SKILL.md")).unwrap().modified().unwrap(), antes, "a atual não é regravada");
    }

    #[test]
    fn a_do_keep_so_no_codex_e_ligado_e_o_botao_a_desliga() {
        let c = casa("automacao-pr-agent-so-codex");
        std::fs::create_dir_all(c.raiz.join(".codex")).unwrap();
        pr_agent_falso(&c);
        let claude = c.raiz.join(".claude");
        std::fs::rename(&claude, c.raiz.join("claude-fora")).unwrap();
        ligar_pr_agent(true).unwrap();
        // O Claude roda aqui depois de ligado: a pasta dele nasce sem a skill.
        std::fs::rename(c.raiz.join("claude-fora"), &claude).unwrap();
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": true, "claude": false, "codex": true }));
        ligar_pr_agent(false).unwrap();
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": false, "claude": false, "codex": false }));
    }

    #[test]
    fn sem_o_pr_agent_a_skill_que_ficou_ainda_desliga() {
        let c = casa("automacao-pr-agent-sem");
        pr_agent_falso(&c);
        ligar_pr_agent(true).unwrap();
        // SAFETY: a casa (e a trava) de pé.
        unsafe { std::env::set_var("KEEP_IA_PR_AGENT_BIN", c.raiz.join("sumiu")) };
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": true, "claude": true, "codex": null }));
        assert!(ligar_pr_agent(true).is_err(), "religar pede o PR-Agent");
        ligar_pr_agent(false).unwrap();
        assert_eq!(estado()["pr-agent"]["instalado"], false, "sem ele e sem a skill, o botão some");
    }

    #[test]
    fn sem_claude_nem_codex_o_pr_agent_nao_liga() {
        let c = casa("automacao-pr-agent-sem-ia");
        std::fs::remove_dir_all(c.raiz.join(".claude")).unwrap();
        pr_agent_falso(&c);
        assert_eq!(estado()["pr-agent"], json!({ "instalado": true, "ligado": false, "claude": null, "codex": null }));
        let e = ligar_pr_agent(true).unwrap_err();
        assert!(e.contains("nem o Claude Code nem o Codex"), "{e}");
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

/// O `pr-agent.sh` da skill, rodado pelo `sh` com o PR-Agent, o `gh` e a
/// configuração do git de mentira; o git é o de verdade, num repositório do
/// teste. Nada aqui mexe no ambiente do processo, e cada teste tem a sua pasta.
#[cfg(all(test, unix))]
mod script {
    use super::{SKILL, conteudo};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    /// Diz o que recebeu (pasta, ambiente, argumentos) e guarda o diff; grava
    /// o Markdown e, na revisão, um JSON com a cobertura, como o de verdade.
    const PR_AGENT_FALSO: &str = r#"#!/bin/sh
{
  echo "pwd=$(pwd -P)"
  echo "model=$CONFIG__MODEL"
  echo "fallback=$CONFIG__FALLBACK_MODELS"
  echo "lang=$CONFIG__RESPONSE_LANGUAGE"
  echo "tokens=$CONFIG__MAX_MODEL_TOKENS"
  echo "effort=$CLAUDE_CODE__EFFORT"
  echo "erros=$CONFIG__PROPAGATE_TOOL_ERRORS"
  for a in "$@"; do echo "arg=$a"; done
} > "$FALSO_LOG"
out=""; js=""; df=""
while [ $# -gt 0 ]; do
  case "$1" in
    --output) out=$2; shift ;;
    --json-output) js=$2; shift ;;
    --diff-file) df=$2; shift ;;
  esac
  shift
done
cp "$df" "$FALSO_LOG.diff"
echo "registro do PR-Agent"
if [ -n "${FALSO_FALHA:-}" ]; then echo "deu errado aqui"; exit 1; fi
printf '## Revisão falsa\n' > "$out"
if [ -n "$js" ]; then
  printf '{\n  "review": {},\n  "coverage": {\n    "files_not_reviewed": [\n      "b.txt"\n    ],\n    "files_partially_reviewed": []\n  }\n}\n' > "$js"
fi
"#;

    struct Lugar {
        raiz: PathBuf,
        repo: PathBuf,
        bin: PathBuf,
        script: PathBuf,
        log: PathBuf,
    }

    fn executavel(p: &Path, texto: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(p, texto).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Os falsos primeiro; depois o git de quem roda e o básico do sistema.
    fn path(bin: &Path) -> String {
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .find(|d| d.join("git").is_file())
            .map_or_else(|| "/usr/bin".to_string(), |d| d.display().to_string());
        format!("{}:{git}:/usr/bin:/bin", bin.display())
    }

    fn comando(l: &Lugar, programa: &str) -> Command {
        let mut c = Command::new(programa);
        c.env_clear()
            .env("PATH", path(&l.bin))
            .env("HOME", &l.raiz)
            .env("TMPDIR", l.raiz.join("tmp"))
            .env("GIT_CONFIG_GLOBAL", l.raiz.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .env("FALSO_LOG", &l.log)
            .current_dir(&l.repo);
        c
    }

    fn git(l: &Lugar, args: &[&str]) -> String {
        let s = comando(l, "git").args(args).output().unwrap();
        assert!(s.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&s.stderr));
        String::from_utf8_lossy(&s.stdout).trim().to_string()
    }

    /// Um repositório com `main` e o ramo `ramo` um commit à frente.
    fn lugar(nome: &str) -> Lugar {
        let raiz = std::env::temp_dir().join(format!("keep-ia-pr-agent-sh-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        for d in ["repo", "bin", "tmp"] {
            std::fs::create_dir_all(raiz.join(d)).unwrap();
        }
        let raiz = raiz.canonicalize().unwrap();
        // A configuração de quem roda pode mudar o formato do diff: a do teste muda de propósito.
        std::fs::write(raiz.join("gitconfig"), "[diff]\n\tnoprefix = true\n[color]\n\tui = always\n").unwrap();
        let l = Lugar {
            repo: raiz.join("repo"),
            bin: raiz.join("bin"),
            script: raiz.join("pr-agent.sh"),
            log: raiz.join("pr-agent.log"),
            raiz,
        };
        std::fs::write(&l.script, conteudo(SKILL[0].1)).unwrap();
        executavel(&l.bin.join("pr-agent"), PR_AGENT_FALSO);
        git(&l, &["init", "-q", "-b", "main"]);
        std::fs::write(l.repo.join("a.txt"), "um\n").unwrap();
        git(&l, &["add", "a.txt"]);
        git(&l, &["commit", "-qm", "base"]);
        git(&l, &["checkout", "-qb", "ramo"]);
        std::fs::write(l.repo.join("a.txt"), "um\ndois commitado\n").unwrap();
        git(&l, &["commit", "-qam", "muda"]);
        l
    }

    fn roda(l: &Lugar, args: &[&str]) -> Output {
        comando(l, "sh").arg(&l.script).args(args).output().unwrap()
    }

    fn texto(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    fn registro(l: &Lugar) -> String {
        std::fs::read_to_string(&l.log).unwrap_or_default()
    }

    fn argumentos(l: &Lugar) -> Vec<String> {
        registro(l).lines().filter_map(|x| x.strip_prefix("arg=")).map(str::to_string).collect()
    }

    fn diff(l: &Lugar) -> String {
        std::fs::read_to_string(format!("{}.diff", l.log.display())).unwrap()
    }

    #[test]
    fn revisa_o_ramo_contra_a_base_com_o_que_nao_foi_commitado() {
        let l = lugar("ramo");
        std::fs::write(l.repo.join("a.txt"), "um\ndois commitado\ntres na pasta\n").unwrap();
        std::fs::write(l.repo.join("solto.txt"), "fora do índice\n").unwrap();
        let s = roda(&l, &["revisar"]);
        let saida = texto(&s.stdout);
        assert!(s.status.success(), "{saida}\n{}", texto(&s.stderr));
        assert!(saida.starts_with("## Revisão falsa\n"), "o Markdown, sem o registro do PR-Agent: {saida}");
        assert!(!saida.contains("registro do PR-Agent"));
        assert!(saida.contains("\"files_not_reviewed\": [\n      \"b.txt\"\n    ]"), "a cobertura: {saida}");
        let r = registro(&l);
        assert!(r.contains(&format!("pwd={}\n", l.repo.display())), "na árvore, para o contexto dos arquivos: {r}");
        for linha in ["model=claude-code/claude-opus-5-5", "fallback=[]", "lang=pt-BR", "tokens=100000", "effort=medium", "erros=true"] {
            assert!(r.contains(&format!("{linha}\n")), "{linha}: {r}");
        }
        let a = argumentos(&l);
        assert_eq!(a.len(), 7, "{a:?}");
        assert_eq!([&a[0], &a[2], &a[4], &a[6]], ["--diff-file", "--output", "--json-output", "review"]);
        let d = diff(&l);
        assert!(d.contains("--- a/a.txt\n+++ b/a.txt\n"), "a/ e b/, sem cor, seja qual for a configuração do git: {d}");
        assert!(d.contains("+dois commitado\n+tres na pasta\n"), "{d}");
        assert!(!d.contains("solto.txt"), "arquivo fora do índice não entra");
    }

    #[test]
    fn modelo_esforco_e_os_outros_comandos() {
        let l = lugar("comandos");
        let s = roda(&l, &["perguntar", "o que muda?", "--modelo", "claude-fable-5-1", "--esforco", "high"]);
        assert!(s.status.success(), "{}", texto(&s.stderr));
        let r = registro(&l);
        assert!(r.contains("model=claude-code/claude-fable-5-1\n") && r.contains("effort=high\n"), "{r}");
        let a = argumentos(&l);
        assert_eq!(a[a.len() - 2..], ["ask", "o que muda?"]);
        assert!(!a.iter().any(|x| x == "--json-output"), "o JSON é só da revisão: {a:?}");
        for (comando, do_pr_agent) in [("descrever", "describe"), ("melhorar", "improve")] {
            assert!(roda(&l, &[comando]).status.success());
            assert_eq!(argumentos(&l).last().map(String::as_str), Some(do_pr_agent));
        }
        assert!(roda(&l, &["revisar", "--base", "ramo"]).status.success(), "--base ramo: nada a revisar");
    }

    #[test]
    fn sem_diferenca_o_pr_agent_nem_roda() {
        let l = lugar("nada");
        git(&l, &["checkout", "-q", "main"]);
        let s = roda(&l, &["revisar"]);
        assert!(s.status.success());
        assert!(texto(&s.stdout).starts_with("Nada a revisar"), "{}", texto(&s.stdout));
        assert!(!l.log.exists());
    }

    #[test]
    fn uso_errado_falha_e_falta_do_pr_agent() {
        let l = lugar("erros");
        for args in [&[][..], &["revisar", "--pr"], &["perguntar"], &["revisar", "--outra", "x"], &["publicar"]] {
            assert_eq!(roda(&l, args).status.code(), Some(2), "{args:?}");
        }
        let s = comando(&l, "sh").arg(&l.script).arg("revisar").env("FALSO_FALHA", "1").output().unwrap();
        assert_eq!(s.status.code(), Some(1));
        let erro = texto(&s.stderr);
        assert!(erro.contains("O PR-Agent falhou (saída 1)") && erro.contains("deu errado aqui"), "{erro}");
        assert!(texto(&s.stdout).is_empty(), "nada de resultado pela metade");
        std::fs::remove_file(l.bin.join("pr-agent")).unwrap();
        let s = roda(&l, &["revisar"]);
        assert_eq!(s.status.code(), Some(1));
        assert!(texto(&s.stderr).contains("uv tool install"), "{}", texto(&s.stderr));
    }

    #[test]
    fn o_pr_pelo_gh_com_a_arvore_so_quando_ela_e_a_cabeca_dele() {
        let l = lugar("pr");
        let cabeca = git(&l, &["rev-parse", "HEAD"]);
        executavel(
            &l.bin.join("gh"),
            &format!(
                "#!/bin/sh\ncase \"$1 $2\" in\n  'pr diff') printf -- '--- a/a.txt\\n+++ b/a.txt\\n@@ -1 +1,2 @@\\n um\\n+do pr\\n' ;;\n  'pr view') echo {cabeca} ;;\nesac\n"
            ),
        );
        let na_arvore = format!("pwd={}\n", l.repo.display());
        let na_pasta_vazia = format!("pwd={}/tmp/pr-agent/", l.raiz.display());
        let s = roda(&l, &["revisar", "--pr", "7"]);
        assert!(s.status.success(), "{}", texto(&s.stderr));
        assert!(registro(&l).contains(&na_arvore), "na cabeça do PR e limpa: {}", registro(&l));
        assert!(diff(&l).contains("+do pr\n"));
        std::fs::write(l.repo.join("a.txt"), "outra coisa\n").unwrap();
        assert!(roda(&l, &["revisar", "--pr", "7"]).status.success());
        assert!(registro(&l).contains(&na_pasta_vazia), "a pasta mudou: só o patch. {}", registro(&l));
        git(&l, &["checkout", "-q", "--", "a.txt"]);
        git(&l, &["commit", "-q", "--allow-empty", "-m", "outro"]);
        assert!(roda(&l, &["revisar", "--pr", "7"]).status.success());
        assert!(registro(&l).contains(&na_pasta_vazia), "outra cabeça: só o patch. {}", registro(&l));
    }
}
