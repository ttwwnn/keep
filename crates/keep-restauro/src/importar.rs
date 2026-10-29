//! `keep restauro importar`: as abas de outro computador, para o Keep daqui recriá-las.
//!
//! Quem leva o Keep para um computador novo traz a pasta que o Keep de lá tinha (no Mac do autor, o pacote
//! do kit-mac): `origem.json` (o HOME e o keepd de lá) e `restauro/retrato.json` (as abas). Aqui o retrato
//! vira o `anterior.json` deste Keep, com as pastas traduzidas para este HOME, e o registro da restauração
//! do keepd daqui volta a "não feito": o Keep aberto as recria em até 30 s (`manter`); sem Keep aberto, na
//! primeira abertura. O que já estava aqui fica: a restauração só acrescenta.
//!
//!  - a pasta que não existe aqui vira o HOME;
//!  - a conversa do Claude volta se o transcrito dela está aqui (ou vai estar: `--disponivel`); a que já
//!    roda aqui não ganha outra aba; a do Codex (a thread não vem) e a sem transcrito voltam como shell;
//!  - o keepd do retrato nunca é o daqui (o nascimento muda), e o retrato fica mais velho que o daqui: um
//!    reinício antes da restauração volta às abas deste computador, não às de lá;
//!  - com a restauração do keepd daqui no meio, nada se grava (`restaurando`).
//!
//!   keep restauro importar --de <pasta> [--disponivel <id>]… [--seco]
//!   keep restauro importado --pid <n> --inicio <ms> [--keepd-atual <pid>:<ms>]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::restaurar::{REGISTRO, Registro};
use crate::retrato::{ANTERIOR, ARQUIVO};
use crate::{Keepd, Retrato};

pub struct Pedido {
    /// A pasta do Keep de lá: `origem.json` e `restauro/retrato.json`.
    pub de: PathBuf,
    /// Conversas cujo transcrito vai estar aqui antes de o Keep restaurar (as que vêm junto).
    pub disponiveis: HashSet<String>,
    pub seco: bool,
}

#[derive(Default, Deserialize)]
struct Origem {
    #[serde(default)]
    home: String,
    #[serde(default)]
    keepd: Option<Keepd>,
}

/// O HOME daqui, como o caminho real (as pastas do retrato de lá começam pelo HOME real de lá).
fn casa() -> String {
    let h = keep_ia::caminhos::casa();
    let h = std::fs::canonicalize(&h).unwrap_or(h);
    h.to_string_lossy().trim_end_matches('/').to_string()
}

/// A pasta de lá, no HOME daqui.
pub fn traduz(cwd: &str, home_origem: &str, home: &str) -> String {
    if !cwd.is_empty() && !home_origem.is_empty() && (cwd == home_origem || cwd.starts_with(&format!("{home_origem}/"))) {
        format!("{home}{}", &cwd[home_origem.len()..])
    } else {
        cwd.to_string()
    }
}

fn curto(id: &str) -> String {
    format!("{}…", id.chars().take(8).collect::<String>())
}

fn guarda_copia(arquivo: &Path, carimbo: u64) -> Option<PathBuf> {
    if !arquivo.is_file() {
        return None;
    }
    let nome = arquivo.file_name()?.to_string_lossy().into_owned();
    let copia = arquivo.with_file_name(format!("{nome}.antes-de-importar-{carimbo}"));
    std::fs::copy(arquivo, &copia).ok().map(|_| copia)
}

