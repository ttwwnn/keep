//! A restauração: no keepd novo, as abas do retrato do keepd anterior.
//!
//! Duas fases, numa ordem que o app segue: `abas` (rápida, o app espera por ela
//! ao abrir, para nada nascer antes no lugar delas) recria os workspaces, as
//! abas na mesma pasta e os painéis ao lado da aba-mãe; `ia` (devagar, em
//! segundo plano) retoma em cada aba a conversa do Claude ou do Codex que ela
//! tinha, uma de cada vez — cada uma espera a anterior subir, para várias não
//! brigarem por memória ao mesmo tempo.
//!
//! O que foi feito vai para `restaurado.json` antes de cada passo: uma rodada
//! que caiu no meio continua de onde parou e nunca cria a mesma aba duas vezes,
//! e a aba que voltou e a pessoa fechou não volta de novo.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use keep_ia::daemon;
use keep_ia::linha::{self, Subida};
use keep_ia::worktrees::ambiente::Ambiente;

use crate::retrato::{self, ANTERIOR, ARQUIVO};
use crate::{AbaGuardada, Ia, Keepd, Retrato};

pub const REGISTRO: &str = "restaurado.json";

/// Quanto esperar uma conversa subir antes de passar à próxima.
const ESPERA_IA: Duration = Duration::from_secs(25);

/// Uma aba que esta restauração criou.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criada {
    pub workspace: String,
    /// O número dela no keepd do retrato.
    pub origem: u32,
    /// O número dela agora.
    pub aba: u32,
}

/// O que a restauração no keepd atual já fez.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Registro {
    pub keepd: Keepd,
    /// O keepd de que veio o retrato.
    #[serde(default)]
    pub origem: Option<Keepd>,
    #[serde(default)]
    pub abas_feitas: bool,
    #[serde(default)]
    pub feito: bool,
    #[serde(default)]
    pub criadas: Vec<Criada>,
    /// As conversas já mandadas subir.
    #[serde(default)]
    pub ias: Vec<String>,
}

/// O que uma rodada fez (a resposta em JSON).
#[derive(Clone, Debug, Default, Serialize)]
pub struct Resumo {
    pub origem: Option<String>,
    pub abas: usize,
    pub conversas: usize,
    pub falhas: Vec<String>,
    pub motivo: String,
}

fn registro() -> Option<Registro> {
    crate::ler(&crate::pasta().join(REGISTRO))
}

fn grava_registro(r: &Registro) {
    let _ = crate::gravar(&crate::pasta().join(REGISTRO), r);
}

/// A restauração no keepd `k` acabou? Sem registro dele, não havia nada pendente nele.
pub fn restauracao_acabou(k: &Keepd) -> bool {
    match registro() {
        Some(r) if r.keepd.mesmo(k) => r.feito,
        _ => true,
    }
}

/// De que retrato restaurar: o mais novo, de outro keepd, com abas.
pub fn escolher_origem(atual: &Keepd, candidatos: Vec<Retrato>) -> Option<Retrato> {
    candidatos
        .into_iter()
        .filter(|r| !r.keepd.mesmo(atual) && !r.abas.is_empty())
        .max_by_key(|r| r.gravado_ms)
}

fn candidatos() -> Vec<Retrato> {
    [ARQUIVO, ANTERIOR].iter().filter_map(|n| crate::ler::<Retrato>(&crate::pasta().join(n))).collect()
}

/// A ordem de criação: por workspace (na ordem do retrato), a aba-mãe antes
/// dos painéis dela (por profundidade na árvore), depois pelo número.
pub fn ordem(r: &Retrato) -> Vec<&AbaGuardada> {
    let por_aba: HashMap<(&str, u32), &AbaGuardada> = r.abas.iter().map(|a| ((a.workspace.as_str(), a.aba), a)).collect();
    let profundidade = |a: &AbaGuardada| {
        let (mut n, mut atual) = (0, a);
        while atual.split_of != 0 && n < 32 {
            match por_aba.get(&(atual.workspace.as_str(), atual.split_of)) {
                Some(m) => {
                    atual = m;
                    n += 1;
                }
                None => break,
            }
        }
        n
    };
    let ws = |a: &AbaGuardada| r.workspaces.iter().position(|w| *w == a.workspace).unwrap_or(usize::MAX);
    let mut abas: Vec<&AbaGuardada> = r.abas.iter().collect();
    abas.sort_by_key(|a| (ws(a), profundidade(a), a.aba));
    abas
}

