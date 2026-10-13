//! O pedaço do keepd que o `keep_ia::daemon` não faz: a aba nova como painel
//! de outra (`split_of`/`split_dir`), que é como uma aba dividida volta.

use keep_proto::net::Stream;
use keep_proto::{ClientMsg, ServerMsg};

/// Uma aba nova em `ws`; com `split_of`, painel dessa aba (que tem de existir), do lado `split_dir`.
pub fn nova_aba(ws: &str, cwd: Option<&str>, cols: u16, rows: u16, split_of: u32, split_dir: u8) -> Result<u32, String> {
    let mut s = Stream::connect(keep_proto::socket_path()).map_err(|e| format!("o Keep não respondeu ({e})"))?;
    ClientMsg::NewTab { workspace: ws.into(), cwd: cwd.map(str::to_string), cols, rows, split_of, split_dir }
        .write(&mut s)
        .map_err(|e| e.to_string())?;
    match ServerMsg::read(&mut s) {
        Ok(Some(ServerMsg::TabCreated { tab })) => Ok(tab),
        Ok(Some(ServerMsg::Error(e))) => Err(e),
        Ok(_) => Err("resposta inesperada do Keep".into()),
        Err(e) => Err(e.to_string()),
    }
}
