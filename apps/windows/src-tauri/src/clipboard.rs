//! The files on the clipboard, for pasting them into a terminal as paths.
//!
//! The Explorer's Ctrl+C puts the files on the clipboard as a `CF_HDROP`: a
//! global memory block that starts with a `DROPFILES` header and goes on with
//! the paths, each ended by a NUL and the list by one more. The page's own
//! paste event hands over the files' contents and not where they are, so the
//! paths are read here. Anywhere but Windows there is nothing to read (the
//! macOS app reads text only, too).

/// `sizeof(DROPFILES)`: the offset of the list (4), the drop point (8), and two flags (4 each).
const HEADER: usize = 20;

/// The paths of a `DROPFILES` block, as the bytes of the whole block lay them out. Empty when the block is
/// not one this can read: too short, a list that begins inside the header or past the end, or in the ANSI
/// code page (Windows 9x; every shell since writes the wide form).
fn parse_drop_files(block: &[u8]) -> Vec<String> {
    if block.len() < HEADER {
        return Vec::new();
    }
    let word = |at: usize| u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
    let start = word(0) as usize;
    let wide = word(16) != 0;
    if !wide || start < HEADER || start > block.len() {
        return Vec::new();
    }
    let units: Vec<u16> = block[start..].chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
    wide_list(&units)
}

/// The strings of a list of NUL-ended UTF-16 strings, which an empty one ends (or the end of the data).
fn wide_list(units: &[u16]) -> Vec<String> {
    units
        .split(|unit| *unit == 0)
        .take_while(|path| !path.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

#[cfg(windows)]
mod system {
    use super::parse_drop_files;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    /// `CF_HDROP` (winuser.h), written out so that a feature of the crate for one number is not needed.
    const CF_HDROP: u32 = 15;
    /// Another process may have the clipboard open (a clipboard manager looks at every copy): a few short tries.
    const TRIES: u32 = 10;
    const WAIT: Duration = Duration::from_millis(15);

    /// Closes the clipboard when it goes out of scope, whichever way the reading ends.
    struct Open;

    impl Open {
        fn new() -> Option<Open> {
            for attempt in 0..TRIES {
                // SAFETY: no window owns the opening, which is allowed for reading; a success is closed by `Drop`.
                if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                    return Some(Open);
                }
                if attempt + 1 < TRIES {
                    std::thread::sleep(WAIT);
                }
            }
            None
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: the clipboard is open, by `new`.
            unsafe { CloseClipboard() };
        }
    }

    pub fn files() -> Vec<String> {
        // SAFETY: asks about a format and nothing else.
        if unsafe { IsClipboardFormatAvailable(CF_HDROP) } == 0 {
            return Vec::new();
        }
        let Some(_open) = Open::new() else { return Vec::new() };
        // SAFETY: the clipboard is open; the handle it gives is the clipboard's, and is not freed here.
        let handle: HANDLE = unsafe { GetClipboardData(CF_HDROP) };
        if handle.is_null() {
            return Vec::new();
        }
        // SAFETY: a CF_HDROP handle is a global memory block; it is locked while it is read (copied, in
        // fact: the slice does not outlive the lock) and unlocked once.
        unsafe {
            let size = GlobalSize(handle);
            let data = GlobalLock(handle) as *const u8;
            if data.is_null() {
                return Vec::new();
            }
            let paths = parse_drop_files(std::slice::from_raw_parts(data, size));
            GlobalUnlock(handle);
            paths
        }
    }
}

#[cfg(not(windows))]
mod system {
    pub fn files() -> Vec<String> {
        Vec::new()
    }
}

/// The paths of the files on the clipboard; empty when it holds anything else.
pub fn files() -> Vec<String> {
    system::files()
}

/// For the page's paste: the files the Explorer copied, as their paths.
#[tauri::command]
pub async fn clipboard_files() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(files).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block as the Explorer builds it: the header, then the wide paths and the closing NUL.
    fn block(start: u32, wide: bool, paths: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&[0; 12]);
        bytes.extend_from_slice(&u32::from(wide).to_le_bytes());
        for path in paths {
            for unit in path.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes.extend_from_slice(&0u16.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }

    #[test]
    fn reads_every_path_of_the_list() {
        let bytes = block(20, true, &[r"C:\Users\ana\a b.txt", r"\\srv\share\ç.png"]);
        assert_eq!(parse_drop_files(&bytes), vec![r"C:\Users\ana\a b.txt".to_string(), r"\\srv\share\ç.png".to_string()]);
    }

    #[test]
    fn reads_characters_outside_the_basic_plane() {
        let bytes = block(20, true, &["C:\\x\\\u{1F4C4}.txt"]);
        assert_eq!(parse_drop_files(&bytes), vec!["C:\\x\\\u{1F4C4}.txt".to_string()]);
    }

    #[test]
    fn follows_the_offset_the_header_gives() {
        let mut bytes = block(24, true, &[r"C:\a"]);
        bytes.splice(20..20, [0xAA; 4]);
        assert_eq!(parse_drop_files(&bytes), vec![r"C:\a".to_string()]);
    }

    #[test]
    fn an_empty_list_is_no_files() {
        assert!(parse_drop_files(&block(20, true, &[])).is_empty());
    }

    #[test]
    fn a_list_without_its_closing_nul_still_gives_its_last_path() {
        let mut bytes = block(20, true, &[r"C:\a", r"C:\b"]);
        bytes.truncate(bytes.len() - 2);
        assert_eq!(parse_drop_files(&bytes), vec![r"C:\a".to_string(), r"C:\b".to_string()]);
    }

    #[test]
    fn a_block_that_cannot_be_read_is_no_files() {
        assert!(parse_drop_files(&[]).is_empty());
        assert!(parse_drop_files(&[0; 19]).is_empty());
        // The ANSI form.
        assert!(parse_drop_files(&block(20, false, &[r"C:\a"])).is_empty());
        // A list that begins inside the header, and one past the end.
        assert!(parse_drop_files(&block(8, true, &[r"C:\a"])).is_empty());
        assert!(parse_drop_files(&block(4096, true, &[r"C:\a"])).is_empty());
    }

    #[test]
    fn an_odd_trailing_byte_is_ignored() {
        let mut bytes = block(20, true, &[r"C:\a"]);
        bytes.push(0x41);
        assert_eq!(parse_drop_files(&bytes), vec![r"C:\a".to_string()]);
    }

    #[test]
    fn not_windows_has_no_files() {
        #[cfg(not(windows))]
        assert!(files().is_empty());
    }
}
