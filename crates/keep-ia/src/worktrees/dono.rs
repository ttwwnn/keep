//! De quem é uma worktree.
//!
//! Ela nasce no instante T (o nascimento de `<comum>/worktrees/<id>`); dona é
//! a ÚNICA conversa (Claude ou Codex) que tinha, em T, uma chamada executora
//! em andamento que cita o nome dela (nível A: o nome da pasta, o id do
//! registro ou o caminho, com fronteira de palavra, na entrada ou no
//! resultado) ou, sem nenhum A, o nome em molde num comando de worktree
//! (nível B). Duas ou mais com A/B: ambígua. Só "fala de worktree" (C):
//! indecidida. Nada: sem dono. Nenhuma dessas três vai para a Lixeira.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::ambiente::Ambiente;
use super::codex::{self, Evidencia};
use super::indice::{Ativa, Indice, Linhas, chamadas_em, nivel, varredura_fria};
use super::prazo::{Estourou, Prazo};
use super::repos::{Worktree, chave, nomes};
use super::texto::iso;
use super::{FOLGA, RECENTE, RETENCAO};

/// A decisão sobre uma worktree, como fica no registro.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decisao {
    /// `dono`, `ambigua`, `indecidida`, `sem-dono` (e, só no `listar`,
    /// `fora-destas-sessoes`).
    pub estado: String,
    #[serde(default)]
    pub dono: Option<String>,
    #[serde(default)]
    pub nivel: Option<String>,
    #[serde(default)]
    pub prova: Option<Value>,
    /// O melhor nível de cada conversa que tinha prova.
    #[serde(default)]
    pub niveis: BTreeMap<String, String>,
    /// Pode mudar: nascimento recente, ou chamada que ainda pode estar
    /// rodando. Fica para a próxima rodada decidir de vez.
    #[serde(default)]
    pub provisorio: bool,
    /// As conversas do Codex que contaram.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motivo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caminho: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nasceu: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decidido_em: Option<String>,
}

impl Decisao {
    fn de(estado: &str, provisorio: bool) -> Decisao {
        Decisao {
            estado: estado.into(),
            dono: None,
            nivel: None,
            prova: None,
            niveis: BTreeMap::new(),
            provisorio,
            codex: None,
            motivo: None,
            comum: None,
            repo: None,
            id: None,
            caminho: None,
            nasceu: None,
            decidido_em: None,
        }
    }

    fn sem_dono(&mut self, estado: &str, motivo: Option<String>) {
        self.estado = estado.into();
        self.dono = None;
        self.nivel = None;
        self.prova = None;
        if motivo.is_some() {
            self.motivo = motivo;
        }
    }
}

/// Julga uma worktree pelas chamadas do Claude em andamento no nascimento
/// e pelas evidências do Codex (que contam do mesmo jeito, na exclusividade
/// e para dono; a de rollout ilegível, só na exclusividade).
pub fn julgar(
    amb: &Ambiente,
    w: &Worktree,
    ativas: &[Ativa],
    linhas: &mut Linhas,
    agora: f64,
    codex: Option<&BTreeMap<String, Evidencia>>,
) -> Decisao {
    let t = w.nasceu;
    if let Some(dono) = &w.codex_dono {
        // O Codex escreveu no registro da worktree de quem ela é.
        let mut d = Decisao::de("dono", false);
        d.dono = Some(dono.clone());
        d.nivel = Some("A".into());
        d.prova = Some(json!({"ferramenta": "codex:codex-thread.json", "thread": dono}));
        d.niveis.insert(dono.clone(), "A".into());
        return d;
    }
    let ns = nomes(w);
    let mut por: HashMap<String, (char, Value)> = HashMap::new();
    let mut provisorio = agora - t < RECENTE;
    for a in ativas {
        if a.aberta {
            provisorio = true;
        }
        if !a.chamada.loc {
            continue;
        }
        let (entrada, resultado, entrando) = linhas.textos(&a.arquivo, &a.chamada);
        let Some(nv) = nivel(&entrada, &resultado, &ns, &w.caminho, entrando) else { continue };
        if por.get(&a.sid).is_none_or(|(v, _)| nv < *v) {
            let arquivo = std::path::Path::new(&a.arquivo)
                .strip_prefix(&amb.projetos)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_else(|_| a.arquivo.clone());
            let prova = json!({
                "arquivo": arquivo,
                "ferramenta": a.chamada.n,
                "tool_use_id": a.chamada.id,
                "inicio": a.chamada.i.map(iso),
                "fim": iso(a.fim),
            });
            por.insert(a.sid.clone(), (nv, prova));
        }
    }
    let cx = codex.cloned().unwrap_or_default();
    for (tid, (nv, prova, aberta)) in &cx {
        if *aberta {
            provisorio = true;
        }
        if let Some(nv) = nv {
            if por.get(tid).is_none_or(|(v, _)| *nv < *v) {
                por.insert(tid.clone(), (*nv, prova.clone()));
            }
        }
    }
    let de_nivel = |n: char| {
        let mut v: Vec<&String> = por.iter().filter(|(_, (x, _))| *x == n).map(|(s, _)| s).collect();
        v.sort();
        v
    };
    let (a, b, c) = (de_nivel('A'), de_nivel('B'), de_nivel('C'));
    let mut d = Decisao::de("sem-dono", provisorio);
    d.niveis = por.iter().map(|(s, (v, _))| (s.clone(), v.to_string())).collect();
    if !cx.is_empty() {
        d.codex = Some(cx.keys().cloned().collect());
    }
    let dono_de = |d: &mut Decisao, s: &String, n: &str| {
        d.estado = "dono".into();
        d.dono = Some(s.clone());
        d.nivel = Some(n.into());
        d.prova = Some(por[s].1.clone());
    };
    if a.len() == 1 && b.is_empty() {
        dono_de(&mut d, a[0], "A");
    } else if a.len() + b.len() > 1 {
        d.estado = "ambigua".into();
    } else if b.len() == 1 {
        dono_de(&mut d, b[0], "B");
    } else if !c.is_empty() {
        d.estado = "indecidida".into();
    }
    let ilegivel = d.prova.as_ref().and_then(|p| p.get("ilegivel")).is_some_and(|v| v == &Value::Bool(true));
    if d.estado == "dono" && d.dono.as_ref().is_some_and(|x| cx.contains_key(x)) && ilegivel {
        // A "prova" é um rollout do Codex que não se lê: podia estar criando a
        // worktree, não dá para saber.
        let quem: String = d.dono.as_deref().unwrap_or("").chars().take(8).collect();
        d.sem_dono("indecidida", Some(format!("o rollout do Codex {quem} não pôde ser lido")));
    }
    if d.estado == "dono" && w.nasceu_git.is_some_and(|g| (g - t).abs() > 2.0) {
        // A pasta e o registro nasceram longe um do outro: restaurada de uma
        // cópia, o nascimento não é o de verdade.
        let g = w.nasceu_git.unwrap();
        d.estado = "indecidida".into();
        d.dono = None;
        d.nivel = None;
        d.motivo = Some(format!("nascimento incoerente (.git {})", iso(g)));
    }
    d
}

