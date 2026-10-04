//! As worktrees git que a conversa de uma aba criou, para irem à Lixeira
//! quando a aba fecha. Ver `docs/ia.md` ("Lixeira de worktrees").
//!
//! O critério é o da pessoa que pediu ("sempre que eu fechar uma aba o wt
//! dela deve ser movido pra lixeira"): a worktree da aba é a que a conversa
//! daquela aba (Claude ou Codex) criou; aba aberta está em uso; na dúvida,
//! não é da aba. O app só mostra a lista no diálogo, fecha as abas e move as
//! pastas; toda a decisão fica aqui.
//!
//! ```text
//! keep worktrees listar [--prazo SEG] ALVO...    ALVO = <workspace>:<aba>[,<painel>,...]
//!     {"versao":1,"abas":[{alvo,agente,vinculo,sessoes,pids}],"lixeira":[{caminho,repo,id,ramo,head,
//!      alteracoes,commits_so_aqui,sessao,submodulos,processos,nasceu_ns}],"mantidas":[{caminho,motivo}],
//!      "avisos":[...]}
//!     O JSON sai SEMPRE antes do prazo (padrão 3 s, contado do nascimento do processo); o que não deu
//!     tempo vira aviso e não vai para a Lixeira. Git que passa de ALVO_RAPIDO sai do registro que o
//!     indexar grava (no máximo GIT_VALIDADE de idade), com aviso.
//! keep worktrees preparar CAMINHO [--nasceu NS]
//!     confere (worktree ligada, destravada, sem processo com a pasta atual dentro, o nascimento
//!     listado) e grava refs/keep-lixeira/<id> = HEAD (+ <id>-sujo = `git stash create`)
//! keep worktrees concluir CAMINHO DESTINO
//!     depois que a pasta saiu: `git worktree remove CAMINHO` (só aquele registro; nunca prune),
//!     menos em repositório com submódulos; uma linha JSON em <estado>/lixeira.log
//! keep worktrees indexar [--json]
//!     (o app chama a cada minuto) nascimentos -> dono, histórico aba -> conversas, o índice dos
//!     transcritos e o git de cada worktree candidata (no máximo GIT_ORCAMENTO s por rodada)
//! ```
//!
//! Dono (algoritmo A4 do kit): a worktree nasce no instante T; dona é a
//! ÚNICA conversa com uma chamada executora em andamento em T que cita o
//! nome dela (A) ou, sem nenhum A, o nome em molde (B); duas ou mais é
//! ambígua, só "fala de worktree" (C) é indecidida. Nunca: `git worktree
//! prune`, apagar pasta, mexer em worktree travada ou na principal.
//!
//! O app do macOS chama a linha de comando (o `keep` que vai dentro dele);
//! o do Windows chama `listar`, `preparar`, `mover_para_lixeira` e
//! `concluir` direto.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[doc(hidden)]
pub mod abas;
#[doc(hidden)]
pub mod ambiente;
#[doc(hidden)]
pub mod codex;
#[doc(hidden)]
pub mod dono;
#[doc(hidden)]
pub mod estado;
#[doc(hidden)]
pub mod git;
#[doc(hidden)]
pub mod indexar;
#[doc(hidden)]
pub mod indice;
#[doc(hidden)]
pub mod listar;
#[doc(hidden)]
pub mod lixeira;
#[doc(hidden)]
pub mod pastas;
#[doc(hidden)]
pub mod prazo;
#[doc(hidden)]
pub mod repos;
#[doc(hidden)]
pub mod sessoes;
#[doc(hidden)]
pub mod texto;
#[doc(hidden)]
pub mod varredura;

pub use abas::{Alvo, analisa_alvo};
pub use ambiente::Ambiente;

