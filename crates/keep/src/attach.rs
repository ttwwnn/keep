//! The attach loop: raw terminal in, session output out.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::terminal;
use keep_proto::{ClientMsg, ServerMsg};

/// Ctrl-\ detaches. Chosen because almost nothing binds it, so it does not
/// shadow a key the program inside the session wanted.
const DETACH_BYTE: u8 = 0x1c;

/// Restores the terminal on every exit path, including panics.
struct RawGuard;

impl RawGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode().context("enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        // Leave the cursor somewhere sane and re-show it: the session may
        // have hidden it or parked it mid-screen.
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?25h\r\n");
        let _ = out.flush();
    }
}

pub enum Outcome {
    Detached,
    SessionEnded,
}

pub fn attach(socket: &std::path::Path, name: &str, tab: u32) -> Result<Outcome> {
    let (cols, rows) = terminal::size().unwrap_or((80, 24));

    let mut sock = UnixStream::connect(socket).context("connect to daemon")?;
    ClientMsg::Attach { session: name.to_string(), tab, cols, rows }.write(&mut sock)?;

    let _raw = RawGuard::enter()?;

    let detached = Arc::new(AtomicBool::new(false));

    // stdin -> daemon
    let mut input_sock = sock.try_clone().context("clone socket")?;
    let input_flag = Arc::clone(&detached);
    std::thread::Builder::new()
        .name("keep-stdin".into())
        .spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buf = [0u8; 4096];
            loop {
                let n = match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if let Some(pos) = buf[..n].iter().position(|b| *b == DETACH_BYTE) {
                    // Forward whatever preceded the detach key, then stop.
                    if pos > 0 {
                        let _ = ClientMsg::Input(buf[..pos].to_vec()).write(&mut input_sock);
                    }
                    input_flag.store(true, Ordering::Release);
                    let _ = input_sock.shutdown(std::net::Shutdown::Both);
                    break;
                }
                if ClientMsg::Input(buf[..n].to_vec()).write(&mut input_sock).is_err() {
                    break;
                }
            }
        })
        .context("spawn stdin thread")?;

    // Window size changes. Polling instead of SIGWINCH keeps this portable
    // and costs one cheap syscall every 200ms.
    let mut resize_sock = sock.try_clone().context("clone socket")?;
    let resize_flag = Arc::clone(&detached);
    std::thread::Builder::new()
        .name("keep-resize".into())
        .spawn(move || {
            let mut last = (cols, rows);
            while !resize_flag.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(200));
                if let Ok(now) = terminal::size() {
                    if now != last {
                        last = now;
                        let msg = ClientMsg::Resize { cols: now.0, rows: now.1 };
                        if msg.write(&mut resize_sock).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .context("spawn resize thread")?;

    // daemon -> stdout
    let mut stdout = std::io::stdout();
    let outcome = loop {
        match ServerMsg::read(&mut sock) {
            Ok(Some(ServerMsg::Repaint(data))) | Ok(Some(ServerMsg::Output(data))) => {
                stdout.write_all(&data)?;
                stdout.flush()?;
            }
            // Which tab we landed on; the caller asked for TAB_ANY.
            Ok(Some(ServerMsg::Attached { .. })) => continue,
            Ok(Some(ServerMsg::Ended)) => break Outcome::SessionEnded,
            Ok(Some(ServerMsg::Error(msg))) => {
                drop(_raw);
                anyhow::bail!("{msg}");
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => {
                break if detached.load(Ordering::Acquire) {
                    Outcome::Detached
                } else {
                    Outcome::SessionEnded
                };
            }
        }
    };

    Ok(outcome)
}
