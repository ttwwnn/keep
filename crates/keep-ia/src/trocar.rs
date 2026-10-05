//! Trocar a IA de uma aba: sair do Claude ou do Codex que roda nela (só com
//! a aba livre, ou com a pessoa dizendo que pode interromper), e subir a IA
//! escolhida na mesma conversa — ou, mudando de motor, numa conversa nova
//! com o contexto da anterior num arquivo privado.
//!
//! Porte do `keep-ia trocar` do kit. A aba é digitada pelo daemon (como a
//! pessoa digitaria); a conta vai no ambiente da linha que sobe a IA, nunca
//! num login copiado.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::abas::{self, SEGUIR_ORDEM};
use crate::contas::{self, Conta, Falta, Motor};
use crate::daemon::{self, Retrato, Teclado};
use crate::historico::{self, Origem};
use crate::linha::{self, Subida};
use crate::sessoes::{self, SessaoClaude};
use crate::tela::{self, Estado, Programa};
use crate::{caminhos, codex_daemon, contexto, processos, programas, uso};

/// Por que não trocou: o motivo (para o app decidir) e o que dizer.
#[derive(Clone, Debug, PartialEq)]
pub struct Recusa {
    pub motivo: String,
    pub detalhe: String,
    pub extras: Map<String, Value>,
}

impl Recusa {
    pub fn nova(motivo: &str, detalhe: impl Into<String>) -> Recusa {
        Recusa { motivo: motivo.into(), detalhe: detalhe.into(), extras: Map::new() }
    }

    pub fn json(&self) -> Value {
        let mut m = self.extras.clone();
        m.insert("ok".into(), json!(false));
        m.insert("motivo".into(), json!(self.motivo));
        m.insert("detalhe".into(), json!(self.detalhe));
        Value::Object(m)
    }
}

fn erro(detalhe: impl Into<String>) -> Recusa {
    Recusa::nova("erro", detalhe)
}

/// O pedido de uma troca.
#[derive(Clone, Debug, Default)]
pub struct Pedido {
    pub ws: String,
    pub aba: u32,
    /// A escolha: `claude:ordem`, `claude:<ap>`, `gpt:<ap>`.
    pub para: String,
    pub interromper: bool,
    /// A aba está parada na tela de limite: um Esc fecha a espera.
    pub cancelar_limite: bool,
    /// Para `claude:ordem`: a conta em que rodar (a sincronização decide).
    pub conta: Option<String>,
}

/// O nome que uma chave tem para quem lê.
pub fn rotulo(chave: &str) -> String {
    if chave == SEGUIR_ORDEM {
        return "Claude (ordem de prioridade)".into();
    }
    match chave.split_once(':') {
        Some(("gpt", ap)) => format!("GPT · {ap}"),
        Some((_, ap)) => format!("Claude · {ap}"),
        None => chave.into(),
    }
}

// --------------------------------------------------------------- destino

/// Onde a aba vai rodar: a escolha dela, a conta e o ambiente da conta.
struct Destino {
    escolha: String,
    conta: Conta,
    ambiente: Vec<(String, String)>,
}

impl Destino {
    fn motor(&self) -> Motor {
        self.conta.engine
    }
}

/// As contas que podem receber trabalho agora, pela última medição.
fn disponiveis() -> Vec<String> {
    uso::em_cache().into_iter().filter(|l| l.disponivel()).map(|l| l.account.key).collect()
}

fn destino(p: &Pedido, contas: &[Conta]) -> Result<Destino, Recusa> {
    if p.para == SEGUIR_ORDEM {
        if contas::gerente_externo() {
            // O kit troca a conta do login global: seguir a ordem é rodar nele.
            let conta = contas
                .iter()
                .find(|c| c.engine == Motor::Claude && c.global())
                .or_else(|| contas.iter().find(|c| c.engine == Motor::Claude))
                .ok_or_else(|| Recusa::nova("sem-conta", "Não há conta do Claude neste Mac."))?;
            return Ok(Destino { escolha: p.para.clone(), conta: conta.clone(), ambiente: Vec::new() });
        }
        if let Some(chave) = &p.conta {
            let c = contas::achar(contas, chave)
                .ok_or_else(|| Recusa::nova("sem-conta", format!("Não há a conta {}.", rotulo(chave))))?;
            let ambiente = contas::ambiente(c).map_err(|_| {
                Recusa::nova("sem-conta", format!("A conta {} não tem login em que uma aba rode.", rotulo(chave)))
            })?;
            return Ok(Destino { escolha: p.para.clone(), conta: c.clone(), ambiente });
        }
        // Seguir a ordem fica no Claude: o GPT só quando a pessoa o escolhe
        // no menu da aba, nunca por conta própria.
        let livres = disponiveis();
        let mut primeira_que_roda: Option<(&Conta, Vec<(String, String)>)> = None;
        for c in contas.iter().filter(|c| c.roda && c.engine == Motor::Claude) {
            let Ok(ambiente) = contas::ambiente(c) else { continue };
            if livres.contains(&c.key) {
                return Ok(Destino { escolha: p.para.clone(), conta: c.clone(), ambiente });
            }
            if primeira_que_roda.is_none() {
                primeira_que_roda = Some((c, ambiente));
            }
        }
        // Todas no limite: a primeira Claude que roda, para a aba não ficar sem IA.
        if let Some((c, ambiente)) = primeira_que_roda {
            return Ok(Destino { escolha: p.para.clone(), conta: c.clone(), ambiente });
        }
        return Err(Recusa::nova(
            "sem-conta",
            "Não há conta em que uma aba possa rodar. Entre numa pelo + do rodapé.",
        ));
    }
    let c = contas::achar(contas, &p.para).ok_or_else(|| {
        Recusa::nova("sem-conta", format!("Não há a conta {}. Entre nela pelo + do rodapé.", rotulo(&p.para)))
    })?;
    match contas::ambiente(c) {
        Ok(ambiente) => Ok(Destino { escolha: p.para.clone(), conta: c.clone(), ambiente }),
        Err(Falta::PrecisaLogin { apelido, .. }) => {
            let email = c.email.clone().unwrap_or_default();
            let (ws_login, aba_login, abriu) =
                crate::login::abre_login_da_conta(&apelido, &email, &c.key, &p.ws, p.aba, &p.para).map_err(erro)?;
            let nome = format!("Entrar no Claude · {apelido}");
            let onde = if abriu {
                format!("Abri a aba “{nome}” neste workspace")
            } else {
                format!("O login dela já está aberto na aba “{nome}” (workspace {ws_login})")
            };
            let mut r = Recusa::nova(
                "precisa-login",
                format!(
                    "A conta {apelido} ainda não tem o login próprio das abas (uma aprovação no navegador, uma vez só). \
                     {onde}: aprove lá e esta aba passa para Claude · {apelido} sozinha. A conversa desta aba não foi mexida."
                ),
            );
            r.extras.insert("ws_login".into(), json!(ws_login));
            r.extras.insert("aba_login".into(), json!(aba_login));
            Err(r)
        }
        Err(Falta::SemConta(_)) => Err(Recusa::nova("sem-conta", format!("Não há a conta {}.", rotulo(&p.para)))),
    }
}

