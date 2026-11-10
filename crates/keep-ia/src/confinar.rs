//! A pasta a que a IA de um workspace fica presa.
//!
//! O app grava em `~/.config/keep/confinement.json` ({"workspace": "/pasta"})
//! a pasta que a IA do workspace pode acessar. Quem sobe a IA numa aba (`keep ia
//! rodar`, a função do PowerShell, a troca de conta ou de IA e a restauração)
//! passa por `keep ia confinar -- <programa> …`, que decide pelo workspace da
//! aba (`KEEP_WORKSPACE`) e roda o programa num sandbox do macOS
//! (`sandbox-exec`): ele e tudo o que ele roda não leem nem gravam em /Users
//! e /Volumes, salvo na pasta escolhida e no mínimo de que a IA precisa.
//! Desse mínimo, `~/.claude` (que o Claude preso lê e grava ao abrir) vem
//! aparado: o que toda sessão executa ou lê como instrução fica só para ler,
//! o cofre e os logins das contas ficam fechados (menos o armazém da aba) e,
//! dos transcritos, só os da pasta presa; e os programas instalados fora de
//! /Users (o Keep.app, o Homebrew) ficam só para ler (docs/ia.md, "O que a IA
//! presa alcança").
//! No Windows o sandbox é um token restrito com permissões só na pasta e no
//! mínimo de que a IA precisa (`confinar_win.rs`): o perfil do usuário fica
//! de fora, para leitura e gravação. Sem a pasta existir, ou sem o sandbox, o
//! programa NÃO sobe: a restrição não se desliga sozinha. Sem restrição para
//! o workspace, a linha continua exatamente a que era.
//!
//! O que manda trabalho para um servidor de fundo, fora do sandbox, é
//! recusado num workspace restrito: `claude agents` e `claude attach` (o
//! daemon do Claude Code), `codex agents` e `codex queue` (o do Codex). O
//! Codex em si sobe com `--no-daemon`, para o servidor dele rodar dentro do
//! próprio processo — e do sandbox.

use crate::caminhos;
use std::path::{Path, PathBuf};

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

/// O `keep` que prende o programa: este mesmo executável (`KEEP_CONFINE_BIN`
/// troca, nos testes).
fn helper() -> Option<PathBuf> {
    match std::env::var_os("KEEP_CONFINE_BIN") {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)).filter(|p| p.is_file()),
        _ => std::env::current_exe().ok(),
    }
}

/// O programa e os argumentos da linha, com `keep ia confinar --` na frente
/// quando o workspace tem pasta permitida; do contrário, os mesmos. Só no
/// macOS e no Windows: o sandbox é o de cada um.
pub fn envolve(ws: &str, programa: String, args: Vec<String>) -> (String, Vec<String>) {
    if !cfg!(any(target_os = "macos", windows)) || pasta(ws).is_none() {
        return (programa, args);
    }
    match helper() {
        Some(h) => {
            let mut novos = vec!["ia".to_string(), "confinar".into(), "--".into(), programa];
            novos.extend(args);
            (h.to_string_lossy().into_owned(), novos)
        }
        None => (programa, args),
    }
}

/// Um caminho como literal do perfil do sandbox.
fn literal(p: &str) -> String {
    format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Uma expressão regular como literal do perfil (`#"…"`). Ali a barra invertida chega crua à expressão, sem o escape
/// de texto de `literal` (com ele, `\.` virava `\\.`, uma barra e mais um caractere, e a regra não casava nada). Aspas
/// não têm como entrar: `None`, e a regra fica de fora (o que ela liberaria continua negado).
fn literal_regex(re: &str) -> Option<String> {
    (!re.contains('"')).then(|| format!("#\"{re}\""))
}

fn escapa_regex(p: &str) -> String {
    let mut s = String::new();
    for c in p.chars() {
        if ".^$|?*+()[]{}\\".contains(c) {
            s.push('\\');
        }
        s.push(c);
    }
    s
}

/// As pastas do git que uma worktree usa e que ficam fora dela.
///
/// O `.git` da worktree é um arquivo de texto DENTRO da pasta presa: a própria IA pode reescrevê-lo, e o que ele
/// apontar ganharia leitura e gravação na subida seguinte (com `gitdir: /`, o disco inteiro). Por isso só vale a
/// forma de uma worktree de verdade, `<repo>/.git/worktrees/<nome>`, que aponta de volta para ESTE `.git` (o arquivo
/// `gitdir` lá dentro, que a IA presa não alcança) e cujo `commondir` é esse `<repo>/.git`. Fora disso, nada: o git
/// da pasta falha, o sandbox não alarga. Submódulo (`.git/modules/...`) também fica de fora.
pub(crate) fn pastas_do_git(pasta: &Path) -> Vec<String> {
    let nada = Vec::new();
    let Ok(texto) = std::fs::read_to_string(pasta.join(".git")) else { return nada };
    let Some(gitdir) = texto.trim().strip_prefix("gitdir:") else { return nada };
    let Ok(gitdir) = pasta.join(gitdir.trim()).canonicalize() else { return nada };
    let Some(worktrees) = gitdir.parent() else { return nada };
    let Some(comum) = worktrees.parent() else { return nada };
    if worktrees.file_name() != Some(std::ffi::OsStr::new("worktrees"))
        || comum.file_name() != Some(std::ffi::OsStr::new(".git"))
    {
        return nada;
    }
    let volta = std::fs::read_to_string(gitdir.join("gitdir")).ok().and_then(|g| Path::new(g.trim()).canonicalize().ok());
    if volta.is_none() || volta != pasta.join(".git").canonicalize().ok() {
        return nada;
    }
    if let Ok(c) = std::fs::read_to_string(gitdir.join("commondir")) {
        if gitdir.join(c.trim()).canonicalize().ok().as_deref() != Some(comum) {
            return nada;
        }
    }
    vec![gitdir.to_string_lossy().into_owned(), comum.to_string_lossy().into_owned()]
}

/// Onde o keepd escuta, nas formas que o sandbox pode comparar: como está e com a pasta canonizada.
fn socket_do_daemon(socket: &Path) -> Vec<String> {
    let mut formas = vec![socket.to_string_lossy().into_owned()];
    if let (Some(pasta), Some(nome)) = (socket.parent(), socket.file_name()) {
        if let Ok(real) = pasta.canonicalize() {
            formas.push(real.join(nome).to_string_lossy().into_owned());
        }
    }
    formas.dedup();
    formas
}

/// O que, em `~/.claude`, a IA presa lê e não grava: o que toda sessão do Claude Code, presa ou não, executa ou toma
/// como instrução ao abrir. Gravado por ela, viraria comando numa aba sem sandbox (um gancho no `settings.json`, um
/// plugin, uma skill, um agente) ou instrução de todas as conversas (o `CLAUDE.md`, as regras, os estilos de saída).
pub(crate) const CLAUDE_SO_LEITURA: [&str; 10] = [
    "settings.json",
    "settings.local.json",
    "CLAUDE.md",
    "skills",
    "plugins",
    "agents",
    "commands",
    "hooks",
    "rules",
    "output-styles",
];

/// Um caminho nas formas que o sandbox compara: como está e, quando é outro, o destino real (um link, `/var` que é
/// `/private/var`). Só para caminhos que a IA presa não altera (os que este perfil nega): seguir um link que ela
/// mesma criou, onde grava, deixaria a subida seguinte liberar o que ela quisesse.
fn formas(p: &Path) -> Vec<String> {
    // O destino real do maior trecho que existe, com o resto do caminho: `/tmp/x` que ainda não existe é
    // `/private/tmp/x`, e é por ele que o sandbox compara quem o cria.
    let real = |p: &Path| {
        p.ancestors().find_map(|a| {
            let resto = p.strip_prefix(a).ok()?;
            let r = a.canonicalize().ok()?;
            Some(if resto.as_os_str().is_empty() { r } else { r.join(resto) })
        })
    };
    let mut caminhos = vec![p.to_path_buf()];
    caminhos.extend(real(p));
    // Um link quebrado: gravar por ele cria o destino, que é o caminho que o sandbox compara.
    if p.canonicalize().is_err() {
        if let Ok(destino) = std::fs::read_link(p) {
            let destino = p.parent().map(|d| d.join(&destino)).unwrap_or(destino);
            caminhos.extend(real(&destino));
            caminhos.push(destino);
        }
    }
    let mut v: Vec<String> = Vec::new();
    for c in caminhos.iter().map(|c| c.to_string_lossy().into_owned()) {
        if !v.contains(&c) {
            v.push(c);
        }
    }
    v
}

/// O nome da pasta dos transcritos de um projeto em `~/.claude/projects`, como o Claude Code o dá (2.1.319): o caminho
/// com cada unidade UTF-16 que não é letra nem algarismo ASCII trocada por `-`; passando de 200, os 200 primeiros e um
/// hash, que fica de fora (a regra casa o começo). Em NFC, que é como o CLI normaliza a pasta, e como está, se for
/// outro.
pub(crate) fn projeto_do_claude(pasta: &Path) -> Vec<String> {
    let mut nomes = Vec::new();
    for nome in nomes_inteiros_do_claude(pasta) {
        let nome: String = nome.chars().take(200).collect();
        if !nomes.contains(&nome) {
            nomes.push(nome);
        }
    }
    nomes
}

/// Os nomes da pasta dos transcritos que são o nome inteiro dela: até 200, inclusive. Os de um caminho mais longo
/// levam o hash que o CLI acrescenta, e a pasta não tem como nascer antes de ele a criar.
pub(crate) fn pastas_exatas_do_claude(pasta: &Path) -> Vec<String> {
    nomes_inteiros_do_claude(pasta).into_iter().filter(|n| n.len() <= 200).collect()
}

/// O nome que o Claude Code dá à pasta dos transcritos, antes de cortar nos 200 (cada caractere é ASCII).
fn nomes_inteiros_do_claude(pasta: &Path) -> Vec<String> {
    use unicode_normalization::UnicodeNormalization;
    let como_esta = pasta.to_string_lossy().into_owned();
    let nfc: String = como_esta.nfc().collect();
    let mut nomes = Vec::new();
    for caminho in [nfc, como_esta] {
        let nome: String = caminho
            .encode_utf16()
            .map(|u| match char::from_u32(u32::from(u)) {
                Some(c) if c.is_ascii_alphanumeric() => c,
                _ => '-',
            })
            .collect();
        if !nomes.contains(&nome) {
            nomes.push(nome);
        }
    }
    nomes
}

/// O armazém de login da aba (`CLAUDE_SECURESTORAGE_CONFIG_DIR`), quando é uma pasta de `contas/abas` ou
/// `contas/fixas`: o Claude preso grava nele as travas do login (`.oauth_refresh.lock`, `.storage-write.lock`). Uma
/// pasta de verdade, não um link: a IA presa grava no armazém e poderia trocá-lo por um link para o que quisesse. A
/// pasta de cima, que ela não alcança, vale como está ou pelo destino real (a variável pode vir num, e a casa no outro).
fn armazem_da_aba(contas: &Path, armazem: &Path) -> Vec<String> {
    let (Some(pai), Some(std::path::Component::Normal(nome))) = (armazem.parent(), armazem.components().next_back())
    else {
        return Vec::new();
    };
    // Sem `.` nem `..`: o destino real deles seria outra pasta (o armazém de outra aba).
    let so_nomes = armazem.components().all(|c| {
        matches!(c, std::path::Component::RootDir | std::path::Component::Prefix(_) | std::path::Component::Normal(_))
    });
    let e_pasta = so_nomes && std::fs::symlink_metadata(armazem).is_ok_and(|m| m.is_dir());
    let pai_real = pai.canonicalize().ok();
    let de_contas = [contas.join("abas"), contas.join("fixas")]
        .iter()
        .any(|c| c.as_path() == pai || (pai_real.is_some() && c.canonicalize().ok() == pai_real));
    if !armazem.is_absolute() || !e_pasta || !de_contas {
        return Vec::new();
    }
    formas(pai).into_iter().map(|p| Path::new(&p).join(nome).to_string_lossy().into_owned()).collect()
}

/// As palavras de um comando, à moda do shell: o que vem entre aspas faz parte da palavra (com espaço e tudo); fora
/// delas, espaço, `;|&()` e `<` separam. Uma palavra `NOME=valor` vale também pelo valor.
fn palavras(texto: &str) -> Vec<String> {
    let mut saida = Vec::new();
    let mut atual = String::new();
    let fecha = |atual: &mut String, saida: &mut Vec<String>| {
        if let Some((_, valor)) = atual.split_once('=') {
            saida.push(valor.to_string());
        }
        if !atual.is_empty() {
            saida.push(std::mem::take(atual));
        }
    };
    let mut letras = texto.chars();
    while let Some(c) = letras.next() {
        match c {
            '"' | '\'' | '`' => atual.extend(letras.by_ref().take_while(|d| *d != c)),
            c if c.is_whitespace() || ";|&()<".contains(c) => fecha(&mut atual, &mut saida),
            c => atual.push(c),
        }
    }
    fecha(&mut atual, &mut saida);
    saida
}

/// Os caminhos que a configuração do Claude Code da pessoa (`~/.claude/settings.json` e `settings.local.json`) cita —
/// o script de um gancho, o da linha de status, um ajudante —, achados em cada texto dela como caminho absoluto, `~/`
/// ou `$HOME/` (no Windows, também `C:\`, `~\`, `%USERPROFILE%\` e `$env:USERPROFILE\`): pelas palavras do shell (um
/// caminho entre aspas, com espaço) e soltos no texto (o que vai dentro de um `bash -c "…"`). Um deles que caia onde a
/// IA presa grava viraria, trocado por ela, comando da sessão seguinte. Alvo de redirecionamento de saída
/// (`2>/dev/null`, `>> ~/.claude/log`) não conta: é onde o comando grava; o de entrada conta (`bash < ~/.claude/x.sh`
/// executa o arquivo). `/dev` fica de fora. É uma leitura do texto: um script chamado por outro, ou por uma variável,
/// não é seguido. Cada um vem com o aviso de que é pasta de programas: o valor de uma variável `…PATH` (`PATH`,
/// `PYTHONPATH`, `NODE_PATH`), onde um programa novo seria achado e rodado.
pub(crate) fn executados_pela_configuracao(claude: &Path, casa: &Path) -> Vec<(PathBuf, bool)> {
    executados_pela_configuracao_em(claude, casa, cfg!(windows))
}

fn executados_pela_configuracao_em(claude: &Path, casa: &Path, windows: bool) -> Vec<(PathBuf, bool)> {
    fn textos<'a>(chave: &'a str, v: &'a serde_json::Value, saida: &mut Vec<(&'a str, &'a str)>) {
        match v {
            serde_json::Value::String(s) => saida.push((chave, s)),
            serde_json::Value::Array(a) => a.iter().for_each(|x| textos(chave, x, saida)),
            serde_json::Value::Object(o) => o.iter().for_each(|(k, x)| textos(k, x, saida)),
            _ => {}
        }
    }
    let saida = regex::Regex::new(r#"(?:\d|&)?>>?\s*(?:"[^"]*"|'[^']*'|[^\s;|&()]+)"#).expect("regex fixa");
    // No Unix, o caminho começa por `/`, com ou sem a casa antes. No Windows (sem caixa nas variáveis, como no `cmd` e
    // no PowerShell, onde `$home` é comum), também `\` depois da casa e a unidade
    // (`C:\`), mas nunca um `\` sozinho: o `\$HOME/x.sh` de um `sh -c "…"` (a expansão adiada para o shell de dentro)
    // seria lido inteiro e perdido, em vez do `$HOME/x.sh`.
    let solto = if windows {
        r#"(?:(?:~|(?i:\$HOME|\$\{HOME\}|%USERPROFILE%|\$env:USERPROFILE))[/\\]|/|\b[A-Za-z]:[/\\])[^\s"'`;|&()<>]+"#
    } else {
        r#"(?:~|\$HOME|\$\{HOME\})?/[^\s"'`;|&()<>]+"#
    };
    let solto = regex::Regex::new(solto).expect("regex fixa");
    let mut achados: Vec<(PathBuf, bool)> = Vec::new();
    for nome in ["settings.json", "settings.local.json"] {
        let Ok(texto) = caminhos::ler_texto_ate(&claude.join(nome), caminhos::LEITURA_MAXIMA) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&texto) else { continue };
        let mut lidos = Vec::new();
        textos("", &json, &mut lidos);
        for (chave, texto) in lidos {
            let de_programas = chave.ends_with("PATH");
            let limpo = saida.replace_all(texto, " ");
            let mut candidatos = palavras(&limpo);
            candidatos.extend(solto.find_iter(&limpo).map(|m| m.as_str().to_string()));
            for caminho in candidatos.iter().flat_map(|c| citados_na_palavra(c, casa, windows)) {
                if caminho.starts_with("/dev") || caminho == casa {
                    continue;
                }
                match achados.iter_mut().find(|(c, _)| *c == caminho) {
                    Some((_, p)) => *p |= de_programas,
                    None => achados.push((caminho, de_programas)),
                }
            }
        }
    }
    achados
}

