//! Onde as coisas ficam. Cada caminho tem uma variável que o troca, para os
//! testes rodarem numa casa falsa sem tocar em conta nenhuma de verdade.

use std::path::PathBuf;

/// A casa do usuário: `KEEP_IA_HOME`, senão a de verdade.
pub fn casa() -> PathBuf {
    if let Some(h) = std::env::var_os("KEEP_IA_HOME") {
        return PathBuf::from(h);
    }
    #[cfg(windows)]
    if let Some(h) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(h);
    }
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// `~/.claude`.
pub fn claude() -> PathBuf {
    casa().join(".claude")
}

/// `~/.claude/contas`: a ordem, as pastas fixas e o cofre do kit.
pub fn contas() -> PathBuf {
    claude().join("contas")
}

/// `~/.claude/contas/fixas`: um armazém de login do Claude por pasta — o
/// dono de cada conta, com a credencial inteira.
pub fn fixas() -> PathBuf {
    contas().join("fixas")
}

/// `~/.claude/contas/abas`: o armazém de cada Claude aberto pelo Keep, só
/// com o token de acesso da conta da aba (`quente.rs`), e o `estado.json`.
pub fn abas() -> PathBuf {
    contas().join("abas")
}

/// `~/.claude/contas/.ordem`.
pub fn ordem() -> PathBuf {
    contas().join(".ordem")
}

/// `~/.codex`: o login principal do GPT.
pub fn codex() -> PathBuf {
    casa().join(".codex")
}

/// `~/.codex-contas`: os logins extras do GPT, um `CODEX_HOME` por pasta.
pub fn codex_contas() -> PathBuf {
    casa().join(".codex-contas")
}

/// Onde o Keep guarda o que é dele (leituras de consumo, donos dos tokens,
/// escolhas): `KEEP_IA_ESTADO`, senão uma pasta do Keep no lugar de cada
/// sistema.
pub fn estado() -> PathBuf {
    if let Some(d) = std::env::var_os("KEEP_IA_ESTADO") {
        return PathBuf::from(d);
    }
    if std::env::var_os("KEEP_IA_HOME").is_some() {
        return casa().join(".keep-ia-estado");
    }
    #[cfg(target_os = "macos")]
    return casa().join("Library/Application Support/Keep/ia");
    #[cfg(windows)]
    return std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("Keep").join("ia"))
        .unwrap_or_else(|| casa().join(".keep-ia"));
    #[cfg(all(unix, not(target_os = "macos")))]
    return std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| casa().join(".local/state"))
        .join("keep/ia");
}

/// O que `canonicalize` devolve no Windows traz o prefixo `\\?\` (`\\?\UNC\` nas pastas de rede): não serve de pasta de
/// uma aba, nem de texto numa linha digitada, nem para a pessoa ler. O mesmo caminho sem ele; o que não é de unidade
/// (`\\?\Volume{…}`) fica como está, e em outros sistemas nada muda.
pub fn sem_prefixo(caminho: PathBuf) -> PathBuf {
    let texto = caminho.to_string_lossy();
    if let Some(rede) = texto.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rede}"));
    }
    match texto.strip_prefix(r"\\?\") {
        Some(resto) if resto.as_bytes().get(1) == Some(&b':') && resto.as_bytes()[0].is_ascii_alphabetic() => {
            PathBuf::from(resto.to_string())
        }
        _ => caminho,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_prefixo_do_windows_sai_so_onde_e_seguro() {
        let p = |s: &str| sem_prefixo(PathBuf::from(s));
        assert_eq!(p(r"\\?\C:\Users\ana\proj"), PathBuf::from(r"C:\Users\ana\proj"));
        assert_eq!(p(r"\\?\d:\x"), PathBuf::from(r"d:\x"));
        assert_eq!(p(r"\\?\UNC\srv\share\proj"), PathBuf::from(r"\\srv\share\proj"));
        // Já sem prefixo, de volume ou de outro sistema: como estão.
        assert_eq!(p(r"C:\Users\ana"), PathBuf::from(r"C:\Users\ana"));
        assert_eq!(p(r"\\?\Volume{1234}\x"), PathBuf::from(r"\\?\Volume{1234}\x"));
        assert_eq!(p("/Users/ana/proj"), PathBuf::from("/Users/ana/proj"));
        assert_eq!(p(r"\\?\"), PathBuf::from(r"\\?\"));
    }
}
