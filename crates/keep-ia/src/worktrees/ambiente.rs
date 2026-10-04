//! Onde a lixeira de worktrees olha: cada pasta tem uma variável que a
//! troca, para os testes rodarem num mundo próprio sem tocar em sessão,
//! repositório, estado ou daemon de verdade.

use std::path::PathBuf;
use std::sync::Arc;

use super::abas::{Alvo, Contexto};
use super::sessoes::SessaoViva;
use super::AbaListada;

/// O vínculo aba → sessões já dado (testes): no lugar do keepd.
pub type VinculoDado = Arc<dyn Fn(&[Alvo]) -> (Vec<AbaListada>, Contexto) + Send + Sync>;

/// Para os testes trocarem um pedaço do mundo.
#[derive(Clone, Default)]
pub struct Ganchos {
    /// As sessões vivas, no lugar das de `sessions/`.
    pub vivas: Option<Vec<SessaoViva>>,
    /// O vínculo das abas, no lugar do keepd.
    pub vinculo: Option<VinculoDado>,
    /// O `decidir` gasta o prazo inteiro.
    pub decidir_lento: bool,
    /// A "Lixeira" para onde `mover_para_lixeira` renomeia.
    pub lixeira_falsa: Option<PathBuf>,
}

/// O mundo da lixeira de worktrees.
#[derive(Clone)]
pub struct Ambiente {
    pub casa: PathBuf,
    /// `~/.claude/projects` (`KEEP_WT_PROJETOS`): os transcritos.
    pub projetos: PathBuf,
    /// `~/.claude/sessions` (`KEEP_WT_SESSOES`): as sessões vivas.
    pub sessoes: PathBuf,
    /// `~/.codex` (`KEEP_WT_CODEX`): os rollouts, em `sessions/`.
    pub codex: PathBuf,
    /// Onde procurar repositórios: cada pasta e um nível abaixo dela
    /// (`KEEP_WT_RAIZES`, separadas como no PATH).
    pub raizes: Vec<PathBuf>,
    /// A Lixeira, para "já está na Lixeira" (`KEEP_WT_LIXEIRA`; no Windows,
    /// além dela, toda `$Recycle.Bin`).
    pub lixeira: Option<PathBuf>,
    /// O estado deste módulo: `<estado do keep-ia>/worktrees`.
    pub estado: PathBuf,
    /// O estado do kit-mac, lido uma vez para trazer as decisões que ele já
    /// tomou (`KIT_KEEP_ESTADO`, senão `~/.local/state/kit-keep`).
    pub kit: Option<PathBuf>,
    /// O endereço do keepd (`KEEP_SOCKET`).
    pub socket: PathBuf,
    /// A pasta de estado do app do macOS (`KEEP_STATE_DIR`): o `names.json`
    /// com os nomes que a pessoa deu às abas.
    pub app: Option<PathBuf>,
    /// O git (`KEEP_GIT`).
    pub git: PathBuf,
    #[doc(hidden)]
    pub ganchos: Ganchos,
}

fn var(nome: &str) -> Option<PathBuf> {
    std::env::var_os(nome).filter(|v| !v.is_empty()).map(PathBuf::from)
}

impl Ambiente {
    /// O mundo de verdade, com as trocas das variáveis.
    pub fn do_sistema() -> Ambiente {
        let casa = crate::caminhos::casa();
        Ambiente {
            projetos: var("KEEP_WT_PROJETOS").unwrap_or_else(|| crate::caminhos::claude().join("projects")),
            sessoes: var("KEEP_WT_SESSOES").unwrap_or_else(|| crate::caminhos::claude().join("sessions")),
            codex: var("KEEP_WT_CODEX").unwrap_or_else(crate::caminhos::codex),
            raizes: std::env::var_os("KEEP_WT_RAIZES")
                .filter(|v| !v.is_empty())
                .map(|v| std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect())
                .unwrap_or_else(|| raizes_padrao(&casa)),
            lixeira: var("KEEP_WT_LIXEIRA").or_else(|| lixeira_padrao(&casa)),
            estado: crate::caminhos::estado().join("worktrees"),
            kit: var("KIT_KEEP_ESTADO").or_else(|| Some(casa.join(".local").join("state").join("kit-keep"))),
            socket: keep_proto::socket_path(),
            app: var("KEEP_STATE_DIR").or_else(|| app_padrao(&casa)),
            git: var("KEEP_GIT").unwrap_or_else(super::git::padrao),
            casa,
            ganchos: Ganchos::default(),
        }
    }

    /// Um mundo inteiro dentro de `base` (testes): transcritos, sessões,
    /// Codex, repositórios, Lixeira, estado e app em pastas próprias, sem kit
    /// para importar, e o keepd em `socket`.
    pub fn de_teste(base: &std::path::Path, socket: PathBuf) -> Ambiente {
        let p = |n: &str| base.join(n);
        Ambiente {
            casa: base.to_path_buf(),
            projetos: p("projects"),
            sessoes: p("sessions"),
            codex: p("codex"),
            raizes: vec![p("projetos")],
            lixeira: Some(p("Trash")),
            estado: p("estado"),
            kit: None,
            socket,
            app: Some(p("app")),
            git: super::git::padrao(),
            ganchos: Ganchos::default(),
        }
    }
}

/// As pastas onde as pessoas costumam guardar repositórios, e a casa.
fn raizes_padrao(casa: &std::path::Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        "projetos",
        "Projetos",
        "projects",
        "Projects",
        "Developer",
        "dev",
        "src",
        "code",
        "Code",
        "repos",
        "git",
        "GitHub",
        "source/repos",
        "Documents/GitHub",
    ]
    .iter()
    .map(|n| casa.join(n))
    .collect();
    v.push(casa.to_path_buf());
    v
}

fn lixeira_padrao(casa: &std::path::Path) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return Some(casa.join(".Trash"));
    }
    if cfg!(windows) {
        return None;
    }
    Some(var("XDG_DATA_HOME").unwrap_or_else(|| casa.join(".local").join("share")).join("Trash"))
}

fn app_padrao(casa: &std::path::Path) -> Option<PathBuf> {
    cfg!(target_os = "macos").then(|| casa.join("Library").join("Application Support").join("Keep"))
}
