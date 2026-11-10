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

/// `~/.claude/presas`: a pasta de configuração do Claude de cada pasta presa
/// (`CLAUDE_CONFIG_DIR`, `confinar::config_da_presa`).
pub fn presas() -> PathBuf {
    claude().join("presas")
}

/// As pastas `sessions` do Claude Code: a de `~/.claude` e a da configuração
/// de cada IA presa (`presas/<nome>/sessions`), esta com a pasta a que a IA
/// está presa (`presas/<nome>/.pasta`, que o Keep grava e a IA presa não
/// troca). Uma configuração sem `.pasta` fica de fora.
pub fn pastas_de_sessoes() -> Vec<(PathBuf, Option<PathBuf>)> {
    pastas_de_sessoes_em(&claude().join("sessions"))
}

/// O mesmo, a partir de uma pasta `sessions`: as das IAs presas ficam ao lado
/// dela, em `presas/`.
pub fn pastas_de_sessoes_em(sessoes: &std::path::Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut pastas = vec![(sessoes.to_path_buf(), None)];
    let Some(claude) = sessoes.parent() else { return pastas };
    let mut presas: Vec<PathBuf> =
        std::fs::read_dir(claude.join("presas")).into_iter().flatten().flatten().map(|e| e.path()).collect();
    presas.sort();
    for raiz in presas {
        let Ok(texto) = std::fs::read_to_string(raiz.join(".pasta")) else { continue };
        let pasta = PathBuf::from(texto.trim_end_matches(['\n', '\r']));
        if pasta.is_absolute() {
            pastas.push((raiz.join("sessions"), Some(pasta)));
        }
    }
    pastas
}

/// Se uma sessão com este `cwd` pode vir da pasta de sessões de uma IA presa
/// a `presa`: só de dentro da pasta dela. A IA presa grava na pasta de
/// sessões dela o que quiser, até a sessão de outro processo.
pub fn sessao_da_pasta(cwd: &str, presa: Option<&std::path::Path>) -> bool {
    presa.is_none_or(|p| !cwd.is_empty() && std::path::Path::new(cwd).starts_with(p))
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

/// Grava `dados` por inteiro ou nada, só para o dono: pasta 0700 e temporário 0600 criados assim (não ajustados
/// depois: entre o `write` e o `chmod` o conteúdo ficaria legível a outros usuários), trocados por `rename`.
pub fn grava_privado(caminho: &std::path::Path, dados: &[u8]) -> std::io::Result<()> {
    crate::worktrees::estado::grava_json(caminho, dados)
}

#[cfg(all(test, unix))]
mod testes_privado {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn as_sessoes_das_ias_presas_so_valem_dentro_da_pasta_delas() {
        let d = tempfile::tempdir().unwrap();
        let claude = d.path().join(".claude");
        let presa = claude.join("presas/app-1");
        std::fs::create_dir_all(presa.join("sessions")).unwrap();
        std::fs::create_dir_all(claude.join("presas/sem-pasta/sessions")).unwrap();
        std::fs::write(presa.join(".pasta"), "/projetos/app\n").unwrap();
        assert_eq!(
            pastas_de_sessoes_em(&claude.join("sessions")),
            vec![(claude.join("sessions"), None), (presa.join("sessions"), Some(PathBuf::from("/projetos/app")))]
        );
        assert!(sessao_da_pasta("/qualquer", None));
        assert!(sessao_da_pasta("/projetos/app/sub", Some(std::path::Path::new("/projetos/app"))));
        assert!(!sessao_da_pasta("/projetos/app-velho", Some(std::path::Path::new("/projetos/app"))));
        assert!(!sessao_da_pasta("", Some(std::path::Path::new("/projetos/app"))));
    }

    #[test]
    fn grava_privado_cria_a_arvore_0700_e_o_arquivo_0600() {
        let d = tempfile::tempdir().unwrap();
        let alvo = d.path().join("contas/abas/estado.json");
        grava_privado(&alvo, b"{}").unwrap();
        let modo = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo(&d.path().join("contas")), 0o700);
        assert_eq!(modo(&d.path().join("contas/abas")), 0o700);
        assert_eq!(modo(&alvo), 0o600);
        assert_eq!(std::fs::read(&alvo).unwrap(), b"{}");
        // Regravar troca por inteiro e mantém o modo.
        grava_privado(&alvo, b"{\"a\":1}").unwrap();
        assert_eq!((modo(&alvo), std::fs::read(&alvo).unwrap()), (0o600, b"{\"a\":1}".to_vec()));
    }

    #[test]
    fn so_um_arquivo_comum_e_com_teto_e_lido() {
        let d = tempfile::tempdir().unwrap();
        let bom = d.path().join("ordem");
        std::fs::write(&bom, "claude:ana\nclaude:bia\n").unwrap();
        assert_eq!(ler_texto_ate(&bom, LEITURA_MAXIMA).unwrap(), "claude:ana\nclaude:bia\n");
        assert_eq!(ler_texto_ate(&bom, 10).unwrap(), "claude:ana");
        // Um link para um dispositivo infinito, uma pasta, uma FIFO: recusados sem abrir nem ler.
        let zero = d.path().join("zero");
        std::os::unix::fs::symlink("/dev/zero", &zero).unwrap();
        assert!(ler_texto_ate(&zero, LEITURA_MAXIMA).is_err());
        assert!(ler_texto_ate(d.path(), LEITURA_MAXIMA).is_err());
        let fifo = d.path().join("fifo");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(ler_texto_ate(&fifo, LEITURA_MAXIMA).is_err());
        assert!(ler_texto_ate(&d.path().join("nao-existe"), LEITURA_MAXIMA).is_err());
    }

}

/// O que o Keep aceita ler de um arquivo de texto de uma pasta que outro processo grava (`~/.claude`, a que a IA
/// presa alcança): 1 MiB.
pub const LEITURA_MAXIMA: u64 = 1 << 20;

/// Lê um arquivo de texto só se for um arquivo comum, e no máximo `max` bytes. Sem isto, um link para `/dev/zero`
/// (ou uma FIFO, que prende quem a abre) no lugar de `.ordem` ou `chamadas.jsonl` esgotaria a memória, ou travaria, o
/// `keep ia ...` que o app chama a cada poucos segundos.
pub fn ler_texto_ate(caminho: &std::path::Path, max: u64) -> std::io::Result<String> {
    use std::io::Read;
    // Antes de abrir: abrir uma FIFO espera por quem escreva nela.
    if !std::fs::metadata(caminho)?.is_file() {
        return Err(std::io::Error::other("não é um arquivo comum"));
    }
    let mut bruto = Vec::new();
    std::fs::File::open(caminho)?.take(max).read_to_end(&mut bruto)?;
    String::from_utf8(bruto).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