/// Segundos em volta da janela [início, fim] de cada chamada.
pub const FOLGA: f64 = 2.0;
/// O índice guarda as chamadas que terminaram nas últimas 6 h.
pub const RETENCAO: f64 = 6.0 * 3600.0;
/// Chamada sem resultado "ainda rodando" (sessão viva), ou o teto da janela.
pub const RODANDO_MAX: f64 = 900.0;
/// Nascimento há menos que isso: decisão provisória (a linha pode não ter
/// chegado ao transcrito).
pub const RECENTE: f64 = 10.0;
pub const PRAZO_PADRAO: f64 = 3.0;
/// Segundos desde o nascimento do processo: o git ainda rodando aí usa o
/// registro do `indexar`.
pub const ALVO_RAPIDO: f64 = 1.1;
/// O `indexar` refaz o git de cada worktree depois disso.
pub const GIT_IDADE: f64 = 60.0;
/// Segundos de git por rodada do `indexar`.
pub const GIT_ORCAMENTO: f64 = 1.0;
/// Registro do git mais velho que isso não substitui o git ao vivo.
pub const GIT_VALIDADE: f64 = 900.0;
/// Reservado, no prazo, para montar e escrever o JSON.
pub const MARGEM: f64 = 0.35;
/// Antes do prazo, o JSON sai de qualquer jeito.
pub const MARGEM_DURA: f64 = 0.2;
/// As ferramentas que executam (as que podem criar uma worktree).
pub const EXECUTORAS: [&str; 6] = ["Bash", "EnterWorktree", "Agent", "Task", "Monitor", "PowerShell"];
pub const ARQ_ESTADO: &str = "worktrees.json";
pub const ARQ_INDICE: &str = "transcritos.json";
pub const LOG_LIXEIRA: &str = "lixeira.log";
pub const LOG: &str = "worktrees.log";
pub const DIR_PENDENTES: &str = "lixeira-pendente";
pub const BLOCO: usize = 8 << 20;
/// A versão dos arquivos de estado (a mesma do kit).
pub const VERSAO_DO_ESTADO: u32 = 1;

/// A resposta do `listar`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Listagem {
    pub versao: u32,
    pub abas: Vec<AbaListada>,
    /// O que vai para a Lixeira.
    pub lixeira: Vec<ItemDaLixeira>,
    /// O que é da aba e fica, com o motivo.
    pub mantidas: Vec<Mantida>,
    pub avisos: Vec<String>,
}

impl Listagem {
    pub fn nova() -> Listagem {
        Listagem { versao: crate::VERSAO, ..Listagem::default() }
    }
}

/// Uma aba (com os painéis) do pedido.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbaListada {
    pub alvo: String,
    /// `claude`, `codex`, `shell` ou `nenhum`.
    pub agente: String,
    /// `exato` (o daemon disse os processos), `provavel` (deduzido: nada se
    /// move), `ambiguo` ou `nenhum`.
    pub vinculo: String,
    /// As conversas da aba (só com vínculo exato).
    pub sessoes: Vec<String>,
    /// Os processos que acabam com a aba: o app espera por eles antes de mover.
    pub pids: Vec<u32>,
}

impl AbaListada {
    pub fn nenhum(alvo: &str) -> AbaListada {
        AbaListada { alvo: alvo.into(), agente: "nenhum".into(), vinculo: "nenhum".into(), ..AbaListada::default() }
    }
}

/// Uma worktree que vai para a Lixeira.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemDaLixeira {
    pub caminho: String,
    pub repo: String,
    pub id: String,
    pub ramo: Option<String>,
    pub head: Option<String>,
    /// Arquivos alterados, como o `git status` conta.
    pub alteracoes: u64,
    pub commits_so_aqui: Option<u64>,
    /// A conversa dona.
    pub sessao: String,
    pub submodulos: bool,
    /// Processos (de fora de qualquer aba) ainda com a pasta atual dentro.
    pub processos: Vec<ProcessoDentro>,
    /// O nascimento, para o `preparar` conferir que é a mesma.
    pub nasceu_ns: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessoDentro {
    pub pid: u32,
    pub nome: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mantida {
    pub caminho: String,
    pub motivo: String,
}

fn nada<T>(v: &Option<T>) -> bool {
    v.is_none()
}

/// A resposta do `preparar`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Preparo {
    pub versao: u32,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "nada")]
    pub motivo: Option<String>,
    /// Quem ainda trabalha dentro (quando é isso que impede).
    #[serde(default, skip_serializing_if = "nada")]
    pub processos: Option<Vec<ProcessoDentro>>,
    #[serde(default, skip_serializing_if = "nada")]
    pub caminho: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub id: Option<String>,
    /// `refs/keep-lixeira/<id>`: a HEAD guardada.
    #[serde(default, skip_serializing_if = "nada")]
    pub r#ref: Option<String>,
    /// `refs/keep-lixeira/<id>-sujo`: as alterações rastreadas guardadas.
    #[serde(default, skip_serializing_if = "nada")]
    pub ref_sujo: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "nada")]
    pub ramo: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "nada")]
    pub head: Option<String>,
}

