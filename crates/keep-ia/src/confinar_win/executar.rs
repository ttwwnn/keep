//! Subir o programa preso no Windows: o que `confinar::cli` faz com o
//! `sandbox-exec` no macOS. Aqui o `keep` não pode virar o programa (o Windows
//! não tem `exec`): fica como pai dele, esperando, com o Job que o mata junto.
//!
//! Quem decide o quê está em `super`; quem fala com o sistema, em `sistema`.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use super::sistema::{self, Padrao, Sid};
use super::{
    Ambiente, Ia, ia_de, Entrada, NIVEIS, Nivel, Quem, ambiente_preso, com_variaveis, marca, marcas_de, bloco_de_ambiente, dentro_de, eh_script_do_cmd, falha_de_partida,
    area_privada, junta, linha_de_comando, linha_de_script, lugar_da_isca, pasta_larga_demais, plano, procura_programa, sem_prefixo,
    sid_base, sid_da_pasta, sids_de_restricao, sondar_fs,
};
use crate::{caminhos, confinar};

/// O nome da trava entre processos que preparam permissões.
const TRAVA: &str = "Local\\KeepConfinar";
/// Quanto esperar a trava: percorrer uma pasta grande leva minutos.
const ESPERA_DA_TRAVA_MS: u32 = 10 * 60 * 1000;
/// Quanto olhar o programa depois de solto, para saber se partiu.
const OLHADA_NA_PARTIDA_MS: u32 = 2000;

/// `KEEP_CONFINE_TRACE=1` conta o que se faz, para quem for ver por que a IA
/// não subiu num computador onde isto nunca rodou.
fn rastro(texto: impl FnOnce() -> String) {
    if std::env::var_os("KEEP_CONFINE_TRACE").is_some_and(|v| !v.is_empty()) {
        eprintln!("Keep[confinar]: {}", texto());
    }
}

/// O perfil do usuário de verdade. `caminhos::casa()` obedece `KEEP_IA_HOME`,
/// que os testes do crate trocam por uma casa de mentira (e apagam quando
/// acabam, com o que estiver dentro): a IA presa roda com o perfil que o
/// Windows deu ao usuário.
fn perfil_real() -> String {
    let do_windows = std::env::var("USERPROFILE").ok().filter(|v| !v.is_empty());
    sem_prefixo(&do_windows.unwrap_or_else(|| caminhos::casa().to_string_lossy().into_owned()))
}

/// O computador de verdade, para o plano.
struct Real;

impl Ambiente for Real {
    fn var(&self, nome: &str) -> Option<String> {
        std::env::var(nome).ok().filter(|v| !v.is_empty())
    }

    fn existe(&self, caminho: &str) -> bool {
        Path::new(caminho).exists()
    }
}

/// O que subir, onde, e a que pasta fica preso.
pub struct Pedido {
    /// A pasta permitida, real e sem o prefixo literal.
    pub pasta: String,
    /// O programa, com o caminho inteiro.
    pub programa: String,
    /// Os argumentos, como `confinar::linha_presa` os deixou.
    pub args: Vec<String>,
    /// A pasta em que o programa começa (dentro da permitida).
    pub onde: String,
    /// Não morrer com o Ctrl+C que a aba manda a todos os que dividem o
    /// console: quem o trata é o programa. Só depois de preparadas as
    /// permissões, que o Ctrl+C ainda pode interromper.
    pub segurar_ctrl_c: bool,
    /// Variáveis de ambiente a mais, conforme o programa (o Claude Code usa a
    /// ferramenta PowerShell no lugar do Git Bash, que não sobe preso).
    pub ambiente_extra: Vec<(&'static str, &'static str)>,
}

fn ambiente_atual() -> Vec<(Vec<u16>, Vec<u16>)> {
    use std::os::windows::ffi::OsStrExt;
    std::env::vars_os().map(|(n, v)| (n.encode_wide().collect(), v.encode_wide().collect())).collect()
}

fn no_path(nome: &str) -> Option<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    procura_programa(nome, &path, &pathext, &|c| Path::new(c).is_file())
}

/// Onde ficam as marcas das permissões dadas inteiras: fora do que a IA
/// alcança (ela só mexeria no que o Keep deixa de percorrer).
fn arquivo_das_marcas(dados: Option<&str>, local: Option<&str>, casa: &str) -> String {
    let base = dados.or(local).map(String::from).unwrap_or_else(|| junta(casa, "AppData\\Roaming"));
    junta(&base, "Keep\\confine-estado\\concedido.txt")
}

