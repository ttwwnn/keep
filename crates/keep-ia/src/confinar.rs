//! A pasta a que a IA de um workspace fica presa.
//!
//! O app grava em `~/.config/keep/confinement.json` ({"workspace": "/pasta"})
//! a pasta que a IA do workspace pode acessar. Quem sobe a IA numa aba (a
//! função `claude()` do shell, e aqui a troca de conta ou de IA) passa por
//! `~/.local/bin/keep-confine`, que decide pelo workspace da aba
//! (`KEEP_WORKSPACE`) e roda o programa num sandbox do macOS. Este módulo só
//! põe o helper na frente da linha quando o workspace tem pasta: sem
//! restrição, a linha continua exatamente a que era.

use crate::caminhos;
use std::path::PathBuf;

/// Onde o app guarda as pastas. `KEEP_CONFINE_CONFIG` troca (testes).
pub fn arquivo() -> PathBuf {
    match std::env::var_os("KEEP_CONFINE_CONFIG") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => caminhos::casa().join(".config/keep/confinement.json"),
    }
}

/// A pasta permitida à IA do workspace, se houver.
pub fn pasta(ws: &str) -> Option<String> {
    let texto = std::fs::read_to_string(arquivo()).ok()?;
    let json: serde_json::Value = serde_json::from_str(&texto).ok()?;
    json.get(ws)?.as_str().filter(|p| !p.is_empty()).map(str::to_string)
}

/// O helper que prende o programa, se estiver instalado.
fn helper() -> Option<PathBuf> {
    let p = std::env::var_os("KEEP_CONFINE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| caminhos::casa().join(".local/bin/keep-confine"));
    p.is_file().then_some(p)
}

/// O programa e os argumentos da linha, com o helper na frente quando o
/// workspace tem pasta permitida; do contrário, os mesmos. Só no Unix: o
/// sandbox é o do macOS.
pub fn envolve(ws: &str, programa: String, args: Vec<String>) -> (String, Vec<String>) {
    if !cfg!(unix) || pasta(ws).is_none() {
        return (programa, args);
    }
    match helper() {
        Some(h) => {
            let mut novos = vec![programa];
            novos.extend(args);
            (h.to_string_lossy().into_owned(), novos)
        }
        None => (programa, args),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_prende_o_workspace_que_tem_pasta() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("confinement.json");
        std::fs::write(&cfg, r#"{"presa": "/pasta/x", "vazia": ""}"#).unwrap();
        let helper = dir.path().join("keep-confine");
        std::fs::write(&helper, "#!/bin/sh\n").unwrap();
        // Variáveis de processo: um teste só mexe nelas.
        unsafe {
            std::env::set_var("KEEP_CONFINE_CONFIG", &cfg);
            std::env::set_var("KEEP_CONFINE_BIN", &helper);
        }
        assert_eq!(pasta("presa").as_deref(), Some("/pasta/x"));
        assert_eq!(pasta("vazia"), None);
        assert_eq!(pasta("livre"), None);
        let (p, a) = envolve("presa", "/bin/claude".into(), vec!["--effort".into(), "max".into()]);
        assert_eq!(p, helper.to_string_lossy());
        assert_eq!(a, vec!["/bin/claude", "--effort", "max"]);
        let (p, a) = envolve("livre", "/bin/claude".into(), vec!["--effort".into()]);
        assert_eq!((p.as_str(), a), ("/bin/claude", vec!["--effort".to_string()]));
        // Sem o helper instalado, a linha é a de sempre.
        std::fs::remove_file(&helper).unwrap();
        let (p, _) = envolve("presa", "/bin/claude".into(), vec![]);
        assert_eq!(p, "/bin/claude");
    }
}