// ------------------------------------------------------------- a aba

fn aba_viva(ws: &str, tab: u32) -> Result<(Retrato, daemon::Aba), Recusa> {
    let r = daemon::listar().map_err(|e| erro(format!("o Keep recusou: {e}")))?;
    let a = r.aba(ws, tab).cloned().ok_or_else(|| erro("A aba não existe mais."))?;
    Ok((r, a))
}

/// As linhas da tela (sem cor; sem cor nem esmaecido).
pub fn tela_de(ws: &str, tab: u32) -> (Vec<String>, Vec<String>) {
    daemon::tela_vt(ws, tab).map(|vt| tela::as_duas(&vt)).unwrap_or_default()
}

/// Digita `texto`; só aperta Enter se ele aparecer na tela. `true` se mandou.
pub fn digita(ws: &str, tab: u32, texto: &str, enter: bool) -> Result<bool, Recusa> {
    let (_, a) = aba_viva(ws, tab)?;
    let mut t = Teclado::liga(&a).map_err(erro)?;
    t.manda(texto.as_bytes()).map_err(erro)?;
    let m = linha::marca(texto);
    let visto = daemon::espera(Duration::from_secs(5), Duration::from_millis(150), || {
        daemon::tela(ws, tab).ok().filter(|tela| linha::chegou(tela, &m)).map(|_| ())
    });
    if visto.is_none() {
        return Ok(false);
    }
    if enter {
        t.manda(b"\r").map_err(erro)?;
        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(true)
}

pub fn tecla(ws: &str, tab: u32, dados: &[u8]) -> Result<(), Recusa> {
    let (_, a) = aba_viva(ws, tab)?;
    let mut t = Teclado::liga(&a).map_err(erro)?;
    t.manda(dados).map_err(erro)?;
    std::thread::sleep(Duration::from_millis(300));
    Ok(())
}

/// A aba voltou ao shell (programa da frente) e o prompt parou de mudar?
struct ShellPronto {
    ws: String,
    tab: u32,
    anterior: Option<String>,
    estavel: u32,
}

impl ShellPronto {
    fn novo(ws: &str, tab: u32) -> ShellPronto {
        ShellPronto { ws: ws.into(), tab, anterior: None, estavel: 0 }
    }

    fn agora(&mut self) -> bool {
        let pronta = daemon::listar().ok().and_then(|r| r.aba(&self.ws, self.tab).cloned()).is_some_and(|a| {
            matches!(tela::programa(&a.info.command), Programa::Shell(_)) && !a.info.busy
        });
        if !pronta {
            self.estavel = 0;
            return false;
        }
        let t = daemon::tela(&self.ws, self.tab).ok();
        self.estavel = if t.is_some() && t == self.anterior { self.estavel + 1 } else { 0 };
        self.anterior = t;
        self.estavel >= 2
    }
}

fn espera_shell(ws: &str, tab: u32, segundos: u64) -> bool {
    let mut p = ShellPronto::novo(ws, tab);
    daemon::espera(Duration::from_secs(segundos), Duration::from_millis(250), || p.agora().then_some(())).is_some()
}

/// O programa da aba virou `agente`?
fn subiu(ws: &str, tab: u32, agente: &str, segundos: u64) -> bool {
    daemon::espera(Duration::from_secs(segundos), Duration::from_millis(250), || {
        daemon::listar()
            .ok()
            .and_then(|r| r.aba(ws, tab).cloned())
            .filter(|a| tela::programa(&a.info.command).agente() == Some(agente))
            .map(|_| ())
    })
    .is_some()
}

/// A situação da aba: a tela, e a sessão do Claude quando ela prova mais.
fn situacao(a: &daemon::Aba, prog: &Programa, sessao: Option<&SessaoClaude>) -> (Estado, String) {
    if let Programa::Shell(_) = prog {
        return if a.info.busy { (Estado::Ocupada, "o shell está rodando algo".into()) } else { (Estado::Livre, String::new()) };
    }
    let (linhas, limpas) = tela_de(&a.ws, a.info.id);
    let (mut est, mut detalhe) = tela::situacao(prog, &linhas, &limpas);
    if est == Estado::Livre {
        if let Some(s) = sessao {
            if s.trabalhando() {
                (est, detalhe) = (Estado::Ocupada, "trabalhando".into());
            } else if s.esperando() {
                (est, detalhe) = (Estado::Dialogo, "com uma pergunta aberta".into());
            }
        }
    }
    (est, detalhe)
}

// ------------------------------------------------------------ processos

/// Encerra um processo e o que ele deixou rodando.
fn mata(pids: &[u32]) {
    #[cfg(unix)]
    {
        for sinal in [libc::SIGTERM, libc::SIGKILL] {
            let vivos: Vec<u32> = pids.iter().copied().filter(|&p| processos::vivo(p)).collect();
            for p in &vivos {
                unsafe {
                    if libc::getpgid(*p as i32) == *p as i32 {
                        libc::killpg(*p as i32, sinal);
                    } else {
                        libc::kill(*p as i32, sinal);
                    }
                }
            }
            if vivos.is_empty() {
                break;
            }
            let _ = daemon::espera(Duration::from_secs(3), Duration::from_millis(100), || {
                vivos.iter().all(|&p| !processos::vivo(p)).then_some(())
            });
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
        for &p in pids {
            let h = unsafe { OpenProcess(PROCESS_TERMINATE, 0, p) };
            if !h.is_null() {
                unsafe {
                    TerminateProcess(h, 1);
                    CloseHandle(h);
                }
            }
        }
    }
}

/// O ambiente de um comando auxiliar do Claude (`stop`, `attach`, `auth`):
/// sem as variáveis que o CLI herda de uma sessão (CLAUDECODE, CLAUDE_CODE_*).
pub fn ambiente_limpo(cmd: &mut std::process::Command) {
    for (n, _) in std::env::vars_os() {
        let n = n.to_string_lossy().into_owned();
        if n == "CLAUDECODE" || n.starts_with("CLAUDE_CODE_") || n == "CLAUDE_SECURESTORAGE_CONFIG_DIR" {
            cmd.env_remove(&n);
        }
    }
}

fn claude_bin() -> String {
    programas::claude().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "claude".into())
}

fn codex_bin() -> String {
    programas::codex().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "codex".into())
}

