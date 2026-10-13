//! A IA presa a uma pasta no Windows.
//!
//! No macOS o `sandbox-exec` diz, por caminho, o que um programa pode tocar
//! (`confinar.rs`). O Windows não tem isso: quem decide é a lista de
//! permissões (DACL) de cada arquivo, contra o token do processo. Aqui o
//! programa roda com um token RESTRITO (`CreateRestrictedToken`), que faz uma
//! segunda checagem de acesso: além de permitir ao usuário, o objeto precisa
//! permitir a um dos SIDs de restrição do token. Os SIDs de restrição são só
//! os do sistema que qualquer programa e o console usam (Todos, Restrito,
//! Usuários, o da sessão de logon) e dois do Keep, que as pastas permitidas
//! recebem de propósito. O perfil do usuário só dá acesso ao usuário: não
//! passa na segunda checagem, nem para ler nem para gravar.
//!
//! Os do sistema vêm em dois níveis (`Nivel`): o compatível (a lista do
//! USER_LIMITED do Chromium) e o amplo (com os grupos de toda sessão
//! interativa). O primeiro que deixa o programa partir vale. Os programas
//! comuns não partem de um token restrito como o Chromium parte os dele (que
//! começa a thread inicial com um token folgado e o aperta depois): por isso
//! os níveis, e por isso a prova abaixo, que vale para os dois. Não há nível
//! mais fechado, com os pacotes de aplicativo (`S-1-15-2-1`, que dá leitura
//! em `C:\Windows` a quem não é o usuário): `CreateRestrictedToken` os recusa
//! como SID de restrição (erro 87, visto no Windows do CI). Sem eles, o que
//! deixa um token restrito ler `C:\Windows` é `Usuários`.
//!
//! Por que token restrito e não AppContainer. O AppContainer isola a rede:
//! sem uma isenção que só o administrador concede, o `localhost` some (o
//! servidor de desenvolvimento que a IA sobe, MCP local, o retorno do login),
//! o que o sandbox do macOS não faz. Ele também troca o espaço de nomes de
//! objetos e o registro, de que o Git Bash (MSYS2), que o Claude Code usa
//! para o Bash, depende. E o console da aba (ConPTY) é herdado por um filho
//! do mesmo usuário, na mesma sessão, com a mesma integridade, sem mudar mais
//! nada: é o menor desvio do processo comum.
//!
//! Como:
//!
//! 1. As pastas que a IA precisa (a pasta escolhida e o git dela, a casa do
//!    Claude Code e do Codex, o estado, a área privada do Keep) recebem uma
//!    permissão para um SID só do Keep. A pasta escolhida tem o SID dela
//!    (derivado do caminho): trocar de pasta não leva a permissão antiga
//!    junto. O resto da lista usa um SID base, do usuário.
//! 2. O token restrito leva esses SIDs, perde os privilégios e os grupos
//!    administrativos, e ganha uma DACL padrão com o usuário, o sistema e o
//!    SID da pasta (senão um objeto criado por um processo preso não abre para
//!    o irmão dele; sem o SID base, o de um workspace não abre para o de outro).
//! 3. O processo nasce suspenso, entra num Job que o mata junto com este
//!    processo, ganha acesso ao próprio token (quase todo programa pergunta
//!    ao token quem ele é) e só então anda. O console da aba passa por
//!    herança, e é conferido que chegou.
//!
//! Falha FECHADA, como no macOS: sem pasta, sem permissão para preparar a
//! pasta, sem token ou sem processo, a IA não sobe. Antes de subir, o token é
//! posto à prova numa thread que se passa por ele (`sondar_fs`): o que está
//! fora tem de ser negado e a pasta tem de funcionar; um sandbox que não
//! contém não é deixado passar calado.
//!
//! O que NÃO contém: o que o Windows deixa qualquer um ler (`C:\Windows`,
//! `Program Files`, o que `Usuários` lê em outras unidades nos níveis que o
//! levam, `C:\Users\Public`), as pastas de rede (o servidor confere o
//! usuário, não o token), o registro do sistema, a área de trabalho (o
//! processo preso divide a do usuário e pode mandar mensagens e teclas às
//! outras janelas) e o que um serviço faz em nome do usuário (abrir um
//! programa pelo shell, o WSL), como no macOS o `open`. O perfil do usuário e
//! o que está nele ficam de fora.
//!
//! Esta parte não chama o Windows: caminhos, SIDs, a lista de permissões, a
//! linha de comando e o ambiente, para os testes rodarem em qualquer sistema.
//! O que fala com o Windows está em `sistema` (só no Windows).

use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;

#[cfg(windows)]
pub mod executar;
#[cfg(windows)]
mod sistema;

/// Onde a função `claude` do PowerShell se encaixa: o `keep ia confinar --`
/// é transparente (sem pasta para o workspace, roda o programa como sempre),
/// então a função pode passar por ele sempre que `KEEP_WORKSPACE` existe.
///
/// ```text
/// & $keep ia confinar -- $claude @args
/// ```
///
/// no lugar do `& $claude @args` final. A função é da fatia `ai`
/// (`crates/keepd/src/workspace.rs`); o `ENCAIXE_POWERSHELL` fica aqui para
/// quem a escreve ter o texto exato e um teste que o segura.
pub const ENCAIXE_POWERSHELL: &str = "& $keep ia confinar -- $claude @args";

// ------------------------------------------------------------------ SIDs

/// SIDs conhecidos do Windows que o token restrito leva.
pub const SID_TODOS: &str = "S-1-1-0";
pub const SID_RESTRITO: &str = "S-1-5-12";
pub const SID_USUARIOS: &str = "S-1-5-32-545";
pub const SID_AUTENTICADOS: &str = "S-1-5-11";
pub const SID_INTERATIVO: &str = "S-1-5-4";
pub const SID_LOCAL: &str = "S-1-2-0";
pub const SID_CONSOLE: &str = "S-1-2-1";
pub const SID_SISTEMA: &str = "S-1-5-18";

/// Um SID só do Keep, derivado de `usuario` (o SID do usuário) e de um
/// rótulo: o mesmo par dá sempre o mesmo SID, outro par dá outro. Na forma do
/// SID de restrição aleatório do Chromium: autoridade nula (`S-1-0`, que só
/// tem o `S-1-0-0`) e quatro sub-autoridades de 32 bits. Nunca coincide com
/// um grupo ou usuário de verdade, nem desperta uma consulta ao domínio.
pub fn sid_do_keep(usuario: &str, rotulo: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"keep-confine\0");
    h.update(usuario.as_bytes());
    h.update(b"\0");
    h.update(rotulo.as_bytes());
    let d = h.finalize();
    let n = |i: usize| u32::from_le_bytes([d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]]);
    format!("S-1-0-{}-{}-{}-{}", n(0), n(1), n(2), n(3))
}

/// O SID de uma pasta presa: o mesmo caminho, escrito como for (caixa,
/// barras), dá o mesmo SID.
pub fn sid_da_pasta(usuario: &str, pasta: &str) -> String {
    sid_do_keep(usuario, &format!("pasta:{}", partes(pasta).join("\\")))
}

/// O SID das permissões que valem para qualquer pasta presa.
pub fn sid_base(usuario: &str) -> String {
    sid_do_keep(usuario, "base")
}

/// Quanto do sistema o token restrito enxerga além do que o Keep concede. O
/// primeiro nível que deixa o programa partir é o que vale; a prova do token
/// vale para os dois (o perfil do usuário fica negado em ambos).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nivel {
    /// A lista do USER_LIMITED do Chromium (que o Firefox e o Chrome usam):
    /// `Usuários` lê os arquivos do sistema. Lê também o que `Usuários` lê em
    /// outras unidades.
    Compativel,
    /// O compatível mais os grupos que todo token de uma sessão interativa
    /// leva (autenticados, interativo, local, console): para o computador em
    /// que algum objeto do console ou da sessão só os nomeia. Também grava
    /// onde os autenticados gravam (a raiz de outras unidades).
    Amplo,
}