/// Os caminhos que uma palavra da configuração cita. Uma lista de pastas (`~/.claude/bin:$PATH`) vale por cada uma; no
/// Windows o `:` da unidade (`C:\`) e o de `$env:` não separam.
fn citados_na_palavra(palavra: &str, casa: &Path, windows: bool) -> Vec<PathBuf> {
    let mut pedacos = Vec::new();
    let mut inicio = 0;
    for (i, c) in palavra.char_indices() {
        if c != ':' {
            continue;
        }
        let antes = &palavra[inicio..i];
        let depois = palavra[i + 1..].chars().next();
        let unidade = antes.len() == 1 && antes.chars().all(|l| l.is_ascii_alphabetic()) && matches!(depois, Some('/' | '\\'));
        if !(windows && (unidade || antes.eq_ignore_ascii_case("$env"))) {
            pedacos.push(antes);
            inicio = i + 1;
        }
    }
    pedacos.push(&palavra[inicio..]);
    pedacos.into_iter().filter_map(|p| caminho_citado(p, casa, windows)).collect()
}

/// Um caminho, se é da casa ou absoluto: `~/`, `$HOME/`, `${HOME}/` e `/…`. No Windows, também com `\`, por
/// `%USERPROFILE%` e `$env:USERPROFILE` (sem caixa, como no `cmd` e no PowerShell), os de unidade (`C:\`, `C:/`) e os
/// do Git Bash (`/c/…`), sempre com `\`.
fn caminho_citado(p: &str, casa: &Path, windows: bool) -> Option<PathBuf> {
    if !windows {
        return match ["~/", "$HOME/", "${HOME}/"].iter().find_map(|i| p.strip_prefix(i)) {
            Some(resto) => Some(casa.join(resto)),
            None if p.starts_with('/') => Some(PathBuf::from(p)),
            None => None,
        };
    }
    let barras = |t: &str| t.replace('/', "\\");
    let minusculas = p.to_ascii_lowercase();
    let da_casa = ["~", "$home", "${home}", "%userprofile%", "$env:userprofile"]
        .iter()
        .find_map(|i| minusculas.strip_prefix(i).filter(|r| r.starts_with(['/', '\\'])).map(|_| &p[i.len() + 1..]));
    if let Some(resto) = da_casa {
        let casa = casa.to_string_lossy();
        return (!resto.is_empty()).then(|| PathBuf::from(format!("{}\\{}", casa.trim_end_matches(['\\', '/']), barras(resto))));
    }
    let b = p.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\') {
        return Some(PathBuf::from(barras(p)));
    }
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/' {
        return Some(PathBuf::from(format!("{}:\\{}", char::from(b[1]).to_ascii_uppercase(), barras(&p[3..]))));
    }
    None
}

/// Os arquivos que o `~/.claude/CLAUDE.md` traz para toda conversa (`@~/caminho`, `@/caminho` ou `@caminho`, relativo
/// à pasta de quem importa), e os que estes trazem, até cinco níveis, como o CLI: instrução de toda sessão, presa ou não.
pub(crate) fn importados_pelo_claude_md(claude: &Path, casa: &Path) -> Vec<PathBuf> {
    let import = regex::Regex::new(r"(?:^|\s)@([^\s`]+)").expect("regex fixa");
    let mut achados: Vec<PathBuf> = Vec::new();
    let mut fila = vec![(claude.join("CLAUDE.md"), 0)];
    while let Some((arquivo, nivel)) = fila.pop() {
        let Ok(texto) = caminhos::ler_texto_ate(&arquivo, caminhos::LEITURA_MAXIMA) else { continue };
        let pasta = arquivo.parent().unwrap_or(claude).to_path_buf();
        for c in import.captures_iter(&texto) {
            // A pontuação da frase não faz parte do caminho (`@~/.claude/regras.md;`).
            let alvo = c[1].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
            // Um absoluto substitui a pasta de quem importa (no Windows, também o `C:\` e o `\` da unidade dela).
            let da_casa = alvo.strip_prefix("~/").or_else(|| alvo.strip_prefix("~\\").filter(|_| cfg!(windows)));
            let caminho = match da_casa {
                Some(resto) => casa.join(resto),
                None => pasta.join(alvo),
            };
            // Um importado que ainda não existe também: criado pela IA presa, viraria instrução do mesmo jeito.
            if alvo.is_empty() || caminho.is_dir() || achados.contains(&caminho) {
                continue;
            }
            achados.push(caminho.clone());
            if nivel < 5 && caminho.is_file() {
                fila.push((caminho, nivel + 1));
            }
        }
    }
    achados
}

/// Se o caminho citado é dado, não programa nem instrução: um arquivo que o gancho só lê ou grava (`jq .
/// ~/.claude.json`, `tail history.jsonl`, `tee -a x.log`), muitas vezes o próprio estado do CLI. Negá-lo tiraria do
/// Claude preso a gravação dele, sem nada a proteger de execução. Um `.md` ou `.txt` citado não é dado: um gancho que
/// o põe na conversa faz dele instrução de toda sessão.
pub(crate) fn e_dado(p: &Path, casa: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let da_casa = p.parent() == Some(casa) && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(".claude.json"));
    matches!(ext.as_str(), "json" | "jsonl" | "log" | "csv" | "lock") || da_casa
}

/// Se o caminho citado é um script, que costuma levar os vizinhos (um módulo, uma biblioteca do shell) junto.
pub(crate) fn e_script(p: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "sh" | "bash" | "zsh" | "py" | "js" | "mjs" | "cjs" | "ts" | "rb" | "pl" | "php" | "lua" | "ps1" | "psm1" | "cmd" | "bat"
    )
}

/// O que, na casa, a IA presa lê e grava: a casa do Claude Code e do Codex, o estado, os caches e o Chaveiro.
const AREAS_DA_CASA: [&str; 6] = [".claude", ".codex", ".local/state", ".cache", "Library/Caches", "Library/Keychains"];

/// As pastas de `~/.claude` que o próprio CLI grava, e a das contas (o armazém da aba, onde ele põe as travas do
/// login): negar a pasta inteira de um arquivo citado nelas quebraria a sessão presa.
pub(crate) const CLAUDE_ESTADO: [&str; 22] = [
    "projects",
    "todos",
    "shell-snapshots",
    "session-env",
    "file-history",
    "sessions",
    "contas",
    "plans",
    "debug",
    "statsig",
    "paste-cache",
    "uploads",
    "downloads",
    "ide",
    "cache",
    "telemetry",
    "backups",
    "jobs",
    "daemon",
    "state",
    "chrome",
    "feedback",
];

/// O perfil do sandbox para uma pasta: nada de /Users e /Volumes, salvo a
/// pasta (e o git dela), a casa do Claude Code e do Codex, o estado e os
/// caches, o Chaveiro; para ler, os programas e a configuração do git.
///
/// Dentro de `~/.claude`, o que toda sessão executa ou lê como instrução fica só
/// para ler (`CLAUDE_SO_LEITURA`, e os scripts que a configuração cita), o cofre
/// e os logins das contas ficam fechados (menos o armazém da própria aba) e, dos
/// transcritos, só os do projeto preso. Fora da casa, os programas instalados
/// (`/Applications`, `/Library`, `/opt`, `/usr/local`) ficam só para ler.
///
/// Com a configuração própria do Claude preso (`presa`, o `CLAUDE_CONFIG_DIR` dele: `config_da_presa`), o estado do
/// CLI na `~/.claude` de verdade, que ele não usa mais, e o `~/.claude.json` ficam fechados.
///
/// As permissões usam as MESMAS operações da negação e vêm depois dela: uma
/// permissão de `file-read*` não vence uma negação de `file-read-data`, que
/// é mais específica, e a última regra que casa é a que vale.
pub fn perfil(pasta: &Path, presa: Option<&Path>) -> String {
    let armazem = std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR").filter(|v| !v.is_empty()).map(PathBuf::from);
    perfil_com(pasta, &keep_proto::socket_path(), &caminhos::casa(), armazem.as_deref(), presa)
}

