//! keepd — the session daemon.
//!
//! Owns PTYs and the screen state they produce, so clients can come and go
//! without taking the work with them.

pub mod registry;
pub mod server;
pub mod session;

pub use registry::Registry;
pub use server::Server;
pub use session::{Attachment, Session};
