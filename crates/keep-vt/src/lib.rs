//! Safe bindings for `libghostty-vt`: terminal state without rendering.
//!
//! This crate owns the piece that makes detach/attach possible. A `Terminal`
//! consumes the raw byte stream coming out of a PTY, keeps the resulting
//! screen state, and can replay that state later — which is exactly what a
//! client needs when it reconnects to a session it did not start.
//!
//! The library does no rendering, no font handling and no PTY management.

use std::ffi::c_void;
use std::os::raw::c_int;

/// Raw FFI surface. Prefer the safe wrappers below.
pub mod ffi {
    use std::ffi::c_void;
    use std::os::raw::c_int;

    #[repr(C)]
    pub struct TerminalImpl {
        _private: [u8; 0],
    }
    pub type Terminal = *mut TerminalImpl;

    #[repr(C)]
    pub struct FormatterImpl {
        _private: [u8; 0],
    }
    pub type Formatter = *mut FormatterImpl;

    pub const SUCCESS: c_int = 0;

    /// `GhosttyTerminalData` values we read.
    pub const DATA_TITLE: c_int = 12;
    pub const DATA_PWD: c_int = 13;

    /// A borrowed string owned by the terminal. Only valid until the next
    /// mutating call, so copy before releasing the lock.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GhosttyString {
        pub ptr: *const u8,
        pub len: usize,
    }

    pub const FORMAT_PLAIN: c_int = 0;
    pub const FORMAT_VT: c_int = 1;
    pub const FORMAT_HTML: c_int = 2;

    /// Screen-level extras. `size` must be set to the struct size.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ScreenExtra {
        pub size: usize,
        pub cursor: bool,
        pub style: bool,
        pub hyperlink: bool,
        pub protection: bool,
        pub kitty_keyboard: bool,
        pub charsets: bool,
    }

    /// Terminal-level extras. `size` must be set to the struct size.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct TerminalExtra {
        pub size: usize,
        pub palette: bool,
        pub modes: bool,
        pub scrolling_region: bool,
        pub tabstops: bool,
        pub pwd: bool,
        pub keyboard: bool,
        pub screen: ScreenExtra,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FormatterOptions {
        /// Must be set to the struct size; this is a versioned struct.
        pub size: usize,
        pub emit: c_int,
        pub unwrap: bool,
        pub trim: bool,
        pub extra: TerminalExtra,
        pub selection: *const c_void,
    }

    extern "C" {
        pub fn ghostty_terminal_new(
            allocator: *const c_void,
            out: *mut Terminal,
            cols: u16,
            rows: u16,
        ) -> c_int;
        pub fn ghostty_terminal_free(terminal: Terminal);
        pub fn ghostty_terminal_get(terminal: Terminal, data: c_int, out: *mut c_void) -> c_int;
        pub fn ghostty_terminal_vt_write(terminal: Terminal, data: *const u8, len: usize);
        pub fn ghostty_terminal_resize(
            terminal: Terminal,
            cols: u16,
            rows: u16,
            cell_width_px: u32,
            cell_height_px: u32,
        ) -> c_int;

        pub fn ghostty_formatter_terminal_new(
            allocator: *const c_void,
            out: *mut Formatter,
            terminal: Terminal,
            options: FormatterOptions,
        ) -> c_int;
        pub fn ghostty_formatter_format_alloc(
            formatter: Formatter,
            allocator: *const c_void,
            out_ptr: *mut *mut u8,
            out_len: *mut usize,
        ) -> c_int;
        pub fn ghostty_formatter_free(formatter: Formatter);

        pub fn ghostty_free(allocator: *const c_void, ptr: *mut u8, len: usize);
    }
}

/// What a formatted snapshot should look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Plain text, no escape sequences. Good for search and previews.
    Plain,
    /// VT sequences preserving colors, styles and hyperlinks.
    ///
    /// This is the repaint format: write it to a host terminal and the
    /// screen is restored as the session left it.
    Vt,
    /// HTML with inline styles.
    Html,
}