/// Os níveis, do mais fechado ao mais aberto.
pub const NIVEIS: [Nivel; 2] = [Nivel::Compativel, Nivel::Amplo];

/// Os SIDs de restrição do token de um nível: o da pasta, o base, o da sessão
/// de logon (sem ele o processo não abre a estação de janelas) e os do
/// sistema. O do usuário e o dos administradores nunca entram: seriam o
/// perfil de volta.
pub fn sids_de_restricao(nivel: Nivel, da_pasta: &str, base: &str, logon: &str) -> Vec<String> {
    let mut v = vec![
        da_pasta.to_string(),
        base.to_string(),
        logon.to_string(),
        SID_TODOS.to_string(),
        SID_RESTRITO.to_string(),
        SID_USUARIOS.to_string(),
    ];
    if nivel == Nivel::Amplo {
        v.extend([SID_AUTENTICADOS, SID_INTERATIVO, SID_LOCAL, SID_CONSOLE].map(String::from));
    }
    v
}

// ------------------------------------------------------------------ caminhos

/// Os componentes de um caminho do Windows, em minúsculas (o sistema de
/// arquivos não distingue caixa), separados por `\` ou `/`. Um caminho de
/// rede (`\\servidor\pasta`) começa por `\\`.
fn partes(p: &str) -> Vec<String> {
    let p = sem_prefixo(p);
    let rede = p.starts_with("\\\\") || p.starts_with("//");
    let mut v: Vec<String> =
        p.split(['\\', '/']).filter(|s| !s.is_empty() && *s != ".").map(|s| s.to_lowercase()).collect();
    if rede {
        if let Some(primeiro) = v.first_mut() {
            *primeiro = format!("\\\\{primeiro}");
        }
    }
    v
}

/// O caminho sem o prefixo de caminho literal que `canonicalize` devolve no
/// Windows: `\\?\C:\x` é `C:\x`, e `\\?\UNC\servidor\pasta` é
/// `\\servidor\pasta`. As chamadas de permissão e as pastas que a IA vê
/// usam a forma comum.
pub fn sem_prefixo(p: &str) -> String {
    if let Some(resto) = p.strip_prefix("\\\\?\\UNC\\") {
        return format!("\\\\{resto}");
    }
    if let Some(resto) = p.strip_prefix("\\\\?\\") {
        return resto.to_string();
    }
    p.to_string()
}

/// `base\resto`, com uma barra só entre eles.
pub fn junta(base: &str, resto: &str) -> String {
    format!("{}\\{}", base.trim_end_matches(['\\', '/']), resto.trim_start_matches(['\\', '/']).replace('/', "\\"))
}

/// Se `filho` é `pai` ou está dentro dele.
pub fn dentro_de(pai: &str, filho: &str) -> bool {
    let (p, f) = (partes(pai), partes(filho));
    !p.is_empty() && f.starts_with(&p)
}

/// A pasta que contém o caminho, se ele tem uma.
pub fn pasta_de(p: &str) -> Option<String> {
    let p = sem_prefixo(p);
    let corte = p.trim_end_matches(['\\', '/']).rfind(['\\', '/'])?;
    let pasta = &p[..corte];
    // `C:` sozinho é a unidade de trabalho do shell, não a raiz dela.
    Some(if pasta.ends_with(':') { format!("{pasta}\\") } else { pasta.to_string() }).filter(|s| !s.is_empty())
}

/// O nome do arquivo, sem a pasta.
pub fn nome_de(p: &str) -> &str {
    p.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(p)
}

/// Por que a pasta não serve de limite para a IA, se não serve: a raiz de uma
/// unidade ou de um compartilhamento, a casa do usuário (ou algo que a
/// contém) e as pastas do sistema. Dar uma permissão a elas atravessaria o
/// perfil ou o sistema inteiro, o que não prende nada e leva minutos.
pub fn pasta_larga_demais(pasta: &str, casa: &str, sistema: &[String]) -> Option<String> {
    let p = partes(pasta);
    let rede = p.first().is_some_and(|s| s.starts_with("\\\\"));
    if p.len() <= if rede { 2 } else { 1 } {
        return Some("a raiz de uma unidade".into());
    }
    if dentro_de(pasta, casa) {
        return Some("a pasta do seu usuário (ou uma que a contém)".into());
    }
    if sistema.iter().any(|s| dentro_de(pasta, s) || dentro_de(s, pasta)) {
        return Some("uma pasta do sistema".into());
    }
    None
}

// ------------------------------------------------------------------ o que a IA precisa tocar

/// O que se dá a um SID em cada lugar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acesso {
    /// Ler e executar.
    Leitura,
    /// Ler, gravar, executar e apagar (o "Modificar" do Explorer).
    Gravacao,
}

impl Acesso {
    /// A máscara de acesso a arquivo: `FILE_GENERIC_READ | FILE_GENERIC_EXECUTE`
    /// e, para gravar, `FILE_GENERIC_WRITE` e `DELETE`. Sem `WRITE_DAC` nem
    /// `WRITE_OWNER`: a IA não reescreve permissão de ninguém.
    pub fn mascara(self) -> u32 {
        match self {
            Acesso::Leitura => 0x0012_00A9,
            Acesso::Gravacao => 0x0013_01BF,
        }
    }
}

/// A quem a permissão é dada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quem {
    /// O SID da pasta presa: só o token dessa pasta a tem.
    Pasta,
    /// O SID base: vale para todo token do Keep deste usuário.
    Base,
}

/// Uma permissão a dar antes de a IA subir.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concessao {
    pub caminho: String,
    pub acesso: Acesso,
    pub quem: Quem,
    /// Uma pasta: o que está nela e o que vier a nascer nela.
    pub heranca: bool,
    /// Criar a pasta se não existir: a IA a usa e não tem como criá-la.
    pub criar: bool,
    /// Sem ela a IA não sobe. As outras, se falharem, só ficam fora.
    pub obrigatoria: bool,
}

/// Qual IA sobe: o que ela precisa criar na casa muda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ia {
    Claude,
    Codex,
    Outro,
}

/// Pelo nome do programa, sem a pasta e sem `.exe`/`.cmd`.
pub fn ia_de(programa: &str) -> Ia {
    let nome = nome_de(programa).to_lowercase();
    let tronco = nome.rsplit_once('.').map(|(t, _)| t).unwrap_or(&nome);
    match tronco {
        "claude" => Ia::Claude,
        "codex" => Ia::Codex,
        _ => Ia::Outro,
    }
}

/// O que o plano precisa saber do computador, para os testes o darem sem ele.
pub trait Ambiente {
    fn var(&self, nome: &str) -> Option<String>;
    fn existe(&self, caminho: &str) -> bool;
}

/// De onde sai o plano.
#[derive(Clone, Debug)]
pub struct Entrada {
    /// A pasta escolhida, já real e sem o prefixo literal.
    pub pasta: String,
    /// As pastas do git que uma worktree usa e que ficam fora dela.
    pub pastas_git: Vec<String>,
    /// O programa, com o caminho inteiro.
    pub programa: String,
    pub casa: String,
    /// `%APPDATA%` e `%LOCALAPPDATA%`.
    pub dados: Option<String>,
    pub local: Option<String>,
    /// O `keep.exe` em uso: a IA o chama (a linha de status, os ganchos).
    pub keep: Option<String>,
    /// O `git.exe` (ou o `bash.exe`) que o Claude Code usará, se achado.
    pub git: Option<String>,
    /// O `node.exe` de que um atalho `.cmd` do npm precisa, se achado.
    pub node: Option<String>,
    /// `%SystemRoot%`, `%ProgramFiles%` e afins: já legíveis, e não são do
    /// usuário para ter a permissão mexida.
    pub sistema: Vec<String>,
}