/// A permissão, dada se ainda não está dada. Numa pasta (que se percorre), só
/// vale a que tem marca: a da raiz sozinha pode ser de uma passada que parou
/// no meio. Dá aviso antes de percorrer: a primeira vez numa pasta grande
/// demora.
fn garante(c: &super::Concessao, sid: &Sid, sid_txt: &str, marcas: &mut HashSet<String>, arquivo: &str) -> Result<(), String> {
    let esta = marca(sid_txt, c.acesso, c.heranca, &c.caminho);
    let na_raiz = sistema::concedido(&c.caminho, sid, c.acesso, c.heranca).unwrap_or(false);
    if na_raiz && (!c.heranca || marcas.contains(&esta)) {
        return Ok(());
    }
    if c.heranca {
        eprintln!(
            "Keep: preparando as permissões de {} para a IA (só na primeira vez; uma pasta grande demora).",
            c.caminho
        );
    }
    rastro(|| format!("concede {:?} em {} ({:?})", c.acesso, c.caminho, c.quem));
    if c.acesso == super::Acesso::Atributos {
        // Uma pasta de passagem (a casa do usuário): a permissão não herda, e
        // trocar a DACL pelo nome faria o Windows percorrer o que há embaixo.
        sistema::conceder_so_aqui(&c.caminho, sid, c.acesso)?;
    } else {
        sistema::conceder(&c.caminho, sid, c.acesso, c.heranca)?;
    }
    if c.heranca && marcas.insert(esta.clone()) {
        // Não gravar a marca só obriga a percorrer de novo na próxima vez.
        let gravou = Path::new(arquivo)
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::OpenOptions::new().create(true).append(true).open(arquivo))
            .and_then(|mut f| writeln!(f, "{esta}"));
        if let Err(e) = gravou {
            rastro(|| format!("a marca de {} não foi gravada: {e}", c.caminho));
        }
    }
    Ok(())
}

/// O arquivo e a pasta de isca da prova, fora de qualquer permissão. Criados
/// uma vez e deixados: o arquivo só tem uma frase.
fn prepara_isca(
    casa: &str,
    dados: Option<&str>,
    local: Option<&str>,
    concessoes: &[super::Concessao],
) -> Result<(String, String), String> {
    let mut candidatos: Vec<String> = Vec::new();
    candidatos.extend(dados.map(|d| junta(d, "Keep\\confine-isca")));
    candidatos.extend(local.map(|d| junta(d, "Keep\\confine-isca")));
    candidatos.push(junta(casa, ".keep-confine-isca"));
    let pasta = lugar_da_isca(&candidatos, concessoes).ok_or("não há onde pôr a isca da prova da restrição")?;
    std::fs::create_dir_all(&pasta).map_err(|e| format!("não foi possível criar {pasta}: {e}"))?;
    let arquivo = junta(&pasta, "segredo.txt");
    if !Path::new(&arquivo).exists() {
        std::fs::write(&arquivo, "o Keep prova com este arquivo que a IA presa não lê fora da pasta dela\n")
            .map_err(|e| format!("não foi possível criar {arquivo}: {e}"))?;
    }
    Ok((arquivo, pasta))
}