fn descreve_job(j: &SessaoClaude) -> String {
    let nome = j.nome.clone().unwrap_or_else(|| j.id.chars().take(8).collect());
    format!("{nome}, job {}", j.job.as_deref().unwrap_or("?"))
}

/// `claude stop <job>`, e os processos que o job deixou (o `stop` não para
/// os shells das tarefas dele).
fn encerra_job(j: &SessaoClaude) -> Result<(), Recusa> {
    let filhos = processos::descendentes(j.pid);
    let job = j.job.clone().unwrap_or_default();
    let mut cmd = std::process::Command::new(claude_bin());
    cmd.args(["stop", &job]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    ambiente_limpo(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let _ = cmd.status().map_err(|e| erro(format!("Não consegui parar a conversa em segundo plano ({}): {e}", descreve_job(j))))?;
    let parou = daemon::espera(Duration::from_secs(15), Duration::from_millis(300), || {
        sessoes::job_vivo(&job).is_none().then_some(())
    });
    if parou.is_none() {
        return Err(erro(format!("A conversa em segundo plano ({}) não parou; nada foi trocado.", descreve_job(j))));
    }
    mata(&filhos);
    Ok(())
}

/// O `claude attach <job>` que a aba mostra, se for isso: a conversa (e a
/// conta) são do processo do job.
fn job_anexado(frente: u32) -> Option<SessaoClaude> {
    let argv = processos::argv(frente)?;
    let i = argv.iter().take(4).position(|a| a == "attach")?;
    sessoes::job_vivo(argv.get(i + 1)?)
}

// --------------------------------------------------------------- sair

fn sai_do_claude(ws: &str, tab: u32, interromper: bool, sintaxe: linha::Sintaxe) -> Result<(), Recusa> {
    let ja_na_tela = tela::jobs_na_tela(&tela_de(ws, tab).0);
    if !digita(ws, tab, "/exit", true)? {
        return Err(Recusa::nova("tela-inesperada", "O /exit não apareceu na caixa; nada foi feito."));
    }
    let mut pronto = ShellPronto::novo(ws, tab);
    let mut respondida = false;
    let fim = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < fim {
        let linhas = tela_de(ws, tab).0;
        let novos: Vec<String> = tela::jobs_na_tela(&linhas).into_iter().filter(|j| !ja_na_tela.contains(j)).collect();
        if let Some(job) = novos.last() {
            // Alguém escolheu "Move to background": a conversa segue no job,
            // com outro id. Retomá-la aqui daria uma segunda cópia viva.
            if daemon::espera(Duration::from_secs(5), Duration::from_millis(250), || sessoes::job_vivo(job)).is_some() {
                let s = Subida { programa: claude_bin(), args: vec!["attach".into(), job.clone()], ..Default::default() };
                let _ = digita(ws, tab, &linha::linha(sintaxe, &s), true);
                return Err(Recusa::nova(
                    "segundo-plano",
                    format!("A conversa foi para segundo plano (job {job}) em vez de sair; voltei com ela para esta aba e nada foi trocado."),
                ));
            }
            return Err(Recusa::nova(
                "segundo-plano",
                format!("A conversa foi para segundo plano (job {job}) em vez de sair; nada foi trocado. Para voltar a ela: claude attach {job}"),
            ));
        }
        if pronto.agora() {
            return Ok(());
        }
        if let Some(p) = tela::pergunta_de_saida(&linhas) {
            if !respondida {
                let tarefas = if p.tarefas.is_empty() { "sem a lista".to_string() } else { p.tarefas.join(", ") };
                if interromper {
                    if let Some(sair) = &p.sair {
                        tecla(ws, tab, sair.as_bytes())?;
                        respondida = true;
                        continue;
                    }
                }
                tecla(ws, tab, p.ficar.as_deref().unwrap_or("\x1b").as_bytes())?;
                let _ = daemon::espera(Duration::from_secs(5), Duration::from_millis(250), || {
                    tela::pergunta_de_saida(&tela_de(ws, tab).0).is_none().then_some(())
                });
                return Err(Recusa::nova(
                    "ocupada",
                    format!("A conversa tem trabalho em segundo plano ({tarefas}), que para quando ela sai do Claude."),
                ));
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let linhas = tela_de(ws, tab).0;
    let fim: String = linhas.iter().filter(|l| !l.trim().is_empty()).rev().take(2).cloned().collect::<Vec<_>>().join(" ");
    Err(Recusa::nova("tela-inesperada", format!("O Claude não saiu (ainda na tela: “{}”).", fim.chars().take(120).collect::<String>())))
}

// ---------------------------------------------------------------- subir

fn home_codex(contas: &[Conta], chave: Option<&str>) -> PathBuf {
    chave
        .and_then(|k| contas::achar(contas, k))
        .and_then(|c| c.codex_home().map(|(h, _)| h.clone()))
        .unwrap_or_else(caminhos::codex)
}

/// A linha que sobe a IA do destino.
fn subida(d: &Destino, ident: Option<&str>, parametros: &[String], contexto: Option<&Path>) -> Subida {
    let mut args = Vec::new();
    let programa = match d.motor() {
        Motor::Claude => {
            args.extend(parametros.iter().cloned());
            if let Some(id) = ident {
                args.push("--resume".into());
                args.push(id.into());
            }
            claude_bin()
        }
        Motor::Codex => {
            if let Some(id) = ident {
                args.push("resume".into());
                args.push(id.into());
            }
            args.extend(parametros.iter().cloned());
            codex_bin()
        }
    };
    if let Some(arq) = contexto {
        args.push(contexto::prompt(arq));
    }
    let mut definir = d.ambiente.clone();
    definir.push(("KEEP_IA_ESCOLHA".into(), d.escolha.clone()));
    Subida {
        programa,
        args,
        definir,
        tirar: ["CLAUDE_SECURESTORAGE_CONFIG_DIR", "CODEX_HOME", "KEEP_IA_ESCOLHA", "CLAUDE_KIT_CONTA"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        pasta: None,
    }
}

/// Onde subir a IA quando a pasta da aba sumiu (uma worktree apagada): a
/// pasta em que a conversa do Claude começou, se ainda existe, ou a casa.
/// Com a pasta da aba de pé, nenhuma — sobe onde a aba está.
fn pasta_de_partida(a: &daemon::Aba, conversa_claude: Option<&str>) -> Option<String> {
    let cwd = a.info.cwd.trim();
    if cwd.is_empty() || Path::new(cwd).is_dir() {
        return None;
    }
    conversa_claude
        .and_then(pasta_onde_comecou)
        .filter(|p| Path::new(p).is_dir())
        .or_else(|| Some(caminhos::casa().to_string_lossy().into_owned()))
}

/// A pasta da primeira fala da conversa `id` do Claude: é por ela que o
/// `--resume` acha o transcrito.
fn pasta_onde_comecou(id: &str) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};
    let t = sessoes::transcritos_claude(id).into_iter().next()?;
    let f = std::fs::File::open(t).ok()?;
    BufReader::new(f.take(4 * 1024 * 1024)).lines().map_while(Result::ok).find_map(|l| {
        if !l.contains("\"cwd\"") {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(&l).ok()?.get("cwd")?.as_str().map(str::to_string)
    })
}

fn sintaxe_da_aba(a: &daemon::Aba, vinculo_shell: Option<u32>) -> linha::Sintaxe {
    if let Programa::Shell(nome) = tela::programa(&a.info.command) {
        return linha::sintaxe(&nome);
    }
    vinculo_shell
        .and_then(processos::um)
        .map(|p| linha::sintaxe(&p.nome))
        .unwrap_or(if cfg!(windows) { linha::Sintaxe::PowerShell } else { linha::Sintaxe::Posix })
}

/// Sobe de novo o Codex na mesma conversa e conta: a troca tirou a tela de
/// um turno que segue rodando no daemon.
fn devolve_ao_codex(
    ws: &str,
    tab: u32,
    tid: &str,
    conta: Option<&Conta>,
    escolha: &str,
    parametros: &[String],
    sintaxe: linha::Sintaxe,
) -> bool {
    let Some(conta) = conta else { return false };
    let Ok(ambiente) = contas::ambiente(conta) else { return false };
    let d = Destino { escolha: escolha.into(), conta: conta.clone(), ambiente };
    let s = subida(&d, Some(tid), parametros, None);
    matches!(digita(ws, tab, &linha::linha(sintaxe, &s), true), Ok(true)) && subiu(ws, tab, "codex", 10)
}

// ---------------------------------------------------------------- trocar

/// Uma troca por aba de cada vez, entre todos os processos.
struct TravaDaAba(std::fs::File);

fn trava_da_aba(ws: &str, tab: u32) -> Result<TravaDaAba, Recusa> {
    use sha2::{Digest, Sha256};
    let nome: String = Sha256::digest(format!("{}|{ws}|{tab}", daemon::socket().display()).as_bytes())
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect();
    let dir = caminhos::estado();
    let _ = std::fs::create_dir_all(&dir);
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("troca-{nome}.trava")))
        .map_err(|e| erro(e.to_string()))?;
    f.try_lock().map_err(|_| Recusa::nova("ocupada", "A conta desta aba já está sendo trocada."))?;
    Ok(TravaDaAba(f))
}

impl Drop for TravaDaAba {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// A troca inteira. `Ok` diz o que foi feito.
pub fn trocar(p: &Pedido) -> Result<String, Recusa> {
    if !abas::chave_valida(&p.para) {
        return Err(Recusa::nova("uso", format!("“{}” não é uma conta.", p.para)));
    }
    let _trava = trava_da_aba(&p.ws, p.aba)?;
    let contas = contas::listar(true);
    let destino = destino(p, &contas)?;
    let agente_alvo = destino.motor().programa();
    let (r, a) = aba_viva(&p.ws, p.aba)?;
    let prog = tela::programa(&a.info.command);
    match &prog {
        Programa::Outro(nome) => {
            return Err(Recusa::nova("outro-programa", format!("Esta aba está rodando {nome}. Feche-o antes de escolher a IA.")));
        }
        Programa::Desconhecido => {
            return Err(Recusa::nova("tela-inesperada", "A aba ainda está abrindo; tente de novo em instantes."));
        }
        _ => {}
    }
    let vinc = crate::vinculo::vincular(&r).get(&(p.ws.clone(), p.aba)).copied();
    let frente = vinc.map(|v| v.frente);
    let sintaxe = sintaxe_da_aba(&a, vinc.map(|v| v.shell));
    if prog.agente().is_some() && frente.is_none() {
        return Err(Recusa::nova(
            "tela-inesperada",
            "Não deu para saber que processo roda nesta aba; tente de novo em instantes.",
        ));
    }
    let anexo = if prog == Programa::Claude { frente.and_then(job_anexado) } else { None };
    let (escolha_agora, atual_agora) = match (prog.agente(), frente) {
        (Some(ag), Some(f)) => {
            let (e, at) = abas::conta_do_processo(anexo.as_ref().map(|j| j.pid).unwrap_or(f), ag, &contas);
            (Some(e), at)
        }
        _ => (None, None),
    };
    if prog.agente() == Some(agente_alvo)
        && escolha_agora.as_deref() == Some(destino.escolha.as_str())
        && atual_agora.as_deref() == Some(destino.conta.order_key.as_str())
    {
        return Ok(format!("a aba já está em {}", rotulo(&destino.conta.order_key)));
    }
    let sessao = if prog == Programa::Claude && anexo.is_none() { frente.and_then(sessoes::sessao_claude) } else { None };

    // A situação da tela.
    let linhas = tela_de(&p.ws, p.aba).0;
    let trabalho = if prog == Programa::Claude && anexo.is_none() { tela::segundo_plano_no_rodape(&linhas) } else { Vec::new() };
    let fundo_txt = if trabalho.is_empty() {
        String::new()
    } else {
        format!(" e tem trabalho em segundo plano ({}), que para quando ela sai do Claude", trabalho.join(", "))
    };
    let (mut est, mut detalhe) = situacao(&a, &prog, sessao.as_ref());
    if est == Estado::Dialogo && p.cancelar_limite && tela::espera_limite(&linhas) {
        for _ in 0..2 {
            tecla(&p.ws, p.aba, b"\x1b")?;
            let _ = daemon::espera(Duration::from_secs(3), Duration::from_millis(200), || {
                (situacao(&a, &prog, None).0 == Estado::Livre).then_some(())
            });
            (est, detalhe) = situacao(&a, &prog, None);
            if !(est == Estado::Dialogo && tela::espera_limite(&tela_de(&p.ws, p.aba).0)) {
                break;
            }
        }
    }
    match est {
        Estado::Ocupada | Estado::Dialogo => {
            if !p.interromper {
                return Err(Recusa::nova("ocupada", format!("A aba está {detalhe}{fundo_txt}.")));
            }
            // UM Esc: dois abrem o rewind (Claude) / backtrack (Codex).
            tecla(&p.ws, p.aba, b"\x1b")?;
            let livre = daemon::espera(Duration::from_secs(15), Duration::from_millis(300), || {
                let a = aba_viva(&p.ws, p.aba).ok()?.1;
                let s = if prog == Programa::Claude { frente.and_then(sessoes::sessao_claude) } else { None };
                (situacao(&a, &prog, s.as_ref()).0 == Estado::Livre).then_some(())
            });
            if livre.is_none() {
                return Err(Recusa::nova("ocupada", "A aba não parou depois do Esc; tente de novo."));
            }
        }
        Estado::Digitado => {
            return Err(Recusa::nova("tela-inesperada", "Há texto digitado na caixa da aba; apague-o antes de trocar."));
        }
        Estado::Livre => {}
    }

    let chave_hist = historico::chave(&r, &p.ws, p.aba);
    let hist = historico::le(&chave_hist);
    let mut anterior_claude = hist.claude.clone();
    let mut por_conta = hist.codex_por_conta.clone();
    let mut origem: Option<Origem> = if matches!(prog, Programa::Shell(_)) { hist.ultima_conversa.clone() } else { None };
    let mut preparado: Option<(String, Option<PathBuf>)> = None;
    let cwd: PathBuf = frente
        .and_then(processos::cwd)
        .or_else(|| (!a.info.cwd.is_empty()).then(|| PathBuf::from(&a.info.cwd)))
        .unwrap_or_else(caminhos::casa);
    let pasta_contextos = caminhos::estado().join("contextos");
    let parametros_agora: Option<Vec<String>> = match (prog.agente(), frente) {
        (Some(ag), Some(f)) => processos::argv(anexo.as_ref().map(|j| j.pid).unwrap_or(f)).map(|argv| linha::parametros(ag, &argv)),
        _ => None,
    };
    let homes_de = |motor: &str, conta: Option<&str>| -> Vec<PathBuf> {
        if motor == "claude" { vec![caminhos::claude()] } else { vec![home_codex(&contas, conta)] }
    };
    let prepara = |motor: &str, id: &str, conta: Option<&str>| -> Result<(String, Option<PathBuf>), Recusa> {
        contexto::criar(motor, id, conta.unwrap_or(""), &destino.escolha, &cwd, &pasta_contextos, &homes_de(motor, conta))
            .map(|arq| (id.to_string(), arq))
            .map_err(|e| {
                Recusa::nova(
                    "contexto-indisponivel",
                    format!("A conversa {id} está preservada, mas não deu para preparar o contexto dela para a troca: {e}. Escolha a conta anterior para retomá-la."),
                )
            })
    };

    // A conversa do Claude que esta aba vai retomar pode estar viva noutro
    // processo (segundo plano, ou aberta noutra aba): conferido ANTES de a
    // aba sair do que roda nela.
    let mut fundo: Option<SessaoClaude> = None;
    if destino.motor() == Motor::Claude && prog != Programa::Claude {
        let alvo = anterior_claude.as_deref().map(sessoes::continuacao);
        let viva = alvo.as_deref().and_then(sessoes::sessao_viva).or_else(|| {
            if matches!(prog, Programa::Shell(_)) { tela::jobs_na_tela(&linhas).last().and_then(|j| sessoes::job_vivo(j)) } else { None }
        });
        match viva {
            Some(v) if v.tipo == "bg" && v.job.is_some() => {
                if !p.interromper {
                    return Err(Recusa::nova(
                        "ocupada",
                        format!("A conversa desta aba continua rodando em segundo plano ({}). Trocar a conta para esse processo, e com ele o trabalho em segundo plano da conversa.", descreve_job(&v)),
                    ));
                }
                anterior_claude = Some(v.id.clone());
                fundo = Some(v);
            }
            Some(v) => {
                return Err(Recusa::nova(
                    "em-uso",
                    format!(
                        "A conversa desta aba está aberta em outro lugar (processo {}, em {}); feche-a lá antes de trocar.",
                        v.pid,
                        v.cwd.as_deref().unwrap_or("?")
                    ),
                ));
            }
            None => anterior_claude = alvo,
        }
    }

    let conta_agora = atual_agora.clone().or(escolha_agora.clone());
    match &prog {
        Programa::Claude if anexo.is_some() => {
            let j = anexo.clone().expect("conferido acima");
            if !p.interromper {
                return Err(Recusa::nova(
                    "ocupada",
                    format!("Esta aba mostra uma conversa que roda em segundo plano ({}). Trocar a conta para esse processo, e com ele o trabalho em segundo plano da conversa.", descreve_job(&j)),
                ));
            }
            if agente_alvo != "claude" {
                preparado = Some(prepara("claude", &j.id, conta_agora.as_deref())?);
            }
            encerra_job(&j)?;
            if !espera_shell(&p.ws, p.aba, 20) {
                return Err(Recusa::nova(
                    "tela-inesperada",
                    format!("A conversa em segundo plano parou, mas a aba não voltou ao shell. Ela está em disco: claude --resume {}", j.id),
                ));
            }
            anterior_claude = Some(j.id.clone());
            origem = Some(Origem { motor: "claude".into(), id: Some(j.id.clone()), conta: conta_agora.clone() });
        }
        Programa::Claude => {
            if !trabalho.is_empty() && !p.interromper {
                return Err(Recusa::nova(
                    "ocupada",
                    format!("A conversa tem trabalho em segundo plano ({}), que para quando ela sai do Claude.", trabalho.join(", ")),
                ));
            }
            let sid = sessao.as_ref().map(|s| s.id.clone()).or_else(|| frente.and_then(|f| abas::conversa_do_processo(f, "claude")));
            if agente_alvo != "claude" {
                if let Some(id) = &sid {
                    preparado = Some(prepara("claude", id, conta_agora.as_deref())?);
                }
            }
            sai_do_claude(&p.ws, p.aba, p.interromper, sintaxe)?;
            let na_tela = tela::id_na_tela(&tela_de(&p.ws, p.aba).0, "claude");
            anterior_claude = na_tela.or(sid);
            origem = Some(Origem { motor: "claude".into(), id: anterior_claude.clone(), conta: conta_agora.clone() });
        }
        Programa::Codex => {
            let tid_vivo = frente.and_then(|f| abas::conversa_do_processo(f, "codex"));
            let conta_codex = atual_agora.clone().filter(|c| c.starts_with("gpt:"));
            let home = home_codex(&contas, conta_codex.as_deref());
            if let Some(tid) = &tid_vivo {
                // O turno roda no daemon do Codex, e a tela já deu como livre
                // um Codex trabalhando: com a conversa conhecida, quem responde
                // é o daemon.
                match codex_daemon::turno(&home, tid) {
                    Some(codex_daemon::Turno::Ativo | codex_daemon::Turno::Pergunta) if !p.interromper => {
                        return Err(Recusa::nova("ocupada", "A aba está trabalhando (o Codex está num turno)."));
                    }
                    Some(codex_daemon::Turno::Ativo | codex_daemon::Turno::Pergunta) => {
                        if codex_daemon::interromper(&home, tid) != Some(true) {
                            return Err(Recusa::nova("ocupada", "O turno do Codex não parou depois de interrompido; tente de novo."));
                        }
                    }
                    _ => {}
                }
                if agente_alvo != "codex" {
                    preparado = Some(prepara("codex", tid, conta_codex.as_deref())?);
                }
            }
            if !digita(&p.ws, p.aba, "/quit", true)? {
                return Err(Recusa::nova("tela-inesperada", "O /quit não apareceu na caixa; nada foi feito."));
            }
            if !espera_shell(&p.ws, p.aba, 20) {
                return Err(Recusa::nova("tela-inesperada", "O Codex não saiu."));
            }
            let na_tela = tela::id_na_tela(&tela_de(&p.ws, p.aba).0, "codex");
            let tid = tid_vivo.clone().or(na_tela.clone());
            // O /quit fecha só a tela: um turno em andamento segue no daemon,
            // sem ninguém vendo. Sem a troca confirmada, a aba volta para ele.
            let candidatos: Vec<String> = [na_tela.clone(), tid_vivo.clone()].into_iter().flatten().collect();
            let rodando = candidatos.iter().find(|t| {
                matches!(codex_daemon::turno(&home, t), Some(codex_daemon::Turno::Ativo | codex_daemon::Turno::Pergunta))
            });
            if let Some(t) = rodando {
                let conta_obj = conta_codex.as_deref().and_then(|k| contas::achar(&contas, k));
                let params = parametros_agora.clone().unwrap_or_default();
                let esc = escolha_agora.clone().unwrap_or_else(|| "gpt:principal".into());
                if !p.interromper && devolve_ao_codex(&p.ws, p.aba, t, conta_obj, &esc, &params, sintaxe) {
                    return Err(Recusa::nova(
                        "ocupada",
                        "A aba está trabalhando: o Codex seguia num turno depois do /quit, e a aba voltou para ele, na mesma conversa.",
                    ));
                }
                if codex_daemon::interromper(&home, t) != Some(true) {
                    devolve_ao_codex(&p.ws, p.aba, t, conta_obj, &esc, &params, sintaxe);
                    return Err(Recusa::nova(
                        "ocupada",
                        format!("O turno do Codex (conversa {t}) não parou depois de interrompido; a aba voltou para ele."),
                    ));
                }
            }
            if let (Some(t), Some(c)) = (&tid, &conta_codex) {
                por_conta.insert(c.clone(), t.clone());
            }
            origem = Some(Origem { motor: "codex".into(), id: tid, conta: conta_codex });
        }
        _ => {}
    }
    if let Some(j) = &fundo {
        // Só agora: se a aba não tivesse saído, a conversa seguiria onde estava.
        encerra_job(j)?;
    }
    let parametros_guardados = hist.parametros.clone();
    historico::anota(&chave_hist, |e| {
        e.claude = anterior_claude.clone();
        e.codex_por_conta = por_conta.clone();
        e.ultima_conversa = origem.clone();
        if let (Some(ag), Some(params)) = (prog.agente(), &parametros_agora) {
            e.parametros.insert(ag.into(), params.clone());
        }
    });

    // Claude → Claude (outra conta): a MESMA conversa. Entre motores, uma
    // conversa nova, com o contexto da anterior.
    let home_destino = destino.conta.codex_home().map(|(h, _)| h.clone()).unwrap_or_else(caminhos::codex);
    let mut ident: Option<String> = match destino.motor() {
        Motor::Claude => anterior_claude.clone(),
        Motor::Codex => destino.conta.chaves().iter().find_map(|k| por_conta.get(k).cloned()),
    };
    ident = ident.filter(|id| match destino.motor() {
        Motor::Claude => sessoes::conversa_claude_existe(id),
        Motor::Codex => sessoes::conversa_codex_existe(&home_destino, id),
    });
    let (_, a) = aba_viva(&p.ws, p.aba)?;
    let sintaxe = sintaxe_da_aba(&a, None);
    let precisa_contexto = origem
        .as_ref()
        .is_some_and(|o| o.id.is_some() && (o.motor != agente_alvo || o.id != ident));
    let mut contexto_arq: Option<PathBuf> = None;
    if precisa_contexto {
        let o = origem.clone().expect("conferido acima");
        let id = o.id.clone().expect("conferido acima");
        contexto_arq = match &preparado {
            Some((pid, arq)) if *pid == id => arq.clone(),
            _ => prepara(&o.motor, &id, o.conta.as_deref())?.1,
        };
        if contexto_arq.is_some() && o.motor != agente_alvo {
            // Retomar a conversa antiga do destino recarregaria o histórico
            // inteiro dela além do resumo: a troca de motor começa leve.
            ident = None;
        }
    }
    let parametros = parametros_guardados
        .get(agente_alvo)
        .cloned()
        .or_else(|| (prog.agente() == Some(agente_alvo)).then(|| parametros_agora.clone().unwrap_or_default()))
        .unwrap_or_default();
    let mut s = subida(&destino, ident.as_deref(), &parametros, contexto_arq.as_deref());
    s.pasta = pasta_de_partida(&a, anterior_claude.as_deref());
    let texto = linha::linha(sintaxe, &s);
    if !digita(&p.ws, p.aba, &texto, true)? {
        return Err(Recusa::nova("tela-inesperada", "O comando não apareceu inteiro na aba; nada foi executado."));
    }
    if !subiu(&p.ws, p.aba, agente_alvo, 20) {
        return Err(Recusa::nova(
            "inicio-falhou",
            "A IA escolhida não iniciou nesta aba. O histórico e o contexto da tarefa continuam salvos; escolha novamente a conta para retomar.",
        ));
    }
    historico::anota(&chave_hist, |e| {
        e.conta = Some(destino.escolha.clone());
        e.contexto = contexto_arq.as_ref().map(|c| c.display().to_string());
        e.ultima_conversa = None;
    });
    let mut feito = format!(
        "{} {}",
        if ident.is_some() { "retomou" } else { "abriu conversa nova em" },
        rotulo(&destino.conta.order_key)
    );
    if destino.escolha == SEGUIR_ORDEM {
        feito.push_str(" (seguindo a ordem)");
    }
    if contexto_arq.is_some() {
        feito.push_str(" com o contexto da tarefa anterior");
    }
    Ok(feito)
}

/// `keep ia trocar --ws=<W> --aba=<N> --para=<chave> [--interromper]`.
pub fn cli(a: &crate::cli::Args) -> i32 {
    let (Some(ws), Some(aba), Some(para)) = (a.opcao("ws"), a.opcao("aba").and_then(|n| n.parse().ok()), a.opcao("para")) else {
        return crate::cli::uso_errado("keep ia trocar --ws=<W> --aba=<N> --para=<chave> [--interromper]");
    };
    let p = Pedido { ws: ws.into(), aba, para: para.into(), interromper: a.tem("interromper"), ..Default::default() };
    match trocar(&p) {
        Ok(feito) => crate::cli::responde(json!({ "ok": true, "feito": feito })),
        Err(r) => crate::cli::responde(r.json()),
    }
}

#[cfg(all(test, unix))]
mod ponta_a_ponta {
    //! Um daemon do próprio teste, com a IA falsa (`examples/ia_falsa.rs`)
    //! no lugar do Claude e do Codex: a troca de verdade, digitada na aba.

    use super::*;
    use crate::contas::testes::{casa, perfis};

    fn falsa() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let p = exe.parent()?.parent()?.join("examples").join("ia_falsa");
        p.is_file().then_some(p)
    }

    fn daemon(dir: &Path) -> PathBuf {
        let sock = dir.join("k.sock");
        let server = std::sync::Arc::new(keepd::Server::bind(&sock).expect("bind"));
        let run = std::sync::Arc::clone(&server);
        std::thread::spawn(move || {
            let _ = run.run();
        });
        std::mem::forget(server);
        sock
    }

    /// A tela mostra `texto` (a tela quebra as linhas compridas: os espaços
    /// e as quebras não contam).
    fn tela_tem(ws: &str, tab: u32, texto: &str) -> bool {
        let sem = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let alvo = sem(texto);
        daemon::espera(Duration::from_secs(15), Duration::from_millis(100), || {
            daemon::tela(ws, tab).ok().filter(|t| sem(t).contains(&alvo)).map(|_| ())
        })
        .is_some()
    }

    fn ia_da_aba(tab: u32) -> abas::IaAba {
        let (_, ia) = abas::ler().expect("a lista");
        ia.into_iter().find(|i| i.workspace == "T" && i.aba == tab).expect("a aba com IA")
    }

    #[test]
    fn a_troca_leva_a_conversa_de_conta_em_conta_e_de_motor() {
        let Some(falsa) = falsa() else {
            eprintln!("sem a IA falsa (cargo build -p keep-ia --examples): pulado");
            return;
        };
        let c = casa("troca");
        perfis(&[("G", "u-1", "ana@x.com"), ("F", "u-2", "bia@y.com")]);
        c.global("G", "r");
        let fixa = c.fixa("k-2", Some(("F", "r")), Some(("bia", "bia@y.com", "u-2")));
        c.codex(&c.raiz.join(".codex"), "ana@x.com", "acct-1");
        let bin = c.raiz.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for nome in ["claude", "codex"] {
            std::fs::copy(&falsa, bin.join(nome)).unwrap();
        }
        let sock = daemon(&c.raiz);
        let shell_antes = std::env::var_os("SHELL");
        // SAFETY: a casa (e a trava) está de pé.
        unsafe {
            std::env::set_var("KEEP_IA_CLAUDE_BIN", bin.join("claude"));
            std::env::set_var("KEEP_IA_CODEX_BIN", bin.join("codex"));
            std::env::set_var("KEEP_SOCKET", &sock);
            std::env::set_var("SHELL", "/bin/sh");
            std::env::set_var("PS1", "$ ");
        }
        let _ = contas::listar(true); // os donos ficam em cache
        let tab = daemon::nova_aba("T", Some(&c.raiz.to_string_lossy()), 160, 40).unwrap();
        assert!(tela_tem("T", tab, "$"), "o prompt do shell");

        // A pessoa sobe o Claude à mão, com os parâmetros dela.
        let mao = format!("{} --effort max", bin.join("claude").display());
        assert!(digita("T", tab, &mao, true).unwrap());
        assert!(tela_tem("T", tab, "FALSO claude"));
        let antes = ia_da_aba(tab);
        assert_eq!((antes.conta.as_str(), antes.atual.as_deref()), ("claude:ordem", Some("claude:ana")));
        let id = antes.conversa.clone().expect("a conversa da sessão anotada");

        // Claude → Claude noutra conta: a MESMA conversa, com os parâmetros.
        let p = |para: &str, interromper: bool| Pedido { ws: "T".into(), aba: tab, para: para.into(), interromper, ..Default::default() };
        assert_eq!(trocar(&p("claude:bia", false)).unwrap(), "retomou Claude · bia");
        let esperado = format!(
            "args=[\"--effort\", \"max\", \"--resume\", \"{id}\"] escolha=claude:bia fixa={}",
            fixa.display()
        );
        assert!(tela_tem("T", tab, &esperado), "{}", daemon::tela("T", tab).unwrap_or_default());
        let agora = ia_da_aba(tab);
        assert_eq!((agora.conta.as_str(), agora.atual.as_deref()), ("claude:bia", Some("claude:bia")));

        // Trabalhando: só com a pessoa dizendo que pode interromper.
        assert!(digita("T", tab, "/trabalhar", true).unwrap());
        assert!(tela_tem("T", tab, "esc to interrupt"));
        assert_eq!(trocar(&p("gpt:principal", false)).unwrap_err().motivo, "ocupada");

        // Claude → Codex: conversa nova, com o contexto da anterior.
        let feito = trocar(&p("gpt:principal", true)).unwrap();
        assert_eq!(feito, "abriu conversa nova em GPT · principal com o contexto da tarefa anterior");
        assert!(tela_tem("T", tab, "FALSO codex"));
        assert!(tela_tem("T", tab, contexto::PROMPT_TROCA.trim_end()));
        let codex = ia_da_aba(tab);
        assert_eq!((codex.agente.as_str(), codex.conta.as_str(), codex.atual.as_deref()), ("codex", "gpt:principal", Some("gpt:principal")));

        // Codex → seguir a ordem: volta ao Claude da frente da fila, mesmo
        // com o GPT no topo dela (o GPT nunca entra sozinho).
        std::fs::write(c.raiz.join(".claude/contas/.ordem"), "gpt:principal\nclaude:ana\nclaude:bia\n").unwrap();
        let feito = trocar(&p("claude:ordem", false)).unwrap();
        assert_eq!(feito, "abriu conversa nova em Claude · ana (seguindo a ordem) com o contexto da tarefa anterior");
        assert!(tela_tem("T", tab, "escolha=claude:ordem fixa= home="));
        assert_eq!(trocar(&p("claude:ordem", false)).unwrap(), "a aba já está em Claude · ana");

        // Seguir a ordem sozinho: a conta da aba chega ao limite (semanal a
        // 100%) e a sincronização a leva para a próxima disponível da fila.
        let _uso = crate::uso::testes::servidor(|p| match p.token.as_str() {
            "G" => (200, vec![], r#"{"five_hour":{"utilization":10},"seven_day":{"utilization":100}}"#.into()),
            _ => (200, vec![], r#"{"five_hour":{"utilization":1},"seven_day":{"utilization":2}}"#.into()),
        });
        let r = crate::sincronizar::rodada();
        assert_eq!(r["alvo"], "claude:bia", "{r}");
        assert_eq!(r["alteradas"].as_array().map(Vec::len), Some(1), "{r}");
        let seguindo = ia_da_aba(tab);
        assert_eq!((seguindo.conta.as_str(), seguindo.atual.as_deref()), ("claude:ordem", Some("claude:bia")));
        assert!(tela_tem("T", tab, "escolha=claude:ordem fixa="));
        // Na conta certa, a rodada seguinte não mexe.
        let r = crate::sincronizar::rodada();
        assert_eq!(r["alteradas"].as_array().map(Vec::len), Some(0), "{r}");

        // SAFETY: a casa (e a trava) está de pé.
        unsafe {
            match shell_antes {
                Some(s) => std::env::set_var("SHELL", s),
                None => std::env::remove_var("SHELL"),
            }
            std::env::remove_var("PS1");
        }
    }

    #[test]
    fn a_aba_aberta_para_o_login_fecha_quando_ele_da_certo() {
        let c = casa("fecha-login");
        let sock = daemon(&c.raiz);
        let shell_antes = std::env::var_os("SHELL");
        // SAFETY: a casa (e a trava) está de pé.
        unsafe {
            std::env::set_var("KEEP_SOCKET", &sock);
            std::env::set_var("SHELL", "/bin/sh");
            std::env::set_var("PS1", "$ ");
            // No lugar do `keep`: mostra a linha que o login receberia.
            std::env::set_var("KEEP_IA_KEEP_BIN", "/bin/echo");
            std::env::set_var("KEEP_IA_FECHAR_EM_MS", "0");
        }
        let vizinha = daemon::nova_aba("T", Some(&c.raiz.to_string_lossy()), 120, 30).unwrap();
        assert!(tela_tem("T", vizinha, "$"), "o prompt do shell");
        let tab = crate::login::abre_aba_com("T", &["ia".into(), "login".into(), "claude".into()]).unwrap();
        let marca = format!("ia login claude --fechar-aba=T:{tab}");
        assert!(tela_tem("T", tab, &marca), "{}", daemon::tela("T", tab).unwrap_or_default());
        // Outra aba nunca: só a dita, e só se existe.
        crate::login::fecha_a_propria_aba(Some("T:999"));
        crate::login::fecha_a_propria_aba(Some(&format!("T:{tab}")));
        let fechou = daemon::espera(Duration::from_secs(5), Duration::from_millis(100), || {
            let r = daemon::listar().ok()?;
            r.aba("T", tab).is_none_or(|a| a.info.finished).then_some(())
        });
        assert!(fechou.is_some(), "a aba do login continua aberta");
        assert!(daemon::listar().unwrap().aba("T", vizinha).is_some_and(|a| !a.info.finished), "a vizinha fechou junto");
        // SAFETY: a casa (e a trava) está de pé.
        unsafe {
            match shell_antes {
                Some(s) => std::env::set_var("SHELL", s),
                None => std::env::remove_var("SHELL"),
            }
            std::env::remove_var("PS1");
            std::env::remove_var("KEEP_IA_KEEP_BIN");
            std::env::remove_var("KEEP_IA_FECHAR_EM_MS");
        }
    }
}

#[cfg(all(test, unix))]
mod pasta_de_partida {
    use super::*;
    use keep_proto::TabInfo;

    fn aba(cwd: &Path) -> daemon::Aba {
        daemon::Aba { ws: "W".into(), info: TabInfo { id: 1, cwd: cwd.to_string_lossy().into_owned(), ..TabInfo::default() } }
    }

    #[test]
    fn a_pasta_que_sumiu_vira_a_da_conversa_ou_a_casa() {
        let c = crate::contas::testes::casa("pasta-de-partida");
        let viva = c.raiz.join("projeto");
        let worktree = c.raiz.join("wt-apagada");
        std::fs::create_dir_all(&viva).unwrap();
        let projetos = c.raiz.join(".claude/projects/-x");
        std::fs::create_dir_all(&projetos).unwrap();
        std::fs::write(
            projetos.join("c-1.jsonl"),
            format!(
                "{{\"type\":\"summary\"}}\n{{\"type\":\"user\",\"cwd\":\"{}\"}}\n{{\"type\":\"user\",\"cwd\":\"{}\"}}\n",
                viva.display(),
                worktree.display()
            ),
        )
        .unwrap();
        // A pasta da aba existe: sobe onde está.
        assert_eq!(pasta_de_partida(&aba(&viva), Some("c-1")), None);
        // Sumiu: a pasta em que a conversa começou.
        assert_eq!(pasta_de_partida(&aba(&worktree), Some("c-1")), Some(viva.to_string_lossy().into_owned()));
        // Sem conversa, ou com a pasta dela também apagada: a casa.
        let casa = caminhos::casa().to_string_lossy().into_owned();
        assert_eq!(pasta_de_partida(&aba(&worktree), None), Some(casa.clone()));
        std::fs::remove_dir_all(&viva).unwrap();
        assert_eq!(pasta_de_partida(&aba(&worktree), Some("c-1")), Some(casa));
    }
}