/// As decisões para as worktrees `wts`. Nascimento coberto pelo índice: por
/// ele; senão, varredura fria. `so_sessoes`: só interessa saber se alguma
/// destas sessões pode ser a dona (a fria lê primeiro só os arquivos delas, e
/// só confere a exclusividade das worktrees em que elas têm alguma prova).
pub fn decidir(
    amb: &Ambiente,
    wts: &[&Worktree],
    ind: Option<&Indice>,
    vivas: &HashSet<String>,
    agora: f64,
    prazo: &Prazo,
    so_sessoes: Option<&HashSet<String>>,
) -> Result<BTreeMap<String, Decisao>, Estourou> {
    if amb.ganchos.decidir_lento {
        loop {
            prazo.checa()?;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    let mut saida = BTreeMap::new();
    let mut linhas = Linhas::default();
    let cobertura = agora - RETENCAO + FOLGA;
    let (recentes, mut antigas): (Vec<&Worktree>, Vec<&Worktree>) =
        wts.iter().copied().partition(|w| ind.is_some() && w.nasceu >= cobertura);
    let cx = if wts.is_empty() { HashMap::new() } else { codex::evidencias(amb, wts, agora, prazo)? };
    if let Some(ind) = ind {
        for w in &recentes {
            prazo.checa()?;
            let ativas = chamadas_em(ind, w.nasceu, vivas, agora);
            saida.insert(chave(w), julgar(amb, w, &ativas, &mut linhas, agora, cx.get(&chave(w))));
        }
    }
    if !antigas.is_empty() {
        if let Some(so) = so_sessoes {
            let ts: Vec<f64> = antigas.iter().map(|w| w.nasceu).collect();
            let fria = varredura_fria(amb, &ts, vivas, prazo, Some(so))?;
            let mut suspeitas = Vec::new();
            for w in antigas {
                let d = julgar(amb, w, &chamadas_em(&fria, w.nasceu, vivas, agora), &mut linhas, agora, cx.get(&chave(w)));
                if !d.niveis.is_empty() {
                    suspeitas.push(w);
                } else {
                    // Nenhuma destas sessões tinha chamada com prova no
                    // nascimento: não é delas (só para o `listar`, que não
                    // grava nada).
                    saida.insert(chave(w), Decisao::de("fora-destas-sessoes", d.provisorio));
                }
            }
            antigas = suspeitas;
        }
    }
    if !antigas.is_empty() {
        let ts: Vec<f64> = antigas.iter().map(|w| w.nasceu).collect();
        let fria = varredura_fria(amb, &ts, vivas, prazo, None)?;
        for w in antigas {
            prazo.checa()?;
            let ativas = chamadas_em(&fria, w.nasceu, vivas, agora);
            saida.insert(chave(w), julgar(amb, w, &ativas, &mut linhas, agora, cx.get(&chave(w))));
        }
    }
    Ok(saida)
}
