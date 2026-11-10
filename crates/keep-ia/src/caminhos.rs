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

/// A configuração própria de uma IA presa, de onde vem uma pasta de sessões:
/// a pasta dela (o `CLAUDE_CONFIG_DIR` do Claude preso) e a pasta a que ela
/// está presa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presa {
    pub raiz: PathBuf,
    pub pasta: PathBuf,
}

/// As pastas `sessions` do Claude Code: a de `~/.claude` e a da configuração
/// de cada IA presa (`presas/<nome>/sessions`), esta com a pasta a que a IA
/// está presa (`presas/<nome>/.pasta`, que o Keep grava e a IA presa não
/// troca). Uma configuração sem `.pasta` fica de fora.
pub fn pastas_de_sessoes() -> Vec<(PathBuf, Option<Presa>)> {
    pastas_de_sessoes_em(&claude().join("sessions"))
}

/// O mesmo, a partir de uma pasta `sessions`: as das IAs presas ficam ao lado
/// dela, em `presas/`.
pub fn pastas_de_sessoes_em(sessoes: &std::path::Path) -> Vec<(PathBuf, Option<Presa>)> {
    let mut pastas = vec![(sessoes.to_path_buf(), None)];
    let Some(claude) = sessoes.parent() else { return pastas };
    let mut presas: Vec<PathBuf> =
        std::fs::read_dir(claude.join("presas")).into_iter().flatten().flatten().map(|e| e.path()).collect();
    presas.sort();
    for raiz in presas {
        // Uma configuração própria de verdade (não um link), e uma pasta presa que não seja a raiz.
        if !std::fs::symlink_metadata(&raiz).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let Ok(texto) = ler_texto_ate(&raiz.join(".pasta"), 4096) else { continue };
        let pasta = PathBuf::from(texto.trim_end_matches(['\n', '\r']));
        let sessoes = raiz.join("sessions");
        if pasta.is_absolute() && pasta.parent().is_some() && std::fs::symlink_metadata(&sessoes).is_ok_and(|m| m.is_dir()) {
            pastas.push((sessoes, Some(Presa { raiz, pasta })));
        }
    }
    pastas
}

/// Se a sessão do processo `pid`, com este `cwd`, pode vir da pasta de
/// sessões da configuração própria `presa`. A IA presa grava ali o que
/// quiser, até a sessão de outro processo (o `procStart` de qualquer um ela lê
/// com `ps`): só vale a de um processo que sobe com essa configuração (o
/// `CLAUDE_CONFIG_DIR` do ambiente dele, que o Keep põe e nenhuma IA presa
/// troca no de outro), num `cwd` absoluto, sem `..` (que `starts_with` não
/// resolve: `/app/../outro` começa por `/app`), dentro da pasta presa.
pub fn sessao_da_presa(pid: u32, cwd: &str, presa: Option<&Presa>) -> bool {
    use std::path::Component;
    let Some(p) = presa else { return true };
    let c = std::path::Path::new(cwd);
    c.is_absolute()
        && c.components().all(|x| matches!(x, Component::RootDir | Component::Prefix(_) | Component::Normal(_)))
        && c.starts_with(&p.pasta)
        && crate::processos::variavel(pid, "CLAUDE_CONFIG_DIR").is_some_and(|v| std::path::Path::new(&v) == p.raiz)
}

/// Uma sessão gravada em `arquivo`, só se for um arquivo comum e pequeno: a IA
/// presa pode pôr na pasta de sessões dela uma FIFO, que travaria quem a
/// lesse, ou um link para `/dev/zero`.
pub fn le_sessao(arquivo: &std::path::Path) -> Option<String> {
    ler_texto_ate(arquivo, LEITURA_MAXIMA).ok()
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
        let dela = Presa { raiz: presa.clone(), pasta: PathBuf::from("/projetos/app") };
        assert_eq!(
            pastas_de_sessoes_em(&claude.join("sessions")),
            vec![(claude.join("sessions"), None), (presa.join("sessions"), Some(dela.clone()))]
        );
        // A sessão de um processo que sobe com a configuração própria, dentro da pasta presa.
        // O próprio binário de teste (o macOS não entrega o ambiente de um programa do sistema, como o `/bin/sleep`).
        let mut filho = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "processos::testes::dorminhoco", "--ignored", "--test-threads=1", "-q"])
            .env("CLAUDE_CONFIG_DIR", &presa)
            // Sem ela, o dorminhoco sai na hora, e o ambiente dele some antes de ser lido.
            .env("KEEP_IA_TESTE_PROCESSO", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = filho.id();
        // Até o filho trocar de programa, o ambiente que se lê dele é o deste processo.
        let ate = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while crate::processos::variavel(pid, "CLAUDE_CONFIG_DIR").is_none() && std::time::Instant::now() < ate {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(sessao_da_presa(pid, "/qualquer", None));
        assert!(sessao_da_presa(pid, "/projetos/app/sub", Some(&dela)));
        assert!(!sessao_da_presa(pid, "/projetos/app-velho", Some(&dela)));
        assert!(!sessao_da_presa(pid, "", Some(&dela)));
        assert!(!sessao_da_presa(pid, "/projetos/app/../outro", Some(&dela)));
        assert!(!sessao_da_presa(pid, "projetos/app/x", Some(&dela)));
        // A de outro processo, que não sobe com ela (este, sem a variável), não vale, mesmo dentro da pasta.
        assert!(!sessao_da_presa(std::process::id(), "/projetos/app/sub", Some(&dela)));
        let _ = filho.kill();
        let _ = filho.wait();
        // Uma pasta de sessões que é link (para a de outra IA, para a de verdade) fica de fora.
        let outra = claude.join("presas/outra");
        std::fs::create_dir_all(&outra).unwrap();
        std::fs::write(outra.join(".pasta"), "/projetos/outra\n").unwrap();
        std::os::unix::fs::symlink(claude.join("sessions"), outra.join("sessions")).unwrap();
        assert_eq!(pastas_de_sessoes_em(&claude.join("sessions")).len(), 2);
        // Nem uma configuração própria que é link, nem uma presa à raiz.
        std::os::unix::fs::symlink(&presa, claude.join("presas/link")).unwrap();
        let raiz = claude.join("presas/raiz");
        std::fs::create_dir_all(raiz.join("sessions")).unwrap();
        std::fs::write(raiz.join(".pasta"), "/\n").unwrap();
        assert_eq!(pastas_de_sessoes_em(&claude.join("sessions")).len(), 2);
        // Uma FIFO na pasta de sessões não trava quem lê.
        let fifo = presa.join("sessions/1.json");
        let c = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert_eq!(le_sessao(&fifo), None);
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
