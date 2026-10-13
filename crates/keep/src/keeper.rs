//! A tab kept by its client while the app has no surface for it.
//!
//! The app lets go of the surfaces of tabs out of sight for long (see
//! `Hibernation` in the macOS app): a surface keeps three render targets of
//! tens of megabytes for as long as it exists. Letting go of one used to mean
//! letting go of everything it held that the daemon does not: the history
//! beyond the page or two the daemon keeps, the main screen under a program on
//! the alternate one, the links in the history, the title, a program's
//! questions answered while nobody was looking.
//!
//! So the client does not go with its surface. All along it has followed what
//! it wrote to the surface into a terminal of its own (`Mirror`), the same
//! size and keeping as much history. When the surface goes, the app having
//! said beforehand that it means to come back (the `hibernate` file beside the
//! client), the client stays attached to the daemon and carries on without
//! one: the tab's output goes into the mirror, the program's questions are
//! answered as the surface would answer them (`facts`, written by the app),
//! and what the program says about itself — a title, a directory, the
//! clipboard, the background colour — is left in files the app watches. When a
//! surface comes back for the tab it finds this client by the directory the
//! app names (`take`), and is handed the whole terminal ([`Terminal::handover`])
//! and the client's very connection to the daemon, so that nothing written in
//! between is missed or repeated. Then the keeper is gone.

use std::io::{self, Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use keep_proto::{ClientMsg, ServerMsg};
use keep_vt::{Device, Facts, Keyboard, Terminal};

use crate::attach::KittyFlags;

/// Written by the app beside a surface it is about to let go of, and taken
/// away when it will not come back for the tab.
pub const FLAG: &str = "hibernate";
/// Where a surface coming back for the tab asks for it.
pub const SOCKET: &str = "keeper.sock";
/// What the surface would answer about itself; see `read_facts`.
pub const FACTS: &str = "facts";
/// What the program said about itself while kept.
pub const TITLE: &str = "title";
pub const PWD: &str = "pwd";
pub const CLIPBOARD: &str = "clipboard";
pub const BACKGROUND: &str = "background";
/// Written once keeping, with this process's id.
pub const KEPT: &str = "kept";
/// The mirror holds what a handover cannot carry (an image), and the app had
/// better not let go of the surface.
pub const UNKEEPABLE: &str = "unkeepable";

const HELLO: &[u8; 4] = b"KEEP";
const HANDED: &[u8; 4] = b"KEPT";

/// What the app asked of this client, beyond which tab to attach to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Follow the surface into a mirror keeping this much history, in
    /// bytes: the surface's own limit. None: no mirror, no keeping.
    pub mirror: Option<usize>,
    /// Whether the surface clusters graphemes (mode 2027) before any
    /// program says: the mirror must cut text into the same cells.
    pub grapheme: bool,
    /// Whether the surface's cursor blinks (mode 12) before any program
    /// says, as a program asking (DECRQM) is told.
    pub blink: bool,
    /// A kept tab to take over, by the directory its keeper is in.
    pub take: Option<PathBuf>,
    /// The app, whose end is the keeper's.
    pub app: Option<i32>,
}

impl Options {
    /// Read the `key=value` lines after the first three of the attach file.
    pub fn parse<'a>(lines: impl Iterator<Item = &'a str>) -> Self {
        let mut options = Self::default();
        for line in lines {
            let Some((key, value)) = line.trim().split_once('=') else { continue };
            match key {
                "mirror" => options.mirror = value.parse().ok().filter(|&n: &usize| n > 0),
                "grapheme" => options.grapheme = value == "1",
                "blink" => options.blink = value == "1",
                "take" => options.take = Some(PathBuf::from(value)).filter(|p| !value.is_empty() && p.is_absolute()),
                "app" => options.app = value.parse().ok().filter(|&p: &i32| p > 0),
                _ => {}
            }
        }
        options
    }
}

/// The terminal a surface shows, followed byte for byte: the bytes this
/// client writes to it, into a terminal of its own the same size.
pub struct Mirror {
    pub terminal: Terminal,
    size: (u16, u16),
    images: bool,
    dir: PathBuf,
    options: Options,
}