impl Format {
    fn as_raw(self) -> c_int {
        match self {
            Format::Plain => ffi::FORMAT_PLAIN,
            Format::Vt => ffi::FORMAT_VT,
            Format::Html => ffi::FORMAT_HTML,
        }
    }
}

/// A libghostty-vt error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub c_int);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "libghostty-vt error {}", self.0)
    }
}

impl std::error::Error for Error {}

/// A headless terminal: feed it PTY bytes, ask it for the screen.
pub struct Terminal {
    raw: ffi::Terminal,
}

// The handle is owned exclusively; libghostty-vt forbids concurrent writes.
unsafe impl Send for Terminal {}

impl Terminal {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        let mut raw: ffi::Terminal = std::ptr::null_mut();
        let rc = unsafe { ffi::ghostty_terminal_new(std::ptr::null(), &mut raw, cols, rows) };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }
        Ok(Self { raw })
    }

    /// Feed bytes read from the PTY.
    pub fn write(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        unsafe { ffi::ghostty_terminal_vt_write(self.raw, data.as_ptr(), data.len()) };
    }

    /// The title the program inside set with OSC 0/2.
    ///
    /// Empty when nothing has set one. Shells and editors set this constantly,
    /// which makes it the cheapest way to label a tab with what it is doing.
    pub fn title(&self) -> String {
        self.borrowed_string(ffi::DATA_TITLE)
    }

    /// The working directory the program inside reported via OSC 7.
    pub fn pwd(&self) -> String {
        self.borrowed_string(ffi::DATA_PWD)
    }

    fn borrowed_string(&self, data: c_int) -> String {
        let mut out = ffi::GhosttyString { ptr: std::ptr::null(), len: 0 };
        let rc = unsafe {
            ffi::ghostty_terminal_get(self.raw, data, &mut out as *mut _ as *mut c_void)
        };
        if rc != ffi::SUCCESS || out.ptr.is_null() || out.len == 0 {
            return String::new();
        }
        // Copy immediately: the pointer dies on the next mutating call.
        let bytes = unsafe { std::slice::from_raw_parts(out.ptr, out.len) };
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// Resize the grid.
    ///
    /// Must be kept in step with the PTY window size, or the child will draw
    /// for one geometry while the grid tracks another.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), Error> {
        let rc = unsafe { ffi::ghostty_terminal_resize(self.raw, cols, rows, 0, 0) };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }
        Ok(())
    }

    /// Render the current screen state into the requested format.
    pub fn snapshot(&self, format: Format) -> Result<Vec<u8>, Error> {
        let extra_screen = ffi::ScreenExtra {
            size: std::mem::size_of::<ffi::ScreenExtra>(),
            cursor: true,
            style: true,
            hyperlink: true,
            protection: false,
            kitty_keyboard: false,
            charsets: false,
        };
        let options = ffi::FormatterOptions {
            size: std::mem::size_of::<ffi::FormatterOptions>(),
            emit: format.as_raw(),
            unwrap: false,
            trim: false,
            extra: ffi::TerminalExtra {
                size: std::mem::size_of::<ffi::TerminalExtra>(),
                palette: false,
                modes: false,
                scrolling_region: false,
                tabstops: false,
                // The directory the shell announced. A client that attaches
                // learns it from the snapshot rather than waiting for the next
                // prompt, which is what decides where a new tab opens.
                pwd: true,
                keyboard: false,
                screen: extra_screen,
            },
            selection: std::ptr::null(),
        };

        let mut formatter: ffi::Formatter = std::ptr::null_mut();
        let rc = unsafe {
            ffi::ghostty_formatter_terminal_new(
                std::ptr::null(),
                &mut formatter,
                self.raw,
                options,
            )
        };
        if rc != ffi::SUCCESS {
            return Err(Error(rc));
        }

        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let rc =
            unsafe { ffi::ghostty_formatter_format_alloc(formatter, std::ptr::null(), &mut ptr, &mut len) };
        if rc != ffi::SUCCESS {
            unsafe { ffi::ghostty_formatter_free(formatter) };
            return Err(Error(rc));
        }

        let out = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
        unsafe {
            ffi::ghostty_free(std::ptr::null(), ptr, len);
            ffi::ghostty_formatter_free(formatter);
        }
        Ok(out)
    }

    /// Convenience: the screen as plain text.
    pub fn text(&self) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(&self.snapshot(Format::Plain)?).into_owned())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        unsafe { ffi::ghostty_terminal_free(self.raw) };
    }
}