pub fn importar(p: &Pedido) -> Result<Value, String> {
    let origem: Origem = crate::ler(&p.de.join("origem.json")).unwrap_or_default();
    let retrato: Retrato = crate::ler(&p.de.join("restauro").join(ARQUIVO))
        .ok_or_else(|| format!("{} não traz restauro/retrato.json no formato do Keep (abas e keepd)", p.de.display()))?;
    let home_origem = origem.home.trim_end_matches('/').to_string();
    if home_origem.is_empty() {
        return Err(format!("{}/origem.json sem o HOME de lá: sem ele as pastas das abas não se traduzem", p.de.display()));
    }
    let home = casa();
    let pasta = crate::pasta();
    // O Keep daqui pelo retrato que o app grava a cada 30 s (o keepd atual e as conversas de pé).
    let aqui: Option<Retrato> = crate::ler(&pasta.join(ARQUIVO));
    let atual = aqui.as_ref().map(|r| r.keepd.clone());
    let restaurando = |reg: Option<Registro>| match (&atual, reg) {
        (Some(a), Some(r)) => r.keepd.mesmo(a) && !r.feito,
        _ => false,
    };
    let vivas: HashSet<&str> = aqui
        .iter()
        .flat_map(|r| r.abas.iter())
        .filter_map(|a| a.ia.as_ref())
        .map(|i| i.conversa.as_str())
        .filter(|c| !c.is_empty())
        .collect();

    let mut keepd = origem.keepd.clone().unwrap_or_else(|| retrato.keepd.clone());
    if keepd.inicio_ms == 0 {
        keepd.inicio_ms = retrato.gravado_ms.max(1);
    }
    if atual.as_ref().is_some_and(|a| keepd.mesmo(a)) {
        keepd.inicio_ms += 1;
    }
    let mut gravado = retrato.gravado_ms.max(1);
    if let Some(a) = aqui.as_ref().filter(|a| a.gravado_ms > 0) {
        gravado = gravado.min(a.gravado_ms.saturating_sub(1000));
    }
    let gravado = gravado.max(1);

    let mut abas = Vec::new();
    let mut linhas = Vec::new();
    for a in retrato.abas.iter().filter(|a| !a.workspace.is_empty()) {
        let mut b = a.clone();
        let mut notas: Vec<String> = Vec::new();
        let cwd = traduz(&a.cwd, &home_origem, &home);
        b.cwd = if cwd.is_empty() {
            notas.push("sem pasta: abre no HOME".into());
            home.clone()
        } else if !Path::new(&cwd).is_dir() {
            notas.push(format!("pasta {cwd} não existe aqui: abre no HOME"));
            home.clone()
        } else {
            cwd
        };
        if let Some(ia) = a.ia.as_ref() {
            let id = ia.conversa.as_str();
            if id.is_empty() {
                b.ia = None;
            } else if ia.agente != "claude" {
                b.ia = None;
                notas.push(format!("conversa do {} não vem: volta como shell", if ia.agente.is_empty() { "?" } else { &ia.agente }));
            } else if vivas.contains(id) {
                notas.push(format!("{} já aberta aqui: o Keep não a duplica", curto(id)));
            } else if !p.disponiveis.contains(id) && crate::continuar::transcrito(id).is_none() {
                b.ia = None;
                notas.push(format!("{} sem transcrito aqui: volta como shell", curto(id)));
            } else if !ia.conta.is_empty() && ia.conta != "claude:ordem" {
                notas.push(format!("retoma {} (conta {}: se não existir aqui, sobe seguindo a ordem)", curto(id), ia.conta));
            } else {
                notas.push(format!("retoma {}", curto(id)));
            }
        }
        linhas.push(json!({ "workspace": a.workspace, "aba": a.aba, "cwd": b.cwd, "notas": notas }));
        abas.push(b);
    }
    let workspaces = if retrato.workspaces.is_empty() {
        let mut w: Vec<String> = abas.iter().map(|a| a.workspace.clone()).collect::<HashSet<_>>().into_iter().collect();
        w.sort();
        w
    } else {
        retrato.workspaces.clone()
    };
    let anterior = Retrato { versao: retrato.versao, gravado_ms: gravado, keepd: keepd.clone(), workspaces, abas };
    let mut resposta = json!({
        "versao": crate::VERSAO,
        "ok": true,
        "seco": p.seco,
        "keepdAtual": atual,
        "keepdSintetico": keepd,
        "workspaces": anterior.workspaces,
        "quantas": anterior.abas.len(),
        "comConversa": anterior.abas.iter().filter(|a| a.ia.is_some()).count(),
        "abas": linhas,
        "gravado": false,
    });
    let registro = || crate::ler::<Registro>(&pasta.join(REGISTRO));
    if p.seco {
        resposta["restaurando"] = json!(restaurando(registro()));
        return Ok(resposta);
    }
    // As duas travas do restauro: nem a restauração nem o retrato do app mexem nestes arquivos enquanto isto grava.
    let espera = Duration::from_secs(20);
    let _restauracao = crate::Trava::esperar(".restaurar.lock", espera).ok_or("outra restauração em andamento")?;
    let _retrato = crate::Trava::esperar(".retrato.lock", espera).ok_or("outro retrato em andamento")?;
    if restaurando(registro()) {
        resposta["ok"] = json!(false);
        resposta["motivo"] = json!("restaurando");
        resposta["detalhe"] = json!("O Keep daqui está no meio de uma restauração; espere ela acabar e rode de novo.");
        return Ok(resposta);
    }
    let carimbo = crate::agora_ms();
    let caminho = pasta.join(ANTERIOR);
    resposta["anteriorGuardado"] = json!(guarda_copia(&caminho, carimbo));
    crate::gravar(&caminho, &anterior).map_err(|e| format!("não deu para gravar {}: {e}", caminho.display()))?;
    resposta["gravado"] = json!(true);
    if let Some(a) = &atual {
        let caminho = pasta.join(REGISTRO);
        resposta["registroGuardado"] = json!(guarda_copia(&caminho, carimbo));
        crate::gravar(&caminho, &Registro { keepd: a.clone(), ..Default::default() })
            .map_err(|e| format!("não deu para gravar {}: {e}", caminho.display()))?;
    }
    resposta["registroZerado"] = json!(atual.is_some());
    Ok(resposta)
}

