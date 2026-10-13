//! Trocar a conta do Claude de uma aba sem sair dele (pedido de 12/10/2026:
//! "pra trocar entre contas do claude não é pra colocar exit, é só trocar").
//!
//! Cada Claude que o Keep abre roda num armazém de login só dele, o da aba
//! (`CLAUDE_SECURESTORAGE_CONFIG_DIR=~/.claude/contas/abas/<chave>`), que tem
//! o token de ACESSO da conta da aba e o refresh nulo. O Claude relê o
//! armazém a cada uso (cache de 30 s no 2.1.296) e, sem refresh, nunca tenta
//! renovar: regravar o armazém com o acesso de outra conta troca a conta do
//! Claude aberto, na mesma conversa, em até 30 s. Provado em 12/10 com o
//! Claude de verdade: a statusLine passou de uma conta para a outra sem
//! /exit, e um token inválido no armazém voltou a funcionar ao ser regravado.
//!
//! Um dono por conta, como a Clínica faz com as dela (`ContasClaude`,
//! `SetupClaudeService::renovarTokenSePossivel`): a pasta fixa da conta (ou o
//! login global, sem o kit) guarda a credencial inteira, e só a renovação do
//! Keep a renova (`renovar.rs`). A cada rodada dela, o acesso de cada dono é
//! espalhado pelos armazéns das abas da conta (`espalhar`); o refresh nunca
//! sai do dono. O armazém de uma aba que acabou é apagado.
//!
//! O estado (`~/.claude/contas/abas/estado.json`) diz, de cada armazém, a
//! pasta, a conta em que ele roda, a escolha da aba (`claude:ordem` ou uma
//! conta), o workspace, a aba e o shell dela.
//!
//! Abas de antes disto:
//! - com uma pasta fixa só delas: a pasta vira o armazém da aba (marcada no
//!   `keep.json`, deixa de ser dona da conta) e a troca é a quente; se ela
//!   era a única dona da conta, a credencial inteira MUDA para uma pasta fixa
//!   nova antes (nunca uma cópia: o refresh fica num lugar só);
//! - no login global: a troca individual é recusada (a ordem global, do kit,
//!   continua trocando todas sem sair).
//!
//! Um login feito dentro de uma aba (`/login`) cai no armazém dela, com
//! refresh: ele é a escolha da aba, e a credencial muda para uma pasta fixa
//! nova quando a conta não tem dono — senão o refresh a mais é descartado.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::abas::SEGUIR_ORDEM;
use crate::contas::credencial::{self, Lido, Local, Oauth};
use crate::contas::{self, Armazem, Conta, Metadados, Motor, donos};
use crate::renovar::{TRAVA_GRAVAR, TRAVA_RENOVAR, TravaDoCli};
use crate::trocar::{Recusa, rotulo};
use crate::{caminhos, daemon, processos, uso, vinculo};

/// A variável que diz ao Claude Code onde está o login.
pub const VAR: &str = "CLAUDE_SECURESTORAGE_CONFIG_DIR";

// ------------------------------------------------------------------ estado

/// Um armazém de aba.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Registro {
    /// O `CLAUDE_SECURESTORAGE_CONFIG_DIR` do Claude da aba.
    pub pasta: PathBuf,
    /// A chave da conta em que ele roda agora (`claude:reserva`).
    pub conta: String,
    /// O dono dessa conta (o `key` dela): o apelido pode mudar, ele não.
    pub dono: String,
    /// A escolha da aba: `claude:ordem` ou a chave de uma conta.
    pub escolha: String,
    pub ws: String,
    pub aba: u32,
    /// O shell da aba, e quando ele nasceu (ms): os dois dizem se é o mesmo.
    pub shell: u32,
    pub nascimento: u64,
    /// Uma pasta fixa de antes, que virou o armazém desta aba.
    pub legada: bool,
    /// A última gravação (ms).
    pub em: u64,
    /// O problema que a renovação viu neste armazém, até ele passar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub falha: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Estado {
    versao: u32,
    abas: BTreeMap<String, Registro>,
}

fn arquivo_estado() -> PathBuf {
    caminhos::abas().join("estado.json")
}

fn cria_pasta(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// Um processo de cada vez mexendo no estado (o `preparar` de cada Claude
/// que sobe, a troca, a renovação).
struct TravaDoEstado(std::fs::File);

impl Drop for TravaDoEstado {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn trava_estado() -> Result<TravaDoEstado, String> {
    cria_pasta(&caminhos::abas()).map_err(|e| e.to_string())?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(caminhos::abas().join("estado.trava"))
        .map_err(|e| e.to_string())?;
    let fim = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(TravaDoEstado(f)),
            Err(_) if std::time::Instant::now() < fim => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("o estado dos armazéns das abas está preso ({e})")),
        }
    }
}