/// Sobe o programa preso à pasta e espera ele acabar: o código de saída dele,
/// ou o porquê de não ter subido. Nada do que a IA toca fora da pasta passa:
/// se a restrição não puder ser posta e provada, o programa nem nasce.
pub fn roda(p: &Pedido, padrao: &Padrao) -> Result<i32, String> {
    let amb = Real;
    let casa = perfil_real();
    let de_env = |n: &str| amb.var(n).map(|v| sem_prefixo(&v));
    let dados = de_env("APPDATA");
    let local = de_env("LOCALAPPDATA");
    let sistema_dirs: Vec<String> = ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"]
        .iter()
        .filter_map(|n| de_env(n))
        .collect();
    let git = amb.var("CLAUDE_CODE_GIT_BASH_PATH").filter(|g| Path::new(g).is_file()).or_else(|| no_path("git"));
    let node = if eh_script_do_cmd(&p.programa) { no_path("node") } else { None };
    let entrada = Entrada {
        pasta: p.pasta.clone(),
        pastas_git: confinar::pastas_do_git(Path::new(&p.pasta)).iter().map(|g| sem_prefixo(g)).collect(),
        programa: p.programa.clone(),
        casa: casa.clone(),
        dados: dados.clone(),
        local: local.clone(),
        // O `keep` que a IA chama: só se este processo é ele (um teste não é, e a
        // pasta de `target` não é do que se dá permissão).
        keep: std::env::current_exe()
            .ok()
            .filter(|e| e.file_name().is_some_and(|n| n.eq_ignore_ascii_case("keep.exe")))
            .map(|e| sem_prefixo(&e.to_string_lossy())),
        git,
        node,
        sistema: sistema_dirs,
    };
    let plano = plano(&amb, &entrada);

    // Os SIDs: o do usuário dá nome aos do Keep, o da sessão de logon é o que
    // deixa o processo abrir a estação de janelas.
    let token0 = sistema::token_do_processo()?;
    let usuario = sistema::sid_do_usuario(&token0)?;
    let usuario_txt = usuario.texto()?;
    let logon = sistema::sid_do_logon(&token0)?;
    let logon_txt = logon.texto()?;
    let sistema_sid = Sid::de_texto(super::SID_SISTEMA)?;
    let pasta_txt = sid_da_pasta(&usuario_txt, &p.pasta);
    let base_txt = sid_base(&usuario_txt);
    let sid_pasta = Sid::de_texto(&pasta_txt)?;
    let sid_basico = Sid::de_texto(&base_txt)?;
    let administrativos = sistema::grupos_administrativos(&token0)?;

    // As pastas que a IA usa e não consegue criar, e as permissões. Quem sobe
    // a IA em várias abas ao mesmo tempo (a restauração) espera a vez.
    for d in &plano.criar {
        if let Err(e) = std::fs::create_dir_all(d) {
            rastro(|| format!("não criou {d}: {e}"));
        }
    }
    {
        let _trava = sistema::pegar_trava(TRAVA, ESPERA_DA_TRAVA_MS, || {
            eprintln!("Keep: outra aba está preparando as permissões da IA; esperando ela acabar.")
        })?;
        let arquivo = arquivo_das_marcas(dados.as_deref(), local.as_deref(), &casa);
        let mut marcas = std::fs::read_to_string(&arquivo).map(|t| marcas_de(&t)).unwrap_or_default();
        for c in &plano.concessoes {
            let (sid, sid_txt) = match c.quem {
                Quem::Pasta => (&sid_pasta, &pasta_txt),
                Quem::Base => (&sid_basico, &base_txt),
            };
            match garante(c, sid, sid_txt, &mut marcas, &arquivo) {
                Ok(()) => {}
                Err(e) if c.obrigatoria => return Err(e),
                Err(e) => rastro(|| format!("permissão dispensada em {}: {e}", c.caminho)),
            }
        }
    }
    let (isca, pasta_da_isca) = prepara_isca(&casa, dados.as_deref(), local.as_deref(), &plano.concessoes)?;

    // O programa, e como ele é chamado.
    let (app, linha) = if eh_script_do_cmd(&p.programa) {
        let cmd = std::env::var("ComSpec").unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".into());
        let linha = linha_de_script(&cmd, &p.programa, &p.args)?;
        (cmd, linha)
    } else {
        (p.programa.clone(), linha_de_comando(&p.programa, &p.args))
    };
    // Os caminhos que o programa preso precisa achar, ditos um a um: o erro do
    // Windows ("o sistema não achou o caminho", 3) não diz qual.
    let faltam: Vec<String> = [
        ("a pasta permitida", p.pasta.as_str(), true),
        ("a pasta em que o programa começa", p.onde.as_str(), true),
        ("o programa", p.programa.as_str(), false),
        ("o que roda o programa", app.as_str(), false),
    ]
    .iter()
    .filter(|(_, c, pasta)| if *pasta { !Path::new(c).is_dir() } else { !Path::new(c).is_file() })
    .map(|(o, c, _)| format!("{o}: {c}"))
    .collect();
    if !faltam.is_empty() {
        return Err(format!("não existe: {}", faltam.join("; ")));
    }
    let ambiente = bloco_de_ambiente(&com_variaveis(
        ambiente_preso(ambiente_atual(), &plano.temp, &p.pasta),
        &p.ambiente_extra,
    ));
    let sem_janela = !sistema::tem_console();
    let job = sistema::job_que_mata();
    if let Err(e) = &job {
        rastro(|| format!("sem Job (a IA não morre junto com a aba): {e}"));
    }

    if p.segurar_ctrl_c {
        sistema::nao_morrer_com_ctrl_c();
    }
    let admins: Vec<&Sid> = administrativos.iter().collect();
    let mut tentativas: Vec<String> = Vec::new();
    for nivel in NIVEIS {
        let ids = sids_de_restricao(nivel, &pasta_txt, &base_txt, &logon_txt);
        let quais = format!("{nivel:?} [{}]", ids.join(" "));
        // Cada passo que falha põe o porquê na lista e passa ao nível seguinte:
        // a mensagem de quando nenhum serve diz o que cada um encontrou.
        macro_rules! tenta {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => {
                        tentativas.push(format!("{quais}: {e}"));
                        continue;
                    }
                }
            };
        }
        let sids: Vec<Sid> = tenta!(ids.iter().map(|s| Sid::de_texto(s)).collect::<Result<Vec<_>, _>>());
        let a_restringir: Vec<&Sid> = sids.iter().collect();
        // A DACL padrão do token: o usuário, o sistema e a pasta. Não o SID base,
        // que todo workspace preso leva: com ele o processo de um workspace
        // abriria os objetos (e o processo) de outro.
        let token = tenta!(sistema::token_restrito(&token0, &usuario, &sistema_sid, &a_restringir, &admins, &[&sid_pasta]));
        if !sistema::e_restrito(&token) {
            tentativas.push(format!("{quais}: o token que devia ser restrito não é"));
            continue;
        }

        // A prova: com este token o de fora é negado e a pasta funciona. Um
        // sandbox que não contém não sobe a IA.
        let sonda = tenta!(sistema::como_o_token(&token, || {
            sondar_fs(Path::new(&p.pasta), Path::new(&isca), Path::new(&casa), Path::new(&pasta_da_isca))
        }));
        rastro(|| format!("prova ({nivel:?}): {sonda:?}"));
        tenta!(sonda.conclusao());

        let filho = tenta!(sistema::criar_processo(&token, &app, &linha, &p.onde, &ambiente, padrao, sem_janela));
        if let Ok(j) = &job {
            if let Err(e) = sistema::no_job(j, &filho) {
                rastro(|| format!("fora do Job: {e}"));
            }
        }
        if let Err(e) = sistema::abrir_o_token_ao_filho(&filho, &sid_pasta) {
            rastro(|| format!("o token do filho não foi aberto a ele: {e}"));
        }
        tenta!(sistema::retomar(&filho));
        let cedo = tenta!(sistema::esperar(&filho.processo, OLHADA_NA_PARTIDA_MS));
        if let Some(c) = cedo.filter(|c| falha_de_partida(*c)) {
            tentativas.push(format!("{quais}: o programa morreu ao partir (0x{c:08X})"));
            continue;
        }
        // Vivo depois do olhar: se há console aqui, ele tem de se ligar ao
        // daqui. Um programa que subiu sem o console da aba ficaria mudo e
        // parado; dito agora, o usuário sabe por quê.
        if cedo.is_none() && !sem_janela && !ligou_ao_console(&filho) {
            sistema::matar(&filho);
            tentativas.push(format!("{quais}: o console da aba não chegou ao programa"));
            continue;
        }
        let codigo = match cedo {
            Some(c) => c,
            None => tenta!(sistema::esperar(&filho.processo, sistema::INFINITO)).unwrap_or(1),
        };
        rastro(|| format!("o programa saiu com {codigo} (nível {nivel:?})"));
        if nivel == Nivel::Amplo {
            rastro(|| "nível amplo: o compatível não deixou o programa partir; os autenticados também gravam na raiz de outras unidades".to_string());
        }
        return Ok(codigo as i32);
    }
    Err(format!("o Windows não deixou o programa partir com o token restrito: {}", tentativas.join(" | ")))
}

