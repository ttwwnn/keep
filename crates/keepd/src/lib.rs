//! keepd — the session daemon.
//!
//! Owns PTYs and the screen state they produce, so clients can come and go
//! without taking the work with them.
//!
//! The shape is: a `Registry` of named `Session`s, each holding one or more
//! `Tab`s. A tab is one child process on one PTY.

pub mod registry;
pub mod server;
pub mod session;
pub mod tab;

pub use registry::Registry;
pub use server::Server;
pub use session::Session;
pub use tab::{Attachment, Tab};