fn perfil_com(pasta: &Path, socket: &Path, casa_dir: &Path, armazem: Option<&Path>, presa: Option<&Path>) -> String {
    let casa = casa_dir.to_string_lossy().into_owned();
    let mut rw = vec![pasta.to_string_lossy().into_owned()];
    rw.extend(pastas_do_git(pasta));
    for sub in AREAS_DA_CASA {
        rw.push(format!("{casa}/{sub}"));
    }
    let rw_da_casa: Vec<PathBuf> = AREAS_DA_CASA.iter().map(|s| casa_dir.join(s)).collect();
    let mut linhas = vec![
        "(version 1)".to_string(),
        "(allow default)".into(),
        "(deny file-read-data file-write* (subpath \"/Users\"))".into(),
        "(deny file-read-data file-write* (subpath \"/Volumes\"))".into(),
    ];
    // Os programas fora de /Users que a pessoa instala e pode trocar (o Keep.app, o Homebrew): lidos e executados, nunca
    // gravados. Trocado pela IA presa, um deles (o `keep` do app, o `python3` de um gancho) rodaria na sessão seguinte,
    // fora do sandbox. Uma pasta presa que esteja ali vem depois e vence.
    for p in ["/Applications", "/Library", "/opt", "/usr/local"] {
        linhas.push(format!("(deny file-write* (subpath {}))", literal(p)));
    }
    // O socket do keepd fica fora de /Users, e o `(allow default)` deixa `connect()` passar: sem esta negação a IA presa
    // abre uma aba nova (shell sem sandbox), digita em qualquer aba e lê as telas de todas, pelo protocolo do daemon.
    // O sandbox casa o caminho canônico (`/private/var/...`, não `/var/...`), então as duas formas entram.
    for socket in socket_do_daemon(socket) {
        linhas.push(format!("(deny network-outbound (remote unix-socket (path-literal {})))", literal(&socket)));
    }
    // O canal de mensagens entre as sessões do Claude Code (`/tmp/cc-socks/<pid>.sock`, anotado na sessão de cada uma em
    // `~/.claude/sessions`): por ele a IA presa mandaria pedidos a uma sessão sem sandbox.
    for raiz in ["/private/tmp/cc-socks/", "/tmp/cc-socks/"] {
        if let Some(re) = literal_regex(&format!("^{}", escapa_regex(raiz))) {
            linhas.push(format!("(deny network-outbound (remote unix-socket (path-regex {re})))"));
        }
    }
    for p in &rw {
        linhas.push(format!("(allow file-read-data file-write* (subpath {}))", literal(p)));
    }
    linhas.push(format!("(allow file-read-data file-write* (literal {}))", literal(&format!("{casa}/.claude.json"))));
    // O CLI grava o `~/.claude.json` por um temporário ao lado (`.claude.json.tmp.<pid>.<…>`) e o renomeia.
    if let Some(re) = literal_regex(&format!("^{}.*", escapa_regex(&format!("{casa}/.claude.json.")))) {
        linhas.push(format!("(allow file-read-data file-write* (regex {re}))"));
    }
    for sub in [".local/bin", ".local/share/claude", ".config/git"] {
        linhas.push(format!("(allow file-read-data (subpath {}))", literal(&format!("{casa}/{sub}"))));
    }
    linhas.push(format!("(allow file-read-data (literal {}))", literal(&format!("{casa}/.gitconfig"))));
    if let Some(re) =
        literal_regex(&format!("^{}.*", escapa_regex(&format!("{casa}/Library/Preferences/com.apple.security"))))
    {
        linhas.push(format!("(allow file-read-data (regex {re}))"));
    }
    // Depois da permissão de `~/.claude`, para vencê-la. O resto da pasta (histórico, `todos`, `shell-snapshots`,
    // `statsig`, as travas do login global) continua do Claude preso, que grava ali ao abrir.
    let claude = casa_dir.join(".claude");
    // Cada item dentro de `~/.claude` e, se ela for link, também no destino (a IA presa não a troca): um item que ainda
    // não existe (`commands/`, `settings.local.json`) seria criado lá, que é o caminho que o sandbox compara.
    let bases = formas(&claude);
    let em_claude = |nome: &str| {
        let mut v: Vec<String> = Vec::new();
        for p in bases.iter().flat_map(|b| formas(&Path::new(b).join(nome))) {
            if !v.contains(&p) {
                v.push(p);
            }
        }
        v
    };
    for nome in CLAUDE_SO_LEITURA {
        for p in em_claude(nome) {
            linhas.push(format!("(deny file-write* (subpath {}))", literal(&p)));
        }
    }
    // A própria `~/.claude`, pelo caminho exato: renomeá-la para outra área que a IA presa grava (`~/.cache/c`), mexer
    // ali no que não tem negação e devolvê-la passaria por cima de tudo. Isso não impede criar nada dentro dela.
    for b in &bases {
        linhas.push(format!("(deny file-write* (literal {}))", literal(b)));
    }
    for p in em_claude("contas") {
        linhas.push(format!("(deny file-read-data file-write* (subpath {}))", literal(&p)));
    }
    // As configurações próprias das IAs presas (`config_da_presa`), em qualquer modo: o Keep as monta de fora do
    // sandbox e lê as sessões delas, e uma IA presa que plantasse ali um link (para a própria `~/.claude`) ou uma
    // `.pasta` faria a montagem apagar o que é da pessoa ou forjaria sessões. A de quem sobe com ela volta abaixo.
    for p in em_claude("presas") {
        linhas.push(format!("(deny file-read-data file-write* (subpath {}))", literal(&p)));
    }
    for p in armazem.map(|a| armazem_da_aba(&claude.join("contas"), a)).unwrap_or_default() {
        linhas.push(format!("(allow file-read-data file-write* (subpath {}))", literal(&p)));
    }
    // Os transcritos e a memória de cada projeto: os da pasta presa e das subpastas dela (`<nome>-<sub>`), nunca os
    // de outra pasta. Uma pasta de transcritos que é link não é seguida (ver `formas`): sem ela, o Claude preso
    // conversa, mas não guarda a conversa.
    let projetos = em_claude("projects");
    for p in &projetos {
        linhas.push(format!("(deny file-read-data file-write* (subpath {}))", literal(p)));
    }
    for p in &projetos {
        for nome in projeto_do_claude(pasta) {
            if let Some(re) = literal_regex(&regra_do_projeto(p, &nome)) {
                linhas.push(format!("(allow file-read-data file-write* (regex {re}))"));
            }
        }
    }
    // Com a configuração própria do Claude preso (`CLAUDE_CONFIG_DIR`): o estado do CLI na `~/.claude` de verdade, que
    // ele não usa mais, fica fechado, para ler e para gravar — o retrato do shell e o ambiente que cada sessão relê, as
    // sessões (com o canal de mensagens e a chave de cada uma), o histórico e os anexos de todas as conversas, as outras
    // configurações próprias — e o `~/.claude.json` também, onde um servidor MCP gravado subiria em toda sessão. Os
    // transcritos do projeto e o armazém da aba ficam como acima; as pastas das ferramentas da pessoa em `~/.claude`
    // (as que os ganchos dela usam), também. Da configuração própria, tudo, menos trocar a pasta e a `.pasta` que o Keep
    // lê para saber a que pasta ela está presa (`caminhos::pastas_de_sessoes`).
    if let Some(presa) = presa {
        for nome in CLAUDE_ESTADO.iter().copied().filter(|n| !matches!(*n, "projects" | "contas")) {
            for p in em_claude(nome) {
                linhas.push(format!("(deny file-read-data file-write* (subpath {}))", literal(&p)));
            }
        }
        for p in em_claude("history.jsonl") {
            linhas.push(format!("(deny file-read-data file-write* (literal {}))", literal(&p)));
        }
        linhas.push(format!("(deny file-read-data file-write* (literal {}))", literal(&format!("{casa}/.claude.json"))));
        if let Some(re) = literal_regex(&format!("^{}.*", escapa_regex(&format!("{casa}/.claude.json.")))) {
            linhas.push(format!("(deny file-read-data file-write* (regex {re}))"));
        }
        for p in formas(presa) {
            linhas.push(format!("(allow file-read-data file-write* (subpath {}))", literal(&p)));
        }
        for p in formas(presa).into_iter().chain(formas(&presa.join(".pasta"))) {
            linhas.push(format!("(deny file-write* (literal {}))", literal(&p)));
        }
    }
    // O que a configuração da pessoa manda executar e cai onde a IA presa grava (um `~/.claude/statusline-command.sh`):
    // o arquivo e, quando é um script numa subpasta de `~/.claude` que não é de estado do CLI, a pasta dele, que um
    // script costuma levar junto. Por último, para vencer também o armazém da aba e a pasta dos transcritos. A pasta
    // presa fica de fora: é dela.
    // Os arquivos que o `CLAUDE.md` importa entram do mesmo jeito, qualquer que seja a extensão: instrução de toda
    // sessão.
    let citados = executados_pela_configuracao(&claude, casa_dir)
        .into_iter()
        .map(|(p, de_programas)| (p, de_programas, false))
        .chain(importados_pelo_claude_md(&claude, casa_dir).into_iter().map(|p| (p, false, true)));
    for (alvo, de_programas, instrucao) in citados {
        if (!instrucao && e_dado(&alvo, casa_dir)) || alvo.starts_with(pasta) || pasta.starts_with(&alvo) {
            continue;
        }
        for p in formas(&alvo) {
            linhas.push(format!("(deny file-write* (literal {}))", literal(&p)));
        }
        // E as pastas acima dele, até a casa (ou até a raiz, fora dela), pelo caminho exato: renomear uma delas,
        // trocar o arquivo e devolvê-la seria outro jeito de trocá-lo. Isso não impede criar nada dentro delas.
        for acima in alvo.ancestors().skip(1).take_while(|d| *d != casa_dir && d.parent().is_some()) {
            for p in formas(acima) {
                linhas.push(format!("(deny file-write* (literal {}))", literal(&p)));
            }
        }
        // A pasta inteira: a de um script numa subpasta de `~/.claude`, que ele costuma levar junto, e a de programas
        // (`~/.claude/bin` no `PATH`, onde um `git` novo rodaria na sessão seguinte), dentro do que a IA presa grava
        // na casa. Nunca uma pasta de estado do CLI, uma dessas áreas inteira, nem uma pasta de dados (um cache).
        let livre = |d: &Path| {
            rw_da_casa.iter().any(|r| d.starts_with(r) && d != r.as_path())
                && !CLAUDE_ESTADO.iter().any(|e| d.starts_with(claude.join(e)))
        };
        let pasta_inteira = if de_programas && alvo.is_dir() {
            Some(alvo.as_path()).filter(|d| livre(d))
        } else {
            alvo.parent().filter(|p| e_script(&alvo) && p.starts_with(&claude) && livre(p))
        };
        for p in pasta_inteira.map(formas).unwrap_or_default() {
            linhas.push(format!("(deny file-write* (subpath {}))", literal(&p)));
        }
    }
    linhas.join("\n") + "\n"
}

/// A configuração própria do Claude preso a `pasta` (o `CLAUDE_CONFIG_DIR` dele): `~/.claude/presas/<nome>`, uma por
/// pasta presa, com o nome que o Claude Code dá aos transcritos dela (até 80 caracteres) e um hash do caminho, que
/// separa duas pastas de nome igual (`/a/b-c` e `/a/b/c`).
#[cfg(unix)]
pub(crate) fn config_da_presa(pasta: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let nome: String = projeto_do_claude(pasta).into_iter().next().unwrap_or_default().chars().take(80).collect();
    let hash = Sha256::digest(pasta.to_string_lossy().as_bytes());
    let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    caminhos::presas().join(format!("{nome}-{hex}"))
}

/// A configuração própria para o Claude preso a `pasta` (`config_da_presa`), quando ele sobe no armazém de login da
/// aba (`CLAUDE_SECURESTORAGE_CONFIG_DIR`, uma pasta de `contas/abas` ou `contas/fixas`). Sem o armazém, o Claude usa o
/// login global, que mora no item do Chaveiro da pasta de configuração (`contas::credencial`): noutra pasta, ele
/// pediria outro login. Com um `CLAUDE_CONFIG_DIR` da pessoa (ou herdado de um Claude preso de fora), fica o que veio.
#[cfg(unix)]
fn presa_para(programa: &str, pasta: &Path) -> Option<PathBuf> {
    if nome(programa) != "claude" || std::env::var_os("CLAUDE_CONFIG_DIR").is_some_and(|v| !v.is_empty()) {
        return None;
    }
    let armazem = std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)?;
    if armazem_da_aba(&caminhos::contas(), &armazem).is_empty() {
        return None;
    }
    Some(config_da_presa(pasta))
}

/// Monta a configuração própria (`presa`) antes de subir, de fora do sandbox: a `.pasta`, com a pasta presa; um link
/// para cada item que toda sessão do Claude lê ao abrir (`CLAUDE_SO_LEITURA`: a configuração, o `CLAUDE.md`, as skills,
/// os plugins, os ganchos…), que o perfil deixa só para ler; um link para a pasta dos transcritos do projeto (e da
/// subpasta em que o programa começa), que fica onde o Keep e o Claude sem sandbox a acham (a retomada, a lista das
/// conversas); e o `.claude.json`, refeito do da pessoa (`semeia_claude_json`).
#[cfg(unix)]
fn prepara_config_da_presa(presa: &Path, pasta: &Path, onde: &Path, casa: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let claude = casa.join(".claude");
    // `presas/` e a configuração própria, pastas de verdade: um link ali (de antes desta negação, ou de quem mais
    // grave na casa) levaria a montagem, que apaga e liga, para dentro da `~/.claude` da pessoa.
    for d in [presa.parent().unwrap_or(presa), presa] {
        match std::fs::symlink_metadata(d) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                std::fs::remove_file(d)?;
                std::fs::DirBuilder::new().mode(0o700).create(d)?;
            }
            Err(_) => std::fs::DirBuilder::new().recursive(true).mode(0o700).create(d)?,
        }
    }
    // Daqui em diante grava-se, de fora do sandbox, onde a IA presa grava o que quiser: um link (ou um arquivo) que
    // ela tenha posto no lugar de `projects/` sai antes, ou as ligações de baixo iriam por ele até onde ela escolheu.
    let projetos = presa.join("projects");
    match std::fs::symlink_metadata(&projetos) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            std::fs::remove_file(&projetos)?;
            std::fs::DirBuilder::new().mode(0o700).create(&projetos)?;
        }
        Err(_) => std::fs::DirBuilder::new().mode(0o700).create(&projetos)?,
    }
    let marca = format!("{}\n", pasta.to_string_lossy());
    if std::fs::read_to_string(presa.join(".pasta")).ok().as_deref() != Some(marca.as_str()) {
        std::fs::write(presa.join(".pasta"), &marca)?;
    }
    for nome in CLAUDE_SO_LEITURA {
        liga(&claude.join(nome), &presa.join(nome))?;
    }
    let mut nomes = pastas_exatas_do_claude(pasta);
    if onde.starts_with(pasta) {
        for nome in pastas_exatas_do_claude(onde) {
            if !nomes.contains(&nome) {
                nomes.push(nome);
            }
        }
    }
    // Passando de 200, o CLI acrescenta um hash ao nome, e a pasta nasce na configuração própria.
    let mut ligacoes = Vec::new();
    for nome in &nomes {
        let real = claude.join("projects").join(nome);
        std::fs::create_dir_all(&real)?;
        ligacoes.push((nome.clone(), real));
    }
    liga_os_projetos(&projetos, &ligacoes)?;
    semeia_claude_json(&casa.join(".claude.json"), &presa.join(".claude.json"), pasta)
}

/// Os links dos transcritos dentro de `projetos`, por um descritor da pasta aberto sem seguir link: outra aba presa à
/// mesma pasta, rodando, que troque `projects/` por um link no meio da montagem não a leva para outro lugar. Uma
/// pasta de verdade ali (de quando o nome não tinha como ser ligado) fica, com o que tem; o resto dá lugar ao link.
#[cfg(unix)]
fn liga_os_projetos(projetos: &Path, ligacoes: &[(String, PathBuf)]) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    struct Descritor(libc::c_int);
    impl Drop for Descritor {
        fn drop(&mut self) {
            unsafe { libc::close(self.0) };
        }
    }
    let texto = |b: &[u8]| CString::new(b).map_err(std::io::Error::other);
    let falhou = |r: libc::c_int| if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) };
    let pasta = texto(projetos.as_os_str().as_bytes())?;
    // SAFETY: caminho terminado em zero; o descritor fecha no `Drop`.
    let fd = unsafe { libc::open(pasta.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let fd = Descritor(fd);
    for (nome, alvo) in ligacoes {
        let (n, a) = (texto(nome.as_bytes())?, texto(alvo.as_os_str().as_bytes())?);
        // SAFETY: `stat` é só dados; os caminhos terminam em zero e são relativos ao descritor aberto acima.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatat(fd.0, n.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } == 0 {
            match st.st_mode & libc::S_IFMT {
                libc::S_IFDIR => continue,
                libc::S_IFLNK => {
                    let mut lido = vec![0u8; 4096];
                    let n_lido = unsafe { libc::readlinkat(fd.0, n.as_ptr(), lido.as_mut_ptr().cast(), lido.len()) };
                    if usize::try_from(n_lido).is_ok_and(|t| &lido[..t] == alvo.as_os_str().as_bytes()) {
                        continue;
                    }
                    falhou(unsafe { libc::unlinkat(fd.0, n.as_ptr(), 0) })?;
                }
                _ => falhou(unsafe { libc::unlinkat(fd.0, n.as_ptr(), 0) })?,
            }
        }
        falhou(unsafe { libc::symlinkat(a.as_ptr(), fd.0, n.as_ptr()) })?;
    }
    Ok(())
}