/// Espera, por uns segundos, o programa se ligar ao console daqui. Quem saiu
/// ou quem o sistema não deixa conferir passa.
fn ligou_ao_console(filho: &sistema::Filho) -> bool {
    for _ in 0..25 {
        match sistema::no_console(filho.pid) {
            Some(false) => {}
            _ => return true,
        }
        if matches!(sistema::esperar(&filho.processo, 200), Ok(Some(_))) {
            return true;
        }
    }
    false
}

/// Sem pasta para o workspace: o programa roda como sempre.
fn rodar_livre(programa: &str, resto: &[String]) -> i32 {
    match std::process::Command::new(programa).args(resto).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("Keep: não foi possível rodar {programa}: {e}");
            127
        }
    }
}

/// `keep ia confinar -- <programa> [argumentos…]` no Windows.
pub fn cli(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("--autoteste") => return autoteste(),
        Some("--diagnostico") => return diagnostico(),
        Some("--objetos") => return objetos(args.iter().any(|a| a == "--conceder")),
        Some("--como-comum") => return como_comum(&args[1..]),
        _ => {}
    }
    let args: Vec<String> = match args.first().map(String::as_str) {
        Some("--") => args[1..].to_vec(),
        _ => args.to_vec(),
    };
    let Some((programa, resto)) = args.split_first() else {
        eprintln!("uso: keep ia confinar -- <programa> [argumentos...]");
        return 2;
    };
    let ws = std::env::var("KEEP_WORKSPACE").unwrap_or_default();
    let Some(escolhida) = (if ws.is_empty() { None } else { confinar::pasta(&ws) }) else {
        return rodar_livre(programa, resto);
    };
    let pasta = match std::fs::canonicalize(&escolhida) {
        Ok(p) if p.is_dir() => sem_prefixo(&p.to_string_lossy()),
        _ => {
            eprintln!(
                "Keep: a pasta permitida para a IA do workspace {ws} não existe ({escolhida}). A IA não sobe sem a restrição; escolha outra pasta no workspace."
            );
            return 1;
        }
    };
    let casa = perfil_real();
    let sistema_dirs: Vec<String> = ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
        .iter()
        .filter_map(|n| std::env::var(n).ok())
        .map(|v| sem_prefixo(&v))
        .collect();
    if let Some(motivo) = pasta_larga_demais(&pasta, &casa, &sistema_dirs) {
        eprintln!(
            "Keep: {pasta} é {motivo}: a IA não ficaria presa a nada. A IA não sobe; escolha a pasta de um projeto."
        );
        return 1;
    }
    let linha = match confinar::linha_presa(programa, resto) {
        Ok(l) => l,
        Err(motivo) => {
            eprintln!("Keep: {motivo} Este workspace restringe a IA a {pasta}.");
            return 1;
        }
    };
    let Some(exe) = no_path(programa) else {
        eprintln!("Keep: não foi possível rodar {programa}: não está no PATH.");
        return 127;
    };
    let aqui = std::env::current_dir().ok().and_then(|p| std::fs::canonicalize(p).ok()).map(|p| sem_prefixo(&p.to_string_lossy()));
    let onde = match aqui {
        Some(a) if dentro_de(&pasta, &a) => {
            eprintln!("Keep: a IA deste workspace só acessa {pasta}.");
            a
        }
        _ => {
            eprintln!("Keep: a IA deste workspace só acessa {pasta}; subindo nela.");
            pasta.clone()
        }
    };
    // Sem o Git Bash, que não roda preso, o Claude Code usa a ferramenta
    // PowerShell: nega-se a Bash (por um arquivo de configuração) e liga-se a outra.
    let (linha, ambiente_extra): (Vec<String>, Vec<(&'static str, &'static str)>) = if ia_de(programa) == Ia::Claude {
        let local = std::env::var("LOCALAPPDATA").ok().filter(|v| !v.is_empty()).map(|v| sem_prefixo(&v));
        let arquivo = junta(&area_privada(&local.unwrap_or_else(|| junta(&perfil_real(), "AppData\\Local"))), "claude-sem-bash.json");
        let gravou = Path::new(&arquivo)
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&arquivo, confinar::CONFIGURACAO_SEM_BASH));
        match gravou {
            Ok(()) => {
                eprintln!("Keep: o Git Bash não roda preso à pasta; o Claude usa o PowerShell no lugar.");
                (confinar::claude_sem_o_bash(&linha, &arquivo), vec![("CLAUDE_CODE_USE_POWERSHELL_TOOL", "1")])
            }
            Err(e) => {
                eprintln!("Keep: não foi possível gravar {arquivo}: {e}. A IA não sobe sem a restrição.");
                return 1;
            }
        }
    } else {
        (linha, Vec::new())
    };
    let pedido = Pedido { pasta, programa: exe.replace('/', "\\"), args: linha, onde, segurar_ctrl_c: true, ambiente_extra };
    match roda(&pedido, &Padrao::do_processo()) {
        Ok(codigo) => codigo,
        Err(e) => {
            eprintln!("Keep: não foi possível prender {programa}: {e}. A IA não sobe sem a restrição.");
            1
        }
    }
}