#[allow(unused)]
fn _assert_c_void_used(_: *const c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_replays_screen_state() {
        let mut t = Terminal::new(60, 8).expect("terminal");
        t.write(b"\x1b[2J\x1b[H");
        t.write(b"\x1b[1;32mproject\x1b[0m: ~/www/777leads\r\n");
        // Cursor pushed past the last row must clamp, keeping the column.
        t.write(b"\x1b[10;5H(clamped)");
        t.write(b"\x1b[3;1Hrow three");

        let text = t.text().expect("text");
        assert!(text.contains("project: ~/www/777leads"), "got: {text:?}");
        assert!(text.contains("row three"), "got: {text:?}");
        assert!(text.contains("    (clamped)"), "column not preserved: {text:?}");

        // The VT snapshot must carry styling the plain one drops.
        let vt = t.snapshot(Format::Vt).expect("vt");
        assert!(vt.windows(2).any(|w| w == b"\x1b["), "no escapes in VT snapshot");
        assert!(vt.len() > text.len(), "VT snapshot should be richer than plain");
    }

    #[test]
    fn split_writes_match_single_write() {
        let mut whole = Terminal::new(20, 3).unwrap();
        whole.write(b"\x1b[1mbold text\x1b[0m");

        // A PTY delivers bytes in arbitrary chunks, including mid-escape.
        let mut chunked = Terminal::new(20, 3).unwrap();
        for chunk in [&b"\x1b"[..], b"[1mbo", b"ld te", b"xt\x1b[0m"] {
            chunked.write(chunk);
        }

        assert_eq!(whole.text().unwrap(), chunked.text().unwrap());
    }
}

/// Layout guards.
///
/// These structs are passed to C **by value**, so a field added upstream
/// silently corrupts every call. libghostty-vt has no stable ABI yet, so the
/// sizes are pinned here on purpose: when you re-vendor the library and these
/// fail, re-read the headers before touching the numbers.
///
/// Re-derive with:
/// ```sh
/// printf '#include <ghostty/vt.h>\n#include <stdio.h>\nint main(void){printf("%zu %zu %zu\\n",\
/// sizeof(GhosttyFormatterScreenExtra),sizeof(GhosttyFormatterTerminalExtra),\
/// sizeof(GhosttyFormatterTerminalOptions));}' > /tmp/s.c && \
///   cc /tmp/s.c -Ivendor/libghostty-vt/include \
///      vendor/libghostty-vt/libghostty-vt.a -o /tmp/s && /tmp/s
/// ```
#[cfg(test)]
mod abi {
    use super::ffi;

    #[test]
    fn struct_sizes_match_c_headers() {
        assert_eq!(std::mem::size_of::<ffi::ScreenExtra>(), 16, "ScreenExtra");
        assert_eq!(std::mem::size_of::<ffi::TerminalExtra>(), 32, "TerminalExtra");
        assert_eq!(std::mem::size_of::<ffi::FormatterOptions>(), 56, "FormatterOptions");
    }
}

#[cfg(test)]
mod title {
    use super::*;

    #[test]
    fn reads_the_title_set_by_osc() {
        let mut t = Terminal::new(40, 5).unwrap();
        assert_eq!(t.title(), "", "a fresh terminal has no title");

        // OSC 2 is what shells and editors use to name the window.
        t.write(b"\x1b]2;building orion\x07");
        assert_eq!(t.title(), "building orion");

        // A later title replaces the earlier one.
        t.write(b"\x1b]2;running tests\x07");
        assert_eq!(t.title(), "running tests");
    }

    #[test]
    fn title_survives_being_split_across_writes() {
        let mut t = Terminal::new(40, 5).unwrap();
        for chunk in [&b"\x1b]2;par"[..], b"tial ti", b"tle\x07"] {
            t.write(chunk);
        }
        assert_eq!(t.title(), "partial title");
    }
}
