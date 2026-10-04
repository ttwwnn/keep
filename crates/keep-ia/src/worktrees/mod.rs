//! As worktrees git que a conversa de uma aba criou, para irem à Lixeira
//! quando a aba fecha. Ver `docs/ia.md` ("Lixeira de worktrees").

#[doc(hidden)]
pub mod pastas;
#[doc(hidden)]
pub mod prazo;
#[doc(hidden)]
pub mod sessoes;
#[doc(hidden)]
pub mod texto;

/// `keep worktrees <args>`: devolve o código de saída.
pub fn cli(args: &[String]) -> i32 {
    let _ = args;
    eprintln!("keep worktrees: ainda não implementado");
    2
}