/// O que dar a quem antes de a IA subir, e a pasta que ela usa como `%TEMP%`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plano {
    pub concessoes: Vec<Concessao>,
    /// Pastas a criar antes de conceder (a IA não as cria sozinha).
    pub criar: Vec<String>,
    pub temp: String,
}

/// A área do Keep que só a IA presa usa: o `%TEMP%` dela. Fica na área local
/// do Keep e não no `%TEMP%` do usuário, que mistura os arquivos de tudo.
pub fn area_privada(local: &str) -> String {
    junta(local, "Keep\\confine")
}

/// A lista do macOS (`confinar::perfil`) no Windows: o que a IA lê e grava, o
/// que só lê e o que precisa existir. O que não está aqui, o token não toca.
pub fn plano(amb: &dyn Ambiente, e: &Entrada) -> Plano {
    let ia = ia_de(&e.programa);
    let local = e.local.clone().unwrap_or_else(|| junta(&e.casa, "AppData\\Local"));
    let sob_a_casa = |p: &str| dentro_de(&e.casa, p);
    let do_sistema = |p: &str| e.sistema.iter().any(|s| dentro_de(s, p));
    let mut concessoes: Vec<Concessao> = Vec::new();
    let mut criar: Vec<String> = Vec::new();
    let mut poe = |c: Concessao| {
        if !concessoes.iter().any(|x| partes(&x.caminho) == partes(&c.caminho) && x.quem == c.quem && x.acesso == c.acesso) {
            concessoes.push(c);
        }
    };
    let grava = |caminho: String, quem: Quem, heranca: bool, criar: bool, obrigatoria: bool| Concessao {
        caminho,
        acesso: Acesso::Gravacao,
        quem,
        heranca,
        criar,
        obrigatoria,
    };
    let le = |caminho: String, heranca: bool| Concessao {
        caminho,
        acesso: Acesso::Leitura,
        quem: Quem::Base,
        heranca,
        criar: false,
        obrigatoria: false,
    };

    // A pasta escolhida e o git dela que fica fora.
    poe(grava(e.pasta.clone(), Quem::Pasta, true, false, true));
    for g in &e.pastas_git {
        if amb.existe(g) && !dentro_de(&e.pasta, g) {
            poe(grava(g.clone(), Quem::Pasta, true, false, true));
        }
    }

    // A área privada (o %TEMP% da IA) e a casa do Claude Code e do Codex.
    let privada = area_privada(&local);
    criar.push(junta(&privada, "tmp"));
    poe(grava(privada.clone(), Quem::Base, true, true, true));
    let casa_da_ia = |sub: &str, qual: Ia| {
        let p = junta(&e.casa, sub);
        (amb.existe(&p) || ia == qual).then_some(p)
    };
    if let Some(p) = casa_da_ia(".claude", Ia::Claude) {
        if !amb.existe(&p) {
            criar.push(p.clone());
        }
        poe(grava(p, Quem::Base, true, false, ia == Ia::Claude));
    }
    if let Some(p) = casa_da_ia(".codex", Ia::Codex) {
        if !amb.existe(&p) {
            criar.push(p.clone());
        }
        poe(grava(p, Quem::Base, true, false, ia == Ia::Codex));
    }
    // O estado: onde a linha de status do Keep grava o modelo e o esforço.
    let estado = junta(&e.casa, ".local\\state");
    if ia == Ia::Claude {
        criar.push(junta(&estado, "keep\\claude"));
    }
    if amb.existe(&estado) || ia == Ia::Claude {
        poe(grava(estado, Quem::Base, true, false, false));
    }
    let cache = junta(&e.casa, ".cache");
    if amb.existe(&cache) {
        poe(grava(cache, Quem::Base, true, false, false));
    }
    // Quem muda a casa da IA por variável (a conta da aba, o Codex extra).
    for var in ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "CLAUDE_SECURESTORAGE_CONFIG_DIR"] {
        if let Some(p) = amb.var(var).filter(|v| !v.trim().is_empty()) {
            let p = sem_prefixo(&p);
            if amb.existe(&p) && !do_sistema(&p) && !dentro_de(&e.pasta, &p) {
                poe(grava(p, Quem::Base, true, false, false));
            }
        }
    }
    // A configuração global do Claude Code: dois arquivos, não a casa.
    for arq in [".claude.json", ".claude.json.backup"] {
        let p = junta(&e.casa, arq);
        if amb.existe(&p) {
            poe(grava(p, Quem::Base, false, false, false));
        }
    }

    // Só leitura: os programas, a configuração do git, o próprio Keep.
    for sub in [".local\\bin", ".local\\share\\claude", ".config\\git"] {
        let p = junta(&e.casa, sub);
        if amb.existe(&p) {
            poe(le(p, true));
        }
    }
    let gitconfig = junta(&e.casa, ".gitconfig");
    if amb.existe(&gitconfig) {
        poe(le(gitconfig, false));
    }
    // O Keep que a IA chama (a linha de status, os ganchos) e o script de
    // status que mora ao lado de `bin`: a pasta de instalação, se o exe está
    // numa `bin`.
    if let Some(keep) = &e.keep {
        let instalacao = pasta_de(keep).map(|d| match (nome_de(&d).eq_ignore_ascii_case("bin"), pasta_de(&d)) {
            (true, Some(raiz)) => raiz,
            _ => d,
        });
        if let Some(d) = instalacao.filter(|d| amb.existe(d) && !do_sistema(d)) {
            poe(le(d, true));
        }
    }
    if let Some(dados) = &e.dados {
        let npm = junta(dados, "npm");
        if amb.existe(&npm) {
            poe(le(npm, true));
        }
    }
    // O programa e, se é um atalho do npm, o node que ele chama; o Git for
    // Windows de um usuário só (o do sistema o Windows já deixa ler).
    let mut programas = vec![e.programa.clone()];
    programas.extend(e.node.clone());
    for p in programas {
        if let Some(d) =
            pasta_de(&p).filter(|d| amb.existe(d) && !do_sistema(d) && !dentro_de(&e.pasta, d) && sob_a_casa(d))
        {
            poe(le(d, true));
        }
    }
    if let Some(git) = &e.git {
        if let Some(raiz) = raiz_do_git(git).filter(|r| amb.existe(r) && sob_a_casa(r) && !do_sistema(r)) {
            poe(le(raiz, true));
        }
    }

    Plano { concessoes, criar, temp: junta(&privada, "tmp") }
}

/// A pasta de instalação do Git for Windows a que um `git.exe` ou `bash.exe`
/// pertence: até o componente `Git`, que o instalador usa. Sem ele, a pasta
/// do próprio arquivo e a de cima (`cmd\git.exe` e `bin\bash.exe`).
pub fn raiz_do_git(exe: &str) -> Option<String> {
    let exe = sem_prefixo(exe);
    let cortes: Vec<usize> = exe.match_indices(['\\', '/']).map(|(i, _)| i).collect();
    for &i in &cortes {
        let ate = &exe[..i];
        if nome_de(ate).eq_ignore_ascii_case("git") {
            return Some(ate.to_string());
        }
    }
    pasta_de(&pasta_de(&exe)?)
}

// ------------------------------------------------------------------ as permissões que foram dadas inteiras