/// Restaura. `so_abas`: só a primeira fase. `seco`: diz o que faria.
pub fn restaurar(so_abas: bool, seco: bool) -> Result<Resumo, String> {
    let Some(_trava) = crate::Trava::pegar(".restaurar.lock") else {
        return Ok(Resumo { motivo: "outra restauração em andamento".into(), ..Default::default() });
    };
    let amb = Ambiente::do_sistema();
    let lista = daemon::listar()?;
    let atual = retrato::keepd_da_lista(&lista, &amb);
    let mut reg = match registro() {
        Some(r) if r.keepd.mesmo(&atual) => r,
        _ => Registro { keepd: atual.clone(), ..Default::default() },
    };
    if reg.feito {
        return Ok(Resumo { motivo: "já restaurado neste keepd".into(), ..Default::default() });
    }
    let todos = candidatos();
    let origem = match &reg.origem {
        Some(o) => todos.into_iter().find(|r| r.keepd.mesmo(o)),
        None => escolher_origem(&atual, todos),
    };
    let Some(origem) = origem else {
        if !seco {
            reg.abas_feitas = true;
            reg.feito = true;
            grava_registro(&reg);
        }
        return Ok(Resumo { motivo: "nada a restaurar".into(), ..Default::default() });
    };
    let mut resumo = Resumo {
        origem: Some(format!("keepd {} de {}", origem.keepd.pid, origem.keepd.inicio_ms)),
        ..Default::default()
    };
    if seco {
        for a in ordem(&origem) {
            resumo.falhas.push(format!(
                "[seco] {}#{} em {}{}{}",
                a.workspace,
                a.aba,
                a.cwd,
                if a.split_of != 0 { format!(", painel de #{}", a.split_of) } else { String::new() },
                a.ia.as_ref().map(|i| format!(", {} {}", i.agente, i.conversa)).unwrap_or_default()
            ));
        }
        resumo.motivo = "seco".into();
        return Ok(resumo);
    }
    reg.origem = Some(origem.keepd.clone());
    grava_registro(&reg);

    if !reg.abas_feitas {
        resumo.abas = recria_abas(&origem, &lista, &mut reg, &mut resumo.falhas);
        devolve_nomes(&amb, &atual, &origem, &reg);
        reg.abas_feitas = true;
        grava_registro(&reg);
    }
    if so_abas {
        resumo.motivo = "abas recriadas; as conversas seguem".into();
        return Ok(resumo);
    }
    resumo.conversas = retoma_conversas(&origem, &mut reg, &mut resumo.falhas);
    reg.feito = true;
    grava_registro(&reg);
    resumo.motivo = "restaurado".into();
    Ok(resumo)
}

/// As conversas que já rodam no keepd atual: a aba delas não é criada de novo.
fn conversas_vivas(lista: &daemon::Retrato) -> HashSet<String> {
    let contas = keep_ia::contas::listar(false);
    keep_ia::abas::das_abas(lista, &contas).into_iter().filter_map(|i| i.conversa).collect()
}

fn recria_abas(origem: &Retrato, lista: &daemon::Retrato, reg: &mut Registro, falhas: &mut Vec<String>) -> usize {
    let vivas = conversas_vivas(lista);
    let mut novas: HashMap<(String, u32), u32> =
        reg.criadas.iter().map(|c| ((c.workspace.clone(), c.origem), c.aba)).collect();
    let mut feitas = 0;
    for a in ordem(origem) {
        let chave = (a.workspace.clone(), a.aba);
        if novas.contains_key(&chave) {
            continue;
        }
        if a.ia.as_ref().is_some_and(|i| vivas.contains(&i.conversa)) {
            continue; // a conversa já está de pé numa aba deste keepd
        }
        let cwd = Some(a.cwd.as_str()).filter(|c| !c.is_empty() && Path::new(c).is_dir());
        let mae = (a.split_of != 0).then(|| novas.get(&(a.workspace.clone(), a.split_of)).copied()).flatten();
        let (cols, rows) = (a.cols.max(20), a.rows.max(5));
        let criada = match mae {
            Some(m) => crate::daemon::nova_aba(&a.workspace, cwd, cols, rows, m, a.split_dir)
                .or_else(|_| crate::daemon::nova_aba(&a.workspace, cwd, cols, rows, 0, 0)),
            None => crate::daemon::nova_aba(&a.workspace, cwd, cols, rows, 0, 0),
        };
        match criada {
            Ok(aba) => {
                novas.insert(chave, aba);
                reg.criadas.push(Criada { workspace: a.workspace.clone(), origem: a.aba, aba });
                grava_registro(reg);
                feitas += 1;
            }
            Err(e) => falhas.push(format!("{}#{}: {e}", a.workspace, a.aba)),
        }
    }
    feitas
}

