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
    Ambiente, Entrada, NIVEIS, Nivel, Quem, ambiente_preso, marca, marcas_de, bloco_de_ambiente, dentro_de, eh_script_do_cmd, falha_de_partida,
    junta, linha_de_comando, linha_de_script, lugar_da_isca, pasta_larga_demais, plano, procura_programa, sem_prefixo,
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
    sistema::conceder(&c.caminho, sid, c.acesso, c.heranca)?;
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
    let casa = sem_prefixo(&caminhos::casa().to_string_lossy());
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
    // O que a prova tenta listar é o perfil do usuário de verdade, que a
    // restrição tem de esconder (a "casa" do plano pode ser a de um teste).
    let perfil = amb.var("USERPROFILE").map(|v| sem_prefixo(&v)).unwrap_or_else(|| casa.clone());

    // O programa, e como ele é chamado.
    let (app, linha) = if eh_script_do_cmd(&p.programa) {
        let cmd = std::env::var("ComSpec").unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".into());
        let linha = linha_de_script(&cmd, &p.programa, &p.args)?;
        (cmd, linha)
    } else {
        (p.programa.clone(), linha_de_comando(&p.programa, &p.args))
    };
    let ambiente = bloco_de_ambiente(&ambiente_preso(ambiente_atual(), &plano.temp, &p.pasta));
    let sem_janela = !sistema::tem_console();
    let job = sistema::job_que_mata();
    if let Err(e) = &job {
        rastro(|| format!("sem Job (a IA não morre junto com a aba): {e}"));
    }

    if p.segurar_ctrl_c {
        sistema::nao_morrer_com_ctrl_c();
    }
    let mut tentativas: Vec<String> = Vec::new();
    for nivel in NIVEIS {
        let sids: Vec<Sid> = sids_de_restricao(nivel, &pasta_txt, &base_txt, &logon_txt)
            .iter()
            .map(|s| Sid::de_texto(s))
            .collect::<Result<_, _>>()?;
        let a_restringir: Vec<&Sid> = sids.iter().collect();
        // A DACL padrão do token: o usuário, o sistema e a pasta. Não o SID base,
        // que todo workspace preso leva: com ele o processo de um workspace
        // abriria os objetos (e o processo) de outro.
        let token = sistema::token_restrito(&token0, &usuario, &sistema_sid, &a_restringir, &administrativos, &[&sid_pasta])?;
        if !sistema::e_restrito(&token) {
            return Err("o token que devia ser restrito não é".into());
        }

        // A prova: com este token o de fora é negado e a pasta funciona. Um
        // sandbox que não contém não sobe a IA, e outro nível não o conserta.
        let sonda = sistema::como_o_token(&token, || {
            sondar_fs(Path::new(&p.pasta), Path::new(&isca), Path::new(&perfil), Path::new(&pasta_da_isca))
        })?;
        rastro(|| format!("prova ({nivel:?}): {sonda:?}"));
        sonda.conclusao()?;

        let filho = match sistema::criar_processo(&token, &app, &linha, &p.onde, &ambiente, padrao, sem_janela) {
            Ok(f) => f,
            Err(e) => {
                tentativas.push(format!("{nivel:?}: {e}"));
                continue;
            }
        };
        if let Ok(j) = &job {
            if let Err(e) = sistema::no_job(j, &filho) {
                rastro(|| format!("fora do Job: {e}"));
            }
        }
        if let Err(e) = sistema::abrir_o_token_ao_filho(&filho, &sid_pasta) {
            rastro(|| format!("o token do filho não foi aberto a ele: {e}"));
        }
        sistema::retomar(&filho)?;
        let cedo = sistema::esperar(&filho.processo, OLHADA_NA_PARTIDA_MS)?;
        if let Some(c) = cedo.filter(|c| falha_de_partida(*c)) {
            tentativas.push(format!("{nivel:?}: o programa morreu ao partir (0x{c:08X})"));
            continue;
        }
        // Vivo depois do olhar: se há console aqui, ele tem de se ligar ao
        // daqui. Um programa que subiu sem o console da aba ficaria mudo e
        // parado; dito agora, o usuário sabe por quê.
        if cedo.is_none() && !sem_janela && !ligou_ao_console(&filho) {
            sistema::matar(&filho);
            tentativas.push(format!("{nivel:?}: o console da aba não chegou ao programa"));
            continue;
        }
        let codigo = match cedo {
            Some(c) => c,
            None => sistema::esperar(&filho.processo, sistema::INFINITO)?.unwrap_or(1),
        };
        rastro(|| format!("o programa saiu com {codigo} (nível {nivel:?})"));
        if nivel != Nivel::Estrito {
            rastro(|| "nível mais aberto que o estrito: outras unidades de disco podem estar legíveis".to_string());
        }
        return Ok(codigo as i32);
    }
    Err(format!("o Windows não deixou o programa partir com o token restrito ({})", tentativas.join("; ")))
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
    if args.first().map(String::as_str) == Some("--autoteste") {
        return autoteste();
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
    let casa = sem_prefixo(&caminhos::casa().to_string_lossy());
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
    let pedido = Pedido { pasta, programa: exe.replace('/', "\\"), args: linha, onde, segurar_ctrl_c: true };
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
    let casa = sem_prefixo(&caminhos::casa().to_string_lossy());
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
        std::fs::write(&roteiro, format!("@echo off\r\n{comando}\r\nexit /b %errorlevel%\r\n")).map_err(|e| e.to_string())?;
        let saida_arq = junta(&raiz, &format!("saida{passo}.txt"));
        let saida = std::fs::File::create(&saida_arq).map_err(|e| e.to_string())?;
        let padrao =
            if com_console { Padrao::do_processo() } else { Padrao::de_arquivos(None, Some(&saida), Some(&saida)) };
        let pedido =
            Pedido { pasta: permitida.clone(), programa: roteiro, args: vec![], onde: permitida.clone(), segurar_ctrl_c: false };
        let codigo = roda(&pedido, &padrao)?;
        drop(padrao);
        let texto = std::fs::read_to_string(&saida_arq).unwrap_or_default();
        Ok((codigo, texto))
    };

    let a = junta(&permitida, "a.txt");
    let b = junta(&fora, "b.txt");
    let c = junta(&fora, "c.txt");
    let d = junta(&permitida, "d.txt");
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
    match roda_linha(&format!("echo x> \"{c}\""), false) {
        Ok((n, _)) => confere("não grava fora", n != 0 && !Path::new(&c).exists(), &format!("saiu com {n}")),
        Err(e) => confere("não grava fora", false, &e),
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
    let _ = std::fs::remove_dir_all(&raiz);
    println!("{}", if falhas == 0 { "tudo certo" } else { "há falhas: a IA não deve ser tida como presa neste computador" });
    i32::from(falhas > 0)
}

#[cfg(test)]
mod testes {
    /// O roteiro de verdade, no Windows do CI: sobe programas presos e confere
    /// o que dá e o que não dá para eles fazerem.
    #[test]
    fn o_programa_preso_so_toca_na_pasta_dele() {
        assert_eq!(super::autoteste(), 0, "veja a lista acima: cada FALHOU é uma garantia que não vale");
    }
}
