//! The font families installed on this machine, for the settings' list of
//! faces for the terminal (the macOS app's FontCatalog).
//!
//! A family is named by the font's own file — the name table of each font in
//! it — and not by what the registry calls the entry ("Cascadia Mono Regular
//! (TrueType)"), which says a style as well as a family. A font has two names
//! for its family: the typographic one ("Cascadia Mono") and the legacy one
//! that GDI groups by ("Cascadia Mono SemiLight"); which of them a page's
//! font matching finds depends on the system, so both are given, best first,
//! and the page keeps the first it can draw. Which files there are is the
//! platform's: on Windows what the registry lists plus the fonts folders, and
//! on a Mac (development) its font folders. The page keeps the families it
//! draws fixed-pitch; this only says what exists.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// The families installed here, once each: for each font the names its family
/// goes by, the typographic one first (see `names_in_table`).
pub fn installed_families() -> Vec<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut families = BTreeSet::new();
    for path in font_files() {
        // The same file can be listed twice: by the registry and by its folder.
        if !seen.insert(path.clone()) {
            continue;
        }
        families.extend(families_in(&path));
    }
    families.into_iter().collect()
}

/// More than this many files in the folders is not a folder of fonts.
const MOST_FILES: usize = 20_000;

fn is_font(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc"))
        .unwrap_or(false)
}

/// The font files in `dir`, and in the folders under it down to `depth` levels.
fn collect(dir: &Path, depth: u32, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if files.len() >= MOST_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                collect(&path, depth - 1, files);
            }
        } else if is_font(&path) {
            files.push(path);
        }
    }
}

#[cfg(not(windows))]
fn font_files() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let folders = [
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/Library/Fonts"),
        home.join("Library/Fonts"),
        PathBuf::from("/usr/share/fonts"),
        home.join(".local/share/fonts"),
        home.join(".fonts"),
    ];
    let mut files = Vec::new();
    for folder in folders {
        collect(&folder, 4, &mut files);
    }
    files
}

/// `%SystemRoot%\Fonts`, where the fonts for every user are.
#[cfg(windows)]
fn system_fonts() -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("Fonts")
}

#[cfg(windows)]
fn font_files() -> Vec<PathBuf> {
    // What the system has registered, wherever the file is; then the folders,
    // for a font registered in a way the registry's list does not show.
    let mut files = registered_files();
    collect(&system_fonts(), 1, &mut files);
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        collect(&PathBuf::from(local).join(r"Microsoft\Windows\Fonts"), 1, &mut files);
    }
    files
}

/// The files the registry lists as fonts, for every user (HKLM) and for this
/// one (HKCU): each value is "name (TrueType)" and holds the file, as a bare
/// name when it is in the system's fonts folder and as a path when it is not.
#[cfg(windows)]
fn registered_files() -> Vec<PathBuf> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ, RegCloseKey, RegEnumValueW,
        RegOpenKeyExW,
    };

    let subkey: Vec<u16> = std::ffi::OsStr::new(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let system = system_fonts();
    let mut files = Vec::new();
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `subkey` ends in a NUL, and `key` is where the call puts the handle it opens.
        if unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut key) } != ERROR_SUCCESS {
            continue;
        }
        // A value that does not fit the buffers is skipped, and no key has this many.
        for index in 0..20_000u32 {
            let mut name = [0u16; 512];
            let mut name_len = name.len() as u32;
            let mut kind = 0u32;
            let mut data = [0u8; 2048];
            let mut data_len = data.len() as u32;
            // SAFETY: the key is open, and the buffers are as long as the lengths given with them say.
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS || (kind != REG_SZ && kind != REG_EXPAND_SZ) {
                continue;
            }
            let units: Vec<u16> = data[..data_len as usize]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&unit| unit != 0)
                .collect();
            let path = PathBuf::from(String::from_utf16_lossy(&units));
            if !is_font(&path) {
                continue;
            }
            files.push(if path.is_absolute() { path } else { system.join(path) });
        }
        // SAFETY: opened above, and closed once.
        unsafe { RegCloseKey(key) };
    }
    files
}