impl Mirror {
    pub fn new(cols: u16, rows: u16, options: &Options, dir: &Path) -> Option<Self> {
        let scrollback = options.mirror?;
        let mut terminal = Terminal::new(cols.max(1), rows.max(1)).ok()?.with_default_cursor_unset();
        terminal.set_scrollback_limit(scrollback).ok()?;
        // The modes the surface starts with that this library does not.
        if options.grapheme {
            terminal.write(b"\x1b[?2027h");
        }
        if options.blink {
            terminal.write(b"\x1b[?12h");
        }
        Some(Self {
            terminal,
            size: (cols.max(1), rows.max(1)),
            images: false,
            dir: dir.to_path_buf(),
            options: options.clone(),
        })
    }

    /// A mirror made again from a handover of this one.
    fn remade(&self, handover: &[u8]) -> Option<Self> {
        let mut again = Self::new(self.size.0, self.size.1, &self.options, &self.dir)?;
        again.terminal.write(handover);
        again.images = self.images;
        Some(again)
    }

    /// Follow what was written to the surface.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.terminal.write(bytes);
        // An image (the Kitty graphics protocol, the only one the surface
        // draws) is something a handover cannot carry: once one has been
        // seen, the app is told not to let go of this surface.
        if !self.images && bytes.windows(3).any(|w| w == b"\x1b_G") {
            self.images = true;
            let _ = std::fs::write(self.dir.join(UNKEEPABLE), "images\n");
        }
    }

    /// The surface's grid, as its terminal reports it: the mirror follows
    /// before what is written next, which the surface will read at that size.
    pub fn follow(&mut self, size: (u16, u16)) {
        if size != self.size && size.0 > 0 && size.1 > 0 && self.terminal.resize(size.0, size.1).is_ok() {
            self.size = size;
        }
    }
}

/// What the surface would answer, as the app wrote it: `key=value` lines,
/// colours as six hex digits.
pub fn read_facts(dir: &Path) -> Facts {
    let body = std::fs::read_to_string(dir.join(FACTS)).unwrap_or_default();
    parse_facts(&body)
}

pub fn parse_facts(body: &str) -> Facts {
    let mut facts = Facts::default();
    for line in body.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "foreground" => facts.foreground = hex(value),
            "background" => facts.background = hex(value),
            "cursor" => facts.cursor = hex(value),
            "palette" => {
                let palette: Vec<[u8; 3]> = value.split(',').filter_map(hex).collect();
                facts.palette = (palette.len() == 256).then_some(palette);
            }
            "scheme" => facts.dark = match value { "dark" => Some(true), "light" => Some(false), _ => None },
            "cell" => {
                facts.cell = value.split_once(',').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
            }
            "xtversion" => facts.xtversion = Some(value.to_string()).filter(|v| !v.is_empty()),
            "da" => {
                // conformance;features…/type;firmware;rom[/unit]
                let parts: Vec<&str> = value.split('/').collect();
                let first: Vec<u16> = parts.first().map(|p| numbers(p)).unwrap_or_default();
                let second: Vec<u16> = parts.get(1).map(|p| numbers(p)).unwrap_or_default();
                if let (Some(&conformance), [device_type, firmware, rom]) = (first.first(), second.as_slice()) {
                    facts.device = Some(Device {
                        conformance,
                        features: first[1..].to_vec(),
                        device_type: *device_type,
                        firmware: *firmware,
                        rom: *rom,
                        tertiary: parts.get(2).and_then(|u| u32::from_str_radix(u, 16).ok()),
                    });
                }
            }
            _ => {}
        }
    }
    facts
}

fn numbers(field: &str) -> Vec<u16> {
    field.split(';').filter_map(|n| n.parse().ok()).collect()
}