/// `link` passa a apontar para `alvo`, que existe; sem `alvo`, um link que estava ali sai, e o que a IA presa criou no
/// lugar (um `settings.json` de quem não tem um) fica. Com `alvo`, o que a IA presa pôs no lugar do link sai.
#[cfg(unix)]
fn liga(alvo: &Path, link: &Path) -> std::io::Result<()> {
    let tem_alvo = alvo.exists();
    match std::fs::symlink_metadata(link) {
        Ok(m) if m.file_type().is_symlink() => {
            if tem_alvo && std::fs::read_link(link)?.as_path() == alvo {
                return Ok(());
            }
            std::fs::remove_file(link)?;
        }
        Ok(m) if tem_alvo => {
            if m.is_dir() {
                std::fs::remove_dir_all(link)?;
            } else {
                std::fs::remove_file(link)?;
            }
        }
        Ok(_) => return Ok(()),
        Err(_) => {}
    }
    if tem_alvo {
        std::os::unix::fs::symlink(alvo, link)?;
    }
    Ok(())
}

/// O `.claude.json` da configuração própria, refeito a cada subida do `~/.claude.json` da pessoa: as preferências, a
/// conta e os servidores MCP dela, e dos projetos só os da pasta presa (os de dentro dela e, dos de cima, a raiz do
/// repositório, pela qual o Claude acha o projeto), com o que o Claude preso gravou nos dele por cima. Uma pasta de
/// cima que não é repositório (a casa) fica de fora: é o que a pessoa fez noutro lugar. Sem o da pessoa (ou
/// ilegível), fica o que está.
#[cfg(unix)]
fn semeia_claude_json(real: &Path, destino: &Path, pasta: &Path) -> std::io::Result<()> {
    use serde_json::Value;
    let Ok(texto) = caminhos::ler_texto_ate(real, 64 << 20) else { return Ok(()) };
    let Ok(Value::Object(mut novo)) = serde_json::from_str::<Value>(&texto) else { return Ok(()) };
    // O da IA presa, só um arquivo comum dela: não um link para um arquivo da pessoa, nem uma FIFO.
    let do_preso = Some(destino)
        .filter(|d| std::fs::symlink_metadata(d).is_ok_and(|m| m.is_file()))
        .and_then(|d| caminhos::ler_texto_ate(d, 64 << 20).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("projects").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let da_pasta = |chave: &str| {
        let p = Path::new(chave);
        p.is_absolute() && (p.starts_with(pasta) || (pasta.starts_with(p) && p.join(".git").exists()))
    };
    let mut projetos = serde_json::Map::new();
    if let Some(Value::Object(todos)) = novo.get("projects") {
        for (chave, v) in todos.iter().filter(|(chave, _)| da_pasta(chave)) {
            projetos.insert(chave.clone(), v.clone());
        }
    }
    for (chave, v) in do_preso.into_iter().filter(|(chave, _)| da_pasta(chave)) {
        match (projetos.get_mut(&chave), v) {
            (Some(Value::Object(base)), Value::Object(dele)) => base.extend(dele),
            (_, v) => {
                projetos.insert(chave, v);
            }
        }
    }
    novo.insert("projects".into(), Value::Object(projetos));
    let dados = serde_json::to_vec_pretty(&Value::Object(novo)).map_err(std::io::Error::other)?;
    // O temporário fica onde a IA presa grava o que quiser, até um link para um arquivo da pessoa (`~/.zshrc`): o que
    // estiver ali sai, e ele nasce novo, sem seguir link (`create_new`). O `rename` troca o destino, link ou não.
    let tmp = destino.with_file_name(".claude.json.keep");
    match std::fs::symlink_metadata(&tmp) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&tmp)?,
        Ok(_) => std::fs::remove_file(&tmp)?,
        Err(_) => {}
    }
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(&dados)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, destino)
}

/// A pasta de transcritos `<projetos>/<nome>` e as das subpastas (`<nome>-<sub>`), com o que houver dentro.
fn regra_do_projeto(projetos: &str, nome: &str) -> String {
    format!("^{}/{nome}(-[^/]*)?(/|$)", escapa_regex(projetos))
}

/// O nome do programa, sem a pasta (de `/` ou de `\`) e, no Windows, sem a
/// extensão (`claude.exe`, `codex.cmd`), em minúsculas.
fn nome(programa: &str) -> String {
    let base = programa.rsplit(['/', '\\']).next().unwrap_or(programa);
    let base = match base.rsplit_once('.') {
        Some((tronco, ext)) if matches!(ext.to_ascii_lowercase().as_str(), "exe" | "cmd" | "bat" | "com") => tronco,
        _ => base,
    };
    base.to_ascii_lowercase()
}

/// O primeiro argumento que não é opção: o subcomando, se houver.
fn subcomando(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| !a.starts_with('-'))
}

/// O que o Claude Code lê para ficar sem a ferramenta Bash (a regra de
/// permissão que a nega por inteiro).
pub const CONFIGURACAO_SEM_BASH: &str = r#"{"permissions":{"deny":["Bash"]}}"#;

/// O nome do arquivo com essa regra: quem lê a linha de um Claude que anda
/// (para subi-lo de novo na aba) a reconhece por ele e a deixa de fora, senão a
/// regra iria junto para um Claude que não está preso.
pub const ARQUIVO_SEM_BASH: &str = "claude-sem-bash.json";

/// A ferramenta de shell do Claude Code no Windows presa a uma pasta. O Git
/// Bash (MSYS2) não sobe sob o token restrito: ele cria de início um canal de
/// sinais com nome e uma DACL só do usuário, e a segunda checagem do token
/// (que não leva o usuário) o recusa (`couldn't create signal pipe`). O Claude
/// Code tem, no lugar, a ferramenta PowerShell: negada a Bash, e com
/// `CLAUDE_CODE_USE_POWERSHELL_TOOL=1` (o ambiente do programa preso leva), ele
/// fica só com ela. A regra vai por um arquivo (`configuracao`), não pela
/// linha: um argumento com aspas não passa por um atalho `.cmd` do npm.
pub fn claude_sem_o_bash(args: &[String], configuracao: &str) -> Vec<String> {
    let mut v = vec!["--settings".to_string(), configuracao.to_string()];
    v.extend(args.iter().cloned());
    v
}

/// Como a linha sobe num workspace restrito: `Err` com o motivo quando o
/// comando mandaria trabalho para fora do sandbox; o Codex ganha
/// `--no-daemon` (o servidor dele dentro do processo).
pub fn linha_presa(programa: &str, args: &[String]) -> Result<Vec<String>, String> {
    let sub = subcomando(args);
    match nome(programa).as_str() {
        "claude" if matches!(sub, Some("agents" | "attach")) => Err(format!(
            "`claude {}` roda as sessões no servidor de fundo do Claude Code, fora da pasta permitida.",
            sub.unwrap_or_default()
        )),
        "codex" if matches!(sub, Some("agents" | "queue")) => Err(format!(
            "`codex {}` depende do servidor de fundo do Codex, fora da pasta permitida.",
            sub.unwrap_or_default()
        )),
        "codex"
            if !args.iter().any(|a| a == "--no-daemon")
                && !matches!(sub, Some("app-server" | "mcp" | "login" | "logout" | "features" | "exec-server" | "remote-control" | "completion" | "help")) =>
        {
            let mut v = vec!["--no-daemon".to_string()];
            v.extend(args.iter().cloned());
            Ok(v)
        }
        _ => Ok(args.to_vec()),
    }
}

/// `keep ia confinar -- <programa> [argumentos…]`: roda o programa preso à
/// pasta do workspace da aba, ou como sempre quando não há pasta.
#[cfg(unix)]
pub fn cli(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    let args: Vec<String> = match args.first().map(String::as_str) {
        Some("--") => args[1..].to_vec(),
        _ => args.to_vec(),
    };
    let Some((programa, resto)) = args.split_first() else {
        eprintln!("uso: keep ia confinar -- <programa> [argumentos...]");
        return 2;
    };
    let ws = std::env::var("KEEP_WORKSPACE").unwrap_or_default();
    let pasta_permitida = if ws.is_empty() || !cfg!(target_os = "macos") { None } else { pasta(&ws) };
    let Some(pasta_permitida) = pasta_permitida else {
        let erro = std::process::Command::new(programa).args(resto).exec();
        eprintln!("Keep: não foi possível rodar {programa}: {erro}");
        return 127;
    };
    let pasta_permitida = PathBuf::from(&pasta_permitida);
    let Ok(pasta_real) = pasta_permitida.canonicalize().map_err(|_| ()).and_then(|p| if p.is_dir() { Ok(p) } else { Err(()) }) else {
        eprintln!(
            "Keep: a pasta permitida para a IA do workspace {ws} não existe ({}). A IA não sobe sem a restrição; escolha outra pasta no workspace.",
            pasta_permitida.display()
        );
        return 1;
    };
    let linha = match linha_presa(programa, resto) {
        Ok(l) => l,
        Err(motivo) => {
            eprintln!("Keep: {motivo} Este workspace restringe a IA a {}.", pasta_real.display());
            return 1;
        }
    };
    let sandbox = "/usr/bin/sandbox-exec";
    if !Path::new(sandbox).is_file() {
        eprintln!("Keep: sandbox-exec não está disponível; a IA não sobe sem a restrição.");
        return 1;
    }
    // A pasta dos transcritos fica negada inteira, menos a do projeto preso: sem ela, o Claude preso não teria onde
    // criar a dele. Uma casa nova ainda não a tem; ela nasce aqui, de fora do sandbox.
    let _ = std::fs::create_dir_all(caminhos::claude().join("projects"));
    let aqui = std::env::current_dir().ok().and_then(|p| p.canonicalize().ok());
    let onde = match aqui {
        Some(a) if a.starts_with(&pasta_real) => {
            eprintln!("Keep: a IA deste workspace só acessa {}.", pasta_real.display());
            a
        }
        _ => {
            let _ = std::env::set_current_dir(&pasta_real);
            eprintln!("Keep: a IA deste workspace só acessa {}; subindo nela.", pasta_real.display());
            pasta_real.clone()
        }
    };
    // A configuração própria do Claude preso: sem ela, o que ele grava na `~/.claude` de verdade (o retrato do shell,
    // as sessões, o `~/.claude.json`) seria lido e rodado pelas sessões sem sandbox.
    let presa = presa_para(programa, &pasta_real);
    if let Some(p) = &presa {
        if let Err(e) = prepara_config_da_presa(p, &pasta_real, &onde, &caminhos::casa()) {
            eprintln!(
                "Keep: não deu para montar a configuração própria da IA presa em {} ({e}); a IA não sobe sem ela.",
                p.display()
            );
            return 1;
        }
    }
    let mut comando = std::process::Command::new(sandbox);
    comando.arg("-p").arg(perfil(&pasta_real, presa.as_deref())).arg(programa).args(&linha).env("KEEP_CONFINED_TO", &pasta_real);
    if let Some(p) = &presa {
        comando.env("CLAUDE_CONFIG_DIR", p);
    }
    let erro = comando.exec();
    eprintln!("Keep: não foi possível prender {programa}: {erro}");
    1
}

