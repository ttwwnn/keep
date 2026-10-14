//! Um link `claude-cli://open?q=…&cwd=…&repo=…` aberto numa aba do Keep (o lado do Windows dos links; o do Mac é o
//! `Ghostty.app` de `links.rs`).
//!
//! No Mac o Claude Code lê o link, acha a pasta e chama o "terminal" com `-e claude --deep-link-origin …`; o Keep só
//! recebe a chamada pronta. No Windows o Claude Code escolhe o terminal sozinho (o `wt.exe` do PATH), e o desvio é
//! outro: o Keep fica registrado como o programa do esquema (`protocolo.rs`) e recebe o LINK CRU. Como o link vem de
//! fora (qualquer página pode abri-lo), tudo aqui é conferido antes de virar alguma coisa, com as mesmas regras que o
//! Claude Code aplica ao link (lidas no executável 2.1.296) ou mais estritas:
//!
//! - só o esquema `claude-cli`, só a ação `open` (a `install-plugin` é recusada: instalar plugin não é papel do Keep);
//! - só os parâmetros `q`, `cwd` e `repo`; os outros são ignorados e nunca repassados;
//! - `q` (o texto a pré-preencher): até 5000 caracteres, sem controle além de quebra de linha e tabulação;
//! - `cwd`: pasta absoluta de unidade (`C:\…`), sem rede (UNC, `\\?\`), sem `..`, sem controle nem caractere
//!   invisível, sem os caracteres que o Windows não aceita num nome, até 4096 caracteres, e que exista e seja pasta;
//! - `repo`: `dono/nome`, só letras, números, `.`, `_` e `-`.
//!
//! O texto NUNCA vira linha de comando: vai ao `claude` em `--prefill-b64=` (base64 de URL), que o Claude Code só
//! PRÉ-PREENCHE na caixa de entrada, sem enviar. É o que o Claude Code faz nos terminais do Mac e do Windows; nenhum
//! shell enxerga o texto. A pasta vai direto ao keepd como pasta da aba. Por isso não há pergunta de confirmação: o
//! efeito de um link é uma aba com uma caixa de texto preenchida, que a pessoa lê e manda ou apaga. Link sem `cwd` (ou
//! com uma pasta que não existe) abre na pasta pessoal, como o `Ghostty.app` do Mac faz.

use std::path::{Path, PathBuf};

use crate::nova::{self, Conteudo, Pedido};

/// O que o Windows aceita numa linha de comando (32767 caracteres) é também o teto do link inteiro.
pub const LIMITE_DO_LINK: usize = 32_768;
/// O teto do `q`, o mesmo do Claude Code.
pub const LIMITE_DO_PEDIDO: usize = 5000;
/// O teto da pasta, o mesmo do Claude Code.
pub const LIMITE_DA_PASTA: usize = 4096;
/// O teto de `dono/nome` (o GitHub aceita 39 + 100).
const LIMITE_DO_REPO: usize = 200;

/// O que um link pede, já conferido no formato (a pasta ainda não foi vista no disco).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// O texto a pré-preencher, sem espaços nas pontas; `None` se não veio ou veio vazio.
    pub pedido: Option<String>,
    pub pasta: Option<String>,
    pub repo: Option<String>,
}

fn controle(c: char) -> bool {
    let n = c as u32;
    n <= 31 || (127..=159).contains(&n)
}

/// Caracteres que não aparecem ou que mudam a ordem da leitura (um nome de pasta que parece outro). Os mesmos que o
/// Claude Code recusa, menos os joiners e seletores de variação que as escritas de várias línguas pedem.
fn invisivel(c: char) -> bool {
    matches!(
        c as u32,
        0xAD | 0x61C | 0x180E | 0x200B | 0x200E | 0x200F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2060..=0x206F
            | 0x2800 | 0x3164 | 0xFEFF | 0xFFA0 | 0xFFF9..=0xFFFB | 0x1D173..=0x1D17A | 0xE0000..=0xE007F
    )
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

/// Decodifica um componente de consulta como o `URLSearchParams` do navegador: `+` é espaço, `%XX` é um byte, um `%`
/// que não é seguido de dois dígitos fica como está. Bytes que não formam UTF-8 recusam o link (o navegador poria
/// U+FFFD; aqui o texto seria outro).
fn decodifica(componente: &str) -> Result<String, String> {
    let b = componente.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => saida.push(b' '),
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                saida.push(hex(b[i + 1]).unwrap_or(0) * 16 + hex(b[i + 2]).unwrap_or(0));
                i += 2;
            }
            outro => saida.push(outro),
        }
        i += 1;
    }
    String::from_utf8(saida).map_err(|_| "o link tem texto que não é UTF-8".to_string())
}