fn hex(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&value[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Write a file the app reads, whole or not at all.
fn publish(dir: &Path, name: &str, body: &[u8]) {
    let staging = dir.join(format!(".{name}.{}", std::process::id()));
    if std::fs::write(&staging, body).is_ok() {
        let _ = std::fs::rename(&staging, dir.join(name));
    }
}

/// Ask a kept tab's keeper for it: the terminal as it stands, at this grid,
/// and the connection to the daemon it was kept through.
pub fn take(dir: &Path, cols: u16, rows: u16) -> io::Result<(UnixStream, Vec<u8>)> {
    // By name, from inside the directory: a socket's path has to fit in a
    // hundred-odd bytes, and the app's temporary directory alone takes half.
    let here = std::env::current_dir()?;
    std::env::set_current_dir(dir)?;
    let connected = UnixStream::connect(SOCKET);
    std::env::set_current_dir(&here)?;
    let mut conn = connected?;
    conn.set_read_timeout(Some(Duration::from_secs(20)))?;
    let mut hello = HELLO.to_vec();
    hello.extend_from_slice(&cols.to_be_bytes());
    hello.extend_from_slice(&rows.to_be_bytes());
    conn.write_all(&hello)?;
    let (header, fd) = recv_with_fd(&conn, 8)?;
    let fd = fd.ok_or_else(|| io::Error::other("the keeper sent no connection"))?;
    // Owned from here, whatever happens next.
    let daemon = unsafe { UnixStream::from_raw_fd(fd) };
    if &header[..4] != HANDED {
        return Err(io::Error::other("the keeper said something else"));
    }
    let len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
    let mut snapshot = vec![0u8; len];
    conn.read_exact(&mut snapshot)?;
    daemon.set_read_timeout(None)?;
    Ok((daemon, snapshot))
}

/// Everything a keeper needs from the client it was.
pub struct Kept {
    pub daemon: UnixStream,
    pub mirror: Mirror,
    pub flags: KittyFlags,
    pub dir: PathBuf,
    pub app: Option<i32>,
}

/// Carry on without a surface, until one comes for the tab or the app lets
/// it go. Never returns.
pub fn keep(kept: Kept) -> ! {
    let Kept { mut daemon, mut mirror, mut flags, dir, app } = kept;
    let mut facts_seen = modified(&dir.join(FACTS));
    let _ = mirror.terminal.act_as_terminal(read_facts(&dir));
    // Nobody is looking: the app hid the surface before letting it go, and
    // its last word to the daemon said so. Said again, in case it did not.
    let _ = ClientMsg::Resize { cols: 0, rows: 0 }.write(&mut daemon);
    let listener = match UnixListener::bind(SOCKET) {
        Ok(listener) => Some(listener),
        Err(_) => {
            // A socket left over from a keeper that died: nobody answers it.
            let _ = std::fs::remove_file(SOCKET);
            UnixListener::bind(SOCKET).ok()
        }
    };
    if let Some(listener) = listener.as_ref() {
        let _ = listener.set_nonblocking(true);
    }
    publish(&dir, KEPT, format!("{}\n", std::process::id()).as_bytes());
    let mut background = mirror.terminal.background_colour();
    let mut checked = Instant::now();

    loop {
        let ready = readable(daemon.as_raw_fd(), listener.as_ref().map(|l| l.as_raw_fd()), 250);
        if ready.0 {
            match ServerMsg::read(&mut daemon) {
                Ok(Some(ServerMsg::Output(data))) => {
                    flags.observe(&data);
                    mirror.feed(&data);
                }
                Ok(Some(ServerMsg::Repaint(data))) => {
                    // As the client writes a repaint to its surface.
                    let mut bytes = b"\x1b[H\x1b[2J".to_vec();
                    if !KittyFlags::declared_in(&data) {
                        bytes.extend_from_slice(KittyFlags::FORGET);
                        flags.reset();
                    }
                    bytes.extend_from_slice(&data);
                    flags.observe(&bytes);
                    mirror.feed(&bytes);
                }
                Ok(Some(ServerMsg::Attached { .. })) => {}
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => leave(&dir),
            }
            let events = mirror.terminal.take_events();
            if !events.replies.is_empty() && ClientMsg::write_input(&mut daemon, &events.replies).is_err() {
                leave(&dir);
            }
            if events.title {
                publish(&dir, TITLE, mirror.terminal.title().as_bytes());
            }
            if events.pwd {
                publish(&dir, PWD, mirror.terminal.pwd().as_bytes());
            }
            if let Some(text) = events.clipboard.last() {
                publish(&dir, CLIPBOARD, text);
            }
            let now = mirror.terminal.background_colour();
            if now != background {
                background = now;
                if let Some([r, g, b]) = now {
                    publish(&dir, BACKGROUND, format!("{r:02x}{g:02x}{b:02x}\n").as_bytes());
                }
            }
        }
        if ready.1 {
            if let Some(listener) = listener.as_ref() {
                if let Ok((conn, _)) = listener.accept() {
                    let keyboard = Keyboard { main: flags.pushed(false), alternate: flags.pushed(true) };
                    match hand_over(conn, &daemon, &mut mirror, &keyboard) {
                        Ok(()) => leave(&dir),
                        // The terminal was read for the handover, which on
                        // the alternate screen means leaving it: made again
                        // from what was to be handed over.
                        Err(Failed(Some(snapshot))) => {
                            if let Some(mut again) = mirror.remade(&snapshot) {
                                let _ = again.terminal.act_as_terminal(read_facts(&dir));
                                mirror = again;
                            }
                        }
                        Err(Failed(None)) => {}
                    }
                }
            }
        }
        if checked.elapsed() >= Duration::from_millis(250) {
            checked = Instant::now();
            if !dir.join(FLAG).exists() || app.map(|pid| !alive(pid)).unwrap_or(false) {
                leave(&dir);
            }
            let seen = modified(&dir.join(FACTS));
            if seen != facts_seen {
                facts_seen = seen;
                let _ = mirror.terminal.act_as_terminal(read_facts(&dir));
            }
        }
    }
}

struct Failed(Option<Vec<u8>>);

/// Give the tab to the client that asked: the whole terminal, fitted to its
/// grid, and the connection to the daemon, unread past the last frame read
/// here.
fn hand_over(mut conn: UnixStream, daemon: &UnixStream, mirror: &mut Mirror, keyboard: &Keyboard) -> Result<(), Failed> {
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let mut hello = [0u8; 8];
    if conn.read_exact(&mut hello).is_err() || &hello[..4] != HELLO {
        return Err(Failed(None));
    }
    let cols = u16::from_be_bytes([hello[4], hello[5]]);
    let rows = u16::from_be_bytes([hello[6], hello[7]]);
    mirror.follow((cols, rows));
    let snapshot = mirror.terminal.handover(Some(keyboard));
    let mut header = HANDED.to_vec();
    header.extend_from_slice(&(snapshot.len() as u32).to_be_bytes());
    if send_with_fd(&conn, &header, daemon.as_raw_fd()).is_err() {
        return Err(Failed(Some(snapshot)));
    }
    // The connection is the other client's now: whatever happens to the
    // rest, nothing more is read from it here.
    let _ = conn.write_all(&snapshot);
    Ok(())
}

/// Stop keeping: the socket and the directory go with this process.
fn leave(dir: &Path) -> ! {
    let _ = std::fs::remove_file(SOCKET);
    let _ = std::fs::remove_dir_all(dir);
    std::process::exit(0)
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn alive(pid: i32) -> bool {
    // Signal 0 asks only whether there is such a process to signal.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Which of the two can be read without waiting, waiting at most `ms`.
fn readable(a: RawFd, b: Option<RawFd>, ms: i32) -> (bool, bool) {
    let mut fds = [
        libc::pollfd { fd: a, events: libc::POLLIN, revents: 0 },
        libc::pollfd { fd: b.unwrap_or(-1), events: libc::POLLIN, revents: 0 },
    ];
    let count = if b.is_some() { 2 } else { 1 };
    let rc = unsafe { libc::poll(fds.as_mut_ptr(), count as libc::nfds_t, ms) };
    if rc <= 0 {
        return (false, false);
    }
    let hit = |f: &libc::pollfd| f.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0;
    (hit(&fds[0]), count == 2 && hit(&fds[1]))
}

/// Wait until the daemon's socket can be read, or `wake` can: the latter is
/// the client's own surface going away.
pub fn wait(daemon: RawFd, wake: RawFd) -> (bool, bool) {
    loop {
        let ready = readable(daemon, Some(wake), -1);
        if ready.0 || ready.1 {
            return ready;
        }
    }
}

fn send_with_fd(sock: &UnixStream, data: &[u8], fd: RawFd) -> io::Result<()> {
    let mut iov = libc::iovec { iov_base: data.as_ptr() as *mut libc::c_void, iov_len: data.len() };
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = space as _;
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() {
            return Err(io::Error::other("no room for the connection"));
        }
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(cmsg) as *mut RawFd, fd);
    }
    let sent = unsafe { libc::sendmsg(sock.as_raw_fd(), &msg, 0) };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    let sent = sent as usize;
    if sent < data.len() {
        (&*sock).write_all(&data[sent..])?;
    }
    Ok(())
}

fn recv_with_fd(sock: &UnixStream, len: usize) -> io::Result<(Vec<u8>, Option<RawFd>)> {
    let mut buf = vec![0u8; len];
    let mut iov = libc::iovec { iov_base: buf.as_mut_ptr() as *mut libc::c_void, iov_len: len };
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = space as _;
    let got = unsafe { libc::recvmsg(sock.as_raw_fd(), &mut msg, 0) };
    if got <= 0 {
        return Err(if got == 0 { io::Error::from(io::ErrorKind::UnexpectedEof) } else { io::Error::last_os_error() });
    }
    let mut fd = None;
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if !cmsg.is_null() && (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
            let received = std::ptr::read_unaligned(libc::CMSG_DATA(cmsg) as *const RawFd);
            libc::fcntl(received, libc::F_SETFD, libc::FD_CLOEXEC);
            fd = Some(received);
        }
    }
    let got = got as usize;
    if got < len {
        (&*sock).read_exact(&mut buf[got..])?;
    }
    Ok((buf, fd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_attach_file_options_are_read() {
        let options = Options::parse(
            ["mirror=10000000", "grapheme=1", "blink=1", "take=/tmp/keep-attach/x", "app=123", "nonsense", "later=1"]
                .into_iter(),
        );
        assert_eq!(options.mirror, Some(10_000_000));
        assert!(options.grapheme);
        assert!(options.blink);
        assert_eq!(options.take.as_deref(), Some(Path::new("/tmp/keep-attach/x")));
        assert_eq!(options.app, Some(123));
        assert_eq!(Options::parse(["take=relative", "mirror=0"].into_iter()), Options::default());
    }

    #[test]
    fn the_facts_are_read() {
        let palette: Vec<String> = (0..256).map(|i| format!("{:02x}0000", i as u8)).collect();
        let body = format!(
            "foreground=ffffff\nbackground=282c34\ncursor=#ffffff\nscheme=light\ncell=16,34\n\
             xtversion=ghostty 1.3.2\nda=62;22;52/1;10;0\npalette={}\n",
            palette.join(",")
        );
        let facts = parse_facts(&body);
        assert_eq!(facts.background, Some([0x28, 0x2c, 0x34]));
        assert_eq!(facts.cursor, Some([0xff; 3]));
        assert_eq!(facts.dark, Some(false));
        assert_eq!(facts.cell, Some((16, 34)));
        assert_eq!(facts.xtversion.as_deref(), Some("ghostty 1.3.2"));
        assert_eq!(facts.palette.as_ref().map(|p| p[7]), Some([7, 0, 0]));
        let device = facts.device.expect("device");
        assert_eq!((device.conformance, device.features.as_slice()), (62, &[22u16, 52][..]));
        assert_eq!((device.device_type, device.firmware, device.rom, device.tertiary), (1, 10, 0, None));
        assert_eq!(parse_facts("da=62;22/1;10;0/00ab01ff\n").device.unwrap().tertiary, Some(0x00ab01ff));
    }

    /// The connection goes across with the bytes, still connected.
    #[test]
    fn a_connection_is_handed_across() {
        let (left, right) = UnixStream::pair().unwrap();
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        send_with_fd(&left, b"KEPT\0\0\0\x05", theirs.as_raw_fd()).unwrap();
        let (header, fd) = recv_with_fd(&right, 8).unwrap();
        assert_eq!(header, b"KEPT\0\0\0\x05");
        let mut received = unsafe { UnixStream::from_raw_fd(fd.expect("an fd")) };
        ours.write_all(b"hello").unwrap();
        let mut got = [0u8; 5];
        received.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hello");
        theirs.write_all(b"x").unwrap();
    }
}
