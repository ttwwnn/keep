//! keepd — the session daemon.
//!
//! Owns PTYs and the screen state they produce, so clients can come and go
//! without taking the work with them.

pub mod session;
pub use session::Session;
