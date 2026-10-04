//! A linha de comando: `keep ia …` e `keep worktrees …`, em JSON.

/// `keep ia <args>`: devolve o código de saída.
pub fn ia(args: &[String]) -> i32 {
    let _ = args;
    eprintln!("keep ia: ainda não implementado");
    2
}

/// `keep worktrees <args>`.
pub fn worktrees(args: &[String]) -> i32 {
    crate::worktrees::cli(args)
}