/// A linha que diz que uma permissão numa pasta foi dada inteira. Dar uma
/// permissão a uma pasta é percorrer tudo o que há nela, e quem interrompe no
/// meio (Ctrl+C, a aba fechada) deixa a pasta com a permissão na raiz e sem
/// ela em parte do que está dentro: só a raiz não diz que acabou. A marca é
/// gravada quando acaba, fora do que a IA alcança.
pub fn marca(sid: &str, acesso: Acesso, heranca: bool, caminho: &str) -> String {
    format!("{sid}|{:08x}|{}|{}", acesso.mascara(), if heranca { "h" } else { "f" }, partes(caminho).join("\\"))
}

/// As marcas de um arquivo de marcas.
pub fn marcas_de(texto: &str) -> HashSet<String> {
    texto.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

// ------------------------------------------------------------------ procurar o programa

/// Onde o programa está: o caminho dado, ou o primeiro do `PATH` que o tem,
/// com as extensões de `PATHEXT` quando o nome não traz uma. A pasta atual
/// não entra: o PowerShell também não a procura.
pub fn procura_programa(nome: &str, path: &str, pathext: &str, existe: &dyn Fn(&str) -> bool) -> Option<String> {
    let exts: Vec<String> = pathext.split(';').filter(|e| !e.is_empty()).map(|e| e.to_lowercase()).collect();
    let tem_extensao = nome_de(nome).contains('.');
    let candidatos = |base: &str| -> Vec<String> {
        let mut v = vec![base.to_string()];
        if !tem_extensao {
            v.extend(exts.iter().map(|e| format!("{base}{e}")));
        }
        v
    };
    let com_pasta = nome.contains(['\\', '/']) || nome.get(1..2) == Some(":");
    let pastas: Vec<String> = if com_pasta {
        vec![String::new()]
    } else {
        path.split(';').map(|s| s.trim().trim_matches('"')).filter(|s| !s.is_empty()).map(String::from).collect()
    };
    for pasta in pastas {
        let base = if pasta.is_empty() { nome.to_string() } else { junta(&pasta, nome) };
        // Sem extensão o nome é de uma pasta ou de um atalho de shell que o
        // Windows não executa: só as extensões contam. Com extensão, o próprio.
        let lista = candidatos(&base);
        let lista = if tem_extensao { lista } else { lista.into_iter().skip(1).collect() };
        if let Some(achado) = lista.into_iter().find(|c| existe(c)) {
            return Some(achado);
        }
    }
    None
}

/// Se o programa é um script do cmd (`.cmd`, `.bat`): ele roda pelo `cmd.exe`.
pub fn eh_script_do_cmd(programa: &str) -> bool {
    let nome = nome_de(programa).to_lowercase();
    nome.ends_with(".cmd") || nome.ends_with(".bat")
}

// ------------------------------------------------------------------ a linha de comando e o ambiente

/// Um argumento como a rotina de partição de linha do Windows (a do C
/// runtime, a que o Node, o Bun e o Rust usam) o lê de volta igual.
fn anexa_arg(saida: &mut String, arg: &str) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{b}', '"']) {
        saida.push_str(arg);
        return;
    }
    saida.push('"');
    let mut barras = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => barras += 1,
            '"' => {
                saida.extend(std::iter::repeat_n('\\', barras * 2 + 1));
                saida.push('"');
                barras = 0;
            }
            _ => {
                saida.extend(std::iter::repeat_n('\\', barras));
                barras = 0;
                saida.push(c);
            }
        }
    }
    // As barras antes da aspa que fecha valem em dobro, senão a escapam.
    saida.extend(std::iter::repeat_n('\\', barras * 2));
    saida.push('"');
}

/// A linha de comando de um executável: o programa entre aspas se tem
/// espaço, e os argumentos como `anexa_arg`.
pub fn linha_de_comando(programa: &str, args: &[String]) -> String {
    let mut linha = String::new();
    if programa.contains([' ', '\t']) {
        linha.push('"');
        linha.push_str(programa);
        linha.push('"');
    } else {
        linha.push_str(programa);
    }
    for a in args {
        linha.push(' ');
        anexa_arg(&mut linha, a);
    }
    linha
}

/// A linha de comando de um script do cmd, e o programa que a roda:
/// `cmd.exe /d /s /c ""script" "arg" …"`. Dentro de aspas o cmd lê `&|<>^()`
/// como texto; o que ele ainda interpreta (`%`, `"` e as quebras de linha)
/// não tem escape que valha, e um argumento com isso é recusado — foi assim
/// que um argumento escolhido virava comando (CVE-2024-24576).
pub fn linha_de_script(cmd: &str, script: &str, args: &[String]) -> Result<String, String> {
    for a in args.iter().chain(std::iter::once(&script.to_string())) {
        if let Some(c) = a.chars().find(|c| matches!(c, '"' | '%' | '\r' | '\n' | '\0')) {
            return Err(format!(
                "o argumento {a:?} traz {c:?}, que o cmd.exe não deixa passar a um script .cmd com segurança; instale o programa nativo (.exe)"
            ));
        }
    }
    let mut interno = String::new();
    interno.push('"');
    interno.push_str(script);
    interno.push('"');
    for a in args {
        interno.push(' ');
        // Sempre entre aspas: dentro delas o cmd não lê nada do que sobra.
        interno.push('"');
        let barras = a.chars().rev().take_while(|c| *c == '\\').count();
        interno.push_str(a);
        interno.push_str(&"\\".repeat(barras));
        interno.push('"');
    }
    let cmd_citado = if cmd.contains([' ', '\t']) { format!("\"{cmd}\"") } else { cmd.to_string() };
    Ok(format!("{cmd_citado} /d /s /c \"{interno}\""))
}

/// O bloco de ambiente de um `CreateProcess` em UTF-16: `NOME=valor\0` por
/// variável, em ordem (o Windows pede sem distinguir caixa), e um `\0` a mais.
/// Uma variável posta duas vezes vale pela última.
pub fn bloco_de_ambiente(vars: &[(Vec<u16>, Vec<u16>)]) -> Vec<u16> {
    let chave = |n: &[u16]| -> String { String::from_utf16_lossy(n).to_uppercase() };
    let mut unicas: Vec<&(Vec<u16>, Vec<u16>)> = Vec::new();
    for v in vars.iter().rev() {
        if !unicas.iter().any(|u| chave(&u.0) == chave(&v.0)) {
            unicas.push(v);
        }
    }
    unicas.sort_by_key(|v| chave(&v.0));
    let mut saida: Vec<u16> = Vec::new();
    for (nome, valor) in unicas {
        saida.extend_from_slice(nome);
        saida.push(u16::from(b'='));
        saida.extend_from_slice(valor);
        saida.push(0);
    }
    if saida.is_empty() {
        saida.push(0);
    }
    saida.push(0);
    saida
}

/// O ambiente do programa preso: o do processo, com o `%TEMP%` na área
/// privada (o do usuário está fora do que ele alcança) e a pasta a que está
/// preso. Os que o Windows põe com `=` no nome (`=C:`) seguem.
pub fn ambiente_preso(
    atual: Vec<(Vec<u16>, Vec<u16>)>,
    temp: &str,
    pasta: &str,
) -> Vec<(Vec<u16>, Vec<u16>)> {
    let largo = |s: &str| -> Vec<u16> { s.encode_utf16().collect() };
    let muda: [(&str, &str); 4] = [("TEMP", temp), ("TMP", temp), ("TMPDIR", temp), ("KEEP_CONFINED_TO", pasta)];
    let mut v: Vec<(Vec<u16>, Vec<u16>)> =
        atual.into_iter().filter(|(n, _)| !muda.iter().any(|(m, _)| chave_igual(n, m))).collect();
    for (n, valor) in muda {
        v.push((largo(n), largo(valor)));
    }
    v
}

