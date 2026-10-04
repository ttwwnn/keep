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

/// `~/.claude/contas/fixas`: um armazém de login do Claude por pasta.
pub fn fixas() -> PathBuf {
    contas().join("fixas")
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