/// A resposta do `concluir`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Conclusao {
    pub versao: u32,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "nada")]
    pub caminho: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub destino: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub ramo: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "nada")]
    pub head: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "nada")]
    pub r#ref: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "nada")]
    pub ref_sujo: Option<Option<String>>,
    /// O registro da worktree saiu do repositório.
    #[serde(default, skip_serializing_if = "nada")]
    pub registro_removido: Option<bool>,
    #[serde(default, skip_serializing_if = "nada")]
    pub nota: Option<String>,
    #[serde(default, skip_serializing_if = "nada")]
    pub motivo: Option<String>,
}

/// O resumo de uma rodada do `indexar`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Indexacao {
    pub versao: u32,
    /// Outra rodada estava em curso: esta não fez nada.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ocupado: bool,
    pub worktrees: usize,
    pub repos: usize,
    pub decididas_agora: usize,
    pub provisorias: usize,
    pub por_estado: std::collections::BTreeMap<String, usize>,
    pub donos: std::collections::BTreeMap<String, Vec<String>>,
    pub historico: Value,
    pub novas_no_historico: usize,
    pub indice: Value,
    pub ms_decidir: u64,
    pub git: Value,
    pub ms: u64,
}

/// As worktrees que vão com as abas `alvos` (`<workspace>:<aba>[,<painel>…]`),
/// em no máximo `prazo` segundos.
pub fn listar(alvos: &[String], prazo: f64) -> Result<Listagem, String> {
    let alvos = alvos.iter().map(|a| analisa_alvo(a)).collect::<Result<Vec<_>, _>>()?;
    Ok(listar_em(&Ambiente::do_sistema(), alvos, prazo, 0.0))
}

/// `listar` num mundo dado; `decorrido`: quanto do prazo já passou.
pub fn listar_em(amb: &Ambiente, alvos: Vec<Alvo>, prazo: f64, decorrido: f64) -> Listagem {
    listar::com_prazo(amb, alvos, prazo, decorrido)
}

/// Confere a worktree e guarda os commits dela; `nasceu_ns`: o que o
/// `listar` mostrou.
pub fn preparar(caminho: &Path, nasceu_ns: Option<i64>) -> Preparo {
    preparar_em(&Ambiente::do_sistema(), caminho, nasceu_ns)
}

pub fn preparar_em(amb: &Ambiente, caminho: &Path, nasceu_ns: Option<i64>) -> Preparo {
    lixeira::preparar(amb, &caminho.to_string_lossy(), nasceu_ns)
}

/// Depois que a pasta saiu do lugar: tira o registro dela do repositório.
pub fn concluir(caminho: &Path, destino: &Path) -> Conclusao {
    concluir_em(&Ambiente::do_sistema(), caminho, destino)
}

pub fn concluir_em(amb: &Ambiente, caminho: &Path, destino: &Path) -> Conclusao {
    lixeira::concluir(amb, &caminho.to_string_lossy(), &destino.to_string_lossy())
}

/// Uma rodada do `indexar` (nada, se outra estiver em curso).
pub fn indexar() -> Indexacao {
    indexar_em(&Ambiente::do_sistema(), None)
}

pub fn indexar_em(amb: &Ambiente, agora: Option<f64>) -> Indexacao {
    let Some(_trava) = estado::trava(amb) else {
        return Indexacao { versao: crate::VERSAO, ocupado: true, ..Indexacao::default() };
    };
    indexar::indexar(amb, agora)
}

/// Move a pasta para a Lixeira do sistema; devolve onde ela ficou (vazio se
/// o sistema não diz).
pub fn mover_para_lixeira(caminho: &Path) -> anyhow::Result<PathBuf> {
    mover_para_lixeira_em(&Ambiente::do_sistema(), caminho)
}

pub fn mover_para_lixeira_em(amb: &Ambiente, caminho: &Path) -> anyhow::Result<PathBuf> {
    lixeira::mover_para_lixeira(amb, caminho)
}

const USO: &str = "\
keep worktrees — as worktrees que a conversa de uma aba criou, para irem à Lixeira com ela

  keep worktrees listar [--prazo SEG] <workspace>:<aba>[,<painel>...]...
  keep worktrees preparar CAMINHO [--nasceu NS]
  keep worktrees concluir CAMINHO DESTINO
  keep worktrees indexar [--json]