fn le_estado() -> Estado {
    std::fs::read_to_string(arquivo_estado()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn grava_estado(e: &Estado) -> Result<(), String> {
    let arq = arquivo_estado();
    cria_pasta(&caminhos::abas()).map_err(|e| e.to_string())?;
    let tmp = arq.with_extension(format!("json.tmp-{}", std::process::id()));
    let mut e = e.clone();
    e.versao = 1;
    std::fs::write(&tmp, serde_json::to_vec_pretty(&e).map_err(|x| x.to_string())?).map_err(|x| x.to_string())?;
    std::fs::rename(&tmp, &arq).map_err(|x| x.to_string())
}

/// Uma pasta como a variável a diria: sem barra no fim, em NFC (o CLI
/// normaliza assim antes de tirar o nome do item).
fn normal(p: &Path) -> String {
    use unicode_normalization::UnicodeNormalization;
    let limpa: PathBuf = p.components().collect();
    limpa.to_string_lossy().nfc().collect()
}

fn mesma(a: &Path, b: &Path) -> bool {
    normal(a) == normal(b)
}

/// O armazém de aba desta pasta, se ela é um.
pub fn registro_da_pasta(pasta: &Path) -> Option<(String, Registro)> {
    le_estado().abas.into_iter().find(|(_, r)| mesma(&r.pasta, pasta))
}

/// O armazém de aba em que um processo roda, se ele roda num.
pub fn registro_do_processo(pid: u32) -> Option<(String, Registro)> {
    let v = processos::variavel(pid, VAR).filter(|v| !v.is_empty())?;
    registro_da_pasta(Path::new(&v))
}

// ------------------------------------------------------------- de onde vem

/// O dono de uma conta, de onde o acesso dela é espalhado.
#[derive(Clone, Debug)]
pub struct Fonte {
    pub armazem: Armazem,
    /// O `claudeAiOauth` inteiro do dono. Nunca sai daqui inteiro.
    oauth: Value,
}

/// O acesso de um `claudeAiOauth`: o mesmo objeto, com o refresh nulo. É o
/// que vai para o armazém de uma aba.
pub fn so_acesso(oauth: &Value) -> Value {
    let mut o = oauth.as_object().cloned().unwrap_or_default();
    o.insert("refreshToken".into(), Value::Null);
    o.remove("refreshTokenExpiresAt");
    Value::Object(o)
}

fn agora() -> u64 {
    donos::agora_ms()
}

/// Os armazéns que podem ser donos de uma conta, na ordem em que valem: sem
/// gerente externo, o global quando é dela (é a casa dela); as pastas fixas
/// dela; com o kit, o global por último (o kit o troca de dono e o renova).
fn candidatos(conta: &Conta) -> Vec<Armazem> {
    let externo = contas::gerente_externo();
    let mut v = Vec::new();
    if !externo && conta.global() {
        v.push(Armazem::Global);
    }
    v.extend(conta.armazens.iter().filter(|a| matches!(a, Armazem::Fixa { .. })).cloned());
    if externo && conta.global() {
        v.push(Armazem::Global);
    }
    v
}

fn local_de(a: &Armazem) -> Option<Local> {
    match a {
        Armazem::Global => Some(credencial::local_global()),
        Armazem::Fixa { pasta } => Some(credencial::local_fixa(pasta)),
        _ => None,
    }
}

/// O dono de uma conta com acesso em dia: o que vai para as abas dela.
/// `exceto` deixa de fora um armazém (o que vai deixar de ser dono).
fn fonte_exceto(conta: &Conta, exceto: Option<&Path>) -> Option<Fonte> {
    if conta.engine != Motor::Claude {
        return None;
    }
    for a in candidatos(conta) {
        if let (Armazem::Fixa { pasta }, Some(x)) = (&a, exceto) {
            if mesma(pasta, x) {
                continue;
            }
        }
        let Some(local) = local_de(&a) else { continue };
        let Lido::Ok(valor) = credencial::ler(&local) else { continue };
        let Some(o) = Oauth::de(&valor) else { continue };
        if o.recusado() || o.acesso.is_none() || crate::renovar::morto(&a, &o) {
            continue;
        }
        // Acesso vencido não vai para aba nenhuma: a renovação cuida dele.
        if o.expira_em.is_some_and(|e| e <= agora()) {
            continue;
        }
        let Some(oauth) = valor.get("claudeAiOauth").cloned() else { continue };
        return Some(Fonte { armazem: a, oauth });
    }
    None
}

pub fn fonte(conta: &Conta) -> Option<Fonte> {
    fonte_exceto(conta, None)
}

/// Se a conta tem dono que roda (com refresh vivo), em dia ou não.
fn tem_dono(conta: &Conta, exceto: Option<&Path>) -> bool {
    candidatos(conta).into_iter().any(|a| {
        if let (Armazem::Fixa { pasta }, Some(x)) = (&a, exceto) {
            if mesma(pasta, x) {
                return false;
            }
        }
        local_de(&a).is_some_and(|l| contas::login_roda(credencial::ler(&l), &a))
    })
}

/// O dono em dia; com o acesso dele vencido, renova antes (uma rodada da
/// renovação) — um Claude que sobe agora não espera o próximo minuto.
fn fonte_renovando(conta: &Conta) -> Option<Fonte> {
    if let Some(f) = fonte(conta) {
        return Some(f);
    }
    if !tem_dono(conta, None) {
        return None;
    }
    for _ in 0..3 {
        if !crate::renovar::rodada(false).em_andamento {
            break;
        }
        std::thread::sleep(Duration::from_millis(700));
    }
    fonte(conta)
}

/// A conta de quem segue a ordem: com o kit, a que ele pôs no login global
/// (`.ativa`); sem ele, a primeira do Claude disponível da fila (pela última
/// medição), ou a primeira que roda.
pub fn conta_da_ordem(lista: &[Conta]) -> Option<&Conta> {
    let claude = |c: &&Conta| c.engine == Motor::Claude;
    if contas::gerente_externo() {
        if let Some(ativa) = contas::ativa_do_kit() {
            if let Some(c) = lista.iter().filter(claude).find(|c| c.aliases.contains(&ativa)) {
                return Some(c);
            }
        }
        return lista.iter().filter(claude).find(|c| c.global()).or_else(|| lista.iter().find(claude));
    }
    let livres = uso::disponiveis_de(lista);
    let rodam: Vec<&Conta> = lista.iter().filter(claude).filter(|c| c.roda).collect();
    rodam.iter().find(|c| livres.contains(&c.key)).or(rodam.first()).copied()
}

fn conta_por_dono<'a>(lista: &'a [Conta], dono: &str) -> Option<&'a Conta> {
    (!dono.is_empty()).then(|| lista.iter().find(|c| c.engine == Motor::Claude && c.key == dono)).flatten()
}