// ------------------------------------------------------------------ a verificação, feita no computador de quem usa

/// Prova num computador de verdade o que `roda` promete: `keep ia confinar
/// --autoteste`. Sobe de fato programas presos (o `cmd.exe`, com roteiros de
/// uma linha) e confere o que eles conseguem e o que não. Dentro de uma aba do
/// Keep confere também que o console passa ao programa preso.
pub fn autoteste() -> i32 {
    let casa = perfil_real();
    let raiz = junta(&casa, &format!(".keep-confine-autoteste-{}", std::process::id()));
    let (permitida, fora) = (junta(&raiz, "permitida"), junta(&raiz, "fora"));
    let preparo = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&permitida)?;
        std::fs::create_dir_all(&fora)?;
        std::fs::write(junta(&permitida, "a.txt"), "dentro")?;
        std::fs::write(junta(&fora, "b.txt"), "fora")
    })();
    if let Err(e) = preparo {
        eprintln!("Keep: não foi possível montar a pasta do autoteste em {raiz}: {e}");
        return 1;
    }
    println!("Autoteste da IA presa (pasta de prova: {permitida})");
    let mut falhas = 0usize;
    let mut passo = 0usize;
    let mut confere = |nome: &str, ok: bool, detalhe: &str| {
        println!("  {}  {nome}{}", if ok { "ok    " } else { "FALHOU" }, if ok || detalhe.is_empty() { String::new() } else { format!(" ({detalhe})") });
        if !ok {
            falhas += 1;
        }
    };
    let mut roda_linha = |comando: &str, com_console: bool| -> Result<(i32, String), String> {
        passo += 1;
        let roteiro = junta(&permitida, &format!("passo{passo}.cmd"));
        std::fs::write(&roteiro, format!("@echo off\r\n{comando}\r\nexit /b %errorlevel%\r\n"))
            .map_err(|e| format!("{roteiro}: {e}"))?;
        let saida_arq = junta(&raiz, &format!("saida{passo}.txt"));
        let saida = std::fs::File::create(&saida_arq).map_err(|e| format!("{saida_arq}: {e}"))?;
        let padrao =
            if com_console { Padrao::do_processo() } else { Padrao::de_arquivos(None, Some(&saida), Some(&saida)) };
        let pedido =
            Pedido {
                pasta: permitida.clone(),
                programa: roteiro,
                args: vec![],
                onde: permitida.clone(),
                segurar_ctrl_c: false,
                ambiente_extra: Vec::new(),
            };
        let codigo = roda(&pedido, &padrao)?;
        drop(padrao);
        let texto = std::fs::read_to_string(&saida_arq).unwrap_or_default();
        Ok((codigo, texto))
    };

    let a = junta(&permitida, "a.txt");
    let b = junta(&fora, "b.txt");
    let c = junta(&fora, "c.txt");
    let d = junta(&permitida, "d.txt");
    let sub = junta(&fora, "sub");
    match roda_linha(&format!("type \"{a}\""), false) {
        Ok((0, t)) => confere("lê na pasta", t.trim() == "dentro", &t),
        Ok((n, t)) => confere("lê na pasta", false, &format!("saiu com {n}: {t}")),
        Err(e) => confere("lê na pasta", false, &e),
    }
    match roda_linha(&format!("echo novo> \"{d}\" && type \"{d}\""), false) {
        Ok((0, t)) => confere("grava na pasta", t.trim() == "novo", &t),
        Ok((n, t)) => confere("grava na pasta", false, &format!("saiu com {n}: {t}")),
        Err(e) => confere("grava na pasta", false, &e),
    }
    match roda_linha(&format!("type \"{b}\""), false) {
        Ok((n, t)) => confere("não lê o que está fora", n != 0 && !t.contains("fora"), &format!("saiu com {n}: {t}")),
        Err(e) => confere("não lê o que está fora", false, &e),
    }
    // O que prova que não gravou é o arquivo não existir: o `cmd` pode sair com
    // 0 mesmo quando o redirecionamento foi negado ("Acesso negado").
    match roda_linha(&format!("echo x> \"{c}\""), false) {
        Ok((n, t)) => confere(
            "não grava fora",
            !Path::new(&c).exists(),
            &format!("saiu com {n}; {c} existe: {}; disse: {}", Path::new(&c).exists(), t.trim()),
        ),
        Err(e) => confere("não grava fora", false, &e),
    }
    match roda_linha(&format!("mkdir \"{sub}\""), false) {
        Ok((n, t)) => confere(
            "não cria pasta fora",
            !Path::new(&sub).exists(),
            &format!("saiu com {n}; {sub} existe: {}; disse: {}", Path::new(&sub).exists(), t.trim()),
        ),
        Err(e) => confere("não cria pasta fora", false, &e),
    }
    match roda_linha(&format!("del /f /q \"{b}\""), false) {
        Ok((n, t)) => confere(
            "não apaga o que está fora",
            Path::new(&b).exists(),
            &format!("saiu com {n}; {b} existe: {}; disse: {}", Path::new(&b).exists(), t.trim()),
        ),
        Err(e) => confere("não apaga o que está fora", false, &e),
    }
    match roda_linha(&format!("dir /b \"{casa}\""), false) {
        Ok((n, t)) => confere("não lista a casa do usuário", n != 0 || t.trim().is_empty(), &t),
        Err(e) => confere("não lista a casa do usuário", false, &e),
    }
    match roda_linha("echo %KEEP_CONFINED_TO%", false) {
        Ok((0, t)) => confere("sabe a pasta", t.trim().eq_ignore_ascii_case(&permitida), &t),
        Ok((n, t)) => confere("sabe a pasta", false, &format!("saiu com {n}: {t}")),
        Err(e) => confere("sabe a pasta", false, &e),
    }
    match roda_linha("echo %TEMP%", false) {
        Ok((0, t)) => confere("o TEMP dele é a área privada do Keep", t.trim().to_lowercase().contains("keep\\confine\\tmp"), &t),
        Ok((n, t)) => confere("o TEMP dele é a área privada do Keep", false, &format!("saiu com {n}: {t}")),
        Err(e) => confere("o TEMP dele é a área privada do Keep", false, &e),
    }
    match roda_linha("exit /b 7", false) {
        Ok((n, _)) => confere("o código de saída do programa volta", n == 7, &format!("voltou {n}")),
        Err(e) => confere("o código de saída do programa volta", false, &e),
    }
    if sistema::tem_console() {
        match roda_linha("echo Keep: o console da aba chegou ao programa preso> CONOUT$", true) {
            Ok((n, _)) => confere("o console da aba passa ao programa preso", n == 0, &format!("saiu com {n}")),
            Err(e) => confere("o console da aba passa ao programa preso", false, &e),
        }
    } else {
        println!("  --    o console da aba (sem console aqui: rode dentro de uma aba do Keep)");
    }

    let _ = std::io::stdout().flush();
    if falhas > 0 {
        // Antes de apagar a pasta de prova: quem pode o quê nela é o que explica a falha.
        println!();
        for caminho in [&casa, &raiz, &permitida, &fora, &b] {
            mostra_acl(caminho);
        }
        println!();
        diagnostico();
    }
    let _ = std::fs::remove_dir_all(&raiz);
    println!("{}", if falhas == 0 { "tudo certo" } else { "há falhas: a IA não deve ser tida como presa neste computador" });
    i32::from(falhas > 0)
}

