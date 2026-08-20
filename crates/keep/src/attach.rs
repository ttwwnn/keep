//! The attach loop: raw terminal in, tab output out.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::terminal;
use keep_proto::{ClientMsg, ServerMsg};

/// Ctrl-\ detaches. Chosen because almost nothing binds it, so it does not
/// shadow a key the program inside the tab wanted.
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
    Ended,
}

pub fn attach(socket: &std::path::Path, name: &str, tab: u32) -> Result<Outcome> {
    let (cols, rows) = terminal::size().unwrap_or((80, 24));

    let mut sock = UnixStream::connect(socket).context("connect to daemon")?;
    ClientMsg::Attach { workspace: name.to_string(), tab, cols, rows }.write(&mut sock)?;

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
            let mut legacy = Legacy::new();
            loop {
                let read = match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let decoded = legacy.decode(&buf[..read]);
                let n = decoded.len();
                if n == 0 {
                    continue;
                }
                let buf = &decoded[..];
                if let Some(pos) = buf[..n].iter().position(|b| *b == DETACH_BYTE) {
                    // Forward whatever preceded the detach key, then stop.
                    if pos > 0 {
                        let _ = ClientMsg::write_input(&mut input_sock, &buf[..pos]);
                    }
                    input_flag.store(true, Ordering::Release);
                    let _ = input_sock.shutdown(std::net::Shutdown::Both);
                    break;
                }
                if ClientMsg::write_input(&mut input_sock, &buf[..n]).is_err() {
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

    // daemon -> stdout. Locked once for the whole loop: `stdout()` takes the
    // process-wide handle on every call, and this loop runs once per chunk of
    // terminal output.
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // Reported after the lock is released — the raw-mode guard writes to
    // stdout on its way out.
    let mut failure: Option<String> = None;
    let outcome = loop {
        match ServerMsg::read(&mut sock) {
            Ok(Some(ServerMsg::Repaint(data))) | Ok(Some(ServerMsg::Output(data))) => {
                out.write_all(&data)?;
                out.flush()?;
            }
            // Which tab we landed on; the caller asked for TAB_ANY.
            Ok(Some(ServerMsg::Attached { .. })) => continue,
            Ok(Some(ServerMsg::Ended)) => break Outcome::Ended,
            Ok(Some(ServerMsg::Error(msg))) => {
                failure = Some(msg);
                break Outcome::Ended;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => {
                break if detached.load(Ordering::Acquire) {
                    Outcome::Detached
                } else {
                    Outcome::Ended
                };
            }
        }
    };
    drop(out);

    if let Some(msg) = failure {
        drop(_raw);
        anyhow::bail!("{msg}");
    }

    Ok(outcome)
}

/// Turn Kitty keyboard sequences back into the bytes a shell expects.
///
/// The terminal this client runs in speaks the Kitty keyboard protocol: it
/// encodes ctrl-c as `CSI 3 ; 5 u` rather than as the byte 0x03. The shell at
/// the other end of the socket never agreed to that protocol and cannot read
/// it — it printed the escape and carried on, and nothing could be cancelled.
///
/// A relay between two terminals that disagree is the place to reconcile
/// them, so the sequences are decoded here on their way through. Only the
/// form `CSI <code> ; <modifiers> u` is touched; everything else, arrows and
/// function keys included, passes untouched because it does not end in `u`.
pub struct Legacy {
    /// A sequence split across two reads. Escape sequences are short, and a
    /// buffer that grows without limit on malformed input is a way to be
    /// killed by whatever is on the other end.
    partial: Vec<u8>,
}

impl Legacy {
    const LIMIT: usize = 32;

    pub fn new() -> Self {
        Self { partial: Vec::new() }
    }

    pub fn decode(&mut self, input: &[u8]) -> Vec<u8> {
        let mut source = std::mem::take(&mut self.partial);
        source.extend_from_slice(input);

        let mut out = Vec::with_capacity(source.len());
        let mut at = 0;
        while at < source.len() {
            if source[at] != 0x1b {
                out.push(source[at]);
                at += 1;
                continue;
            }
            match Self::sequence(&source[at..]) {
                // A complete CSI ... u: keep what it means, drop how it was
                // written.
                Step::Decoded(byte, length) => {
                    if let Some(byte) = byte {
                        out.push(byte);
                    }
                    at += length;
                }
                Step::Untouched(length) => {
                    out.extend_from_slice(&source[at..at + length]);
                    at += length;
                }
                // Ends mid-sequence: hold it for the next read rather than
                // forwarding half an escape.
                Step::Incomplete => {
                    if source.len() - at <= Self::LIMIT {
                        self.partial.extend_from_slice(&source[at..]);
                    } else {
                        out.extend_from_slice(&source[at..]);
                    }
                    return out;
                }
            }
        }
        out
    }
}

enum Step {
    /// A `CSI ... u` worth this byte, and how long it was.
    Decoded(Option<u8>, usize),
    /// Something else of this length, to be passed on as it is.
    Untouched(usize),
    Incomplete,
}

impl Legacy {
    fn sequence(bytes: &[u8]) -> Step {
        if bytes.len() < 2 {
            return Step::Incomplete;
        }
        if bytes[1] != b'[' {
            return Step::Untouched(1);
        }
        let mut end = 2;
        while end < bytes.len() {
            let byte = bytes[end];
            // Parameters and separators, then a final byte that says what the
            // sequence is.
            if byte.is_ascii_digit() || byte == b';' || byte == b':' {
                end += 1;
                continue;
            }
            if !byte.is_ascii_alphabetic() && byte != b'~' {
                return Step::Untouched(end + 1);
            }
            if byte != b'u' {
                return Step::Untouched(end + 1);
            }
            let params = &bytes[2..end];
            return Step::Decoded(Self::byte_for(params), end + 1);
        }
        if bytes.len() > Self::LIMIT {
            Step::Untouched(bytes.len())
        } else {
            Step::Incomplete
        }
    }

    /// The byte a `CSI code ; modifiers u` stands for.
    ///
    /// Kitty writes the modifiers as a bitmask plus one, with control at bit
    /// two. A key pressed with control is the byte the key would send with
    /// control held: letters fold into the C0 range, and a code already in
    /// that range is the byte itself — which is what this terminal sends for
    /// ctrl-c, `CSI 3 ; 5 u`, three being the byte it already means.
    fn byte_for(params: &[u8]) -> Option<u8> {
        let text = std::str::from_utf8(params).ok()?;
        let mut fields = text.split(';');
        let code: u32 = fields.next()?.split(':').next()?.parse().ok()?;
        let modifiers: u32 = fields
            .next()
            .and_then(|f| f.split(':').next())
            .and_then(|f| f.parse().ok())
            .unwrap_or(1);
        let held = modifiers.saturating_sub(1);
        let ctrl = held & 0b100 != 0;

        if code < 0x20 {
            return u8::try_from(code).ok();
        }
        if ctrl && (b'a'..=b'z').contains(&(code as u8)) {
            return Some((code as u8) & 0x1f);
        }
        if ctrl && (b'A'..=b'Z').contains(&(code as u8)) {
            return Some((code as u8) & 0x1f);
        }
        // Not something this cares about: a plain key that the terminal chose
        // to report in the protocol. Its own byte.
        if code < 0x80 && held == 0 {
            return u8::try_from(code).ok();
        }
        None
    }
}

#[cfg(test)]
mod legacy_tests {
    use super::Legacy;

    #[test]
    fn ctrl_c_becomes_the_byte_a_tty_turns_into_a_signal() {
        let mut legacy = Legacy::new();
        assert_eq!(legacy.decode(b"\x1b[3;5u"), vec![0x03]);
    }

    #[test]
    fn a_letter_held_with_control_folds_into_the_control_range() {
        let mut legacy = Legacy::new();
        assert_eq!(legacy.decode(b"\x1b[100;5u"), vec![0x04]);
        assert_eq!(legacy.decode(b"\x1b[122;5u"), vec![0x1a]);
    }

    #[test]
    fn ordinary_typing_passes_through() {
        let mut legacy = Legacy::new();
        assert_eq!(legacy.decode(b"ls -la\r"), b"ls -la\r".to_vec());
    }

    #[test]
    fn arrows_and_other_escapes_are_left_alone() {
        let mut legacy = Legacy::new();
        assert_eq!(legacy.decode(b"\x1b[A\x1b[3~\x1bOP"), b"\x1b[A\x1b[3~\x1bOP".to_vec());
    }

    #[test]
    fn a_sequence_split_across_reads_is_still_decoded() {
        let mut legacy = Legacy::new();
        assert_eq!(legacy.decode(b"abc\x1b[3"), b"abc".to_vec());
        assert_eq!(legacy.decode(b";5u!"), vec![0x03, b'!']);
    }

    #[test]
    fn nonsense_does_not_grow_without_limit() {
        let mut legacy = Legacy::new();
        let junk = [0x1b; 200];
        let out = legacy.decode(&junk);
        assert!(!out.is_empty(), "a flood of escapes was swallowed whole");
    }
}