/// Confere o texto do `q` e o devolve sem espaços nas pontas; vazio vira `None`.
fn confere_pedido(bruto: &str) -> Result<Option<String>, String> {
    let texto = bruto.replace("\r\n", "\n").replace('\r', "\n");
    let texto = texto.trim();
    if texto.is_empty() {
        return Ok(None);
    }
    if texto.chars().any(|c| controle(c) && c != '\n' && c != '\t') {
        return Err("o texto do link tem caracteres de controle".into());
    }
    let n = texto.chars().count();
    if n > LIMITE_DO_PEDIDO {
        return Err(format!("o texto do link passa de {LIMITE_DO_PEDIDO} caracteres ({n})"));
    }
    Ok(Some(texto.to_string()))
}

/// Confere o formato de uma pasta de link. `windows`: as regras do Windows (unidade `C:\`); fora dele, caminho
/// absoluto `/…` (para os testes e para o `keep links abrir` num Mac).
pub fn confere_pasta(pasta: &str, windows: bool) -> Result<(), String> {
    let recusa = |porque: &str| Err(format!("pasta do link recusada ({porque}): {pasta:?}"));
    if pasta.chars().count() > LIMITE_DA_PASTA {
        return recusa("grande demais");
    }
    if pasta.chars().any(controle) {
        return recusa("tem caracteres de controle");
    }
    if pasta.chars().any(invisivel) {
        return recusa("tem caracteres invisíveis");
    }
    let separador = |c: char| c == '/' || c == '\\';
    let mut letras = pasta.chars();
    let (a, b) = (letras.next(), letras.next());
    if a.is_some_and(separador) && b.is_some_and(separador) {
        return recusa("caminho de rede ou de dispositivo");
    }
    if pasta.split(separador).any(|parte| parte == "..") {
        return recusa("tem ..");
    }
    if windows {
        let bytes = pasta.as_bytes();
        let unidade = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
        if !unidade {
            return recusa("não começa por uma unidade, como C:\\");
        }
        // `:` só depois da letra da unidade (fora disso é fluxo alternativo de arquivo); os outros, o Windows não aceita.
        if pasta[2..].chars().any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*' | ':')) {
            return recusa("tem caractere que o Windows não aceita num nome");
        }
    } else if !pasta.starts_with('/') {
        return recusa("não é um caminho absoluto");
    }
    Ok(())
}

fn confere_repo(repo: &str) -> Result<(), String> {
    let parte = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) && !p.chars().all(|c| c == '.');
    match repo.split_once('/') {
        Some((dono, nome)) if repo.len() <= LIMITE_DO_REPO && parte(dono) && parte(nome) => Ok(()),
        _ => Err(format!("repo do link recusado (esperado dono/nome): {repo:?}")),
    }
}