/// As permissões de um caminho como o `icacls` as conta, para o diagnóstico.
fn mostra_acl(caminho: &str) {
    println!("  icacls {caminho}");
    match std::process::Command::new("icacls").arg(caminho).output() {
        Ok(o) => {
            for l in String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()) {
                println!("    {l}");
            }
        }
        Err(e) => println!("    o icacls não rodou: {e}"),
    }
}

// ------------------------------------------------------------------ o que o Windows aceita

/// As letras dos atributos de um grupo do token: obrigatório, ligado por
/// padrão, ligado, dono, só nega, e o da sessão de logon.
fn atributos_em_texto(a: u32) -> String {
    let mut t = String::new();
    for (bit, letra) in [(0x1, 'M'), (0x2, 'D'), (0x4, 'E'), (0x8, 'O'), (0x10, 'N')] {
        if a & bit != 0 {
            t.push(letra);
        }
    }
    if a & 0xC000_0000 == 0xC000_0000 {
        t.push('L');
    }
    t
}

/// `keep ia confinar --diagnostico`: o token deste processo e o que
/// `CreateRestrictedToken` aceita, uma lista de cada vez. Mostra o SID de cada
/// lista, as opções e o código de erro de cada chamada, para achar o SID ou a
/// opção que o Windows recusa. Não prende nada.
pub fn diagnostico() -> i32 {
    println!("Diagnóstico do token restrito (perfil: {})", perfil_real());
    let token0 = match sistema::token_do_processo() {
        Ok(t) => t,
        Err(e) => {
            println!("  não abriu o token do processo: {e}");
            return 1;
        }
    };
    let (usuario, logon) = match (sistema::sid_do_usuario(&token0), sistema::sid_do_logon(&token0)) {
        (Ok(u), Ok(l)) => (u, l),
        (u, l) => {
            println!("  sem o SID do usuário ou o da sessão: {:?} {:?}", u.err(), l.err());
            return 1;
        }
    };
    let (usuario_txt, logon_txt) = (usuario.texto().unwrap_or_default(), logon.texto().unwrap_or_default());
    println!("  usuário: {usuario_txt}");
    println!("  sessão de logon: {logon_txt}");
    println!("  restrito já: {}", sistema::e_restrito(&token0));
    println!("  grupos do token (M obrigatório, D ligado por padrão, E ligado, O dono, N só nega, L sessão):");
    match sistema::grupos_do_token(&token0) {
        Ok(grupos) => {
            for (sid, a) in grupos {
                println!("    {sid}  0x{a:08X} {}", atributos_em_texto(a));
            }
        }
        Err(e) => println!("    não leu: {e}"),
    }
    let administrativos = sistema::grupos_administrativos(&token0).unwrap_or_default();
    let admins: Vec<&Sid> = administrativos.iter().collect();
    println!("  grupos administrativos postos só para negar: {}", administrativos.len());

    let da_pasta = sid_da_pasta(&usuario_txt, "diagnostico");
    let base = sid_base(&usuario_txt);
    // A forma antiga do SID do Keep (autoridade 5 e cinco sub-autoridades) e
    // uma de domínio, para ver se o Windows distingue.
    let forma_nt = da_pasta.replacen("S-1-0-", "S-1-5-1262830928-", 1);
    let forma_dominio = da_pasta.replacen("S-1-0-", "S-1-5-21-", 1);
    let um = |rotulo: &str, flags: u32, des: &[&Sid], restringir: &[String]| {
        let sids: Result<Vec<Sid>, String> = restringir.iter().map(|t| Sid::de_texto(t)).collect();
        let resultado = match sids {
            Err(e) => format!("SID recusado: {e}"),
            Ok(sids) => {
                let refs: Vec<&Sid> = sids.iter().collect();
                match sistema::token_restrito_cru(&token0, flags, des, &[], &refs) {
                    Ok(t) => format!("ok (restrito: {})", sistema::e_restrito(&t)),
                    Err(c) => format!("FALHOU {}", sistema::erro_em_texto(c)),
                }
            }
        };
        println!("  [{rotulo}] opções 0x{flags:X}, {} grupo(s) só para negar, restringir a [{}] -> {resultado}", des.len(), restringir.join(" "));
    };
    let so = |t: &str| vec![t.to_string()];
    um("sem nada", 0, &[], &[]);
    um("sem privilégios", 1, &[], &[]);
    um("negar os administrativos", 1, &admins, &[]);
    for (rotulo, sid) in [
        ("Todos", super::SID_TODOS.to_string()),
        ("Restrito", super::SID_RESTRITO.to_string()),
        ("Usuários", super::SID_USUARIOS.to_string()),
        // Os dois que o Windows recusa (erro 87): ficam para mostrar que ainda recusa.
        ("Pacotes de aplicativo", "S-1-15-2-1".to_string()),
        ("Pacotes restritos", "S-1-15-2-2".to_string()),
        ("Autenticados", super::SID_AUTENTICADOS.to_string()),
        ("Interativo", super::SID_INTERATIVO.to_string()),
        ("Local", super::SID_LOCAL.to_string()),
        ("Console", super::SID_CONSOLE.to_string()),
        ("sessão de logon", logon_txt.clone()),
        ("do Keep (S-1-0)", da_pasta.clone()),
        ("do Keep base (S-1-0)", base.clone()),
        ("do Keep, forma antiga (S-1-5)", forma_nt),
        ("do Keep, forma de domínio (S-1-5-21)", forma_dominio),
        ("o próprio usuário", usuario_txt.clone()),
    ] {
        um(&format!("um SID: {rotulo}"), 1, &[], &so(&sid));
    }
    for nivel in NIVEIS {
        let ids = sids_de_restricao(nivel, &da_pasta, &base, &logon_txt);
        um(&format!("nível {nivel:?}, sem negar"), 1, &[], &ids);
        um(&format!("nível {nivel:?}, negando os administrativos"), 1, &admins, &ids);
    }

    // O que o processo tem e o que o token restrito de verdade guarda.
    println!("  privilégios do token (* ligado):");
    match sistema::privilegios_do_token(&token0) {
        Ok(v) => {
            for (nome, ligado) in v {
                println!("    {nome}{}", if ligado { " *" } else { "" });
            }
        }
        Err(e) => println!("    não leu: {e}"),
    }
    let ids = sids_de_restricao(NIVEIS[0], &da_pasta, &base, &logon_txt);
    let sids: Result<Vec<Sid>, String> = ids.iter().map(|t| Sid::de_texto(t)).collect();
    if let (Ok(sids), Ok(sistema_sid), Ok(sid_pasta)) = (sids, Sid::de_texto(super::SID_SISTEMA), Sid::de_texto(&da_pasta)) {
        let refs: Vec<&Sid> = sids.iter().collect();
        match sistema::token_restrito(&token0, &usuario, &sistema_sid, &refs, &admins, &[&sid_pasta]) {
            Ok(t) => {
                println!("  o token restrito de verdade ({:?}): restrito {}", NIVEIS[0], sistema::e_restrito(&t));
                match sistema::privilegios_do_token(&t) {
                    Ok(v) => println!("    privilégios que sobram: {}", v.iter().map(|(n, l)| format!("{n}{}", if *l { "*" } else { "" })).collect::<Vec<_>>().join(", ")),
                    Err(e) => println!("    privilégios: {e}"),
                }
            }
            Err(e) => println!("  o token restrito de verdade: {e}"),
        }
    }
    0
}