/// O nome que a pessoa deu a cada aba volta para o app (`names.json`, do keepd atual).
fn devolve_nomes(amb: &Ambiente, atual: &Keepd, origem: &Retrato, reg: &Registro) {
    let nomes: Vec<(String, u32, String)> = reg
        .criadas
        .iter()
        .filter_map(|c| {
            let a = origem.abas.iter().find(|a| a.workspace == c.workspace && a.aba == c.origem)?;
            Some((c.workspace.clone(), c.aba, a.nome.clone().filter(|_| a.split_of == 0)?))
        })
        .collect();
    crate::nomear_no_app(amb, atual.socket_nasceu, &nomes, false);
}

fn retoma_conversas(origem: &Retrato, reg: &mut Registro, falhas: &mut Vec<String>) -> usize {
    let contas = keep_ia::contas::listar(false);
    let mut feitas = 0;
    let criadas = reg.criadas.clone();
    for c in &criadas {
        let Some(a) = origem.abas.iter().find(|a| a.workspace == c.workspace && a.aba == c.origem) else { continue };
        let Some(ia) = &a.ia else { continue };
        if reg.ias.contains(&ia.conversa) {
            continue;
        }
        reg.ias.push(ia.conversa.clone());
        grava_registro(reg);
        match retoma(&c.workspace, c.aba, ia, &contas) {
            Ok(()) => feitas += 1,
            Err(e) => falhas.push(format!("{}#{} ({} {}): {e}", c.workspace, c.aba, ia.agente, ia.conversa)),
        }
    }
    feitas
}