/// A conta que uma escolha pede agora. `manter` é a conta em que a aba
/// está, para a rodada de `espalhar`: sem o kit, quem segue a ordem fica
/// nela (quem a move é a sincronização, com as regras dela).
fn conta_da_escolha<'a>(lista: &'a [Conta], escolha: &str, manter: Option<&'a Conta>) -> Option<&'a Conta> {
    if escolha == SEGUIR_ORDEM {
        if !contas::gerente_externo() {
            if let Some(c) = manter.filter(|c| c.roda) {
                return Some(c);
            }
        }
        return conta_da_ordem(lista);
    }
    contas::achar(lista, escolha).filter(|c| c.engine == Motor::Claude)
}

// ------------------------------------------------------------ o armazém

/// As duas travas do CLI na pasta de um armazém: ninguém renova nem grava
/// nele enquanto o Keep mexe.
struct TravasDaPasta {
    _renovar: TravaDoCli,
    _gravar: TravaDoCli,
}

impl TravasDaPasta {
    fn pega(pasta: &Path) -> Result<TravasDaPasta, String> {
        let r = TravaDoCli::pega(pasta, TRAVA_RENOVAR, Duration::from_secs(5))
            .ok_or("o Claude desta aba está renovando o login agora; tente de novo")?;
        let g = TravaDoCli::pega(pasta, TRAVA_GRAVAR, Duration::from_secs(5))
            .ok_or("o Claude desta aba está gravando o login agora; tente de novo")?;
        Ok(TravasDaPasta { _renovar: r, _gravar: g })
    }
}

/// O que o armazém de uma pasta tem (um objeto; vazio se não tem nada).
fn le_pasta(pasta: &Path) -> Result<Value, String> {
    match credencial::fixa(pasta) {
        Lido::Ok(v) => Ok(v),
        Lido::Ausente => Ok(json!({})),
        Lido::Ilegivel(por) => Err(format!("o armazém {} não se lê ({por})", pasta.display())),
    }
}

/// Grava `oauth` como o `claudeAiOauth` da pasta, mantendo o resto (os
/// `mcpOAuth` que o Claude da aba guardou). Com as travas na mão.
fn grava_travado(_t: &TravasDaPasta, pasta: &Path, oauth: &Value) -> Result<bool, String> {
    let mut valor = le_pasta(pasta)?;
    if valor.get("claudeAiOauth") == Some(oauth) {
        return Ok(false);
    }
    let obj = valor.as_object_mut().ok_or("o armazém não é um objeto")?;
    obj.insert("claudeAiOauth".into(), oauth.clone());
    credencial::gravar(&credencial::local_fixa(pasta), &valor)?;
    Ok(true)
}

