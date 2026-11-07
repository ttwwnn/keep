// No console of its own on Windows: the daemon is started by the app or by a
// client, and a console window would sit open for as long as it runs.
#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::Result;

fn main() -> Result<()> {
    let path = keep_proto::socket_path();
    // The socket is born closed to everyone but its owner, and the umask goes back before any shell is started: the
    // shells of the tabs inherit it, and a user's files are not to come out 0600 because of the daemon.
    #[cfg(unix)]
    let umask = unsafe { libc::umask(0o177) };
    let bound = keepd::Server::bind(&path);
    #[cfg(unix)]
    unsafe {
        libc::umask(umask);
    }
    let server = match bound {
        Ok(server) => server,
        Err(e) => {
            log_failure(&e);
            return Err(e);
        }
    };
    eprintln!("keepd listening on {}", server.path().display());
    server.run()
}

/// Without a console there is nowhere to print why the daemon would not
/// start, so on Windows it is said in a file the app can read back.
fn log_failure(error: &anyhow::Error) {
    #[cfg(windows)]
    if let Ok(dir) = std::env::var("LOCALAPPDATA") {
        let dir = std::path::Path::new(&dir).join("Keep");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("keepd.log"), format!("{error:#}\n"));
    }
    #[cfg(not(windows))]
    let _ = error;
}