/// A linha que retoma a conversa, como o `keep ia trocar` a monta: o programa
/// pelo caminho, os parâmetros da aba, a conta dela e a pasta presa do
/// workspace (confinar).
pub fn subida(ws: &str, ia: &Ia, programa: String, conversa: &str, contas: &[keep_ia::contas::Conta]) -> Subida {
    let mut args = Vec::new();
    if ia.agente == "codex" {
        args.push("resume".to_string());
        args.push(conversa.to_string());
        args.extend(ia.parametros.iter().cloned());
    } else {
        args.extend(ia.parametros.iter().cloned());
        args.push("--resume".into());
        args.push(conversa.to_string());
    }
    let mut definir = Vec::new();
    if !ia.conta.is_empty() {
        if let Some(conta) = keep_ia::contas::achar(contas, &ia.conta) {
            if let Ok(vars) = keep_ia::contas::ambiente(conta) {
                definir.extend(vars);
            }
        }
        definir.push(("KEEP_IA_ESCOLHA".to_string(), ia.conta.clone()));
    }
    let (programa, args) = keep_ia::confinar::envolve(ws, programa, args);
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

fn retoma(ws: &str, tab: u32, ia: &Ia, contas: &[keep_ia::contas::Conta]) -> Result<(), String> {
    let (programa, conversa) = if ia.agente == "codex" {
        let p = keep_ia::programas::codex().ok_or("o Codex não está instalado")?;
        (p, ia.conversa.clone())
    } else {
        let p = keep_ia::programas::claude().ok_or("o Claude Code não está instalado")?;
        let id = keep_ia::sessoes::continuacao(&ia.conversa);
        if !keep_ia::sessoes::conversa_claude_existe(&id) {
            return Err("a conversa não está mais em disco".into());
        }
        (p, id)
    };
    let shell = espera_shell(ws, tab).ok_or("a aba não mostrou o prompt")?;
    let s = subida(ws, ia, programa.to_string_lossy().into_owned(), &conversa, contas);
    let texto = linha::linha(linha::sintaxe(&shell), &s);
    match keep_ia::trocar::digita(ws, tab, &texto, true) {
        Ok(true) => {}
        Ok(false) => return Err("a linha não apareceu na tela".into()),
        Err(r) => return Err(r.json().to_string()),
    }
    espera_subir(ws, tab, &ia.agente);
    Ok(())
}

/// O shell da aba parou no prompt (algo na tela, nada rodando, duas leituras iguais). Devolve o nome dele.
pub(crate) fn espera_shell(ws: &str, tab: u32) -> Option<String> {
    let mut anterior = String::new();
    daemon::espera(Duration::from_secs(20), Duration::from_millis(300), || {
        let l = daemon::listar().ok()?;
        let a = l.aba(ws, tab)?;
        let tela = daemon::tela(ws, tab).ok()?;
        let pronto = !a.info.busy && !tela.trim().is_empty() && tela == anterior;
        anterior = tela;
        pronto.then(|| a.info.command.clone())
    })
}

/// Espera a IA da aba subir, respondendo a pergunta de confiança na pasta que
/// o Claude faz ao abrir numa pasta nova (como faria a pessoa que a abriu).
fn espera_subir(ws: &str, tab: u32, agente: &str) {
    let contas = keep_ia::contas::listar(false);
    let mut respostas = 0;
    let _ = daemon::espera(ESPERA_IA, Duration::from_millis(500), || {
        let tela = daemon::tela(ws, tab).unwrap_or_default();
        if let Some(tecla) = resposta_de_confianca(&tela) {
            if respostas < 4 {
                respostas += 1;
                std::thread::sleep(Duration::from_millis(1200));
                let _ = keep_ia::trocar::tecla(ws, tab, tecla);
            }
            return None;
        }
        let l = daemon::listar().ok()?;
        keep_ia::abas::das_abas(&l, &contas).into_iter().find(|i| i.workspace == ws && i.aba == tab && i.agente == agente)
    });
}

/// A pergunta "confiar nesta pasta?" na tela: Enter com o ponteiro no sim, seta para cima com ele no não.
pub fn resposta_de_confianca(tela: &str) -> Option<&'static [u8]> {
    if !tela.contains("Yes, I trust this folder") || !tela.contains("No, exit") {
        return None;
    }
    let sim = tela.lines().find(|l| l.contains("Yes, I trust this folder"))?;
    Some(if sim.contains('❯') { b"\r" } else { b"\x1b[A" })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn aba(ws: &str, n: u32, mae: u32) -> AbaGuardada {
        AbaGuardada {
            workspace: ws.into(), aba: n, cwd: "/".into(), split_of: mae, split_dir: if mae != 0 { 1 } else { 0 },
            cols: 80, rows: 24, nome: None, ia: None,
        }
    }

    fn retrato(k: u64, abas: Vec<AbaGuardada>) -> Retrato {
        Retrato {
            versao: 1, gravado_ms: k, keepd: Keepd { pid: k as u32, inicio_ms: k, socket_nasceu: None },
            workspaces: vec!["B".into(), "A".into()], abas,
        }
    }

    #[test]
    fn a_mae_vem_antes_do_painel_e_os_workspaces_na_ordem() {
        let r = retrato(1, vec![aba("A", 1, 0), aba("B", 3, 2), aba("B", 2, 5), aba("B", 5, 0), aba("B", 9, 77)]);
        let o: Vec<(&str, u32)> = ordem(&r).iter().map(|a| (a.workspace.as_str(), a.aba)).collect();
        // B primeiro (ordem do retrato); 5 (solta) e 9 (mãe que não está no retrato), depois 2, depois 3
        assert_eq!(o, vec![("B", 5), ("B", 9), ("B", 2), ("B", 3), ("A", 1)]);
    }

    #[test]
    fn origem_e_o_mais_novo_de_outro_keepd_com_abas() {
        let atual = Keepd { pid: 3, inicio_ms: 3, socket_nasceu: None };
        let vazio = retrato(2, vec![]);
        let velho = retrato(1, vec![aba("A", 1, 0)]);
        let do_atual = retrato(3, vec![aba("A", 1, 0)]);
        assert_eq!(escolher_origem(&atual, vec![vazio, velho.clone(), do_atual]).map(|r| r.gravado_ms), Some(1));
        let novo = retrato(2, vec![aba("A", 1, 0)]);
        assert_eq!(escolher_origem(&atual, vec![velho, novo]).map(|r| r.gravado_ms), Some(2));
    }

    #[test]
    fn confianca_na_pasta() {
        let no_sim = "Do you trust the files in this folder?\n ❯ 1. Yes, I trust this folder\n   2. No, exit\n";
        let no_nao = "Do you trust the files in this folder?\n   1. Yes, I trust this folder\n ❯ 2. No, exit\n";
        assert_eq!(resposta_de_confianca(no_sim), Some(&b"\r"[..]));
        assert_eq!(resposta_de_confianca(no_nao), Some(&b"\x1b[A"[..]));
        assert_eq!(resposta_de_confianca("❯ "), None);
    }

    #[test]
    fn subida_do_claude_e_do_codex() {
        let ia = Ia { agente: "claude".into(), conversa: "c1".into(), conta: String::new(), parametros: vec!["--effort".into(), "max".into()] };
        let s = subida("W", &ia, "/bin/claude".into(), "c2", &[]);
        assert_eq!(s.args, vec!["--effort", "max", "--resume", "c2"]);
        let cx = Ia { agente: "codex".into(), conversa: "t1".into(), conta: "gpt:principal".into(), parametros: vec!["-m".into(), "x".into()] };
        let s = subida("W", &cx, "/bin/codex".into(), "t1", &[]);
        assert_eq!(s.args, vec!["resume", "t1", "-m", "x"]);
        assert!(s.definir.contains(&("KEEP_IA_ESCOLHA".to_string(), "gpt:principal".to_string())));
    }
}
