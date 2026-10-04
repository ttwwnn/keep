//! O daemon do Keep, do jeito que a IA precisa dele: a lista com os
//! processos de cada aba, a tela de uma aba, digitar numa aba e abrir uma
//! aba nova. O socket é o de `KEEP_SOCKET` (o app passa o dele), senão o
//! padrão.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use keep_proto::net::Stream;
use keep_proto::{ClientMsg, DaemonInfo, ServerMsg, TabInfo};

/// Uma aba e o workspace dela.
#[derive(Clone, Debug)]
pub struct Aba {
    pub ws: String,
    pub info: TabInfo,
}

/// A lista que o daemon deu, e de que daemon ela é.
#[derive(Clone, Debug)]
pub struct Retrato {
    pub daemon: DaemonInfo,
    pub abas: Vec<Aba>,
    /// O daemon disse os processos de cada aba (`List3`). Sem isso, nada que
    /// mexa numa aba é feito: a conta e a conversa saem desses processos.
    pub exato: bool,
}

impl Retrato {
    pub fn aba(&self, ws: &str, tab: u32) -> Option<&Aba> {
        self.abas.iter().find(|a| a.ws == ws && a.info.id == tab && !a.info.finished)
    }
}

pub fn socket() -> PathBuf {
    keep_proto::socket_path()
}

fn conecta() -> Result<Stream, String> {
    Stream::connect(socket()).map_err(|e| format!("o Keep não respondeu ({e})"))
}

fn pergunta(msg: ClientMsg) -> Result<ServerMsg, String> {
    let mut s = conecta()?;
    msg.write(&mut s).map_err(|e| e.to_string())?;
    match ServerMsg::read(&mut s) {
        Ok(Some(ServerMsg::Error(e))) => Err(e),
        Ok(Some(r)) => Ok(r),
        Ok(None) => Err("o Keep fechou sem responder".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// A lista, com os processos de cada aba quando o daemon sabe dizê-los.
pub fn listar() -> Result<Retrato, String> {
    match pergunta(ClientMsg::List3) {
        Ok(ServerMsg::Workspaces3(daemon, lista)) => Ok(Retrato { daemon, abas: achata(lista), exato: true }),
        Ok(_) => Err("resposta inesperada do Keep".into()),
        Err(_) => match pergunta(ClientMsg::List2) {
            Ok(ServerMsg::Workspaces2(lista) | ServerMsg::Workspaces(lista)) => {
                Ok(Retrato { daemon: DaemonInfo::default(), abas: achata(lista), exato: false })
            }
            Ok(_) => Err("resposta inesperada do Keep".into()),
            Err(e) => Err(e),
        },
    }
}

fn achata(lista: Vec<keep_proto::WorkspaceInfo>) -> Vec<Aba> {
    lista
        .into_iter()
        .flat_map(|w| {
            let ws = w.name;
            w.tabs.into_iter().map(move |info| Aba { ws: ws.clone(), info })
        })
        .collect()
}

/// A tela de uma aba, em texto.
pub fn tela(ws: &str, tab: u32) -> Result<String, String> {
    match pergunta(ClientMsg::Preview { workspace: ws.into(), tab })? {
        ServerMsg::PreviewText(t) => Ok(t),
        _ => Err("resposta inesperada do Keep".into()),
    }
}

/// A mesma tela com as cores (o texto esmaecido da sugestão do Claude e do
/// Codex só se distingue do digitado por elas).
pub fn tela_vt(ws: &str, tab: u32) -> Result<String, String> {
    match pergunta(ClientMsg::PreviewVt { workspace: ws.into(), tab })? {
        ServerMsg::PreviewText(t) => Ok(t),
        _ => Err("resposta inesperada do Keep".into()),
    }
}

/// Uma aba nova em `ws`, do tamanho dado. Devolve o número dela.
pub fn nova_aba(ws: &str, cwd: Option<&str>, cols: u16, rows: u16) -> Result<u32, String> {
    match pergunta(ClientMsg::NewTab {
        workspace: ws.into(),
        cwd: cwd.map(str::to_string),
        cols,
        rows,
        split_of: keep_proto::TAB_ANY,
        split_dir: keep_proto::SPLIT_NONE,
    })? {
        ServerMsg::TabCreated { tab } => Ok(tab),
        _ => Err("resposta inesperada do Keep".into()),
    }
}

/// Um teclado ligado a uma aba: o que se manda chega ao programa dela como
/// se tivesse sido digitado. Entra com o tamanho que a aba já tem, para não
/// mexer nele; sai ao ser solto.
pub struct Teclado {
    conexao: Stream,
}

impl Teclado {
    pub fn liga(aba: &Aba) -> Result<Teclado, String> {
        let mut conexao = conecta()?;
        ClientMsg::Attach { workspace: aba.ws.clone(), tab: aba.info.id, cols: aba.info.cols, rows: aba.info.rows }
            .write(&mut conexao)
            .map_err(|e| e.to_string())?;
        match ServerMsg::read(&mut conexao) {
            Ok(Some(ServerMsg::Attached { .. })) => {}
            Ok(Some(ServerMsg::Error(e))) => return Err(e),
            _ => return Err("o Keep não deixou entrar na aba".into()),
        }
        // A tela inteira e o que a aba escrever daqui em diante: lidos e
        // jogados fora, para o daemon nunca esperar por este cliente.
        let mut leitor = conexao.try_clone().map_err(|e| e.to_string())?;
        std::thread::spawn(move || while let Ok(Some(_)) = ServerMsg::read(&mut leitor) {});
        Ok(Teclado { conexao })
    }

    pub fn manda(&mut self, dados: &[u8]) -> Result<(), String> {
        ClientMsg::write_input(&mut self.conexao, dados).map_err(|e| e.to_string())?;
        self.conexao.flush().map_err(|e| e.to_string())
    }
}

impl Drop for Teclado {
    fn drop(&mut self) {
        let _ = self.conexao.shutdown(std::net::Shutdown::Both);
    }
}

/// Repete `cond` até ela dar `Some`/`true` ou o prazo acabar.
pub fn espera<T>(prazo: Duration, passo: Duration, mut cond: impl FnMut() -> Option<T>) -> Option<T> {
    let fim = Instant::now() + prazo;
    loop {
        if let Some(v) = cond() {
            return Some(v);
        }
        if Instant::now() >= fim {
            return None;
        }
        std::thread::sleep(passo);
    }
}