Respostas em JSON (\"versao\": 1). Ver docs/ia.md.";

/// Quanto o processo já viveu (o prazo do app conta do spawn).
fn decorrido() -> f64 {
    let agora_ms = texto::agora() * 1000.0;
    crate::processos::um(std::process::id())
        .map(|p| p.inicio_ms)
        .filter(|&t| t > 0)
        .map(|t| ((agora_ms - t as f64) / 1000.0).max(0.0))
        .unwrap_or(0.0)
}

/// `keep worktrees <args>`: devolve o código de saída.
pub fn cli(args: &[String]) -> i32 {
    let (codigo, saida) = executar(&Ambiente::do_sistema(), args, decorrido());
    if !saida.is_empty() {
        println!("{saida}");
    }
    codigo
}

fn em_json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// A linha de comando num mundo dado: (código de saída, o que imprimir).
#[doc(hidden)]
pub fn executar(amb: &Ambiente, args: &[String], decorrido: f64) -> (i32, String) {
    let Some(cmd) = args.first() else { return (2, USO.into()) };
    let resto = &args[1..];
    match cmd.as_str() {
        "-h" | "--help" | "ajuda" => (0, USO.into()),
        "listar" => cmd_listar(amb, resto, decorrido),
        "preparar" => cmd_preparar(amb, resto),
        "concluir" => cmd_concluir(amb, resto),
        "indexar" => {
            let r = indexar_em(amb, None);
            if resto.iter().any(|a| a == "--json") {
                let texto = if r.ocupado {
                    em_json(&json!({"versao": crate::VERSAO, "ocupado": true}))
                } else {
                    serde_json::to_string_pretty(&r).unwrap_or_default()
                };
                (0, texto)
            } else {
                (0, String::new())
            }
        }
        outro => (2, em_json(&json!({"versao": crate::VERSAO, "erro": format!("subcomando desconhecido: {outro}")}))),
    }
}

fn cmd_listar(amb: &Ambiente, args: &[String], decorrido: f64) -> (i32, String) {
    let uso = |e: String| {
        (2, em_json(&json!({"versao": crate::VERSAO, "erro": format!("uso: keep worktrees listar [--prazo SEG] ALVO... ({e})")})))
    };
    let mut prazo = PRAZO_PADRAO;
    let mut alvos = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let valor = if a == "--prazo" {
            i += 1;
            Some(args.get(i).cloned().unwrap_or_default())
        } else {
            a.strip_prefix("--prazo=").map(str::to_string)
        };
        match valor {
            Some(v) => match v.trim().parse::<f64>() {
                Ok(p) if p.is_finite() => prazo = p,
                _ => return uso(format!("prazo inválido: {v:?}")),
            },
            None => match analisa_alvo(a) {
                Ok(alvo) => alvos.push(alvo),
                Err(e) => return uso(e),
            },
        }
        i += 1;
    }
    (0, em_json(&listar_em(amb, alvos, prazo, decorrido)))
}

fn cmd_preparar(amb: &Ambiente, args: &[String]) -> (i32, String) {
    let uso = || {
        (
            2,
            em_json(&json!({"versao": crate::VERSAO, "ok": false, "motivo": "uso: keep worktrees preparar CAMINHO [--nasceu NS]"})),
        )
    };
    let mut nasceu = None;
    let mut caminhos = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--nasceu" {
            match args.get(i + 1).filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())) {
                Some(v) => nasceu = v.parse::<i64>().ok(),
                None => return uso(),
            }
            i += 2;
            continue;
        }
        caminhos.push(&args[i]);
        i += 1;
    }
    if caminhos.len() != 1 {
        return uso();
    }
    (0, em_json(&preparar_em(amb, Path::new(caminhos[0]), nasceu)))
}

fn cmd_concluir(amb: &Ambiente, args: &[String]) -> (i32, String) {
    if args.len() != 2 {
        return (
            2,
            em_json(&json!({"versao": crate::VERSAO, "ok": false, "motivo": "uso: keep worktrees concluir CAMINHO DESTINO"})),
        );
    }
    (0, em_json(&concluir_em(amb, Path::new(&args[0]), Path::new(&args[1]))))
}
