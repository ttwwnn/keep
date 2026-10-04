//! keepd — the session daemon.
//!
//! Owns PTYs and the screen state they produce, so clients can come and go
//! without taking the work with them.
//!
//! The shape is: a `Registry` of named `Workspace`s, each holding one or
//! more `Tab`s. A tab is one child process on one PTY.

pub mod registry;
pub mod server;
pub mod workspace;
pub mod tab;
#[cfg(windows)]
pub mod winproc;

pub use registry::Registry;
pub use server::Server;
pub use workspace::Workspace;
pub use tab::{Attachment, Tab};