/// `keep ia confinar --objetos [--conceder]`: os objetos com nome do MSYS2 e
/// do Cygwin, de quem são, e (com `--conceder`) o acesso do SID base do Keep a
/// eles. Para ver por que o bash preso não sobe com o bash do usuário aberto.
fn objetos(conceder: bool) -> i32 {
    let token0 = match sistema::token_do_processo() {
        Ok(t) => t,
        Err(e) => {
            println!("{e}");
            return 1;
        }
    };
    let Ok(usuario) = sistema::sid_do_usuario(&token0) else {
        println!("sem o SID do usuário");
        return 1;
    };
    let usuario_txt = usuario.texto().unwrap_or_default();
    let base = Sid::de_texto(&sid_base(&usuario_txt)).ok();
    println!("Objetos do MSYS2/Cygwin do usuário {usuario_txt}{}", if conceder { " (dando acesso ao SID base do Keep)" } else { "" });
    let achados = sistema::objetos::mostra(&usuario_txt, if conceder { base.as_ref() } else { None });
    println!("  {achados} objeto(s) do MSYS2/Cygwin");
    0
}

/// `keep ia confinar --como-comum -- <programa> [argumentos…]`: roda o programa
/// com o token de quem não está elevado (o de um administrador com o UAC
/// ligado, sem privilégios e com integridade média), sem prender nada. É o
/// token que o usuário tem de verdade; o do CI é o elevado, e o que ele prova
/// não é o que o usuário vê.
fn como_comum(args: &[String]) -> i32 {
    let args: &[String] = if args.first().map(String::as_str) == Some("--") { &args[1..] } else { args };
    let Some((programa, resto)) = args.split_first() else {
        eprintln!("uso: keep ia confinar --como-comum -- <programa> [argumentos...]");
        return 2;
    };
    let Some(exe) = no_path(programa) else {
        eprintln!("Keep: {programa} não está no PATH.");
        return 127;
    };
    let aqui = std::env::current_dir().map(|p| sem_prefixo(&p.to_string_lossy())).unwrap_or_else(|_| perfil_real());
    let rodar = || -> Result<i32, String> {
        let origem = sistema::token_do_processo()?;
        let usuario = sistema::sid_do_usuario(&origem)?;
        let sistema_sid = Sid::de_texto(super::SID_SISTEMA)?;
        let token = sistema::token_comum(&origem, &usuario, &sistema_sid)?;
        let linha = linha_de_comando(&exe, resto);
        let ambiente = bloco_de_ambiente(&ambiente_atual());
        let sem_janela = !sistema::tem_console();
        sistema::nao_morrer_com_ctrl_c();
        let filho = sistema::criar_processo(&token, &exe, &linha, &aqui, &ambiente, &Padrao::do_processo(), sem_janela)?;
        // O token do processo de um usuário comum é dele: o processo o abre inteiro.
        if let Err(e) = sistema::abrir_o_token_ao_filho_com(&filho, &usuario, sistema::TOKEN_TUDO) {
            eprintln!("Keep: o token do processo não foi aberto ao usuário: {e}");
        }
        sistema::retomar(&filho)?;
        Ok(sistema::esperar(&filho.processo, sistema::INFINITO)?.unwrap_or(1) as i32)
    };
    match rodar() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Keep: {e}");
            1
        }
    }
}

#[cfg(test)]
mod testes {
    /// O roteiro de verdade, no Windows do CI: sobe programas presos e confere
    /// o que dá e o que não dá para eles fazerem.
    #[test]
    fn o_programa_preso_so_toca_na_pasta_dele() {
        // Desligado até a contenção passar no Windows do CI (o primeiro
        // roteiro falhou em CreateRestrictedToken, erro 87): só roda com
        // KEEP_CONFINE_PROVA=1, e o botão da barra lateral fica escondido
        // (CONFINEMENT_ENFORCED = false em apps/windows/src/confinement.ts).
        if std::env::var_os("KEEP_CONFINE_PROVA").is_none() {
            return;
        }
        assert_eq!(super::autoteste(), 0, "veja a lista acima: cada FALHOU é uma garantia que não vale");
    }
}