fn chave_igual(nome: &[u16], outro: &str) -> bool {
    String::from_utf16_lossy(nome).eq_ignore_ascii_case(outro)
}

// ------------------------------------------------------------------ a prova de que o token contém

/// Como uma tentativa de acessar o que está fora da pasta terminou.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Veredito {
    /// O sistema recusou o acesso: é o que se quer.
    Negado,
    /// Funcionou: a restrição não valeu.
    Permitido,
    /// Falhou por outro motivo: não prova nada.
    Inconclusivo(String),
}

fn veredito<T>(r: std::io::Result<T>) -> Veredito {
    match r {
        Ok(_) => Veredito::Permitido,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Veredito::Negado,
        Err(e) => Veredito::Inconclusivo(e.to_string()),
    }
}

/// O resultado da prova.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sonda {
    /// Criar, ler e apagar um arquivo na pasta permitida.
    pub dentro: Result<(), String>,
    /// Ler um arquivo de fora, listar uma pasta de fora e criar um arquivo
    /// em outra pasta de fora.
    pub le_fora: Veredito,
    pub lista_fora: Veredito,
    pub grava_fora: Veredito,
}

impl Sonda {
    /// Nada de fora acessível e a pasta permitida funcionando, ou o porquê.
    pub fn conclusao(&self) -> Result<(), String> {
        if let Err(e) = &self.dentro {
            return Err(format!("a pasta permitida não pôde ser usada pelo processo preso ({e})"));
        }
        for (o_que, v) in [
            ("ler um arquivo de fora da pasta", &self.le_fora),
            ("listar uma pasta de fora da pasta", &self.lista_fora),
            ("criar um arquivo fora da pasta", &self.grava_fora),
        ] {
            match v {
                Veredito::Negado => {}
                Veredito::Permitido => return Err(format!("o processo preso conseguiu {o_que}: a restrição não valeu")),
                Veredito::Inconclusivo(e) => {
                    return Err(format!("não foi possível provar a restrição ({o_que}: {e})"));
                }
            }
        }
        Ok(())
    }
}

/// A prova, feita por quem tem o token (a thread que se passa por ele): usa
/// a pasta permitida e tenta o que não deveria dar certo.
pub fn sondar_fs(dentro: &Path, canario: &Path, listar: &Path, onde_criar: &Path) -> Sonda {
    let marca = dentro.join(format!(".keep-sonda-{}", std::process::id()));
    let usar_dentro = || -> Result<(), String> {
        std::fs::write(&marca, b"keep").map_err(|e| format!("criar: {e}"))?;
        let lido = std::fs::read(&marca).map_err(|e| format!("ler: {e}"));
        let apagado = std::fs::remove_file(&marca).map_err(|e| format!("apagar: {e}"));
        if lido? != b"keep" {
            return Err("ler: o conteúdo voltou diferente".into());
        }
        apagado
    };
    let dentro = usar_dentro();
    let le_fora = veredito(std::fs::File::open(canario).and_then(|mut f| {
        use std::io::Read;
        let mut b = [0u8; 1];
        f.read(&mut b)
    }));
    let lista_fora = veredito(std::fs::read_dir(listar).map(|mut d| d.next().is_some()));
    let criado = onde_criar.join(format!(".keep-sonda-{}", std::process::id()));
    let grava_fora = veredito(std::fs::File::create(&criado));
    if grava_fora == Veredito::Permitido {
        // Quem criou limpa: a prova não deixa lixo nem quando falha.
        let _ = std::fs::remove_file(&criado);
    }
    Sonda { dentro, le_fora, lista_fora, grava_fora }
}

/// Onde pôr a isca da prova: o primeiro dos lugares que nenhuma permissão
/// alcança. Uma isca dentro de uma pasta permitida não provaria nada.
pub fn lugar_da_isca(candidatos: &[String], concessoes: &[Concessao]) -> Option<String> {
    candidatos.iter().find(|c| !concessoes.iter().any(|g| g.heranca && dentro_de(&g.caminho, c))).cloned()
}

// ------------------------------------------------------------------ o NTSTATUS de um processo que não nasceu