/// Uma pasta fixa nova, dona de `dono`, com a credencial inteira `valor` (o
/// que estava num armazém que deixa de tê-la: o refresh MUDA de lugar).
fn promove(valor: &Value, dono: &donos::Dono, apelido: &str) -> Result<PathBuf, String> {
    let nova = contas::pasta_fixa_nova();
    cria_pasta(&nova).map_err(|e| e.to_string())?;
    let local = credencial::local_fixa(&nova);
    let so_login = json!({ "claudeAiOauth": valor.get("claudeAiOauth").cloned().unwrap_or(Value::Null) });
    if let Err(e) = credencial::gravar(&local, &so_login) {
        let _ = credencial::apagar(&local);
        let _ = std::fs::remove_dir_all(&nova);
        return Err(format!("não deu para guardar o login numa pasta fixa nova ({e})"));
    }
    contas::gravar_metadados(
        &nova,
        &Metadados {
            versao: 1,
            apelido: apelido.into(),
            email: dono.email.clone(),
            uuid: dono.uuid.clone(),
            criada_em: agora(),
            aba: false,
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(nova)
}

/// Um apelido livre para uma conta que acabou de aparecer.
fn apelido_novo(lista: &[Conta], email: &str) -> String {
    let usados: Vec<String> = lista.iter().flat_map(|c| c.chaves()).collect();
    let base = contas::apelido_de_email(email);
    let mut ap = base.clone();
    let mut n = 2;
    while ap == "ordem" || usados.contains(&Motor::Claude.chave(&ap)) {
        ap = format!("{base}-{n}");
        n += 1;
    }
    ap
}

/// O login inteiro (com refresh) que está no armazém de uma aba — feito
/// nela com `/login`, ou de uma pasta fixa que vira armazém de aba — sai
/// dali: muda para uma pasta fixa nova quando a conta dele não tem outro
/// dono, e é descartado quando tem. O armazém fica só com o acesso. Devolve
/// o dono do login, se havia um.
fn tira_o_refresh(t: &TravasDaPasta, pasta: &Path, lista: &[Conta], meta: Option<&Metadados>) -> Result<Option<donos::Dono>, String> {
    let valor = le_pasta(pasta)?;
    let Some(oauth) = valor.get("claudeAiOauth").cloned() else { return Ok(None) };
    let Some(o) = Oauth::de(&valor) else { return Ok(None) };
    if o.refresh.is_none() {
        return Ok(None);
    }
    let acesso = o.acesso.clone().unwrap_or_default();
    let dono = donos::dono(&acesso, true)
        .or_else(|| meta.filter(|m| !m.uuid.is_empty()).map(|m| donos::Dono { uuid: m.uuid.clone(), email: m.email.clone() }))
        .ok_or("não deu para saber de quem é o login deste armazém (sem rede?)")?;
    let conta = lista.iter().find(|c| c.engine == Motor::Claude && c.key == dono.uuid);
    let outro_dono = conta.is_some_and(|c| tem_dono(c, Some(pasta)));
    if !outro_dono {
        let apelido = match (conta, meta) {
            (Some(c), _) => c.alias.clone(),
            (None, Some(m)) if contas::apelido_valido(&m.apelido) => m.apelido.clone(),
            _ => apelido_novo(lista, &dono.email),
        };
        promove(&valor, &dono, &apelido)?;
        if conta.is_none() {
            let _ = contas::acrescentar_na_ordem(&Motor::Claude.chave(&apelido));
        }
    }
    grava_travado(t, pasta, &so_acesso(&oauth))?;
    Ok(Some(dono))
}

/// Apaga o armazém de uma aba que acabou: o item e a pasta.
fn apaga(r: &Registro) -> Result<(), String> {
    credencial::apagar(&credencial::local_fixa(&r.pasta))?;
    // O CLI deixa ao lado a trava antiga da renovação (`<pasta>.lock`).
    let _ = std::fs::remove_dir_all(PathBuf::from(format!("{}.lock", r.pasta.display())));
    std::fs::remove_dir_all(&r.pasta).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(|e| e.to_string())
}

// ------------------------------------------------------------ processos

/// Se um processo é um Claude Code (pelo nome, ou pelo programa da linha de
/// comando: o instalador põe cada versão num arquivo com o nome dela).
fn e_claude(p: &processos::Processo) -> bool {
    [Some(p.nome.clone()), processos::programa(p.pid)]
        .into_iter()
        .flatten()
        .any(|n| crate::tela::programa(&n) == crate::tela::Programa::Claude || n.to_ascii_lowercase().starts_with("claude"))
}

/// Quem roda com cada armazém: o valor da variável → os Claude que a têm sem
/// tê-la herdado do pai (o de cada aba, não um aberto de dentro dele). Os
/// outros processos com a variável (um servidor MCP, um shell da ferramenta
/// Bash, um que ficou órfão) não leem o login.
fn usos() -> HashMap<String, Vec<u32>> {
    let eu = std::process::id();
    let lista = processos::todos();
    let mut com: HashMap<u32, String> = HashMap::new();
    for p in &lista {
        if p.pid == eu || p.pid <= 1 {
            continue;
        }
        if let Some(v) = processos::variavel(p.pid, VAR).filter(|v| !v.is_empty()) {
            com.insert(p.pid, normal(Path::new(&v)));
        }
    }
    let mut saida: HashMap<String, Vec<u32>> = HashMap::new();
    for p in lista.iter().filter(|p| com.contains_key(&p.pid)) {
        let v = &com[&p.pid];
        if com.get(&p.ppid) == Some(v) || !e_claude(p) {
            continue;
        }
        saida.entry(v.clone()).or_default().push(p.pid);
    }
    saida
}

/// O shell da aba de um processo: ele, ou o ancestral cujo pai é o keepd
/// (o do daemon que responde: nunca outro).
fn shell_da_aba(pid: u32, r: Option<&daemon::Retrato>) -> u32 {
    let Some(keepd) = r.map(|r| r.daemon.pid).filter(|p| *p != 0) else { return pid };
    let mut atual = pid;
    for _ in 0..32 {
        match processos::pai(atual) {
            Some(p) if p == keepd => return atual,
            Some(p) if p > 1 && p != atual => atual = p,
            _ => break,
        }
    }
    pid
}

/// O mesmo processo de antes? Pelo pid e por quando nasceu.
fn mesmo_processo(pid: u32, nascimento: u64) -> bool {
    processos::inicio_ms(pid).is_some_and(|n| n.abs_diff(nascimento) <= 2000)
}

// --------------------------------------------------------------- preparar

/// O armazém pronto para um Claude subir numa aba.
#[derive(Clone, Debug, PartialEq)]
pub struct Preparada {
    pub chave: String,
    pub pasta: PathBuf,
    pub conta: String,
    pub escolha: String,
}

/// Cria (ou atualiza) o armazém da aba do shell `shell` e põe nele o acesso
/// da conta da escolha. `conta` já decidida (a troca que sobe o Claude)
/// vale mais que a escolha. Sem escolha: a que a aba já tinha, ou seguir a
/// ordem.
pub fn preparar(ws: &str, shell: u32, escolha: Option<&str>, conta: Option<&Conta>) -> Result<Preparada, Recusa> {
    if let Some(e) = escolha {
        if e != SEGUIR_ORDEM && !(e.starts_with("claude:") && crate::abas::chave_valida(e)) {
            return Err(Recusa::nova("uso", format!("“{e}” não é uma conta do Claude.")));
        }
    }
    let r = daemon::listar().ok();
    let shell = shell_da_aba(shell, r.as_ref());
    let nascimento = processos::inicio_ms(shell)
        .ok_or_else(|| Recusa::nova("erro", format!("O shell {shell} não existe.")))?;
    let (ws, aba) = r
        .as_ref()
        .and_then(|r| vinculo::vincular(r).into_iter().find(|(_, v)| v.shell == shell).map(|((w, a), _)| (w, a)))
        .unwrap_or_else(|| (ws.to_string(), 0));
    let chave = format!("{shell}-{nascimento}");
    let lista = contas::listar(false);
    let antes = le_estado().abas.get(&chave).cloned();
    let pasta = antes.as_ref().map(|r| r.pasta.clone()).unwrap_or_else(|| caminhos::abas().join(&chave));

    // Um /login feito no armazém desta aba é a escolha dela. Sem como tirar o
    // login dali agora (sem rede para saber de quem é), o armazém fica como
    // está: o Claude roda no login dele, e a renovação o adota depois.
    let mut escolha_antes = antes.as_ref().map(|r| r.escolha.clone());
    let mut dono_antes = antes.as_ref().map(|r| r.dono.clone()).unwrap_or_default();
    if let Some(r) = antes.as_ref().filter(|r| r.pasta.is_dir()) {
        let t = TravasDaPasta::pega(&pasta).map_err(|e| Recusa::nova("ocupada", e))?;
        match tira_o_refresh(&t, &pasta, &lista, None) {
            Ok(Some(d)) => {
                if d.uuid != dono_antes {
                    escolha_antes = None;
                }
                dono_antes = d.uuid;
            }
            Ok(None) => {}
            Err(_) => {
                return Ok(Preparada { chave, pasta, conta: r.conta.clone(), escolha: r.escolha.clone() });
            }
        }
    }
    let lista = if dono_antes.is_empty() || conta_por_dono(&lista, &dono_antes).is_some() { lista } else { contas::listar(false) };
    let ja = conta_por_dono(&lista, &dono_antes);
    let escolha = escolha
        .map(str::to_string)
        .or(escolha_antes.filter(|e| !e.is_empty()))
        .or_else(|| ja.filter(|_| antes.is_some()).map(|c| c.order_key.clone()))
        .unwrap_or_else(|| SEGUIR_ORDEM.into());
    let alvo = match conta {
        Some(c) => Some(c),
        None => conta_da_escolha(&lista, &escolha, None),
    }
    .ok_or_else(|| Recusa::nova("sem-conta", format!("Não há a conta {} para esta aba.", rotulo(&escolha))))?;
    let f = fonte_renovando(alvo).ok_or_else(|| {
        Recusa::nova(
            "precisa-login",
            format!("A conta {} não tem um login em dia que o Keep renove; entre nela pelo + do rodapé.", rotulo(&alvo.order_key)),
        )
    })?;
    let _e = trava_estado().map_err(|e| Recusa::nova("erro", e))?;
    cria_pasta(&pasta).map_err(|e| Recusa::nova("erro", e.to_string()))?;
    {
        let t = TravasDaPasta::pega(&pasta).map_err(|e| Recusa::nova("ocupada", e))?;
        grava_travado(&t, &pasta, &so_acesso(&f.oauth)).map_err(|e| Recusa::nova("erro", e))?;
    }
    let mut estado = le_estado();
    let legada = antes.as_ref().is_some_and(|r| r.legada);
    estado.abas.insert(
        chave.clone(),
        Registro {
            pasta: pasta.clone(),
            conta: alvo.order_key.clone(),
            dono: alvo.key.clone(),
            escolha: escolha.clone(),
            ws,
            aba,
            shell,
            nascimento,
            legada,
            em: agora(),
            falha: None,
        },
    );
    grava_estado(&estado).map_err(|e| Recusa::nova("erro", e))?;
    Ok(Preparada { chave, pasta, conta: alvo.order_key.clone(), escolha })
}

/// `keep ia preparar --ws=<W> --shell=<pid> [--escolha=<chave>]`: o armazém
/// do Claude que vai subir nesta aba (a função `claude()` do shell chama e
/// põe `CLAUDE_SECURESTORAGE_CONFIG_DIR=<pasta>` na linha dele).
pub fn cli_preparar(a: &crate::cli::Args) -> i32 {
    let (Some(ws), Some(shell)) = (a.opcao("ws"), a.opcao("shell").and_then(|s| s.parse::<u32>().ok())) else {
        return crate::cli::uso_errado("keep ia preparar --ws=<W> --shell=<pid> [--escolha=claude:ordem|claude:<apelido>]");
    };
    let escolha = a.opcao("escolha").filter(|e| !e.is_empty());
    match preparar(ws, shell, escolha, None) {
        Ok(p) => crate::cli::responde(json!({
            "ok": true,
            "chave": p.chave,
            "pasta": p.pasta.to_string_lossy(),
            "conta": p.conta,
            "escolha": p.escolha,
        })),
        Err(r) => crate::cli::responde(r.json()),
    }
}

// ----------------------------------------------------------------- trocar

/// O que a troca a quente fez.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feito {
    /// Já estava nessa conta (a chave dela).
    Ja(String),
    /// Passou para essa conta.
    Trocou(String),
}

/// A pasta fixa de antes em que um Claude roda vira o armazém da aba dele:
/// só se ninguém mais roda nela. O login inteiro sai dela (muda para uma
/// pasta nova se ela era a única dona da conta), e o `keep.json` a marca.
fn converte(pasta: &Path, pid: u32, lista: &[Conta]) -> Result<Option<donos::Dono>, Recusa> {
    let outros: Vec<u32> =
        usos().get(&normal(pasta)).map(|v| v.iter().copied().filter(|p| *p != pid).collect()).unwrap_or_default();
    // O Claude da aba pode ter subido de dentro de outro (herdou a pasta):
    // então ele não é raiz, e a raiz é o outro.
    if !outros.is_empty() {
        return Err(Recusa::nova(
            "pasta-compartilhada",
            format!(
                "O login desta aba ({}) é usado também por outro Claude (processo {}). Trocar a conta dele aqui trocaria a dos dois; escolha a conta quando só esta aba usar essa pasta.",
                pasta.display(),
                outros.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    let t = TravasDaPasta::pega(pasta).map_err(|e| Recusa::nova("ocupada", e))?;
    let mut meta = contas::ler_metadados(pasta).unwrap_or_default();
    if meta.uuid.is_empty() {
        // Pasta do kit: `conta.json` diz de quem ela é.
        if let Some(v) = std::fs::read_to_string(pasta.join("conta.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            let txt = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            meta.uuid = txt("accountUuid");
            meta.email = txt("email");
            if meta.apelido.is_empty() {
                meta.apelido = txt("apelido");
            }
        }
    }
    let dono = tira_o_refresh(&t, pasta, lista, Some(&meta)).map_err(|e| Recusa::nova("erro", e))?;
    meta.versao = meta.versao.max(1);
    meta.aba = true;
    contas::gravar_metadados(pasta, &meta).map_err(|e| Recusa::nova("erro", e.to_string()))?;
    Ok(dono)
}

/// Troca a conta do Claude `pid` (o da aba `ws`#`aba`, de shell `shell`)
/// para a escolha `para` (`claude:ordem` ou uma conta; com a ordem, `conta`
/// é a conta já decidida pela sincronização), regravando o armazém dele.
/// Nada é digitado na aba.
pub fn trocar_processo(
    pid: u32,
    shell: u32,
    ws: &str,
    aba: u32,
    para: &str,
    conta: Option<&str>,
    lista: &[Conta],
) -> Result<Feito, Recusa> {
    let Some(pasta) = processos::variavel(pid, VAR).filter(|v| !v.is_empty()).map(PathBuf::from) else {
        return Err(Recusa::nova(
            "claude-global",
            "O Claude desta aba usa o login global: foi aberto antes da troca a quente. A conta dele muda com a ordem de prioridade, junto com a das outras abas no login global, sem sair; para esta aba trocar sozinha, o próximo Claude aberto nela já vem pronto para isso.",
        ));
    };
    let registro = registro_da_pasta(&pasta);
    let dentro_de = |base: &Path| pasta.parent().is_some_and(|p| mesma(p, base));
    let legada = registro.is_none() && dentro_de(&caminhos::fixas());
    if registro.is_none() && !legada && !dentro_de(&caminhos::abas()) {
        return Err(Recusa::nova(
            "armazem-desconhecido",
            format!("O Claude desta aba usa um login que o Keep não conhece ({}); a troca a quente não mexe nele.", pasta.display()),
        ));
    }
    let alvo = match (para, conta) {
        (SEGUIR_ORDEM, Some(c)) => contas::achar(lista, c),
        _ => conta_da_escolha(lista, para, None),
    }
    .filter(|c| c.engine == Motor::Claude)
    .ok_or_else(|| Recusa::nova("sem-conta", format!("Não há a conta {}. Entre nela pelo + do rodapé.", rotulo(para))))?;
    let f = fonte_renovando(alvo).ok_or_else(|| {
        Recusa::nova("precisa-login", format!("A conta {} não tem um login em dia para as abas.", rotulo(&alvo.order_key)))
    })?;
    let _e = trava_estado().map_err(|e| Recusa::nova("erro", e))?;
    let mut estado = le_estado();
    let chave = registro.as_ref().map(|(k, _)| k.clone()).unwrap_or_else(|| {
        let nome = pasta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if legada { format!("fixa-{nome}") } else { nome }
    });
    let antes = estado.abas.get(&chave).cloned().unwrap_or_default();
    let shell_vivo = processos::inicio_ms(shell);
    let registro_de = |conta: &str, dono: &str| Registro {
        pasta: pasta.clone(),
        conta: conta.to_string(),
        dono: dono.to_string(),
        escolha: para.to_string(),
        ws: ws.to_string(),
        aba,
        shell: if shell_vivo.is_some() { shell } else { antes.shell },
        nascimento: shell_vivo.unwrap_or(antes.nascimento),
        legada: legada || antes.legada,
        em: agora(),
        falha: None,
    };
    if legada {
        // A pasta vira armazém de aba e entra no estado na hora: daqui em
        // diante quem a alimenta é a renovação, mesmo se a gravação abaixo
        // falhar.
        let dono = converte(&pasta, pid, lista)?.map(|d| d.uuid).unwrap_or_default();
        let conta = conta_por_dono(lista, &dono).map(|c| c.order_key.clone()).unwrap_or_default();
        estado.abas.insert(chave.clone(), registro_de(&conta, &dono));
        grava_estado(&estado).map_err(|e| Recusa::nova("erro", e))?;
    }
    {
        let t = TravasDaPasta::pega(&pasta).map_err(|e| Recusa::nova("ocupada", e))?;
        if !legada {
            // Um /login feito na aba: o login sai do armazém antes da troca.
            tira_o_refresh(&t, &pasta, lista, None).map_err(|e| Recusa::nova("erro", e))?;
        }
        grava_travado(&t, &pasta, &so_acesso(&f.oauth)).map_err(|e| Recusa::nova("erro", e))?;
    }
    let igual = antes.dono == alvo.key && antes.escolha == para;
    estado.abas.insert(chave, registro_de(&alvo.order_key, &alvo.key));
    grava_estado(&estado).map_err(|e| Recusa::nova("erro", e))?;
    Ok(if igual { Feito::Ja(alvo.order_key.clone()) } else { Feito::Trocou(alvo.order_key.clone()) })
}

// ---------------------------------------------------------------- espalhar

/// O que uma rodada de `espalhar` fez.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Espalhado {
    /// Armazéns que passaram para outra conta: (chave, conta).
    pub trocadas: Vec<(String, String)>,
    /// Armazéns que receberam o acesso renovado da mesma conta.
    pub atualizadas: usize,
    /// Armazéns de abas que acabaram, apagados.
    pub apagadas: Vec<String>,
    pub falhas: Vec<(String, String)>,
}

impl Espalhado {
    pub fn json(&self) -> Value {
        json!({
            "trocadas": self.trocadas.iter().map(|(c, k)| json!({ "chave": c, "conta": k })).collect::<Vec<_>>(),
            "atualizadas": self.atualizadas,
            "apagadas": self.apagadas,
            "falhas": self.falhas.iter().map(|(c, m)| json!({ "chave": c, "motivo": m })).collect::<Vec<_>>(),
        })
    }
}

/// O que a rodada fez com um armazém.
enum Passo {
    Nada,
    Atualizado,
    Trocado(String),
}

/// Um armazém de aba na rodada: o login feito nela sai (e vira a escolha
/// dela), e o acesso atual do dono da conta entra.
fn espalha_um(r: &mut Registro, lista: &[Conta]) -> Result<Passo, String> {
    let t = TravasDaPasta::pega(&r.pasta)?;
    if let Some(d) = tira_o_refresh(&t, &r.pasta, lista, None)? {
        if d.uuid != r.dono {
            // Login noutra conta, feito na aba: é a escolha dela.
            if let Some(c) = conta_por_dono(&contas::listar(false), &d.uuid) {
                r.escolha = c.order_key.clone();
                r.conta = c.order_key.clone();
            }
            r.dono = d.uuid;
        }
    }
    let agora_lista;
    let lista = if r.dono.is_empty() || conta_por_dono(lista, &r.dono).is_some() {
        lista
    } else {
        // Uma conta que acabou de aparecer (um login feito na aba).
        agora_lista = contas::listar(false);
        &agora_lista
    };
    let manter = conta_por_dono(lista, &r.dono);
    let alvo = conta_da_escolha(lista, &r.escolha, manter).ok_or_else(|| format!("não há a conta {}", rotulo(&r.escolha)))?;
    let f = fonte(alvo).ok_or_else(|| format!("a conta {} está sem login em dia", rotulo(&alvo.order_key)))?;
    let gravou = grava_travado(&t, &r.pasta, &so_acesso(&f.oauth))?;
    let trocou = alvo.key != r.dono;
    r.dono = alvo.key.clone();
    r.conta = alvo.order_key.clone();
    if gravou || trocou {
        r.em = agora();
    }
    Ok(if trocou {
        Passo::Trocado(alvo.order_key.clone())
    } else if gravou {
        Passo::Atualizado
    } else {
        Passo::Nada
    })
}

/// A rodada: cada armazém de aba recebe o acesso atual do dono da conta dele
/// (com o kit, quem segue a ordem passa para a conta que o kit pôs no login
/// global); o de uma aba que acabou, sem ninguém mais rodando nele, é
/// apagado. Um problema num armazém aparece uma vez, quando surge (fica no
/// registro dele), e não a cada rodada.
pub fn espalhar(lista: &[Conta]) -> Espalhado {
    let mut saida = Espalhado::default();
    let Ok(_e) = trava_estado() else {
        saida.falhas.push(("estado".into(), "o estado dos armazéns das abas está preso".into()));
        return saida;
    };
    let mut estado = le_estado();
    let mut em_uso: Option<HashMap<String, Vec<u32>>> = None;
    let antes = estado.clone();
    let chaves: Vec<String> = estado.abas.keys().cloned().collect();
    for chave in chaves {
        let mut r = estado.abas[&chave].clone();
        if !r.pasta.is_dir() {
            estado.abas.remove(&chave);
            continue;
        }
        if !mesmo_processo(r.shell, r.nascimento) {
            let usos = em_uso.get_or_insert_with(usos);
            if usos.get(&normal(&r.pasta)).is_none_or(Vec::is_empty) {
                match apaga(&r) {
                    Ok(()) => {
                        estado.abas.remove(&chave);
                        saida.apagadas.push(chave);
                    }
                    Err(e) => saida.falhas.push((chave, format!("não apaguei o armazém: {e}"))),
                }
                continue;
            }
        }
        match espalha_um(&mut r, lista) {
            Ok(passo) => {
                r.falha = None;
                match passo {
                    Passo::Trocado(conta) => saida.trocadas.push((chave.clone(), conta)),
                    Passo::Atualizado => saida.atualizadas += 1,
                    Passo::Nada => {}
                }
            }
            Err(e) => {
                if r.falha.as_deref() != Some(e.as_str()) {
                    saida.falhas.push((chave.clone(), e.clone()));
                }
                r.falha = Some(e);
            }
        }
        estado.abas.insert(chave, r);
    }
    // Pastas em abas/ que o estado não conhece (um `preparar` que caiu no
    // meio): sem ninguém rodando nelas, saem.
    if let Ok(dir) = std::fs::read_dir(caminhos::abas()) {
        for p in dir.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()) {
            if estado.abas.values().any(|r| mesma(&r.pasta, &p)) {
                continue;
            }
            let usos = em_uso.get_or_insert_with(usos);
            if usos.get(&normal(&p)).is_none_or(Vec::is_empty) {
                let nome = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                match apaga(&Registro { pasta: p.clone(), ..Default::default() }) {
                    Ok(()) => saida.apagadas.push(nome),
                    Err(e) => saida.falhas.push((nome, format!("não apaguei o armazém solto: {e}"))),
                }
            }
        }
    }
    if estado.abas != antes.abas {
        if let Err(e) = grava_estado(&estado) {
            saida.falhas.push(("estado".into(), e));
        }
    }
    saida
}

#[cfg(all(test, unix))]
mod testes;