/// Lê e confere um link. Os erros são frases para a pessoa e para o log.
pub fn analisar(url: &str) -> Result<Link, String> {
    if url.len() > LIMITE_DO_LINK {
        return Err(format!("o link passa de {LIMITE_DO_LINK} bytes"));
    }
    // Um navegador entrega o endereço já limpo; controle (inclusive quebra de linha) aqui é tentativa de enganar um
    // programa que depois monte uma linha.
    if url.chars().any(controle) || url.chars().any(char::is_whitespace) {
        return Err("o link tem espaço ou caractere de controle".into());
    }
    let (esquema, resto) = url.split_once(':').ok_or("não é um link claude-cli://")?;
    if !esquema.eq_ignore_ascii_case("claude-cli") {
        return Err(format!("esquema {esquema:?}: só claude-cli:// é aceito"));
    }
    // O Claude Code aceita `claude-cli:open` como `claude-cli://open`.
    let resto = resto.strip_prefix("//").unwrap_or(resto);
    let resto = resto.split('#').next().unwrap_or("");
    let (antes, consulta) = match resto.split_once('?') {
        Some((a, c)) => (a, c),
        None => (resto, ""),
    };
    let (autoridade, caminho) = match antes.find('/') {
        Some(i) => (&antes[..i], &antes[i..]),
        None => (antes, ""),
    };
    if !autoridade.eq_ignore_ascii_case("open") {
        return Err(format!("ação {autoridade:?}: só a open é aceita"));
    }
    if !(caminho.is_empty() || caminho == "/") {
        return Err("o link tem caminho depois de open".into());
    }
    let (mut pedido, mut pasta, mut repo) = (None, None, None);
    for par in consulta.split('&') {
        let (nome, valor) = par.split_once('=').unwrap_or((par, ""));
        // O primeiro de cada nome vale (`searchParams.get`). Nomes que o Keep não conhece não são lidos.
        match decodifica(nome)?.as_str() {
            "q" if pedido.is_none() => pedido = Some(decodifica(valor)?),
            "cwd" if pasta.is_none() => pasta = Some(decodifica(valor)?),
            "repo" if repo.is_none() => repo = Some(decodifica(valor)?),
            _ => {}
        }
    }
    let pasta = pasta.filter(|p| !p.is_empty());
    if let Some(p) = &pasta {
        confere_pasta(p, cfg!(windows))?;
    }
    let repo = repo.filter(|r| !r.is_empty());
    if let Some(r) = &repo {
        confere_repo(r)?;
    }
    let pedido = match pedido {
        Some(q) => confere_pedido(&q)?,
        None => None,
    };
    Ok(Link { pedido, pasta, repo })
}