/// Em que pé está a importação cujo keepd sintético é `sint`: `feito` (o Keep recriou as abas),
/// `restaurando`, `pendente` (o `anterior.json` espera o Keep), `keepd-mudou` (reiniciou antes de o Keep
/// restaurar: as abas de lá não voltam sozinhas) ou `perdido` (outro `anterior.json` tomou o lugar).
/// `atual_ao_importar`: o keepd daqui quando se importou.
pub fn importado(sint: &Keepd, atual_ao_importar: Option<&Keepd>) -> Value {
    let pasta = crate::pasta();
    let consumido = crate::ler::<Registro>(&pasta.join(REGISTRO)).filter(|r| r.origem.as_ref().is_some_and(|o| o.mesmo(sint)));
    let estado = if let Some(r) = consumido {
        return json!({
            "versao": crate::VERSAO, "ok": true, "estado": if r.feito { "feito" } else { "restaurando" },
            "criadas": r.criadas.len(), "conversas": r.ias.len(),
        });
    } else if crate::ler::<Retrato>(&pasta.join(ANTERIOR)).is_some_and(|a| a.keepd.mesmo(sint)) {
        let atual = crate::ler::<Retrato>(&pasta.join(ARQUIVO)).map(|r| r.keepd);
        match (atual, atual_ao_importar) {
            (Some(a), Some(antes)) if !a.mesmo(antes) => "keepd-mudou",
            _ => "pendente",
        }
    } else {
        "perdido"
    };
    json!({ "versao": crate::VERSAO, "ok": true, "estado": estado })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{AbaGuardada, Ia};

    /// Uma casa falsa (HOME, Claude e restauro) só deste teste; as variáveis são do processo: um teste por vez.
    struct Casa {
        raiz: PathBuf,
        _trava: std::sync::MutexGuard<'static, ()>,
    }

    static TRAVA: std::sync::Mutex<()> = std::sync::Mutex::new(());

    impl Casa {
        fn nova(nome: &str) -> Casa {
            let trava = TRAVA.lock().unwrap_or_else(|e| e.into_inner());
            let raiz = std::env::temp_dir().join(format!("keep-importar-{nome}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&raiz);
            std::fs::create_dir_all(raiz.join("home/Projetos/foo")).unwrap();
            std::fs::create_dir_all(raiz.join("home/.claude/projects")).unwrap();
            let raiz = std::fs::canonicalize(&raiz).unwrap();
            // SAFETY: a trava garante um teste por vez mexendo no ambiente.
            unsafe {
                std::env::set_var("KEEP_IA_HOME", raiz.join("home"));
                std::env::set_var("KEEP_RESTAURO_ESTADO", raiz.join("restauro"));
            }
            Casa { raiz, _trava: trava }
        }

        fn home(&self) -> String {
            self.raiz.join("home").to_string_lossy().into_owned()
        }

        fn restauro(&self, nome: &str) -> PathBuf {
            self.raiz.join("restauro").join(nome)
        }

        fn transcrito(&self, id: &str) {
            let d = self.raiz.join("home/.claude/projects/-x");
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{id}.jsonl")), "{}\n").unwrap();
        }

        /// O Keep daqui: retrato do keepd atual com uma conversa aberta, e a restauração dele feita ou não.
        fn keep_aqui(&self, feito: bool) {
            let r = retrato(aqui(), vec![aba("emilia", 1, &self.home(), Some(("claude", ABERTA, "claude:ordem")))], 1_792_530_000_000);
            crate::gravar(&self.restauro(ARQUIVO), &r).unwrap();
            crate::gravar(&self.restauro(REGISTRO), &Registro { keepd: aqui(), abas_feitas: feito, feito, ..Default::default() }).unwrap();
        }

        /// A pasta que veio do Keep de lá.
        fn de_la(&self, keepd_origem: Keepd) -> PathBuf {
            let de = self.raiz.join("pacote/keep");
            let abas = vec![
                aba("W1", 1, &format!("{LA}/Projetos/foo"), None),
                aba("W2", 1, LA, Some(("claude", VEM, "claude:reserva"))),
                AbaGuardada { split_of: 1, ..aba("W2", 2, LA, None) },
                aba("W1", 2, &format!("{LA}/nada"), Some(("claude", ABERTA, "claude:ordem"))),
                aba("W3", 1, &format!("{LA}/Projetos/foo"), Some(("claude", SEM, "claude:ordem"))),
                aba("W3", 2, LA, Some(("codex", "thread-1", "gpt:principal"))),
            ];
            let mut r = retrato(la(), abas, 1_792_527_000_000);
            r.workspaces = vec!["W1".into(), "W2".into(), "W3".into()];
            crate::gravar(&de.join("restauro").join(ARQUIVO), &r).unwrap();
            crate::gravar(&de.join("origem.json"), &json!({ "home": LA, "keepd": keepd_origem })).unwrap();
            de
        }
    }

    impl Drop for Casa {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("KEEP_IA_HOME");
                std::env::remove_var("KEEP_RESTAURO_ESTADO");
            }
            let _ = std::fs::remove_dir_all(&self.raiz);
        }
    }

    const LA: &str = "/Users/origem";
    const ABERTA: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const VEM: &str = "bbbbbbbb-0000-0000-0000-000000000002";
    const SEM: &str = "cccccccc-0000-0000-0000-000000000003";

    fn aqui() -> Keepd {
        Keepd { pid: 2069, inicio_ms: 1_792_528_768_404, socket_nasceu: Some(1_792_528_768.404_165_5) }
    }

    fn la() -> Keepd {
        Keepd { pid: 777, inicio_ms: 1_792_000_000_000, socket_nasceu: Some(1_792_000_000.0) }
    }

    fn retrato(keepd: Keepd, abas: Vec<AbaGuardada>, gravado_ms: u64) -> Retrato {
        Retrato { versao: 1, gravado_ms, keepd, workspaces: Vec::new(), abas }
    }

    fn aba(ws: &str, n: u32, cwd: &str, ia: Option<(&str, &str, &str)>) -> AbaGuardada {
        AbaGuardada {
            workspace: ws.into(),
            aba: n,
            cwd: cwd.into(),
            split_of: 0,
            split_dir: 0,
            cols: 100,
            rows: 30,
            nome: Some(format!("{ws}#{n}")),
            ia: ia.map(|(agente, conversa, conta)| Ia {
                agente: agente.into(),
                conversa: conversa.into(),
                conta: conta.into(),
                parametros: vec!["--effort".into(), "max".into()],
            }),
            adormecida: false,
            nome_posto: None,
        }
    }

    fn pedido(de: PathBuf, disponiveis: &[&str], seco: bool) -> Pedido {
        Pedido { de, disponiveis: disponiveis.iter().map(|s| s.to_string()).collect(), seco }
    }

    #[test]
    fn o_retrato_de_la_vira_o_anterior_daqui_e_a_restauracao_volta_a_nao_feita() {
        let c = Casa::nova("aplica");
        c.keep_aqui(true);
        let de = c.de_la(la());
        let r = importar(&pedido(de, &[VEM], false)).unwrap();
        assert_eq!((r["ok"].as_bool(), r["gravado"].as_bool(), r["registroZerado"].as_bool()), (Some(true), Some(true), Some(true)), "{r}");
        let ant: Retrato = crate::ler(&c.restauro(ANTERIOR)).unwrap();
        assert_eq!(ant.workspaces, ["W1", "W2", "W3"]);
        assert_eq!(ant.keepd, la(), "o keepd de lá fica (é outro, não o daqui)");
        assert!(ant.gravado_ms < 1_792_530_000_000, "mais velho que o retrato daqui: um reinício prefere as abas deste computador");
        let por = |ws: &str, n: u32| ant.abas.iter().find(|a| a.workspace == ws && a.aba == n).unwrap().clone();
        assert_eq!(por("W1", 1).cwd, format!("{}/Projetos/foo", c.home()), "pasta traduzida para este HOME");
        assert_eq!(por("W1", 1).nome.as_deref(), Some("W1#1"));
        assert_eq!(por("W1", 2).cwd, c.home(), "pasta inexistente cai para o HOME");
        assert_eq!(por("W1", 2).ia.unwrap().conversa, ABERTA, "conversa aberta aqui fica: o Keep não a duplica");
        assert_eq!(por("W2", 1).ia.unwrap().parametros, ["--effort", "max"]);
        assert_eq!(por("W2", 2).split_of, 1);
        assert!(por("W3", 1).ia.is_none(), "sem transcrito em lugar nenhum: volta como shell");
        assert!(por("W3", 2).ia.is_none(), "thread do Codex não vem");
        let reg: Registro = crate::ler(&c.restauro(REGISTRO)).unwrap();
        assert_eq!(reg, Registro { keepd: aqui(), ..Default::default() });
        let aqui_depois: Retrato = crate::ler(&c.restauro(ARQUIVO)).unwrap();
        assert_eq!(aqui_depois.keepd, aqui(), "o retrato daqui não é tocado");
        assert!(r["registroGuardado"].as_str().is_some_and(|p| Path::new(p).is_file()), "o registro de antes fica guardado");
        // O transcrito que já está aqui também vale, sem --disponivel (o Keep já restaurou a primeira importação).
        c.keep_aqui(true);
        c.transcrito(SEM);
        importar(&pedido(c.raiz.join("pacote/keep"), &[], false)).unwrap();
        let ant: Retrato = crate::ler(&c.restauro(ANTERIOR)).unwrap();
        assert!(ant.abas.iter().find(|a| a.workspace == "W3" && a.aba == 1).unwrap().ia.is_some());
        assert!(ant.abas.iter().find(|a| a.workspace == "W2" && a.aba == 1).unwrap().ia.is_none(), "sem o transcrito nem --disponivel: shell");
    }

    #[test]
    fn retrato_de_la_mais_novo_fica_mais_velho_que_o_daqui() {
        let c = Casa::nova("gravado");
        c.keep_aqui(true);
        let de = c.de_la(la());
        let mut r: Retrato = crate::ler(&de.join("restauro").join(ARQUIVO)).unwrap();
        r.gravado_ms = 1_792_600_000_000;
        crate::gravar(&de.join("restauro").join(ARQUIVO), &r).unwrap();
        importar(&pedido(de, &[], false)).unwrap();
        let ant: Retrato = crate::ler(&c.restauro(ANTERIOR)).unwrap();
        assert_eq!(ant.gravado_ms, 1_792_530_000_000 - 1000, "um reinício antes da restauração volta às abas daqui");
    }

    #[test]
    fn seco_nao_grava_e_restauracao_no_meio_bloqueia() {
        let c = Casa::nova("seco");
        c.keep_aqui(false);
        let de = c.de_la(la());
        let antes = std::fs::read(c.restauro(REGISTRO)).unwrap();
        let r = importar(&pedido(de.clone(), &[VEM], true)).unwrap();
        assert_eq!((r["gravado"].as_bool(), r["restaurando"].as_bool()), (Some(false), Some(true)), "{r}");
        assert!(r["abas"].to_string().contains("não existe aqui: abre no HOME"));
        assert!(r["abas"].to_string().contains("conta claude:reserva"));
        let r = importar(&pedido(de, &[VEM], false)).unwrap();
        assert_eq!((r["ok"].as_bool(), r["motivo"].as_str()), (Some(false), Some("restaurando")), "{r}");
        assert!(!c.restauro(ANTERIOR).exists(), "nada gravado");
        assert_eq!(std::fs::read(c.restauro(REGISTRO)).unwrap(), antes);
    }

    #[test]
    fn keepd_de_la_igual_ao_daqui_e_afastado_e_keep_nunca_aberto() {
        let c = Casa::nova("keepd");
        c.keep_aqui(true);
        let de = c.de_la(aqui());
        importar(&pedido(de.clone(), &[], false)).unwrap();
        let ant: Retrato = crate::ler(&c.restauro(ANTERIOR)).unwrap();
        assert!(!ant.keepd.mesmo(&aqui()), "nunca o mesmo keepd que o daqui");
        // Keep nunca aberto aqui: o anterior.json vale na primeira abertura, e não há registro a zerar.
        std::fs::remove_dir_all(c.raiz.join("restauro")).unwrap();
        let r = importar(&pedido(de, &[], false)).unwrap();
        assert_eq!(r["registroZerado"].as_bool(), Some(false));
        assert!(c.restauro(ANTERIOR).is_file());
        assert!(!c.restauro(REGISTRO).exists());
    }

    #[test]
    fn importado_pendente_restaurando_feito_keepd_mudou_e_perdido() {
        let c = Casa::nova("estado");
        c.keep_aqui(true);
        let r = importar(&pedido(c.de_la(la()), &[VEM], false)).unwrap();
        let sint: Keepd = serde_json::from_value(r["keepdSintetico"].clone()).unwrap();
        assert_eq!(importado(&sint, Some(&aqui()))["estado"], "pendente");
        let reg = |feito: bool| Registro { keepd: aqui(), origem: Some(sint.clone()), abas_feitas: true, feito, ias: vec![VEM.into()], ..Default::default() };
        crate::gravar(&c.restauro(REGISTRO), &reg(false)).unwrap();
        assert_eq!(importado(&sint, Some(&aqui()))["estado"], "restaurando");
        crate::gravar(&c.restauro(REGISTRO), &reg(true)).unwrap();
        let v = importado(&sint, Some(&aqui()));
        assert_eq!((v["estado"].as_str(), v["conversas"].as_u64()), (Some("feito"), Some(1)), "{v}");
        // Reiniciou antes de o Keep restaurar: o keepd daqui é outro.
        crate::gravar(&c.restauro(REGISTRO), &Registro { keepd: aqui(), ..Default::default() }).unwrap();
        let novo = Keepd { pid: 500, inicio_ms: 1_792_540_000_000, socket_nasceu: None };
        crate::gravar(&c.restauro(ARQUIVO), &retrato(novo, Vec::new(), 1_792_540_001_000)).unwrap();
        assert_eq!(importado(&sint, Some(&aqui()))["estado"], "keepd-mudou");
        // O keepd novo girou o retrato por cima do anterior.json importado.
        crate::gravar(&c.restauro(ANTERIOR), &retrato(aqui(), Vec::new(), 1_792_530_000_000)).unwrap();
        assert_eq!(importado(&sint, Some(&aqui()))["estado"], "perdido");
    }

    #[test]
    fn sem_retrato_ou_sem_home_de_la_e_erro() {
        let c = Casa::nova("erro");
        let de = c.raiz.join("vazio");
        std::fs::create_dir_all(&de).unwrap();
        assert!(importar(&pedido(de.clone(), &[], true)).unwrap_err().contains("retrato.json"));
        let de = c.de_la(la());
        crate::gravar(&de.join("origem.json"), &json!({ "keepd": la() })).unwrap();
        assert!(importar(&pedido(de, &[], true)).unwrap_err().contains("HOME"));
    }
}