#[cfg(windows)]
pub fn cli(args: &[String]) -> i32 {
    crate::confinar_win::executar::cli(args)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um repositório com uma worktree de verdade: `<repo>/.git/worktrees/w`, que aponta de volta para `<wt>/.git`.
    fn repo_com_worktree(raiz: &Path) -> (PathBuf, PathBuf) {
        let repo = raiz.join("repo");
        let gitdir = repo.join(".git/worktrees/w");
        let wt = raiz.join("wt");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        std::fs::write(gitdir.join("gitdir"), format!("{}\n", wt.join(".git").display())).unwrap();
        std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
        (wt, repo)
    }

    #[test]
    fn a_worktree_de_verdade_leva_o_git_dela() {
        let dir = tempfile::tempdir().unwrap();
        let (wt, repo) = repo_com_worktree(dir.path());
        let git = repo.join(".git").canonicalize().unwrap();
        assert_eq!(
            pastas_do_git(&wt),
            vec![git.join("worktrees/w").to_string_lossy().into_owned(), git.to_string_lossy().into_owned()]
        );
    }

    #[test]
    fn um_git_reescrito_pela_ia_nao_alarga_o_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let (wt, _) = repo_com_worktree(dir.path());
        // A raiz do disco, a casa e uma pasta qualquer: nada entra.
        for alvo in ["/", dir.path().to_str().unwrap()] {
            std::fs::write(wt.join(".git"), format!("gitdir: {alvo}\n")).unwrap();
            assert!(pastas_do_git(&wt).is_empty(), "{alvo}");
        }
        // A worktree de OUTRO repositório: o `gitdir` de lá aponta para a pasta dele, não para esta.
        let outro = dir.path().join("outro");
        std::fs::create_dir_all(&outro).unwrap();
        let (_, repo_alheio) = repo_com_worktree(&outro);
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", repo_alheio.join(".git/worktrees/w").display())).unwrap();
        assert!(pastas_do_git(&wt).is_empty());
        // Um commondir que sai do repositório também derruba tudo.
        let (wt2, repo2) = repo_com_worktree(&dir.path().join("terceiro"));
        std::fs::write(repo2.join(".git/worktrees/w/commondir"), "/\n").unwrap();
        assert!(pastas_do_git(&wt2).is_empty());
    }

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
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(p, helper.to_string_lossy());
            assert_eq!(a, vec!["ia", "confinar", "--", "/bin/claude", "--effort", "max"]);
        } else {
            // Sem sandbox neste sistema (Linux): a linha fica a de sempre.
            assert_eq!((p.as_str(), a), ("/bin/claude", vec!["--effort".to_string(), "max".to_string()]));
        }
        let (p, a) = envolve("livre", "/bin/claude".into(), vec!["--effort".into()]);
        assert_eq!((p.as_str(), a), ("/bin/claude", vec!["--effort".to_string()]));
        // Sem o helper, a linha é a de sempre.
        std::fs::remove_file(&helper).unwrap();
        let (p, _) = envolve("presa", "/bin/claude".into(), vec![]);
        assert_eq!(p, "/bin/claude");
    }

    #[test]
    fn o_que_sai_do_sandbox_e_recusado_e_o_codex_sobe_sem_daemon() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(linha_presa("/x/claude", &v(&["agents"])).is_err());
        assert!(linha_presa("claude", &v(&["attach", "abc"])).is_err());
        assert_eq!(linha_presa("claude", &v(&["--effort", "max"])).unwrap(), v(&["--effort", "max"]));
        // No Windows o Claude fica sem a ferramenta Bash (o Git Bash não sobe preso).
        assert_eq!(
            claude_sem_o_bash(&v(&["--effort", "max"]), r"C:\k\sem-bash.json"),
            v(&["--settings", r"C:\k\sem-bash.json", "--effort", "max"])
        );
        assert!(!CONFIGURACAO_SEM_BASH.contains(' '));
        assert!(linha_presa("codex", &v(&["agents"])).is_err());
        assert!(linha_presa("codex", &v(&["queue", "x"])).is_err());
        assert_eq!(linha_presa("/b/codex", &v(&["resume", "id"])).unwrap(), v(&["--no-daemon", "resume", "id"]));
        assert_eq!(linha_presa("codex", &v(&["--no-daemon"])).unwrap(), v(&["--no-daemon"]));
        assert_eq!(linha_presa("codex", &v(&["login"])).unwrap(), v(&["login"]));
    }

    #[test]
    fn o_programa_do_windows_tem_pasta_com_barra_invertida_e_extensao() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(linha_presa(r"C:\Users\a\.local\bin\claude.exe", &v(&["agents"])).is_err());
        assert!(linha_presa(r"C:\Users\a\.local\bin\Claude.EXE", &v(&["attach", "x"])).is_err());
        assert!(linha_presa(r"C:\npm\codex.cmd", &v(&["queue"])).is_err());
        assert_eq!(linha_presa(r"C:\npm\codex.cmd", &v(&["resume", "id"])).unwrap(), v(&["--no-daemon", "resume", "id"]));
        assert_eq!(linha_presa(r"C:\u\codex.exe", &v(&["--no-daemon"])).unwrap(), v(&["--no-daemon"]));
        assert_eq!(linha_presa(r"C:\u\claude.exe", &v(&["--effort", "max"])).unwrap(), v(&["--effort", "max"]));
        // O que só parece com o nome não é o programa.
        assert_eq!(linha_presa(r"C:\u\claude-code.exe", &v(&["agents"])).unwrap(), v(&["agents"]));
        assert_eq!(linha_presa(r"C:\u\claude.exe.bak", &v(&["agents"])).unwrap(), v(&["agents"]));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn o_perfil_fecha_o_socket_do_daemon_para_a_ia_presa() {
        use std::process::Command;
        let d = tempfile::tempdir().unwrap();
        let socket = d.path().join("k.sock");
        let _escuta = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let perfil = perfil_com(Path::new("/tmp/proj"), &socket, &caminhos::casa(), None, None);
        assert!(perfil.contains("(deny network-outbound (remote unix-socket (path-literal"), "{perfil}");
        // O perl do sistema conecta e sai com 0 se o socket atendeu (o `nc -U` ficaria esperando o outro lado).
        let conecta = ["-MIO::Socket::UNIX", "-e", "exit(IO::Socket::UNIX->new(Peer => $ARGV[0]) ? 0 : 1)"];
        let com_sandbox = |p: &str| {
            Command::new("/usr/bin/sandbox-exec").args(["-p", p, "/usr/bin/perl"]).args(conecta).arg(&socket).status().unwrap().success()
        };
        // Sem o sandbox o socket atende; sandbox que nem se aplica (um já em volta) não prova nada.
        assert!(Command::new("/usr/bin/perl").args(conecta).arg(&socket).status().unwrap().success());
        if !com_sandbox("(version 1)\n(allow default)\n") {
            eprintln!("sandbox-exec não se aplica aqui: pulado");
            return;
        }
        assert!(!com_sandbox(&perfil), "a IA presa conectou ao socket do daemon");
    }

    #[test]
    fn o_perfil_nega_a_casa_e_libera_a_pasta_depois() {
        let p = perfil(Path::new("/Users/x/projeto"), None);
        let nega = p.find("(deny file-read-data file-write* (subpath \"/Users\"))").unwrap();
        let libera = p.find("(allow file-read-data file-write* (subpath \"/Users/x/projeto\"))").unwrap();
        assert!(libera > nega, "a permissão precisa vir depois da negação");
        assert!(p.contains("/.claude\"))") && p.contains("/.codex\"))"));
        assert!(!p.contains("(allow file-read*"), "permissão com operações diferentes da negação perde para ela");
    }

    /// Uma casa de mentira com o que a IA presa acha em `~/.claude`: a configuração e as pastas que toda sessão
    /// executa, o cofre e dois armazéns de aba, o resto do estado e os transcritos do projeto preso, de uma subpasta
    /// dele e de outro projeto.
    struct Casa {
        _dir: tempfile::TempDir,
        raiz: PathBuf,
        casa: PathBuf,
        claude: PathBuf,
        pasta: PathBuf,
        projeto: PathBuf,
    }

    fn casa_de_mentira() -> Casa {
        let dir = tempfile::tempdir().unwrap();
        // O sandbox compara o caminho real (`/private/var/...`, não `/var/...`).
        let raiz = dir.path().canonicalize().unwrap();
        let (casa, pasta) = (raiz.join("casa"), raiz.join("projetos/app"));
        let claude = casa.join(".claude");
        let projeto = claude.join("projects").join(&projeto_do_claude(&pasta)[0]);
        let sub = PathBuf::from(format!("{}-sub", projeto.display()));
        for d in [&pasta, &projeto, &sub, &claude.join("projects/-outro"), &claude.join("todos")] {
            std::fs::create_dir_all(d).unwrap();
        }
        for d in ["skills", "plugins", "agents", "commands", "hooks", "rules", "output-styles", "contas/abas/aba", "contas/abas/outra"] {
            std::fs::create_dir_all(claude.join(d)).unwrap();
            std::fs::write(claude.join(d).join("x"), "dentro\n").unwrap();
        }
        for f in ["settings.json", "settings.local.json", "CLAUDE.md", "history.jsonl", "contas/principal.json"] {
            std::fs::write(claude.join(f), "{}\n").unwrap();
        }
        for p in [&projeto, &sub, &claude.join("projects/-outro")] {
            std::fs::write(p.join("t.jsonl"), "{}\n").unwrap();
        }
        Casa { _dir: dir, raiz, casa, claude, pasta, projeto }
    }

    impl Casa {
        /// O perfil da casa de mentira, que não fica em /Users: a negação de /Users vale também para a pasta dela.
        fn perfil(&self, armazem: Option<&Path>) -> String {
            self.perfil_com_presa(armazem, None)
        }

        /// O mesmo, com a configuração própria do Claude preso.
        fn perfil_com_presa(&self, armazem: Option<&Path>, presa: Option<&Path>) -> String {
            let p = perfil_com(&self.pasta, Path::new("/tmp/nenhum.sock"), &self.casa, armazem, presa);
            let volumes = "(deny file-read-data file-write* (subpath \"/Volumes\"))";
            let raiz = literal(&self.raiz.to_string_lossy());
            assert!(p.contains(volumes));
            p.replacen(volumes, &format!("{volumes}\n(deny file-read-data file-write* (subpath {raiz}))"), 1)
        }
    }

    #[test]
    fn o_nome_da_pasta_dos_transcritos_e_o_do_claude_code() {
        let um = |p: &str| projeto_do_claude(Path::new(p));
        assert_eq!(um("/Users/a/Projetos/keep"), vec!["-Users-a-Projetos-keep"]);
        assert_eq!(um("/Users/a/psi/.claude/worktrees/w_1"), vec!["-Users-a-psi--claude-worktrees-w-1"]);
        // Uma letra acentuada é UMA unidade UTF-16 em NFC e duas decomposta: o CLI usa a NFC; a outra também vale.
        assert_eq!(um("/Users/a/S\u{e3}o"), vec!["-Users-a-S-o"]);
        assert_eq!(um("/Users/a/Sa\u{303}o"), vec!["-Users-a-S-o", "-Users-a-Sa-o"]);
        // Fora do plano básico (um emoji) são duas unidades.
        assert_eq!(um("/Users/a/\u{1f600}x"), vec!["-Users-a---x"]);
        // Passando de 200, só o começo: o CLI põe `-<hash>` depois dos 200.
        let longo = format!("/Users/a/{}", "b".repeat(300));
        assert_eq!(um(&longo)[0].len(), 200);
        // A pasta que dá para criar antes é a do nome inteiro, até 200 inclusive.
        let duzentos = format!("/{}", "b".repeat(199));
        assert_eq!(pastas_exatas_do_claude(Path::new(&duzentos)), vec![format!("-{}", "b".repeat(199))]);
        assert!(pastas_exatas_do_claude(Path::new(&format!("/{}", "b".repeat(200)))).is_empty());
    }

    #[test]
    fn a_regra_dos_transcritos_casa_o_projeto_e_as_subpastas_e_nada_mais() {
        let regra = regex::Regex::new(&regra_do_projeto("/Users/a/.claude/projects", "-Users-a-app")).unwrap();
        for sim in ["", "/", "/x.jsonl", "/memory/MEMORY.md", "-sub", "-sub/x.jsonl", "--claude-worktrees-w/x.jsonl"] {
            assert!(regra.is_match(&format!("/Users/a/.claude/projects/-Users-a-app{sim}")), "{sim}");
        }
        for nao in ["-Users-a-appx", "-Users-a-ap", "-Users-a-outro", "-Users-a"] {
            assert!(!regra.is_match(&format!("/Users/a/.claude/projects/{nao}/x.jsonl")), "{nao}");
        }
        // O ponto do `.claude` é um ponto, não qualquer letra.
        assert!(!regra.is_match("/Users/a/xclaude/projects/-Users-a-app/x.jsonl"));
        assert!(!regra.is_match("/Users/a/.claude/projects"));
    }

    #[test]
    fn as_negacoes_de_dentro_do_claude_vem_depois_da_permissao_dele() {
        let c = casa_de_mentira();
        let armazem = c.claude.join("contas").join("abas").join("aba");
        let p = c.perfil(Some(&armazem));
        let regra = |acao: &str, caminho: &str| format!("({acao} (subpath {}))", literal(caminho));
        let em = |nome: &str| c.claude.join(nome).to_string_lossy().into_owned();
        let claude = format!("{}/.claude", c.casa.to_string_lossy());
        let libera = p.find(&regra("allow file-read-data file-write*", &claude)).unwrap();
        let depois = |r: &str| {
            let i = p.find(r).unwrap_or_else(|| panic!("falta {r}\n{p}"));
            assert!(i > libera, "{r} precisa vir depois da permissão de ~/.claude");
            i
        };
        // Só para ler: a negação é de gravação, e a leitura continua com a permissão de ~/.claude.
        for nome in CLAUDE_SO_LEITURA {
            depois(&regra("deny file-write*", &em(nome)));
            assert!(!p.contains(&regra("deny file-read-data file-write*", &em(nome))), "{nome}");
        }
        let contas = depois(&regra("deny file-read-data file-write*", &em("contas")));
        let aba = depois(&regra("allow file-read-data file-write*", &armazem.to_string_lossy()));
        assert!(aba > contas, "o armazém da aba volta depois de fechar as contas");
        let projetos = depois(&regra("deny file-read-data file-write*", &em("projects")));
        let projeto = depois("(-[^/]*)?(/|$)\"))");
        assert!(projeto > projetos, "o projeto preso volta depois de fechar os transcritos");
    }

    #[test]
    fn so_o_armazem_de_verdade_de_uma_aba_e_liberado() {
        let c = casa_de_mentira();
        let contas = c.claude.join("contas");
        let aba = contas.join("abas").join("aba");
        assert_eq!(armazem_da_aba(&contas, &aba).first(), Some(&aba.to_string_lossy().into_owned()));
        std::fs::create_dir_all(contas.join("fixas").join("f")).unwrap();
        assert!(!armazem_da_aba(&contas, &contas.join("fixas").join("f")).is_empty());
        // O cofre inteiro, a pasta de todas as abas, o que fica fundo, o que não existe, o relativo e o de fora: não.
        std::fs::create_dir_all(aba.join("fundo")).unwrap();
        let sumiu = contas.join("abas").join("sumiu");
        for nao in [contas.clone(), contas.join("abas"), aba.join("fundo"), sumiu, c.pasta.clone()] {
            assert!(armazem_da_aba(&contas, &nao).is_empty(), "{}", nao.display());
        }
        assert!(armazem_da_aba(&contas, Path::new("abas/aba")).is_empty());
        #[cfg(unix)]
        {
            // Um `..` no caminho não é o armazém de verdade (no Windows, juntar a um caminho `\\?\` já o resolve).
            assert!(armazem_da_aba(&contas, &contas.join("abas/aba/../outra")).is_empty());
            // Um armazém trocado por um link (a IA presa grava no dela): nem o link, nem para onde ele aponta.
            let link = contas.join("abas/link");
            std::os::unix::fs::symlink(&c.casa, &link).unwrap();
            assert!(armazem_da_aba(&contas, &link).is_empty());
            assert!(!c.perfil(Some(&link)).contains(&format!("(subpath \"{}\")", c.casa.display())));
        }
    }

    /// Roda `sh -c <comando>` preso pelo perfil; `None` quando o sandbox nem se aplica aqui (um já em volta).
    #[cfg(target_os = "macos")]
    fn preso(perfil: &str, comando: &str) -> Option<bool> {
        use std::process::{Command, Stdio};
        let roda = |p: &str, c: &str| {
            Command::new("/usr/bin/sandbox-exec")
                .args(["-p", p, "/bin/sh", "-c", c])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        if !roda("(version 1)\n(allow default)\n", "true") {
            eprintln!("sandbox-exec não se aplica aqui: pulado");
            return None;
        }
        Some(roda(perfil, comando))
    }

    #[cfg(unix)]
    #[test]
    fn a_configuracao_propria_tem_o_nome_do_projeto_e_separa_as_pastas_de_nome_igual() {
        let a = config_da_presa(Path::new("/a/b-c"));
        assert_ne!(a, config_da_presa(Path::new("/a/b/c")));
        assert_eq!(a, config_da_presa(Path::new("/a/b-c")));
        assert!(a.file_name().unwrap().to_string_lossy().starts_with("-a-b-c-"), "{}", a.display());
        assert_eq!(a.parent().unwrap(), caminhos::presas());
        let longa = config_da_presa(&Path::new("/x").join("y".repeat(300)));
        assert_eq!(longa.file_name().unwrap().len(), 80 + 1 + 8);
    }

    #[cfg(unix)]
    #[test]
    fn a_configuracao_propria_liga_o_que_toda_sessao_le_e_refaz_o_claude_json() {
        use serde_json::{Map, Value, json};
        let c = casa_de_mentira();
        let presa = c.claude.join("presas/app-1");
        let pasta = c.pasta.to_string_lossy().into_owned();
        let raiz_do_repo = c.raiz.join("projetos").to_string_lossy().into_owned();
        std::fs::create_dir_all(c.raiz.join("projetos/.git")).unwrap();
        let mut projetos = Map::new();
        projetos.insert(pasta.clone(), json!({"hasTrustDialogAccepted": true, "allowedTools": []}));
        projetos.insert(format!("{pasta}/sub"), json!({"x": 1}));
        projetos.insert(raiz_do_repo.clone(), json!({"raiz": true}));
        projetos.insert("/outro/projeto".into(), json!({"segredo": true}));
        projetos.insert(c.raiz.to_string_lossy().into_owned(), json!({"de-cima-sem-repositorio": true}));
        let real = json!({"hasCompletedOnboarding": true, "mcpServers": {"m": {"command": "x"}}, "projects": projetos});
        std::fs::write(c.casa.join(".claude.json"), real.to_string()).unwrap();
        prepara_config_da_presa(&presa, &c.pasta, &c.pasta.join("sub"), &c.casa).unwrap();

        // A pasta presa, para o Keep saber de quem são as sessões gravadas ali.
        assert_eq!(std::fs::read_to_string(presa.join(".pasta")).unwrap(), format!("{pasta}\n"));
        // Um link para cada item que toda sessão lê, e para os transcritos do projeto e da subpasta em que começa.
        for nome in CLAUDE_SO_LEITURA {
            assert_eq!(std::fs::read_link(presa.join(nome)).unwrap(), c.claude.join(nome), "{nome}");
        }
        for nome in [projeto_do_claude(&c.pasta).remove(0), projeto_do_claude(&c.pasta.join("sub")).remove(0)] {
            let real = c.claude.join("projects").join(&nome);
            assert_eq!(std::fs::read_link(presa.join("projects").join(&nome)).unwrap(), real, "{nome}");
            assert!(real.is_dir(), "{nome}");
        }
        // Do `.claude.json` da pessoa: tudo, menos os projetos de outras pastas.
        let lido = |p: &Path| serde_json::from_str::<Value>(&std::fs::read_to_string(p).unwrap()).unwrap();
        let semeado = lido(&presa.join(".claude.json"));
        assert_eq!(semeado["hasCompletedOnboarding"], true);
        assert_eq!(semeado["mcpServers"]["m"]["command"], "x");
        let deles = semeado["projects"].as_object().unwrap();
        assert!(deles.contains_key(&pasta) && deles.contains_key(&format!("{pasta}/sub")), "{deles:?}");
        assert!(deles.contains_key(&raiz_do_repo), "a de cima, como a raiz do repositório: {deles:?}");
        assert!(!deles.contains_key("/outro/projeto"), "{deles:?}");
        assert!(!deles.contains_key(&*c.raiz.to_string_lossy()), "a de cima que não é repositório: {deles:?}");

        // O Claude preso grava no projeto dele; a IA presa troca um link por um arquivo dela e cria um item que a
        // pessoa não tem; a pessoa apaga um item. Na subida seguinte:
        let mut dele = semeado.clone();
        dele["projects"][&pasta]["allowedTools"] = json!(["Bash(ls)"]);
        dele["projects"][&pasta]["lastSessionId"] = json!("s-1");
        std::fs::write(presa.join(".claude.json"), dele.to_string()).unwrap();
        std::fs::remove_file(presa.join("settings.json")).unwrap();
        std::fs::write(presa.join("settings.json"), "{\"hooks\":{}}").unwrap();
        std::fs::remove_file(c.claude.join("settings.local.json")).unwrap();
        std::fs::remove_dir_all(c.claude.join("output-styles")).unwrap();
        std::fs::remove_file(presa.join("output-styles")).unwrap();
        std::fs::create_dir_all(presa.join("output-styles")).unwrap();
        prepara_config_da_presa(&presa, &c.pasta, &c.pasta, &c.casa).unwrap();
        let de_novo = lido(&presa.join(".claude.json"));
        // o que o Claude preso gravou no projeto dele fica por cima do da pessoa;
        assert_eq!(de_novo["projects"][&pasta]["allowedTools"], json!(["Bash(ls)"]));
        assert_eq!(de_novo["projects"][&pasta]["lastSessionId"], "s-1");
        assert_eq!(de_novo["projects"][&pasta]["hasTrustDialogAccepted"], true);
        // o link volta por cima do que a IA presa pôs no lugar; o que só ela tem fica; o link sem alvo sai.
        assert_eq!(std::fs::read_link(presa.join("settings.json")).unwrap(), c.claude.join("settings.json"));
        assert!(presa.join("output-styles").is_dir() && std::fs::read_link(presa.join("output-styles")).is_err());
        assert!(std::fs::symlink_metadata(presa.join("settings.local.json")).is_err(), "o link sem alvo sai");
        // A IA presa pôs links no lugar do temporário e de `projects/`, para um arquivo e uma pasta da pessoa: a
        // montagem, de fora do sandbox, não grava neles.
        let da_pessoa = c.casa.join("arquivo-da-pessoa");
        let pasta_da_pessoa = c.casa.join("pasta-da-pessoa");
        std::fs::write(&da_pessoa, "intocado\n").unwrap();
        std::fs::create_dir_all(&pasta_da_pessoa).unwrap();
        std::os::unix::fs::symlink(&da_pessoa, presa.join(".claude.json.keep")).unwrap();
        std::fs::remove_dir_all(presa.join("projects")).unwrap();
        std::os::unix::fs::symlink(&pasta_da_pessoa, presa.join("projects")).unwrap();
        prepara_config_da_presa(&presa, &c.pasta, &c.pasta, &c.casa).unwrap();
        assert_eq!(std::fs::read_to_string(&da_pessoa).unwrap(), "intocado\n");
        assert_eq!(std::fs::read_dir(&pasta_da_pessoa).unwrap().count(), 0, "nada nasce na pasta da pessoa");
        assert!(std::fs::symlink_metadata(presa.join("projects")).unwrap().is_dir());
        assert!(std::fs::symlink_metadata(presa.join(".claude.json")).unwrap().is_file());
        // Uma pasta de transcritos de verdade na configuração própria (de um nome que não tinha como ser ligado) fica
        // com o que tem; um arquivo no lugar de um link dá lugar a ele.
        let nome = projeto_do_claude(&c.pasta).remove(0);
        std::fs::remove_file(presa.join("projects").join(&nome)).unwrap();
        std::fs::create_dir_all(presa.join("projects").join(&nome)).unwrap();
        std::fs::write(presa.join("projects").join(&nome).join("t.jsonl"), "{}\n").unwrap();
        let sub = projeto_do_claude(&c.pasta.join("sub")).remove(0);
        std::fs::write(presa.join("projects").join(&sub), "lixo").unwrap();
        prepara_config_da_presa(&presa, &c.pasta, &c.pasta.join("sub"), &c.casa).unwrap();
        assert!(presa.join("projects").join(&nome).join("t.jsonl").is_file());
        assert_eq!(std::fs::read_link(presa.join("projects").join(&sub)).unwrap(), c.claude.join("projects").join(&sub));
        // Uma configuração própria que é link (para a própria `~/.claude`) não leva a montagem até lá: sai, e a pasta
        // nasce de verdade; o que é da pessoa fica.
        let plantada = c.claude.join("presas/plantada");
        std::os::unix::fs::symlink(&c.claude, &plantada).unwrap();
        prepara_config_da_presa(&plantada, &c.pasta, &c.pasta, &c.casa).unwrap();
        assert!(std::fs::symlink_metadata(&plantada).unwrap().is_dir());
        assert_eq!(std::fs::read_to_string(c.claude.join("settings.json")).unwrap(), "{}\n");
        assert!(c.claude.join("skills/x").is_file());
        // Sem o `.claude.json` da pessoa, o dela fica.
        std::fs::remove_file(c.casa.join(".claude.json")).unwrap();
        prepara_config_da_presa(&presa, &c.pasta, &c.pasta, &c.casa).unwrap();
        assert_eq!(lido(&presa.join(".claude.json"))["projects"][&pasta]["lastSessionId"], "s-1");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn com_a_configuracao_propria_o_estado_do_cli_na_casa_fica_fechado() {
        let c = casa_de_mentira();
        let armazem = c.claude.join("contas/abas/aba");
        let presa = c.claude.join("presas/app-1");
        for d in ["sessions", "shell-snapshots", "session-env", "file-history", "jev", "presas/outra"] {
            std::fs::create_dir_all(c.claude.join(d)).unwrap();
            std::fs::write(c.claude.join(d).join("x"), "dentro\n").unwrap();
        }
        std::fs::write(c.casa.join(".claude.json"), "{}\n").unwrap();
        prepara_config_da_presa(&presa, &c.pasta, &c.pasta, &c.casa).unwrap();
        let p = c.perfil_com_presa(Some(&armazem), Some(&presa));
        let (cl, pr, ca) = (c.claude.display().to_string(), presa.display().to_string(), c.casa.display().to_string());
        let tem = |cmd: &str| preso(&p, cmd);
        if tem("true").is_none() {
            return;
        }
        // O estado do CLI na `~/.claude` de verdade, que o Claude preso não usa mais: nem ler, nem gravar.
        for d in ["sessions", "shell-snapshots", "session-env", "file-history", "presas/outra"] {
            assert_eq!(tem(&format!("cat '{cl}/{d}/x'")), Some(false), "ler {d}");
            assert_eq!(tem(&format!("echo y > '{cl}/{d}/novo'")), Some(false), "gravar {d}");
        }
        assert_eq!(tem(&format!("cat '{cl}/history.jsonl'")), Some(false), "o histórico");
        assert_eq!(tem(&format!("cat '{ca}/.claude.json'")), Some(false), "ler o ~/.claude.json");
        let troca = format!("echo '{{}}' > '{ca}/.claude.json.tmp.1' && mv '{ca}/.claude.json.tmp.1' '{ca}/.claude.json'");
        assert_eq!(tem(&troca), Some(false), "gravar o ~/.claude.json");
        // A configuração própria: grava o que quiser, menos a `.pasta` e a própria pasta.
        let dela = format!("mkdir -p '{pr}/todos' && echo y > '{pr}/todos/x' && cat '{pr}/.claude.json' >/dev/null");
        assert_eq!(tem(&dela), Some(true), "a configuração própria");
        assert_eq!(tem(&format!("echo /outra > '{pr}/.pasta'")), Some(false), "a .pasta");
        assert_eq!(tem(&format!("mv '{pr}' '{}/levada'", c.pasta.display())), Some(false), "levar a configuração própria");
        // Os links: a configuração da pessoa se lê e não se grava; os transcritos do projeto se gravam onde o Keep os
        // acha; um link plantado para outro projeto não leva até ele.
        assert_eq!(tem(&format!("cat '{pr}/settings.json' '{pr}/skills/x' '{pr}/CLAUDE.md'")), Some(true), "ler pelos links");
        assert_eq!(tem(&format!("echo y >> '{pr}/settings.json'")), Some(false), "gravar a configuração da pessoa");
        let nome = projeto_do_claude(&c.pasta).remove(0);
        assert_eq!(tem(&format!("echo '{{}}' > '{pr}/projects/{nome}/novo.jsonl'")), Some(true), "o transcrito do projeto");
        assert!(c.projeto.join("novo.jsonl").is_file(), "o transcrito cai na pasta de verdade");
        let plantado = format!("ln -s '{cl}/projects/-outro' '{pr}/projects/-outro' && cat '{pr}/projects/-outro/t.jsonl'");
        assert_eq!(tem(&plantado), Some(false), "outro projeto por um link plantado");
        // O armazém da aba e a pasta de uma ferramenta da pessoa ficam como estavam.
        assert_eq!(tem(&format!("echo y > '{}/trava'", armazem.display())), Some(true), "o armazém da aba");
        assert_eq!(tem(&format!("echo y >> '{cl}/jev/x'")), Some(true), "a pasta de uma ferramenta da pessoa");
        // O canal de mensagens entre as sessões, nos dois modos.
        let canal = "(deny network-outbound (remote unix-socket (path-regex #\"^/private/tmp/cc-socks/\")))";
        assert!(p.contains(canal) && c.perfil(Some(&armazem)).contains(canal));
        // Sem a configuração própria, as delas ficam fechadas também: nem ler, nem plantar um link ou uma `.pasta`.
        let sem = c.perfil(Some(&armazem));
        assert_eq!(preso(&sem, &format!("cat '{pr}/.pasta'")), Some(false), "ler uma configuração própria");
        let planta = format!("ln -s '{cl}' '{cl}/presas/plantada'");
        assert_eq!(preso(&sem, &planta), Some(false), "plantar um link em presas/");
        assert_eq!(preso(&sem, &format!("echo / > '{cl}/presas/outra/.pasta'")), Some(false), "forjar uma .pasta");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn as_regras_por_expressao_casam_de_verdade() {
        let c = casa_de_mentira();
        let p = c.perfil(None);
        let casa = c.casa.display().to_string();
        let preferencias = c.casa.join("Library/Preferences");
        std::fs::create_dir_all(&preferencias).unwrap();
        std::fs::write(preferencias.join("com.apple.security.x.plist"), "x").unwrap();
        std::fs::write(preferencias.join("com.apple.outra.plist"), "x").unwrap();
        std::fs::write(c.raiz.join("fora.txt"), "x").unwrap();
        let Some(fora) = preso(&p, &format!("cat '{}/fora.txt'", c.raiz.display())) else { return };
        assert!(!fora, "a negação da casa de mentira vale");
        // O temporário com que o CLI grava o `~/.claude.json`, e só ele.
        let tmp = format!("{casa}/.claude.json.tmp.1.a");
        assert_eq!(preso(&p, &format!("echo '{{}}' > '{tmp}' && mv '{tmp}' '{casa}/.claude.json'")), Some(true));
        assert_eq!(preso(&p, &format!("echo x > '{casa}/.claude.jsonx'")), Some(false));
        assert_eq!(preso(&p, &format!("cat '{}/com.apple.security.x.plist'", preferencias.display())), Some(true));
        assert_eq!(preso(&p, &format!("cat '{}/com.apple.outra.plist'", preferencias.display())), Some(false));
        assert!(literal_regex("^/a\"b").is_none(), "aspas não entram numa expressão do perfil");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn o_que_toda_sessao_executa_fica_so_para_ler() {
        let c = casa_de_mentira();
        let p = c.perfil(None);
        let q = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
        for nome in CLAUDE_SO_LEITURA {
            let alvo = c.claude.join(nome);
            let a = q(&alvo.display().to_string());
            let (le, grava, troca) = if alvo.is_dir() {
                (format!("ls {a} | grep -q x && cat {a}/x"), format!("echo y > {a}/novo"), format!("mv {a} {a}.velha"))
            } else {
                (format!("cat {a}"), format!("echo y >> {a}"), format!("echo y > {a}.tmp && mv {a}.tmp {a}"))
            };
            let Some(leu) = preso(&p, &le) else { return };
            assert!(leu, "{nome}: a IA presa lê");
            assert_eq!(preso(&p, &grava), Some(false), "{nome}: a IA presa grava");
            assert_eq!(preso(&p, &troca), Some(false), "{nome}: a IA presa troca");
            assert_eq!(preso(&p, &format!("rm -rf {a}")), Some(false), "{nome}: a IA presa apaga");
            assert!(alvo.exists());
        }
        assert_eq!(std::fs::read_to_string(c.claude.join("settings.json")).unwrap(), "{}\n");
        // Nem levando `~/.claude` inteira para outra área que a IA presa grava, mexendo lá e devolvendo.
        std::fs::create_dir_all(c.casa.join(".cache")).unwrap();
        let (cl, ca) = (c.claude.display(), c.casa.display());
        let troca = format!("mv '{cl}' '{ca}/.cache/c' && echo '{{\"hooks\":1}}' > '{ca}/.cache/c/settings.json' && mv '{ca}/.cache/c' '{cl}'");
        assert_eq!(preso(&p, &troca), Some(false), "a troca de ~/.claude inteira");
        assert_eq!(std::fs::read_to_string(c.claude.join("settings.json")).unwrap(), "{}\n");
        // Nem por um link físico criado numa área que ela grava, para escrever pelo outro nome.
        let elo = format!("ln '{cl}/settings.json' '{ca}/.cache/s' && echo '{{\"hooks\":1}}' > '{ca}/.cache/s'");
        assert_eq!(preso(&p, &elo), Some(false), "o link físico");
        assert_eq!(std::fs::read_to_string(c.claude.join("settings.json")).unwrap(), "{}\n");
        // O resto da casa do Claude continua dele: ele grava ali ao abrir.
        let resto = format!("echo '{{}}' >> '{0}/history.jsonl' && echo n > '{0}/todos/novo.json'", c.claude.display());
        assert_eq!(preso(&p, &resto), Some(true));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn o_cofre_e_os_logins_das_contas_ficam_fechados_menos_o_da_aba() {
        let c = casa_de_mentira();
        let contas = c.claude.join("contas");
        let p = c.perfil(Some(&contas.join("abas/aba")));
        let cs = contas.display();
        let Some(cofre) = preso(&p, &format!("cat '{cs}/principal.json'")) else { return };
        assert!(!cofre, "a IA presa leu o cofre");
        assert_eq!(preso(&p, &format!("ls '{cs}/abas/outra'")), Some(false), "leu o armazém de outra aba");
        assert_eq!(preso(&p, &format!("echo y > '{cs}/novo.json'")), Some(false), "gravou no cofre");
        std::fs::create_dir_all(c.casa.join(".cache")).unwrap();
        let elo = format!("ln '{cs}/principal.json' '{}/.cache/p' && cat '{}/.cache/p'", c.casa.display(), c.casa.display());
        assert_eq!(preso(&p, &elo), Some(false), "leu o cofre por um link físico");
        let trava = format!("mkdir '{cs}/abas/aba/.storage-write.lock' && rmdir '{cs}/abas/aba/.storage-write.lock'");
        assert_eq!(preso(&p, &trava), Some(true), "o Claude preso grava as travas no armazém dele");
        // Sem armazém de aba (o login global), nada de contas.
        assert_eq!(preso(&c.perfil(None), &trava), Some(false));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dos_transcritos_so_os_do_projeto_preso() {
        let c = casa_de_mentira();
        let p = c.perfil(None);
        let projetos = c.claude.join("projects");
        let (proj, outro) = (c.projeto.display().to_string(), projetos.join("-outro").display().to_string());
        let dele = format!("cat '{proj}/t.jsonl' && echo n >> '{proj}/t.jsonl' && mkdir -p '{proj}/memory'");
        let Some(leu) = preso(&p, &dele) else { return };
        assert!(leu, "o Claude preso lê e grava os transcritos dele");
        assert_eq!(preso(&p, &format!("cat '{proj}-sub/t.jsonl'")), Some(true), "e os de uma subpasta da pasta presa");
        assert_eq!(preso(&p, &format!("cat '{outro}/t.jsonl'")), Some(false), "leu o transcrito de outro projeto");
        assert_eq!(preso(&p, &format!("echo n > '{outro}/memory.md'")), Some(false), "gravou na memória de outro");
        assert_eq!(preso(&p, &format!("ls '{}'", projetos.display())), Some(false), "listou os projetos");
        // Trocar a própria pasta por um link para a de outro projeto não dá a leitura dele.
        let troca = format!("rm -rf '{proj}' && ln -s '{outro}' '{proj}' && cat '{proj}/t.jsonl'");
        assert_eq!(preso(&p, &troca), Some(false));
    }

    #[test]
    fn le_os_caminhos_que_a_configuracao_manda_executar() {
        let d = tempfile::tempdir().unwrap();
        let casa = d.path().join("casa");
        let claude = casa.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let gancho = r#"X=$HOME/.claude/x.env /bin/sh '${HOME}/.claude/scripts/a.sh' > /tmp/log 2>>~/.claude/g.log"#;
        let config = serde_json::json!({
            "statusLine": {"command": "bash ~/.claude/statusline.sh 2>/dev/null < ~/.claude/todos/lista.json"},
            "permissions": {"allow": ["Bash(python3:*)"]},
            "hooks": {"Stop": [{"hooks": [
                {"command": gancho},
                {"command": r#"bash "$HOME/.claude/meus scripts/b.sh"; sh <~/.claude/c.sh 2>&1"#},
                {"command": r#"bash -c "source ~/.claude/lib.sh; roda" && sh -c '~/.claude/x.sh --flag'"#},
            {"command": r#"sh -c "exec \$HOME/.claude/adiado.sh""#},
                {"command": "python3 /tmp/gancho.py && cat /dev/null"},
            ]}]},
        });
        std::fs::write(claude.join("settings.json"), config.to_string()).unwrap();
        let env = r#"{"env":{"P":"/opt/ferramenta/bin","PATH":"~/.claude/bin:$PATH","UV_CACHE_DIR":"~/.cache/uv"}}"#;
        std::fs::write(claude.join("settings.local.json"), env).unwrap();
        // A leitura do Unix, em qualquer sistema: a do Windows tem o teste dela.
        let com_marca = executados_pela_configuracao_em(&claude, &casa, false);
        // A pasta de programas vem marcada (o valor de um `…PATH`); a de um cache, não.
        assert!(com_marca.contains(&(casa.join(".claude/bin"), true)), "{com_marca:?}");
        assert!(com_marca.contains(&(casa.join(".cache/uv"), false)), "{com_marca:?}");
        let achados: Vec<PathBuf> = com_marca.into_iter().map(|(p, _)| p).collect();
        // O que a entrada lê (`bash < x.sh` executa o arquivo), o caminho entre aspas com espaço, o que vai dentro de um
        // `bash -c "…"`, e o que fica fora da casa onde se grava (`/tmp`) ou não.
        for esperado in [
            casa.join(".claude/statusline.sh"),
            casa.join(".claude/todos/lista.json"),
            casa.join(".claude/x.env"),
            casa.join(".claude/scripts/a.sh"),
            casa.join(".claude/meus scripts/b.sh"),
            casa.join(".claude/c.sh"),
            casa.join(".claude/lib.sh"),
            casa.join(".claude/x.sh"),
            casa.join(".claude/adiado.sh"),
            PathBuf::from("/tmp/gancho.py"),
            PathBuf::from("/bin/sh"),
            PathBuf::from("/opt/ferramenta/bin"),
        ] {
            assert!(achados.contains(&esperado), "{} em {achados:?}", esperado.display());
        }
        // Os alvos de saída (`/tmp/log`, o log do gancho), `/dev` e o que não é caminho, não.
        for fora in [PathBuf::from("/tmp/log"), casa.join(".claude/g.log"), PathBuf::from("/dev/null")] {
            assert!(!achados.contains(&fora), "{} em {achados:?}", fora.display());
        }
        assert!(!achados.iter().any(|p| p.to_string_lossy().contains("python3:")), "{achados:?}");
        assert_eq!(palavras(r#"a "b c"d 'e' X=/f g;h|i&j(k)<l"#), ["a", "b cd", "e", "/f", "X=/f", "g", "h", "i", "j", "k", "l"]);
        // Sem configuração, ou com uma que não é JSON: nada.
        std::fs::write(claude.join("settings.json"), "{").unwrap();
        std::fs::remove_file(claude.join("settings.local.json")).unwrap();
        assert!(executados_pela_configuracao_em(&claude, &casa, false).is_empty());
    }

    #[test]
    fn no_windows_os_caminhos_citados_tem_as_formas_dele() {
        let casa = Path::new(r"C:\Users\ana");
        let w = |p: &str| -> Vec<String> {
            citados_na_palavra(p, casa, true).iter().map(|c| c.to_string_lossy().into_owned()).collect()
        };
        assert_eq!(w(r"C:\Users\ana\.claude\s.ps1"), [r"C:\Users\ana\.claude\s.ps1"]);
        assert_eq!(w("C:/Users/ana/.claude/s.ps1"), [r"C:\Users\ana\.claude\s.ps1"]);
        assert_eq!(w(r"~\.claude\a.ps1"), [r"C:\Users\ana\.claude\a.ps1"]);
        assert_eq!(w("~/.claude/b.sh"), [r"C:\Users\ana\.claude\b.sh"]);
        assert_eq!(w(r"%userprofile%\.claude\c.cmd"), [r"C:\Users\ana\.claude\c.cmd"]);
        assert_eq!(w(r"$Env:USERPROFILE\.claude\d.ps1"), [r"C:\Users\ana\.claude\d.ps1"]);
        assert_eq!(w("$HOME/.claude/e.sh"), [r"C:\Users\ana\.claude\e.sh"]);
        assert_eq!(w(r"$home\.claude\e.ps1"), [r"C:\Users\ana\.claude\e.ps1"]);
        assert_eq!(w("/c/Users/ana/.claude/f.sh"), [r"C:\Users\ana\.claude\f.sh"]);
        // Uma lista à moda do Unix vale por cada pasta; o `:` da unidade não separa.
        assert_eq!(w(r"~/.claude/bin:C:\ferramentas"), [r"C:\Users\ana\.claude\bin", r"C:\ferramentas"]);
        for nada in ["%PATH%", "$env:PATH", "~", r"relativo\x.ps1", "/tmp/x.sh"] {
            assert!(w(nada).is_empty(), "{nada}: {:?}", w(nada));
        }
        // No Unix, nada disso é caminho: um `C:\…` viraria uma regra relativa, que o `sandbox-exec` recusa.
        assert!(citados_na_palavra(r"C:\Users\ana\x.ps1", Path::new("/Users/ana"), false).is_empty());
        assert!(citados_na_palavra(r"%USERPROFILE%\x.ps1", Path::new("/Users/ana"), false).is_empty());

        // Numa configuração de verdade: a linha de status do PowerShell (entre aspas, com espaço), o gancho dentro de
        // um `pwsh -c "…"` e o `PATH` separado por `;`. O alvo de saída fica de fora.
        let d = tempfile::tempdir().unwrap();
        let claude = d.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let config = serde_json::json!({
            "statusLine": {"command": r#"powershell -NoProfile -File "%USERPROFILE%\.claude\linha de status.ps1""#},
            "hooks": {"Stop": [{"hooks": [
                {"command": r#"pwsh -c "& $env:USERPROFILE\.claude\gancho.ps1 2>> C:\Users\ana\log.txt""#},
                {"command": r#"sh -c "exec \$HOME/.claude/adiado.sh""#},
                {"command": r#"pwsh -c "& $home\.claude\minusculo.ps1""#},
            ]}]},
            "env": {"PATH": r"C:\Users\ana\.claude\bin;%PATH%"},
        });
        std::fs::write(claude.join("settings.json"), config.to_string()).unwrap();
        let achados: Vec<(String, bool)> = executados_pela_configuracao_em(&claude, casa, true)
            .into_iter()
            .map(|(p, b)| (p.to_string_lossy().into_owned(), b))
            .collect();
        for esperado in [
            (r"C:\Users\ana\.claude\linha de status.ps1", false),
            (r"C:\Users\ana\.claude\gancho.ps1", false),
            (r"C:\Users\ana\.claude\adiado.sh", false),
            (r"C:\Users\ana\.claude\minusculo.ps1", false),
            (r"C:\Users\ana\.claude\bin", true),
        ] {
            assert!(achados.contains(&(esperado.0.to_string(), esperado.1)), "{esperado:?} em {achados:?}");
        }
        assert!(!achados.iter().any(|(p, _)| p.ends_with("log.txt")), "{achados:?}");
    }

    #[test]
    fn dado_e_o_que_o_gancho_le_ou_grava_e_script_inclui_os_do_windows() {
        let casa = Path::new("/casa");
        assert!(e_dado(&casa.join(".claude.json"), casa));
        assert!(e_dado(&casa.join(".claude.json.backup"), casa));
        assert!(e_dado(&casa.join(".claude/history.jsonl"), casa));
        assert!(!e_dado(&casa.join(".claude/regras.md"), casa));
        assert!(!e_dado(&casa.join("outra/.claude.json.sh"), casa));
        for script in ["a.sh", "b.py", "c.ps1", "d.CMD", "e.bat", "f.psm1"] {
            assert!(e_script(Path::new(script)), "{script}");
        }
        assert!(!e_script(Path::new("g.md")));
    }

    #[test]
    fn le_o_que_o_claude_md_importa() {
        let d = tempfile::tempdir().unwrap();
        let casa = d.path().join("casa");
        let claude = casa.join(".claude");
        std::fs::create_dir_all(claude.join("extra")).unwrap();
        let md = "Regras: @~/.claude/regras.md e @extra/b.md; @locais.md ainda não existe; escreva para ana@exemplo.com.\n";
        std::fs::write(claude.join("CLAUDE.md"), md).unwrap();
        std::fs::write(claude.join("regras.md"), "nada\n").unwrap();
        // O que um importado importa também vale, relativo à pasta dele, e com extensão de dado também.
        std::fs::write(claude.join("extra/b.md"), "@c.md @contexto.json\n").unwrap();
        std::fs::write(claude.join("extra/c.md"), "fim\n").unwrap();
        let mut achados = importados_pelo_claude_md(&claude, &casa);
        achados.sort();
        // O que ainda não existe também entra: criado pela IA presa, viraria instrução.
        let mut esperados = vec![
            claude.join("regras.md"),
            claude.join("extra/b.md"),
            claude.join("locais.md"),
            claude.join("extra/c.md"),
            claude.join("extra/contexto.json"),
        ];
        esperados.sort();
        assert_eq!(achados, esperados);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn os_scripts_da_configuracao_e_os_programas_fora_da_casa_ficam_so_para_ler() {
        let c = casa_de_mentira();
        std::fs::create_dir_all(c.claude.join("scripts")).unwrap();
        std::fs::write(c.claude.join("statusline-command.sh"), "#!/bin/sh\n").unwrap();
        std::fs::write(c.claude.join("scripts/a.py"), "").unwrap();
        std::fs::create_dir_all(c.claude.join("bin")).unwrap();
        std::fs::write(c.claude.join("bin/avisa"), "#!/bin/sh\n").unwrap();
        std::fs::write(c.casa.join(".claude.json"), "{}\n").unwrap();
        let config = serde_json::json!({
            "statusLine": {"type": "command",
                "command": "bash ~/.claude/statusline-command.sh 2>/dev/null; jq -r .x ~/.claude.json"},
            "hooks": {"Stop": [{"hooks": [
                {"type": "command",
                    "command": "python3 $HOME/.claude/scripts/a.py $HOME/.claude/todos/lista.json >> ~/.claude/gancho.log"},
                {"type": "command", "command": "~/.claude/bin/avisa | tee -a ~/.claude/avisos.log"},
                {"type": "command", "command": "tail -1 ~/.claude/history.jsonl"},
            ]}]},
        });
        std::fs::write(c.claude.join("settings.json"), config.to_string()).unwrap();
        let p = c.perfil(None);
        let cl = c.claude.display();
        let Some(trocou) = preso(&p, &format!("echo x >> '{cl}/statusline-command.sh'")) else { return };
        assert!(!trocou, "a IA presa trocou o script da linha de status");
        assert_eq!(preso(&p, &format!("echo x >> '{cl}/scripts/a.py'")), Some(false), "o script do gancho");
        assert_eq!(preso(&p, &format!("echo x > '{cl}/scripts/b.py'")), Some(false), "o vizinho dele");
        // Um executável sem extensão: nem ele, nem a troca pela pasta-mãe (renomear, trocar, devolver).
        assert_eq!(preso(&p, &format!("echo x >> '{cl}/bin/avisa'")), Some(false), "o executável");
        let troca = format!("mv '{cl}/bin' '{cl}/bin2' && echo x > '{cl}/bin2/avisa' && mv '{cl}/bin2' '{cl}/bin'");
        assert_eq!(preso(&p, &troca), Some(false), "a troca pela pasta-mãe");
        assert_eq!(std::fs::read_to_string(c.claude.join("bin/avisa")).unwrap(), "#!/bin/sh\n");
        assert_eq!(preso(&p, &format!("echo x > '{cl}/bin/outro'")), Some(true), "outro arquivo na mesma pasta");
        // Uma pasta de programas (`~/.claude/bin` no `PATH`): nada novo dentro dela; a de um cache citado continua
        // gravável por dentro; o resto continua do Claude.
        std::fs::create_dir_all(c.casa.join(".cache/uv")).unwrap();
        let env = r#"{"env":{"PATH":"~/.claude/bin:/usr/bin","UV_CACHE_DIR":"~/.cache/uv"}}"#;
        std::fs::write(c.claude.join("settings.local.json"), env).unwrap();
        let com_path = c.perfil(None);
        let ca = c.casa.display();
        assert_eq!(preso(&com_path, &format!("echo x > '{cl}/bin/git'")), Some(false), "um programa novo no PATH");
        assert_eq!(preso(&com_path, &format!("echo x > '{ca}/.cache/uv/pacote'")), Some(true), "o cache citado");
        assert_eq!(preso(&com_path, &format!("echo x > '{cl}/solto.txt'")), Some(true), "fora da pasta citada");
        std::fs::remove_file(c.claude.join("settings.local.json")).unwrap();
        // O que o `CLAUDE.md` importa: instrução de toda sessão, só para ler, exista ou não, seja `.md` ou `.json`.
        std::fs::write(c.claude.join("regras.md"), "seja breve\n").unwrap();
        std::fs::write(c.claude.join("CLAUDE.md"), "@~/.claude/regras.md @~/.claude/locais.md @~/.claude/ctx.json\n").unwrap();
        let com_import = c.perfil(None);
        assert_eq!(preso(&com_import, &format!("echo x >> '{cl}/regras.md'")), Some(false), "o importado");
        assert_eq!(preso(&com_import, &format!("echo x > '{cl}/locais.md'")), Some(false), "o que ainda não existe");
        assert_eq!(preso(&com_import, &format!("echo x > '{cl}/ctx.json'")), Some(false), "o de extensão de dado");
        std::fs::write(c.claude.join("CLAUDE.md"), "{}\n").unwrap();
        assert_eq!(preso(&p, &format!("echo '{{}}' >> '{cl}/history.jsonl'")), Some(true), "o resto continua do Claude");
        // Onde a configuração grava, o que ela só lê e o estado do CLI continuam graváveis.
        assert_eq!(preso(&p, "echo x > /dev/null"), Some(true), "/dev/null");
        assert_eq!(preso(&p, &format!("echo x >> '{cl}/gancho.log'")), Some(true), "o log do gancho");
        assert_eq!(preso(&p, &format!("echo x >> '{cl}/avisos.log'")), Some(true), "o log do tee");
        assert_eq!(preso(&p, &format!("echo '[]' > '{cl}/todos/outra.json'")), Some(true), "a pasta todos/");
        assert_eq!(preso(&p, &format!("echo '[]' > '{cl}/todos/lista.json'")), Some(true), "o dado que o gancho lê");
        assert_eq!(preso(&p, &format!("echo '{{}}' >> '{cl}/history.jsonl'")), Some(true), "o histórico");
        let ca = c.casa.display();
        let json = format!("echo '{{}}' > '{ca}/.claude.json.tmp.1' && mv '{ca}/.claude.json.tmp.1' '{ca}/.claude.json'");
        assert_eq!(preso(&p, &json), Some(true), "o ~/.claude.json, gravado por temporário");
        // Um script citado dentro do que volta depois (o armazém da aba, a pasta dos transcritos) também fica só para
        // ler; o resto dessas pastas continua do Claude preso.
        let aba = c.claude.join("contas/abas/aba");
        std::fs::write(c.projeto.join("gancho.sh"), "").unwrap();
        std::fs::write(aba.join("ajuda.sh"), "").unwrap();
        let (pr, ab) = (c.projeto.display().to_string(), aba.display().to_string());
        // E um fora da casa, onde a IA presa grava (como o `/tmp`), e outro que ainda não existe, citado por um link
        // (como `/tmp`, que é `/private/tmp`): criá-lo pelo caminho real também é recusado.
        let fora = tempfile::tempdir().unwrap();
        let fo = fora.path().canonicalize().unwrap().display().to_string();
        std::fs::write(format!("{fo}/gancho.py"), "").unwrap();
        std::fs::create_dir_all(format!("{fo}/real")).unwrap();
        std::os::unix::fs::symlink(format!("{fo}/real"), format!("{fo}/link")).unwrap();
        let config = serde_json::json!({"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": format!("sh {pr}/gancho.sh")},
            {"type": "command", "command": format!("sh {ab}/ajuda.sh")},
            {"type": "command", "command": format!("python3 {fo}/gancho.py 2>/dev/null; python3 {fo}/link/novo.py")},
        ]}]}});
        std::fs::write(c.claude.join("settings.json"), config.to_string()).unwrap();
        let com_aba = c.perfil(Some(&aba));
        assert_eq!(preso(&com_aba, &format!("echo x >> '{pr}/gancho.sh'")), Some(false), "na pasta dos transcritos");
        assert_eq!(preso(&com_aba, &format!("echo x >> '{ab}/ajuda.sh'")), Some(false), "no armazém da aba");
        assert_eq!(preso(&com_aba, &format!("echo x >> '{fo}/gancho.py'")), Some(false), "fora da casa");
        assert_eq!(preso(&com_aba, &format!("echo x > '{fo}/real/novo.py'")), Some(false), "o que ainda não existe");
        let resto = format!(
            "echo '{{}}' >> '{pr}/t.jsonl' && mkdir '{ab}/.trava' && rmdir '{ab}/.trava' && echo x > '{fo}/outro' \
             && echo x > /dev/null"
        );
        assert_eq!(preso(&com_aba, &resto), Some(true), "o resto do armazém, dos transcritos e da pasta de fora");
        // Um programa instalado fora da casa (o Homebrew, um app): onde a pessoa grava sem sandbox, a IA presa não.
        for dir in ["/opt/homebrew", "/Applications", "/usr/local", "/Library"] {
            let sonda = format!("{dir}/.keep-teste-{}", std::process::id());
            let cria = format!("mkdir '{sonda}' && rmdir '{sonda}'");
            if !std::process::Command::new("/bin/sh").args(["-c", &cria]).status().unwrap().success() {
                continue;
            }
            assert_eq!(preso(&p, &cria), Some(false), "{dir}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn com_a_casa_do_claude_num_link_o_destino_tambem_fica_so_para_ler() {
        let c = casa_de_mentira();
        // `~/.claude` vira um link para uma pasta de fora (que a negação da casa de mentira não cobre), sem os itens
        // que muitas casas ainda não têm.
        let fora = tempfile::tempdir().unwrap();
        let real = fora.path().canonicalize().unwrap().join("claude");
        std::fs::rename(&c.claude, &real).unwrap();
        std::os::unix::fs::symlink(&real, &c.claude).unwrap();
        std::fs::remove_dir_all(real.join("commands")).unwrap();
        std::fs::remove_file(real.join("settings.local.json")).unwrap();
        let p = c.perfil(None);
        let r = real.display();
        let Some(criou) = preso(&p, &format!("mkdir '{r}/commands'")) else { return };
        assert!(!criou, "a IA presa criou commands/ no destino do link");
        // Um item protegido que é um link quebrado: o destino, que gravar pelo link criaria, também é negado.
        let solto = fora.path().canonicalize().unwrap().join("solto.json");
        std::os::unix::fs::symlink(&solto, real.join("settings.local.json")).unwrap();
        let com_link = c.perfil(None);
        assert_eq!(preso(&com_link, &format!("echo '{{}}' > '{}/settings.local.json'", r)), Some(false), "pelo link");
        assert_eq!(preso(&com_link, &format!("echo '{{}}' > '{}'", solto.display())), Some(false), "no destino");
        std::fs::remove_file(real.join("settings.local.json")).unwrap();
        assert_eq!(preso(&p, &format!("echo '{{}}' > '{r}/settings.local.json'")), Some(false));
        assert_eq!(preso(&p, &format!("echo x > '{r}/hooks/novo.sh'")), Some(false));
        assert_eq!(preso(&p, &format!("echo '{{}}' >> '{r}/history.jsonl'")), Some(true), "o resto continua do Claude");
        // O armazém da aba dado pelo caminho real, com a casa pelo link: liberado do mesmo jeito.
        let aba = real.join("contas").join("abas").join("aba");
        assert!(!armazem_da_aba(&c.claude.join("contas"), &aba).is_empty());
    }
}