/// Base64 de URL, sem preenchimento (`Buffer.toString("base64url")`): o alfabeto é só `A-Za-z0-9_-`, que nenhum shell
/// interpreta.
pub fn base64_url(dados: &[u8]) -> String {
    const TABELA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut saida = String::with_capacity(dados.len().div_ceil(3) * 4);
    for grupo in dados.chunks(3) {
        let n = (u32::from(grupo[0]) << 16) | (u32::from(*grupo.get(1).unwrap_or(&0)) << 8) | u32::from(*grupo.get(2).unwrap_or(&0));
        for i in 0..=grupo.len() {
            saida.push(TABELA[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    saida
}

/// Os argumentos do `claude` para o link: os mesmos que o Claude Code monta para o terminal (`--deep-link-origin`, que
/// faz o Claude mostrar de onde veio a sessão, `--deep-link-repo=`, `--deep-link-cwd-b64=`, `--prefill-b64=`). Todos
/// são seguros para uma linha de comando: o que não é do alfabeto de base64 de URL passou antes pelo `confere_repo`.
/// `pasta`: a pasta em que a aba abriu, se foi a do link (senão o Claude não é avisado de uma pasta que não é a dele).
pub fn argumentos_do_claude(link: &Link, pasta: Option<&Path>) -> Vec<String> {
    let mut args = vec!["--deep-link-origin".to_string()];
    if let Some(repo) = &link.repo {
        args.push(format!("--deep-link-repo={repo}"));
    }
    if let Some(p) = pasta {
        args.push(format!("--deep-link-cwd-b64={}", base64_url(p.to_string_lossy().as_bytes())));
    }
    if let Some(q) = &link.pedido {
        args.push(format!("--prefill-b64={}", base64_url(q.as_bytes())));
    }
    args
}

/// A pasta do link no disco: existe, é pasta, e não passa por um caminho de rede (uma unidade mapeada, uma junção).
/// `None`: não há pasta no link, ou ela não existe.
pub fn resolve_pasta(link: &Link) -> Result<Option<PathBuf>, String> {
    let Some(pasta) = &link.pasta else { return Ok(None) };
    let Ok(real) = Path::new(pasta).canonicalize() else { return Ok(None) };
    let real = keep_ia::caminhos::sem_prefixo(real);
    if cfg!(windows) && real.to_string_lossy().starts_with(r"\\") {
        return Err(format!("a pasta do link passa por um caminho de rede: {}", real.display()));
    }
    Ok(real.is_dir().then_some(real))
}

/// O pedido para o `keep nova`: o `claude` com os argumentos do link, numa aba nova do workspace da aba usada por último.
pub fn pedido_do_link(link: &Link) -> Result<Pedido, String> {
    let pasta = resolve_pasta(link)?;
    let args = argumentos_do_claude(link, pasta.as_deref());
    let pasta = pasta.unwrap_or_else(keep_ia::caminhos::casa);
    Ok(Pedido { workspace: None, pasta, conteudo: Conteudo::Argumentos(args), nome: None, sem_enter: false })
}

fn registra(texto: &str) {
    let arq = keep_ia::caminhos::estado().join("links.log");
    if let Some(p) = arq.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    // Um log de links é pequeno; passando de 256 KiB, o começo vai embora.
    if std::fs::metadata(&arq).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::rename(&arq, arq.with_extension("log.antigo"));
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(arq) {
        let _ = writeln!(f, "{} {texto}", crate::agora_ms());
    }
}

/// Abre o link numa aba. Com o Keep ainda subindo (o link que abriu o app), tenta de novo a cada segundo por até
/// `esperar_s` segundos. Devolve o workspace e a aba.
pub fn abrir(url: &str, esperar_s: u64) -> Result<(String, u32), String> {
    let link = match analisar(url) {
        Ok(l) => l,
        Err(e) => {
            registra(&format!("recusado: {e}"));
            return Err(e);
        }
    };
    let pedido = pedido_do_link(&link).inspect_err(|e| registra(&format!("recusado: {e}")))?;
    // Sem o texto do pedido no log: pode ser longo e é da pessoa.
    registra(&format!(
        "link: pasta={} repo={:?} pedido={}",
        pedido.pasta.display(),
        link.repo,
        link.pedido.as_ref().map_or("nenhum".to_string(), |q| format!("{} caracteres", q.chars().count()))
    ));
    let fim = std::time::Instant::now() + std::time::Duration::from_secs(esperar_s);
    loop {
        match nova::abrir(&pedido) {
            Ok((ws, aba, _)) => {
                registra(&format!("aberto em {ws} aba {aba}"));
                return Ok((ws, aba));
            }
            Err(e) if std::time::Instant::now() >= fim => {
                registra(&format!("falhou: {e}"));
                return Err(e);
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(1)),
        }
    }
}

/// `keep links abrir [--esperar SEGUNDOS] LINK`
pub fn cli_abrir(args: &[String]) -> i32 {
    let mut esperar = 0;
    let mut link = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--esperar" => {
                esperar = args.get(i + 1).and_then(|s| s.parse::<u64>().ok()).map(|s| s.min(300)).unwrap_or(0);
                i += 1;
            }
            outro if link.is_none() => link = Some(outro.to_string()),
            _ => {
                eprintln!("uso: keep links abrir [--esperar SEGUNDOS] claude-cli://open?…");
                return 2;
            }
        }
        i += 1;
    }
    let Some(link) = link else {
        eprintln!("uso: keep links abrir [--esperar SEGUNDOS] claude-cli://open?…");
        return 2;
    };
    match abrir(&link, esperar) {
        Ok((ws, aba)) => {
            println!("{ws} aba {aba}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ok(url: &str) -> Link {
        analisar(url).unwrap_or_else(|e| panic!("{url}: {e}"))
    }

    fn recusa(url: &str) -> String {
        analisar(url).expect_err(url)
    }

    #[test]
    fn o_link_de_sempre() {
        let (pasta, codificada) = if cfg!(windows) { (r"C:\Users\ana", "C%3A%5CUsers%5Cana") } else { ("/tmp", "%2Ftmp") };
        let l = ok(&format!("claude-cli://open?q=oi%20mundo&cwd={codificada}&repo=ttwwnn%2Fkeep"));
        assert_eq!(l, Link { pedido: Some("oi mundo".into()), pasta: Some(pasta.into()), repo: Some("ttwwnn/keep".into()) });
    }

    #[test]
    fn so_o_que_o_link_traz() {
        assert_eq!(ok("claude-cli://open"), Link::default());
        assert_eq!(ok("claude-cli://open?"), Link::default());
        assert_eq!(ok("claude-cli://open/"), Link::default());
        assert_eq!(ok("claude-cli://open?q="), Link::default());
        assert_eq!(ok("claude-cli://open?q=%20%20").pedido, None, "só espaço é pedido vazio");
        assert_eq!(ok("claude-cli://open?q=x").pedido.as_deref(), Some("x"));
    }

    #[test]
    fn o_esquema_e_a_acao_nao_dependem_de_maiusculas_e_a_barra_dupla_e_opcional() {
        assert_eq!(ok("CLAUDE-CLI://OPEN?q=a").pedido.as_deref(), Some("a"));
        assert_eq!(ok("claude-cli:open?q=a").pedido.as_deref(), Some("a"));
        assert_eq!(ok("claude-cli://open?q=a#fragmento").pedido.as_deref(), Some("a"));
    }

    #[test]
    fn mais_e_espaco_e_o_dois_mais_e_mais() {
        assert_eq!(ok("claude-cli://open?q=a+b%2Bc").pedido.as_deref(), Some("a b+c"));
    }

    #[test]
    fn utf8_e_quebra_de_linha_passam() {
        assert_eq!(ok("claude-cli://open?q=a%C3%A7%C3%A3o%0D%0Aoi%09x").pedido.as_deref(), Some("ação\noi\tx"));
        // O navegador pode entregar o texto sem codificar.
        assert_eq!(ok("claude-cli://open?q=ação").pedido.as_deref(), Some("ação"));
    }

    #[test]
    fn o_primeiro_de_cada_nome_vale_e_o_resto_e_ignorado() {
        let l = ok("claude-cli://open?q=um&q=dois&cwd=&x=1&--flag=2&q2=3");
        assert_eq!(l, Link { pedido: Some("um".into()), pasta: None, repo: None });
    }

    #[test]
    fn outro_esquema_ou_outra_acao_e_recusado() {
        for url in [
            "http://open?q=x",
            "https://example.com/claude-cli://open",
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "claude://open?q=x",
            "claude-cli2://open",
            "claude-cli",
            "",
            "open?q=x",
            "claude-cli://install-plugin?plugin=x",
            "claude-cli://opens?q=x",
            "claude-cli://xopen?q=x",
            "claude-cli://user@open?q=x",
            "claude-cli://open:80?q=x",
            "claude-cli://open/outra?q=x",
            "claude-cli:///open?q=x",
            "claude-cli://?q=x",
        ] {
            recusa(url);
        }
    }

    #[test]
    fn caracteres_de_controle_e_espaco_no_link_sao_recusados() {
        for url in [
            "claude-cli://open?q=a\nb",
            "claude-cli://open?q=a\rb",
            "claude-cli://open?q=a\0b",
            "claude-cli://open?q=a b",
            "claude-cli://open?q=a\tb",
            "claude-cli://open?q=a\u{85}b",
            " claude-cli://open",
            "claude-cli://open ",
        ] {
            recusa(url);
        }
    }

    #[test]
    fn controle_no_texto_decodificado_e_recusado_menos_quebra_e_tabulacao() {
        for q in ["a%00b", "a%1Bb", "a%7Fb", "a%C2%85b", "a%08b"] {
            recusa(&format!("claude-cli://open?q={q}"));
        }
        ok("claude-cli://open?q=a%0Ab%09c");
    }

    #[test]
    fn utf8_quebrado_e_recusado() {
        recusa("claude-cli://open?q=%FF%FE");
        recusa("claude-cli://open?q=%C3");
    }

    #[test]
    fn por_cento_solto_fica_como_esta() {
        assert_eq!(ok("claude-cli://open?q=100%").pedido.as_deref(), Some("100%"));
        assert_eq!(ok("claude-cli://open?q=%zz%4").pedido.as_deref(), Some("%zz%4"));
    }

    #[test]
    fn o_pedido_tem_teto() {
        let ok5000 = "a".repeat(LIMITE_DO_PEDIDO);
        assert!(analisar(&format!("claude-cli://open?q={ok5000}")).is_ok());
        let demais = "a".repeat(LIMITE_DO_PEDIDO + 1);
        assert!(recusa(&format!("claude-cli://open?q={demais}")).contains("5000"));
        // O teto é em caracteres, não em bytes.
        let acentos = "ç".repeat(LIMITE_DO_PEDIDO);
        assert!(analisar(&format!("claude-cli://open?q={acentos}")).is_ok());
    }

    #[test]
    fn o_link_tem_teto() {
        let enorme = format!("claude-cli://open?q={}", "a".repeat(LIMITE_DO_LINK));
        assert!(recusa(&enorme).contains("bytes"));
        let milhoes = format!("claude-cli://open?{}", "x=1&".repeat(20_000));
        assert!(analisar(&milhoes).is_err());
    }

    #[test]
    fn aspas_barras_pipes_e_e_comercial_no_pedido_so_viram_texto() {
        // Nada disso chega a uma linha de comando: o pedido segue em base64.
        let l = ok("claude-cli://open?q=%22%20%7C%20calc%20%26%20%24%28id%29%20%60x%60%20%25PATH%25%20%3B%20rm%20-rf%20%2F");
        assert_eq!(l.pedido.as_deref(), Some("\" | calc & $(id) `x` %PATH% ; rm -rf /"));
        let args = argumentos_do_claude(&l, None);
        for a in &args {
            assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || "-_=".contains(c)), "{a}");
        }
    }

    #[test]
    fn pasta_do_windows() {
        let boa = |p: &str| confere_pasta(p, true).unwrap_or_else(|e| panic!("{p}: {e}"));
        let ruim = |p: &str| {
            confere_pasta(p, true).expect_err(p);
        };
        boa(r"C:\Users\ana\proj");
        boa("c:/Users/ana/proj");
        boa(r"D:\a b\ç");
        boa(r"C:\");
        boa(r"C:\a..b\c");
        for p in [
            r"\\servidor\share\x",
            r"\\?\C:\x",
            r"\\.\COM1",
            "//servidor/share",
            "//?/C:/x",
            r"/Users/ana",
            r"C:",
            r"C:x",
            r"relativa\x",
            r".\x",
            r"C:\a\..\b",
            r"C:\a\..",
            r"C:/../x",
            r"..\x",
            "C:\\a\u{0}b",
            "C:\\a\nb",
            "C:\\a\u{202E}b",
            "C:\\a\u{200B}b",
            r"C:\a<b",
            r"C:\a>b",
            r#"C:\a"b"#,
            r"C:\a|b",
            r"C:\a?b",
            r"C:\a*b",
            r"C:\a:fluxo",
            r"1:\x",
            "",
        ] {
            ruim(p);
        }
        ruim(&format!(r"C:\{}", "a".repeat(LIMITE_DA_PASTA)));
        boa(&format!(r"C:\{}", "a".repeat(LIMITE_DA_PASTA - 3)));
    }

    #[test]
    fn pasta_fora_do_windows() {
        confere_pasta("/Users/ana/proj", false).unwrap();
        for p in ["relativa", "//rede/x", "/a/../b", "/a\u{0}b", "C:\\x"] {
            confere_pasta(p, false).expect_err(p);
        }
    }

    #[test]
    fn pasta_do_link_pelo_codigo_recusa_o_mesmo() {
        if cfg!(windows) {
            recusa("claude-cli://open?cwd=%5C%5Cservidor%5Cshare");
            recusa("claude-cli://open?cwd=C%3A%5Ca%5C..%5Cb");
        } else {
            recusa("claude-cli://open?cwd=%2Fa%2F..%2Fb");
            recusa("claude-cli://open?cwd=%2F%2Frede");
            recusa("claude-cli://open?cwd=relativa");
        }
        recusa("claude-cli://open?cwd=%00");
    }

    #[test]
    fn repo_so_dono_barra_nome() {
        for r in ["ttwwnn/keep", "a/b", "A_b.c-d/e.f-g_h", "a.b/c"] {
            ok(&format!("claude-cli://open?repo={}", r.replace('/', "%2F")));
        }
        for r in [
            "keep", "a/b/c", "/b", "a/", "a//b", "../..", "./.", "a/..", "a b/c", "a/b;c", "a/b&c", "a/$(x)", "a/b\"c", "a:b/c",
            "https://x/y", "-r/x/", "a/\u{ff}",
        ] {
            let enc: String = r.bytes().map(|b| format!("%{b:02X}")).collect();
            recusa(&format!("claude-cli://open?repo={enc}"));
        }
        recusa(&format!("claude-cli://open?repo={}%2F{}", "a".repeat(150), "b".repeat(150)));
    }

    #[test]
    fn base64_de_url_confere_com_os_vetores_do_rfc_4648() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(b"foob"), "Zm9vYg");
        assert_eq!(base64_url(b"fooba"), "Zm9vYmE");
        assert_eq!(base64_url(b"foobar"), "Zm9vYmFy");
        // O alfabeto de URL: `-` e `_` no lugar de `+` e `/`.
        assert_eq!(base64_url(&[0xfb, 0xff, 0xfe]), "-__-");
        assert_eq!(base64_url("ação".as_bytes()), "YcOnw6Nv");
    }

    #[test]
    fn os_argumentos_do_claude_sao_os_do_terminal_do_claude_code() {
        let l = Link { pedido: Some("oi".into()), pasta: Some("/p".into()), repo: Some("a/b".into()) };
        assert_eq!(
            argumentos_do_claude(&l, Some(Path::new("/p"))),
            ["--deep-link-origin", "--deep-link-repo=a/b", "--deep-link-cwd-b64=L3A", "--prefill-b64=b2k"]
        );
        // Sem a pasta no disco, o Claude não é avisado de uma pasta que não é a dele.
        assert_eq!(argumentos_do_claude(&l, None), ["--deep-link-origin", "--deep-link-repo=a/b", "--prefill-b64=b2k"]);
        assert_eq!(argumentos_do_claude(&Link::default(), None), ["--deep-link-origin"]);
    }

    #[test]
    fn a_pasta_do_link_tem_de_existir_e_ser_pasta() {
        let d = tempfile::tempdir().unwrap();
        let pasta = d.path().canonicalize().unwrap();
        let arquivo = pasta.join("a.txt");
        std::fs::write(&arquivo, "x").unwrap();
        let com = |p: &Path| Link { pasta: Some(p.to_string_lossy().into_owned()), ..Link::default() };
        assert_eq!(resolve_pasta(&com(&pasta)).unwrap().as_deref(), Some(pasta.as_path()));
        assert_eq!(resolve_pasta(&com(&pasta.join("nao-existe"))).unwrap(), None);
        assert_eq!(resolve_pasta(&com(&arquivo)).unwrap(), None, "arquivo não é pasta");
        assert_eq!(resolve_pasta(&Link::default()).unwrap(), None);
    }

    #[test]
    fn sem_pasta_a_aba_abre_na_pasta_pessoal_e_nao_avisa_o_claude() {
        let l = Link { pedido: Some("oi".into()), ..Link::default() };
        let p = pedido_do_link(&l).unwrap();
        assert_eq!(p.pasta, keep_ia::caminhos::casa());
        assert_eq!(p.workspace, None);
        assert!(!p.sem_enter);
        assert_eq!(p.conteudo, Conteudo::Argumentos(vec!["--deep-link-origin".into(), "--prefill-b64=b2k".into()]));
    }
}
