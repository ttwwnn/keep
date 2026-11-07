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
    connect(path).is_ok()
}

/// Connect to the daemon at `path`, and only to one that belongs to this user: whoever answers on a socket in a
/// shared folder (`/tmp/keep-<user>.sock` when no `TMPDIR` is set) gets every key typed and every screen asked
/// for, so a socket made by another user is refused. (On Windows the pipe client already checks the server's SID.)
pub fn connect(path: &std::path::Path) -> std::io::Result<Stream> {
    let stream = Stream::connect(path)?;
    #[cfg(unix)]
    {
        let (uid, euid) = (peer_uid(&stream)?, unsafe { libc::geteuid() });
        if uid != euid && euid != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("o socket {} é de outro usuário (uid {uid}), não do seu", path.display()),
            ));
        }
    }
    Ok(stream)
}

/// The uid of the process at the other end of a unix socket.
#[cfg(all(unix, not(target_os = "linux")))]
fn peer_uid(stream: &Stream) -> std::io::Result<libc::uid_t> {
    use std::os::fd::AsRawFd;
    let (mut uid, mut gid) = (0 as libc::uid_t, 0 as libc::gid_t);
    // SAFETY: both out-pointers are valid for the call, and the fd is the stream's own.
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(uid)
}

#[cfg(target_os = "linux")]
fn peer_uid(stream: &Stream) -> std::io::Result<libc::uid_t> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred` and `len` are valid for the call, and the fd is the stream's own.
    let r = unsafe {
        libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&mut cred as *mut libc::ucred).cast(), &mut len)
    };
    if r != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(cred.uid)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn connects_to_a_socket_of_the_same_user() {
        let dir = std::env::temp_dir().join(format!("keep-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("k.sock");
        let _ = std::fs::remove_file(&path);
        let _listener = Listener::bind(&path).unwrap();
        let s = connect(&path).expect("a socket of this user is accepted");
        assert_eq!(peer_uid(&s).unwrap(), unsafe { libc::geteuid() });
        assert!(is_listening(&path));
        assert!(connect(&dir.join("none.sock")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