// ---------------------------------------------------------------- the files

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// `length` bytes of the file from `offset`; nothing when it is not all there.
fn read_at(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// A name table over this is not one a font has.
const MOST_NAME_TABLE: usize = 1 << 20;

/// The family names of the fonts in a file: one set for a font, one for each
/// in a collection (.ttc). Nothing for what is not a font, or is damaged.
fn families_in(path: &Path) -> Vec<Vec<String>> {
    let Ok(mut file) = File::open(path) else { return Vec::new() };
    let Some(head) = read_at(&mut file, 0, 12) else { return Vec::new() };
    let offsets: Vec<u64> = if &head[..4] == b"ttcf" {
        // A collection: its header lists where each font's table directory is.
        let count = u32_at(&head, 8).unwrap_or(0).min(64) as usize;
        let Some(list) = read_at(&mut file, 12, count * 4) else { return Vec::new() };
        (0..count).filter_map(|i| u32_at(&list, i * 4)).map(u64::from).collect()
    } else {
        vec![0]
    };
    offsets.into_iter().filter_map(|offset| family_of_font(&mut file, offset)).collect()
}

/// The family names of the font whose table directory is at `offset`.
fn family_of_font(file: &mut File, offset: u64) -> Option<Vec<String>> {
    let head = read_at(file, offset, 12)?;
    // TrueType 1.0, "OTTO" (CFF), "true" and "typ1": the versions of an sfnt.
    if !matches!(&head[..4], [0, 1, 0, 0] | b"OTTO" | b"true" | b"typ1") {
        return None;
    }
    let tables = u16_at(&head, 4)? as usize;
    let directory = read_at(file, offset + 12, tables * 16)?;
    let mut name = None;
    for record in directory.chunks_exact(16) {
        if &record[..4] == b"name" {
            name = Some((u32_at(record, 8)? as u64, u32_at(record, 12)? as usize));
            break;
        }
    }
    let (start, length) = name?;
    if length > MOST_NAME_TABLE {
        return None;
    }
    let names = names_in_table(&read_at(file, start, length)?);
    (!names.is_empty()).then_some(names)
}

/// What a name record's bytes say, in the encodings fonts use for it: UTF-16
/// big-endian for Windows and Unicode records. A Macintosh record is in the
/// script of its font (Korean, Japanese, Roman...), which is not said here:
/// only what is plain ASCII is read, and the rest is for the font's other records.
fn decode(platform: u16, bytes: &[u8]) -> Option<String> {
    let text = match platform {
        0 | 3 => {
            let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect();
            String::from_utf16(&units).ok()?
        }
        1 if bytes.is_ascii() => bytes.iter().map(|&b| char::from(b)).collect(),
        _ => return None,
    };
    let text = text.trim().to_string();
    (!text.is_empty() && !text.starts_with('.') && !text.chars().any(char::is_control)).then_some(text)
}

/// The names a font's family goes by, in a `name` table: the typographic family
/// (ID 16, "Cascadia Mono") when the font has one, then the legacy family (ID 1,
/// "Cascadia Mono SemiLight"), once if they are the same. Each in English if the
/// font has it in English, which every font does that is meant to be found by
/// name; failing that in another Windows language, then Unicode, then Macintosh.
fn names_in_table(table: &[u8]) -> Vec<String> {
    let (Some(count), Some(strings)) = (u16_at(table, 2), u16_at(table, 4)) else { return Vec::new() };
    // The best record so far for each of the two IDs: typographic first, legacy second.
    let mut best: [Option<(u8, String)>; 2] = [None, None];
    for i in 0..count as usize {
        let at = 6 + i * 12;
        let (Some(platform), Some(language), Some(id), Some(length), Some(offset)) =
            (u16_at(table, at), u16_at(table, at + 4), u16_at(table, at + 6), u16_at(table, at + 8), u16_at(table, at + 10))
        else {
            break;
        };
        let slot = match id {
            16 => 0,
            1 => 1,
            _ => continue,
        };
        let from = strings as usize + offset as usize;
        let Some(text) = table.get(from..from + length as usize).and_then(|bytes| decode(platform, bytes)) else {
            continue;
        };
        let rank = match (platform, language) {
            (3, 0x0409) => 0,
            (3, _) => 1,
            (0, _) => 2,
            _ => 3,
        };
        if best[slot].as_ref().map(|(held, _)| rank < *held).unwrap_or(true) {
            best[slot] = Some((rank, text));
        }
    }
    let mut names = Vec::new();
    for (_, text) in best.into_iter().flatten() {
        if !names.contains(&text) {
            names.push(text);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `name` table with these records: (platform, language, ID, the string's bytes).
    fn table_of(records: &[(u16, u16, u16, Vec<u8>)]) -> Vec<u8> {
        let strings = 6 + records.len() * 12;
        let mut table = vec![0, 0];
        table.extend((records.len() as u16).to_be_bytes());
        table.extend((strings as u16).to_be_bytes());
        let mut at = 0u16;
        for (platform, language, id, bytes) in records {
            for field in [*platform, if *platform == 1 { 0 } else { 1 }, *language, *id, bytes.len() as u16, at] {
                table.extend(field.to_be_bytes());
            }
            at += bytes.len() as u16;
        }
        for (_, _, _, bytes) in records {
            table.extend(bytes);
        }
        table
    }

    /// A `name` table with these records: (platform, language, ID, text).
    fn name_table(records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let encoded: Vec<(u16, u16, u16, Vec<u8>)> = records
            .iter()
            .map(|(platform, language, id, text)| {
                let bytes = match platform {
                    1 => text.bytes().collect(),
                    _ => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                };
                (*platform, *language, *id, bytes)
            })
            .collect();
        table_of(&encoded)
    }

    /// A font that is only a header and a `name` table, placed at `base` in a file whose first `base` bytes are the caller's.
    fn font_at(base: u32, table: &[u8]) -> Vec<u8> {
        let mut font = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        font.extend(b"name");
        font.extend([0u8; 4]);
        font.extend((base + 12 + 16).to_be_bytes());
        font.extend((table.len() as u32).to_be_bytes());
        font.extend(table);
        font
    }

    fn scratch(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-app-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn names(table: &[u8]) -> Vec<String> {
        names_in_table(table)
    }

    #[test]
    fn a_font_goes_by_its_typographic_family_and_its_legacy_one() {
        let table = name_table(&[
            (3, 0x409, 1, "Foo Mono SemiLight"),
            (3, 0x409, 2, "Regular"),
            (3, 0x409, 4, "Foo Mono SemiLight"),
            (3, 0x409, 16, "Foo Mono"),
        ]);
        assert_eq!(names(&table), vec!["Foo Mono".to_string(), "Foo Mono SemiLight".to_string()]);
    }

    #[test]
    fn a_font_without_a_typographic_family_goes_by_the_legacy_one_alone() {
        assert_eq!(names(&name_table(&[(3, 0x409, 1, "Bar Code"), (3, 0x409, 2, "Bold")])), vec!["Bar Code".to_string()]);
    }

    #[test]
    fn the_same_name_twice_is_given_once() {
        assert_eq!(names(&name_table(&[(3, 0x409, 1, "Same"), (3, 0x409, 16, "Same")])), vec!["Same".to_string()]);
    }

    #[test]
    fn english_comes_before_another_language_and_windows_before_the_macintosh() {
        let table = name_table(&[(3, 0x411, 16, "ファミリー"), (3, 0x409, 16, "Family"), (1, 0, 16, "Mac Family")]);
        assert_eq!(names(&table), vec!["Family".to_string()]);
        let table = name_table(&[(1, 0, 1, "Mac Family"), (3, 0x411, 1, "ファミリー")]);
        assert_eq!(names(&table), vec!["ファミリー".to_string()]);
        assert_eq!(names(&name_table(&[(1, 0, 1, "Only Mac")])), vec!["Only Mac".to_string()]);
    }

    #[test]
    fn a_macintosh_record_in_another_script_is_not_read_as_roman() {
        // The bytes of a Korean name in the Macintosh Korean encoding: not letters in Roman, and not guessed at.
        let korean = vec![0xBB, 0xEA, 0xB5, 0xB9, 0x20, 0x4E, 0x65, 0x6F];
        let face: Vec<u8> = "Hangul Face".encode_utf16().flat_map(u16::to_be_bytes).collect();
        assert_eq!(names(&table_of(&[(1, 0, 1, korean.clone()), (3, 0x409, 16, face)])), vec!["Hangul Face".to_string()]);
        assert!(names(&table_of(&[(1, 0, 1, korean)])).is_empty());
    }

    #[test]
    fn a_name_that_is_not_one_is_passed_over() {
        let table = name_table(&[(3, 0x409, 16, ".Hidden"), (3, 0x409, 1, "Shown")]);
        assert_eq!(names(&table), vec!["Shown".to_string()]);
        assert!(names(&name_table(&[(3, 0x409, 4, "Only a full name")])).is_empty());
        assert!(names(&[]).is_empty());
        assert!(names(&[0, 0, 0, 9, 0, 6]).is_empty());
    }

    #[test]
    fn a_file_with_a_font_gives_its_names() {
        let path = scratch("one.ttf", &font_at(0, &name_table(&[(3, 0x409, 1, "Solo Mono")])));
        assert_eq!(families_in(&path), vec![vec!["Solo Mono".to_string()]]);
    }

    #[test]
    fn a_collection_gives_the_names_of_each_font_in_it() {
        let first = name_table(&[(3, 0x409, 1, "Duo Regular Face"), (3, 0x409, 16, "Duo")]);
        let second = name_table(&[(3, 0x409, 1, "Duo Mono")]);
        // "ttcf", version 1, two fonts; then the fonts, each with its table directory where the header says.
        let header_len = 12 + 2 * 4;
        let first_at = header_len as u32;
        let second_at = first_at + (12 + 16 + first.len()) as u32;
        let mut file = b"ttcf".to_vec();
        file.extend([0, 1, 0, 0, 0, 0, 0, 2]);
        file.extend(first_at.to_be_bytes());
        file.extend(second_at.to_be_bytes());
        file.extend(font_at(first_at, &first));
        file.extend(font_at(second_at, &second));
        let path = scratch("two.ttc", &file);
        assert_eq!(
            families_in(&path),
            vec![vec!["Duo".to_string(), "Duo Regular Face".to_string()], vec!["Duo Mono".to_string()]]
        );
    }

    #[test]
    fn what_is_not_a_font_gives_nothing() {
        assert!(families_in(&scratch("text.ttf", b"this is not a font at all")).is_empty());
        assert!(families_in(&scratch("short.ttf", b"OT")).is_empty());
        assert!(families_in(&scratch("empty.otf", b"")).is_empty());
        assert!(families_in(Path::new("/no/such/file.ttf")).is_empty());
        // The right version, a directory that says more tables than the file has.
        assert!(families_in(&scratch("cut.ttf", &[0, 1, 0, 0, 0, 200, 0, 0, 0, 0, 0, 0])).is_empty());
        // A font with no name to be found by.
        assert!(families_in(&scratch("nameless.ttf", &font_at(0, &name_table(&[(3, 0x409, 4, "Full name only")])))).is_empty());
    }

    #[test]
    fn only_fonts_are_listed_from_a_folder() {
        let dir = std::env::temp_dir().join(format!("keep-app-fonts-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub/deeper/deepest")).unwrap();
        for name in ["a.ttf", "B.OTF", "c.txt", "sub/d.ttc", "sub/deeper/e.otc", "sub/deeper/deepest/f.ttf"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let mut files = Vec::new();
        collect(&dir, 2, &mut files);
        let mut names: Vec<String> = files.iter().map(|p| p.strip_prefix(&dir).unwrap().display().to_string().replace('\\', "/")).collect();
        names.sort();
        assert_eq!(names, vec!["B.OTF", "a.ttf", "sub/d.ttc", "sub/deeper/e.otc"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A Mac has Menlo whatever else it has: development reads it as the Windows build reads Consolas.
    #[cfg(target_os = "macos")]
    #[test]
    fn this_machines_fonts_are_listed() {
        let families = installed_families();
        assert!(families.iter().any(|names| names.iter().any(|n| n == "Menlo")), "{} families, no Menlo", families.len());
        assert!(families.windows(2).all(|pair| pair[0] < pair[1]), "sorted, once each");
    }
}
