//! The connection to the daemon: a unix socket where there is one, a named
//! pipe on Windows.
//!
//! The frames are the same either way, so the daemon, the terminal client and
//! the Windows app share one connection type and one way of finding the
//! daemon. On unix this is the standard library's socket and nothing else —
//! the code that was written against `UnixStream` keeps running on exactly
//! that.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(windows)]
pub use crate::pipe::{Listener, Stream};

/// Whether a daemon answers at `path`.
pub fn is_listening(path: &std::path::Path) -> bool {
    Stream::connect(path).is_ok()
}
