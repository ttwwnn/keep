//! A IA no Keep: as contas do Claude e do GPT, o consumo de cada uma, a IA de
//! cada aba e as worktrees que uma conversa deixa — tudo do próprio Keep, no
//! macOS e no Windows. O desenho está em `docs/ia.md`.
//!
//! O app do macOS fala com isto pela linha de comando (`keep ia …`,
//! `keep worktrees …`, em JSON); o do Windows chama as funções direto.

pub mod caminhos;
pub mod cli;
pub mod contas;
pub mod daemon;
pub mod processos;
pub mod programas;
pub mod rede;
pub mod tela;
pub mod uso;
pub mod vinculo;
pub mod worktrees;

/// Versão dos objetos JSON da linha de comando.
pub const VERSAO: u32 = 1;