/// Se o código de saída de um processo que morreu logo é o de um que o
/// Windows não conseguiu iniciar: biblioteca que não carrega, acesso negado
/// ao carregá-la ou falha de inicialização. É o sinal de que o token deste
/// nível não serve ao programa, e de tentar o seguinte.
pub fn falha_de_partida(codigo: u32) -> bool {
    matches!(
        codigo,
        0xC000_0142 // STATUS_DLL_INIT_FAILED
        | 0xC000_0022 // STATUS_ACCESS_DENIED
        | 0xC000_0135 // STATUS_DLL_NOT_FOUND
        | 0xC000_0139 // STATUS_ENTRYPOINT_NOT_FOUND
        | 0xC000_007B // STATUS_INVALID_IMAGE_FORMAT
        | 0xC000_0034 // STATUS_OBJECT_NAME_NOT_FOUND
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::{HashMap, HashSet};

    struct Falso {
        vars: HashMap<String, String>,
        existem: HashSet<String>,
    }

    impl Falso {
        fn novo(existem: &[&str]) -> Falso {
            Falso { vars: HashMap::new(), existem: existem.iter().map(|s| partes(s).join("\\")).collect() }
        }
    }

    impl Ambiente for Falso {
        fn var(&self, nome: &str) -> Option<String> {
            self.vars.get(nome).cloned()
        }
        fn existe(&self, caminho: &str) -> bool {
            self.existem.contains(&partes(caminho).join("\\"))
        }
    }

    const CASA: &str = r"C:\Users\ana";

    fn entrada(programa: &str) -> Entrada {
        Entrada {
            pasta: r"D:\projetos\app".into(),
            pastas_git: vec![],
            programa: programa.into(),
            casa: CASA.into(),
            dados: Some(r"C:\Users\ana\AppData\Roaming".into()),
            local: Some(r"C:\Users\ana\AppData\Local".into()),
            keep: Some(r"C:\Users\ana\AppData\Local\Keep\bin\keep.exe".into()),
            git: None,
            node: None,
            sistema: vec![r"C:\Windows".into(), r"C:\Program Files".into(), r"C:\Program Files (x86)".into()],
        }
    }

    fn de(plano: &Plano, caminho: &str) -> Vec<Concessao> {
        plano.concessoes.iter().filter(|c| partes(&c.caminho) == partes(caminho)).cloned().collect()
    }

    #[test]
    fn o_sid_do_keep_e_estavel_e_distingue_pastas() {
        let a = sid_da_pasta("S-1-5-21-1-2-3-1001", r"D:\Projetos\App");
        assert_eq!(a, sid_da_pasta("S-1-5-21-1-2-3-1001", "d:/projetos/app/"));
        assert_ne!(a, sid_da_pasta("S-1-5-21-1-2-3-1001", r"D:\Projetos\Outro"));
        assert_ne!(a, sid_da_pasta("S-1-5-21-1-2-3-1002", r"D:\Projetos\App"));
        assert_ne!(a, sid_base("S-1-5-21-1-2-3-1001"));
        // A forma do SID aleatório de restrição do Chromium: S-1-0 e quatro de 32 bits.
        let partes: Vec<&str> = a.split('-').collect();
        assert_eq!(&partes[..3], ["S", "1", "0"]);
        assert_eq!(partes.len(), 7, "{a}");
        assert!(partes[3..].iter().all(|p| p.parse::<u32>().is_ok()), "{a}");
    }

    #[test]
    fn os_niveis_vao_do_compativel_ao_amplo_e_nenhum_traz_o_usuario() {
        let c = sids_de_restricao(Nivel::Compativel, "P", "B", "L");
        let a = sids_de_restricao(Nivel::Amplo, "P", "B", "L");
        for lista in [&c, &a] {
            assert_eq!(&lista[..6], ["P", "B", "L", SID_TODOS, SID_RESTRITO, SID_USUARIOS]);
        }
        // O compatível é a lista do Chromium; o amplo a acrescenta. Os pacotes
        // de aplicativo (S-1-15-2-*) não entram: o Windows os recusa (erro 87).
        assert_eq!(c.len(), 6);
        assert!(a.starts_with(&c));
        for s in [SID_AUTENTICADOS, SID_INTERATIVO, SID_LOCAL, SID_CONSOLE] {
            assert!(a.contains(&s.to_string()) && !c.contains(&s.to_string()), "{s}");
        }
        assert_eq!(NIVEIS, [Nivel::Compativel, Nivel::Amplo]);
        // O usuário (S-1-5-21-…) e os administradores nunca entram, em nível
        // nenhum: seriam o perfil de volta.
        for lista in [&c, &a] {
            assert!(!lista.iter().any(|s| s.starts_with("S-1-5-21-") || s == "S-1-5-32-544" || s.starts_with("S-1-15-")));
        }
    }

    #[test]
    fn caminhos_do_windows_se_comparam_sem_caixa_e_sem_prefixo() {
        assert_eq!(sem_prefixo(r"\\?\C:\Users\ana"), r"C:\Users\ana");
        assert_eq!(sem_prefixo(r"\\?\UNC\srv\pasta\x"), r"\\srv\pasta\x");
        assert_eq!(sem_prefixo(r"C:\x"), r"C:\x");
        assert!(dentro_de(r"C:\Users\Ana", r"c:/users/ana/Projetos"));
        assert!(dentro_de(r"C:\Users\Ana", r"C:\USERS\ANA"));
        assert!(!dentro_de(r"C:\Users\Ana", r"C:\Users\Ana2"));
        assert!(!dentro_de(r"C:\Users\Ana\x", r"C:\Users\Ana"));
        assert!(!dentro_de("", r"C:\x"));
        assert_eq!(junta(r"C:\a\", r"\b/c"), r"C:\a\b\c");
        assert_eq!(pasta_de(r"C:\a\b.exe").as_deref(), Some(r"C:\a"));
        assert_eq!(pasta_de(r"C:\b.exe").as_deref(), Some(r"C:\"));
        assert_eq!(pasta_de("b.exe"), None);
        assert_eq!(nome_de(r"C:\a\Claude.EXE"), "Claude.EXE");
        // Um caminho de rede começa por `\\`: a raiz dele é o compartilhamento.
        assert!(!dentro_de(r"\\srv\a", r"C:\srv\a\b"));
    }

    #[test]
    fn pasta_larga_demais_recusa_a_raiz_a_casa_e_o_sistema() {
        let sistema: Vec<String> = vec![r"C:\Windows".into(), r"C:\Program Files".into()];
        let r = |p: &str| pasta_larga_demais(p, CASA, &sistema);
        assert!(r(r"C:\").is_some());
        assert!(r(r"D:\").is_some());
        assert!(r("D:").is_some());
        assert!(r(r"\\srv\pasta").is_some());
        assert!(r(r"C:\Users\ana").is_some());
        assert!(r(r"C:\Users").is_some());
        assert!(r(r"C:\Windows").is_some());
        assert!(r(r"C:\Windows\System32").is_some());
        assert!(r(r"C:\Program Files\Git").is_some());
        assert!(r(r"C:\Users\ana\Projetos\app").is_none());
        assert!(r(r"D:\projetos\app").is_none());
        assert!(r(r"D:\projetos").is_none());
        assert!(r(r"\\srv\pasta\app").is_none());
    }

    #[test]
    fn a_linha_de_comando_volta_igual_pela_regra_do_c() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(linha_de_comando(r"C:\a\claude.exe", &args(&["--effort", "max"])), r"C:\a\claude.exe --effort max");
        assert_eq!(linha_de_comando(r"C:\Arquivos de Programa\c.exe", &args(&["a b"])), r#""C:\Arquivos de Programa\c.exe" "a b""#);
        assert_eq!(linha_de_comando("p.exe", &args(&[""])), r#"p.exe """#);
        assert_eq!(linha_de_comando("p.exe", &args(&[r#"diz "oi""#])), r#"p.exe "diz \"oi\"""#);
        // Barras antes de uma aspa dobram, e as do fim de um argumento entre aspas também.
        assert_eq!(linha_de_comando("p.exe", &args(&[r#"a\"b"#])), r#"p.exe "a\\\"b""#);
        assert_eq!(linha_de_comando("p.exe", &args(&[r"C:\pasta com espaço\"])), r#"p.exe "C:\pasta com espaço\\""#);
        assert_eq!(linha_de_comando("p.exe", &args(&[r"C:\sem\espaco\"])), r"p.exe C:\sem\espaco\");
    }

    #[test]
    fn um_script_do_cmd_recusa_o_que_o_cmd_interpreta() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            linha_de_script(r"C:\Windows\System32\cmd.exe", r"C:\npm\claude.cmd", &args(&["--effort", "max", "a & b"])).unwrap(),
            r#"C:\Windows\System32\cmd.exe /d /s /c ""C:\npm\claude.cmd" "--effort" "max" "a & b"""#
        );
        assert_eq!(
            linha_de_script("cmd.exe", r"C:\Meus Programas\x.cmd", &args(&[r"C:\p\"])).unwrap(),
            r#"cmd.exe /d /s /c ""C:\Meus Programas\x.cmd" "C:\p\\"""#
        );
        for ruim in ["%PATH%", "a\"b", "linha\nnova"] {
            assert!(linha_de_script("cmd.exe", r"C:\x.cmd", &args(&[ruim])).is_err(), "{ruim:?}");
        }
        assert!(linha_de_script("cmd.exe", "C:\\100%\\x.cmd", &[]).is_err());
    }

    #[test]
    fn o_bloco_de_ambiente_vai_em_ordem_e_termina_em_dois_nulos() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let vars = vec![(w("b"), w("2")), (w("A"), w("1")), (w("=C:"), w(r"C:\x")), (w("B"), w("3"))];
        let bloco = bloco_de_ambiente(&vars);
        let texto = String::from_utf16(&bloco).unwrap();
        // `=C:` vem antes das letras; a segunda `B` (a última posta) vence a `b`.
        assert_eq!(texto, "=C:=C:\\x\0A=1\0B=3\0\0");
        assert_eq!(bloco_de_ambiente(&[]), vec![0, 0]);
    }

    #[test]
    fn o_ambiente_preso_troca_o_temp_e_diz_a_pasta() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let atual = vec![(w("Temp"), w(r"C:\Users\ana\AppData\Local\Temp")), (w("PATH"), w("x")), (w("=C:"), w(r"C:\p"))];
        let v = ambiente_preso(atual, r"C:\priv\tmp", r"D:\app");
        let texto = String::from_utf16(&bloco_de_ambiente(&v)).unwrap();
        assert_eq!(
            texto,
            "=C:=C:\\p\0KEEP_CONFINED_TO=D:\\app\0PATH=x\0TEMP=C:\\priv\\tmp\0TMP=C:\\priv\\tmp\0TMPDIR=C:\\priv\\tmp\0\0"
        );
    }

    #[test]
    fn procura_o_programa_no_path_com_as_extensoes() {
        let existem: HashSet<&str> =
            [r"C:\bin\claude.cmd", r"D:\ferramentas\claude.exe", r"C:\bin\x.exe", r"C:\bin\sem_extensao"].into_iter().collect();
        let existe = |p: &str| existem.contains(p);
        let path = r"C:\bin;D:\ferramentas;C:\vazio";
        let ext = ".COM;.EXE;.BAT;.CMD";
        // O primeiro do PATH que tem alguma extensão vale: `claude.cmd` em `C:\bin`.
        assert_eq!(procura_programa("claude", path, ext, &existe).as_deref(), Some(r"C:\bin\claude.cmd"));
        assert_eq!(procura_programa("claude.exe", path, ext, &existe).as_deref(), Some(r"D:\ferramentas\claude.exe"));
        assert_eq!(procura_programa(r"C:\bin\x", path, ext, &existe).as_deref(), Some(r"C:\bin\x.exe"));
        assert_eq!(procura_programa(r"C:\bin\x.exe", "", ext, &existe).as_deref(), Some(r"C:\bin\x.exe"));
        assert_eq!(procura_programa("sem_extensao", path, ext, &existe), None);
        assert_eq!(procura_programa("nada", path, ext, &existe), None);
        assert!(eh_script_do_cmd(r"C:\npm\claude.CMD") && eh_script_do_cmd("x.bat") && !eh_script_do_cmd("x.exe"));
    }

    #[test]
    fn o_plano_de_um_claude_do_instalador_nativo() {
        let amb = Falso::novo(&[
            r"C:\Users\ana\.claude",
            r"C:\Users\ana\.claude.json",
            r"C:\Users\ana\.local\bin",
            r"C:\Users\ana\.local\share\claude",
            r"C:\Users\ana\.gitconfig",
            r"C:\Users\ana\AppData\Roaming\npm",
            r"C:\Users\ana\AppData\Local\Keep",
        ]);
        let mut e = entrada(r"C:\Users\ana\.local\bin\claude.exe");
        e.pastas_git = vec![r"D:\projetos\repo\.git\worktrees\app".into(), r"D:\nao\existe".into()];
        let p = plano(&amb, &e);

        // A pasta e o git dela, com o SID da pasta, gravando, herdando.
        let pasta = de(&p, r"D:\projetos\app");
        assert_eq!(pasta.len(), 1);
        assert_eq!((pasta[0].quem, pasta[0].acesso, pasta[0].heranca, pasta[0].obrigatoria), (Quem::Pasta, Acesso::Gravacao, true, true));
        // O git que não existe fica de fora; o que existe só entra se existir.
        assert!(de(&p, r"D:\nao\existe").is_empty());
        // A casa do Claude: grava, é obrigatória e existe, então nada a criar.
        let claude = de(&p, r"C:\Users\ana\.claude");
        assert_eq!((claude[0].quem, claude[0].acesso, claude[0].obrigatoria), (Quem::Base, Acesso::Gravacao, true));
        assert!(!p.criar.iter().any(|c| partes(c) == partes(r"C:\Users\ana\.claude")));
        // A do Codex não existe e o programa não é o Codex: nem criada, nem concedida.
        assert!(de(&p, r"C:\Users\ana\.codex").is_empty());
        // O estado é criado (a linha de status grava lá) e a área privada também.
        assert!(p.criar.iter().any(|c| partes(c) == partes(r"C:\Users\ana\.local\state\keep\claude")));
        assert!(p.criar.iter().any(|c| partes(c) == partes(r"C:\Users\ana\AppData\Local\Keep\confine\tmp")));
        assert_eq!(p.temp, r"C:\Users\ana\AppData\Local\Keep\confine\tmp");
        // Os dois arquivos da configuração, um a um, sem herança.
        let json = de(&p, r"C:\Users\ana\.claude.json");
        assert_eq!((json[0].acesso, json[0].heranca), (Acesso::Gravacao, false));
        assert!(de(&p, r"C:\Users\ana\.claude.json.backup").is_empty());
        // Só leitura: os programas, o git, o npm e a instalação do Keep (a
        // pasta de cima do `bin`, onde está o script da linha de status).
        for so_le in [
            r"C:\Users\ana\.local\bin",
            r"C:\Users\ana\.local\share\claude",
            r"C:\Users\ana\.gitconfig",
            r"C:\Users\ana\AppData\Roaming\npm",
            r"C:\Users\ana\AppData\Local\Keep",
        ] {
            let c = de(&p, so_le);
            assert_eq!((c.len(), c[0].acesso, c[0].quem), (1, Acesso::Leitura, Quem::Base), "{so_le}");
        }
        // A casa inteira nunca é concedida.
        assert!(de(&p, CASA).is_empty());
        assert!(!p.concessoes.iter().any(|c| dentro_de(CASA, &c.caminho) && partes(&c.caminho).len() <= partes(CASA).len()));
    }

    #[test]
    fn o_plano_do_codex_cria_a_casa_dele_e_nao_a_do_claude() {
        let amb = Falso::novo(&[]);
        let p = plano(&amb, &entrada(r"C:\Users\ana\.local\bin\codex.exe"));
        assert!(p.criar.iter().any(|c| partes(c) == partes(r"C:\Users\ana\.codex")));
        assert!(!p.criar.iter().any(|c| partes(c) == partes(r"C:\Users\ana\.claude")));
        assert!(!p.criar.iter().any(|c| c.to_lowercase().contains("keep\\claude")));
        let codex = de(&p, r"C:\Users\ana\.codex");
        assert!(codex[0].obrigatoria);
        assert!(de(&p, r"C:\Users\ana\.claude").is_empty());
        // Outro programa qualquer: só a pasta e a área privada.
        let p = plano(&amb, &entrada(r"C:\Windows\System32\cmd.exe"));
        let quais: Vec<&str> = p.concessoes.iter().map(|c| c.caminho.as_str()).collect();
        assert_eq!(quais, [r"D:\projetos\app", r"C:\Users\ana\AppData\Local\Keep\confine"]);
    }

    #[test]
    fn o_plano_nao_mexe_em_pasta_do_sistema_e_segue_as_variaveis_da_ia() {
        let mut amb = Falso::novo(&[r"C:\Windows\System32", r"E:\contas\codex2", r"C:\Program Files\nodejs", r"C:\Users\ana\fnm\node"]);
        amb.vars.insert("CODEX_HOME".into(), r"E:\contas\codex2".into());
        amb.vars.insert("CLAUDE_CONFIG_DIR".into(), r"C:\Windows\System32".into());
        let mut e = entrada(r"C:\Windows\System32\cmd.exe");
        e.node = Some(r"C:\Users\ana\fnm\node\node.exe".into());
        let p = plano(&amb, &e);
        // O Codex extra entra; a variável que aponta para o sistema, não.
        assert_eq!(de(&p, r"E:\contas\codex2").len(), 1);
        assert!(de(&p, r"C:\Windows\System32").is_empty());
        // O node de um usuário só ganha leitura; o programa em System32, não.
        assert_eq!(de(&p, r"C:\Users\ana\fnm\node")[0].acesso, Acesso::Leitura);
    }

    #[test]
    fn a_pasta_do_git_for_windows_vai_ate_o_componente_git() {
        assert_eq!(raiz_do_git(r"C:\Users\ana\AppData\Local\Programs\Git\cmd\git.exe").as_deref(), Some(r"C:\Users\ana\AppData\Local\Programs\Git"));
        assert_eq!(raiz_do_git(r"C:\Users\ana\git\bin\bash.exe").as_deref(), Some(r"C:\Users\ana\git"));
        assert_eq!(raiz_do_git("git.exe"), None);
        let amb = Falso::novo(&[r"C:\Users\ana\AppData\Local\Programs\Git", r"C:\Program Files\Git"]);
        let mut e = entrada(r"C:\Users\ana\.local\bin\claude.exe");
        e.git = Some(r"C:\Users\ana\AppData\Local\Programs\Git\cmd\git.exe".into());
        let p = plano(&amb, &e);
        assert_eq!(de(&p, r"C:\Users\ana\AppData\Local\Programs\Git")[0].acesso, Acesso::Leitura);
        // O do sistema o Windows já deixa ler.
        e.git = Some(r"C:\Program Files\Git\cmd\git.exe".into());
        let p = plano(&amb, &e);
        assert!(de(&p, r"C:\Program Files\Git").is_empty());
    }

    #[test]
    fn so_as_ias_conhecidas_tem_nome() {
        assert_eq!(ia_de(r"C:\x\Claude.exe"), Ia::Claude);
        assert_eq!(ia_de(r"C:\npm\codex.cmd"), Ia::Codex);
        assert_eq!(ia_de(r"C:\x\cmd.exe"), Ia::Outro);
        assert_eq!(ia_de(r"C:\x\claude-code.exe"), Ia::Outro);
    }

    #[test]
    fn as_mascaras_nao_dao_poder_sobre_a_permissao_dos_outros() {
        for a in [Acesso::Leitura, Acesso::Gravacao] {
            let m = a.mascara();
            assert_eq!(m & 0x0004_0000, 0, "WRITE_DAC");
            assert_eq!(m & 0x0008_0000, 0, "WRITE_OWNER");
            assert_ne!(m & 0x0010_0000, 0, "SYNCHRONIZE");
            assert_ne!(m & 0x0000_0001, 0, "FILE_READ_DATA");
        }
        assert_ne!(Acesso::Gravacao.mascara() & 0x0001_0000, 0, "DELETE");
        assert_eq!(Acesso::Leitura.mascara() & 0x0001_0000, 0);
    }

    #[test]
    fn a_marca_de_uma_permissao_nao_confunde_pasta_sid_nem_acesso() {
        let m = marca("S-1-5-9-1", Acesso::Gravacao, true, r"D:\Projetos\App\");
        assert_eq!(m, marca("S-1-5-9-1", Acesso::Gravacao, true, "d:/projetos/app"));
        assert_ne!(m, marca("S-1-5-9-2", Acesso::Gravacao, true, r"D:\Projetos\App"));
        assert_ne!(m, marca("S-1-5-9-1", Acesso::Leitura, true, r"D:\Projetos\App"));
        assert_ne!(m, marca("S-1-5-9-1", Acesso::Gravacao, false, r"D:\Projetos\App"));
        assert_ne!(m, marca("S-1-5-9-1", Acesso::Gravacao, true, r"D:\Projetos\Outro"));
        let texto = format!("{m}\n\n  {}  \n{m}\n", marca("S-1-5-9-1", Acesso::Leitura, true, r"C:\x"));
        let lidas = marcas_de(&texto);
        assert_eq!(lidas.len(), 2);
        assert!(lidas.contains(&m));
        assert!(marcas_de("").is_empty());
    }

    #[test]
    fn a_isca_fica_onde_nenhuma_permissao_alcanca() {
        let amb = Falso::novo(&[]);
        let mut e = entrada(r"C:\Users\ana\.local\bin\claude.exe");
        // A pasta escolhida é a do Roaming: a isca que estaria nela não prova nada.
        e.pasta = r"C:\Users\ana\AppData\Roaming".into();
        let p = plano(&amb, &e);
        let candidatos: Vec<String> = vec![
            r"C:\Users\ana\AppData\Roaming\Keep\isca".into(),
            r"C:\Users\ana\AppData\Local\Keep\confine\isca".into(),
            r"C:\Users\ana\.keep-isca".into(),
        ];
        assert_eq!(lugar_da_isca(&candidatos, &p.concessoes).as_deref(), Some(r"C:\Users\ana\.keep-isca"));
        e.pasta = r"D:\projetos\app".into();
        let p = plano(&amb, &e);
        assert_eq!(lugar_da_isca(&candidatos, &p.concessoes).as_deref(), Some(r"C:\Users\ana\AppData\Roaming\Keep\isca"));
        assert_eq!(lugar_da_isca(&candidatos[1..2], &p.concessoes), None);
    }

    #[test]
    fn falhas_de_partida_sao_so_as_de_carregar() {
        assert!(falha_de_partida(0xC000_0142) && falha_de_partida(0xC000_0135));
        assert!(!falha_de_partida(0) && !falha_de_partida(1) && !falha_de_partida(0xC000_0005));
    }

    #[test]
    fn o_texto_do_encaixe_e_o_que_a_funcao_do_powershell_chama() {
        assert!(ENCAIXE_POWERSHELL.contains("ia confinar --"));
    }

    // ---- a prova: tem de saber falhar (sem o sandbox, o que está fora é lido)

    #[test]
    fn a_prova_acusa_um_processo_que_nao_esta_preso() {
        let raiz = tempfile::tempdir().unwrap();
        let (dentro, fora) = (raiz.path().join("dentro"), raiz.path().join("fora"));
        std::fs::create_dir_all(&dentro).unwrap();
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("segredo.txt"), "x").unwrap();
        let s = sondar_fs(&dentro, &fora.join("segredo.txt"), &fora, &fora);
        assert_eq!(s.dentro, Ok(()));
        for v in [&s.le_fora, &s.lista_fora, &s.grava_fora] {
            assert_eq!(v, &Veredito::Permitido);
        }
        let c = s.conclusao().unwrap_err();
        assert!(c.contains("a restrição não valeu"), "{c}");
        // E ela não deixa lixo: o arquivo que criou fora foi apagado.
        assert_eq!(std::fs::read_dir(&fora).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_prova_aceita_quando_o_sistema_nega_e_a_pasta_funciona() {
        use std::os::unix::fs::PermissionsExt;
        let raiz = tempfile::tempdir().unwrap();
        let (dentro, fora) = (raiz.path().join("dentro"), raiz.path().join("fora"));
        std::fs::create_dir_all(&dentro).unwrap();
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("segredo.txt"), "x").unwrap();
        let negar = |p: &Path, modo| std::fs::set_permissions(p, std::fs::Permissions::from_mode(modo)).unwrap();
        negar(&fora.join("segredo.txt"), 0o000);
        negar(&fora, 0o000);
        // Quem roda como root lê tudo, e a prova (com razão) não passa: o teste
        // só vale para um usuário comum.
        let negado = std::fs::read_dir(&fora).is_err();
        let s = sondar_fs(&dentro, &fora.join("segredo.txt"), &fora, &fora);
        negar(&fora, 0o755);
        negar(&fora.join("segredo.txt"), 0o644);
        if negado {
            assert_eq!(s.conclusao(), Ok(()), "{s:?}");
        }
    }

    #[test]
    fn a_prova_nao_confunde_pasta_que_nao_existe_com_acesso_negado() {
        let raiz = tempfile::tempdir().unwrap();
        let dentro = raiz.path().join("dentro");
        std::fs::create_dir_all(&dentro).unwrap();
        let sumido = raiz.path().join("sumiu");
        let s = sondar_fs(&dentro, &sumido.join("a.txt"), &sumido, &sumido);
        for v in [&s.le_fora, &s.lista_fora, &s.grava_fora] {
            assert!(matches!(v, Veredito::Inconclusivo(_)), "{v:?}");
        }
        assert!(s.conclusao().unwrap_err().contains("não foi possível provar"));
        // E uma pasta permitida que não funciona também reprova.
        let s = sondar_fs(&raiz.path().join("nao-existe"), &sumido.join("a.txt"), &sumido, &sumido);
        assert!(s.conclusao().unwrap_err().contains("a pasta permitida não pôde ser usada"));
    }
}
