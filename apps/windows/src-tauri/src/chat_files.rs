//! The files of a Claude Code conversation, for the panel beside it: the ones
//! the person sent (a picture pasted, a file dropped or named in a message, an
//! upload) and the ones the conversation made for them: handed over with
//! SendUserFile, or named in an answer (by path or by name) and written or
//! created since it began. What it made only on the way (the HTML a PDF was
//! printed from, a script, a scratch file) and never showed is left out.
//! Newest first. Beside them, the person's Downloads folder.
//!
//! A port of `ChatFilesReader` in the macOS app (Model/ChatFiles.swift), with
//! its rules and its cases (tools/chat-files-test/main.swift), widened for
//! Windows: drive letters, backslashes, `/c/…` from Git Bash (where Claude
//! Code's Bash tool runs), names compared without case, and no question to a
//! server on the network (asking Windows whether `\\host\share\x` is there
//! signs in to that host with the person's credentials).
//!
//! The transcript is read as it grows: each read takes only the lines added
//! since the last, so a transcript of a hundred megabytes is read in full once.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

/// A file as its row shows it: the name without its extension, the extension as its type, when it last
/// changed (milliseconds since 1970), how big it is.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Item {
    pub path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub modified: f64,
    pub size: u64,
    /// A folder (Downloads holds them too): opened in the file manager, never shown as a file to run.
    pub dir: bool,
}

impl Item {
    fn of(path: &Path) -> Item {
        let meta = std::fs::metadata(path).ok();
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = meta.as_ref().is_some_and(|m| m.is_dir());
        let (name, kind) = if dir { (file, String::new()) } else { split_name(&file) };
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0);
        let size = meta.filter(|m| !m.is_dir()).map(|m| m.len()).unwrap_or(0);
        Item { path: path.to_string_lossy().into_owned(), name, kind, modified, size, dir }
    }
}

/// `Relatório.final.pdf` is `Relatório.final` of type `pdf`; `.gitignore` and `LEIAME` have no type.
fn split_name(file: &str) -> (String, String) {
    match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem.to_string(), ext.to_lowercase()),
        _ => (file.to_string(), String::new()),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Files {
    pub sent: Vec<Item>,
    pub made: Vec<Item>,
    pub downloads: Vec<Item>,
    /// The folder is there but could not be read: not the same as empty.
    pub downloads_denied: bool,
}

/// Paths as this system writes them: Windows' (case does not tell two apart) or POSIX's.
const WINDOWS: bool = cfg!(windows);

/// What a path is compared by: on Windows `C:\A.pdf` and `c:\a.PDF` are one file.
fn key(path: &str, windows: bool) -> String {
    if windows {
        path.to_lowercase()
    } else {
        path.to_string()
    }
}

struct Made {
    path: String,
    /// Written by the conversation, whatever the file's date (otherwise only if created after it began).
    own: bool,
    /// Given to the person with SendUserFile.
    handed: bool,
}

/// One conversation's transcript, read as it grows. One read at a time (the caller holds it under a lock).
pub struct Reader {
    transcript: String,
    session: String,
    /// Where a pasted picture is written out, so it can be opened: the transcript holds it only as base64.
    image_folder: PathBuf,
    downloads_folder: Option<PathBuf>,
    home: String,
    windows: bool,
    offset: u64,
    start: Option<SystemTime>,
    sent: Vec<String>,
    sent_seen: HashSet<String>,
    /// In the order the conversation came to them.
    made: Vec<Made>,
    made_index: HashMap<String, usize>,
    /// What its answers name, by path and by bare name: a file it made and named there is a result, not a step.
    answered: HashSet<String>,
    answered_names: HashSet<String>,
    /// The Downloads listing, read again only when the folder's date moves (a file added, renamed or taken out).
    /// shortcut: a file still downloading keeps the size it was listed with until something else changes the folder.
    downloads_cache: Option<(SystemTime, Vec<Item>)>,
}

impl Reader {
    pub fn new(transcript: &str, session: &str, image_folder: PathBuf, downloads_folder: Option<PathBuf>, home: &str) -> Reader {
        Reader::with_flavor(transcript, session, image_folder, downloads_folder, home, WINDOWS)
    }

    fn with_flavor(
        transcript: &str,
        session: &str,
        image_folder: PathBuf,
        downloads_folder: Option<PathBuf>,
        home: &str,
        windows: bool,
    ) -> Reader {
        Reader {
            transcript: transcript.to_string(),
            session: session.to_string(),
            image_folder,
            downloads_folder,
            home: home.to_string(),
            windows,
            offset: 0,
            start: None,
            sent: Vec::new(),
            sent_seen: HashSet::new(),
            made: Vec::new(),
            made_index: HashMap::new(),
            answered: HashSet::new(),
            answered_names: HashSet::new(),
            downloads_cache: None,
        }
    }

    pub fn read(&mut self) -> Files {
        // Here and not at the start: a big cache takes a while to go through, and this runs off the page's thread.
        prune_once(&self.image_folder, SystemTime::now());
        self.read_new_lines();
        let w = self.windows;
        let uploads = Path::new(&self.home).join(".claude").join("uploads").join(&self.session);
        let mut uploaded: Vec<String> = std::fs::read_dir(&uploads)
            .map(|dir| dir.flatten().map(|e| e.path().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        uploaded.sort();
        let mut seen = HashSet::new();
        let sent_now: Vec<String> = self
            .sent
            .iter()
            .chain(uploaded.iter())
            .filter(|p| seen.insert(key(p, w)))
            .filter(|p| is_file(p))
            .cloned()
            .collect();
        let sent_keys: HashSet<String> = sent_now.iter().map(|p| key(p, w)).collect();
        let begun = self.start.unwrap_or(UNIX_EPOCH);
        // Its own: handed over, written by it, or created since it began (a file it only changed, a config, a
        // test, is not one it made).
        let candidates: Vec<&Made> = self
            .made
            .iter()
            .filter(|entry| {
                if !is_file(&entry.path) {
                    return false;
                }
                if entry.handed || entry.own {
                    return true;
                }
                if ignored(&entry.path, &self.home, w) {
                    return false;
                }
                std::fs::metadata(&entry.path).and_then(|m| m.created()).is_ok_and(|created| created >= begun)
            })
            .collect();
        // A bare name in an answer stands for a file only when one of its own goes by that name: `index.html`
        // named once does not bring in every other index.html it made on the way.
        let mut same_name: HashMap<String, usize> = HashMap::new();
        for entry in &candidates {
            *same_name.entry(key(file_name(&entry.path), w)).or_default() += 1;
        }
        let transcript = key(&self.transcript, w);
        let made_now: Vec<Item> = candidates
            .iter()
            .filter(|entry| {
                let name = key(file_name(&entry.path), w);
                entry.handed
                    || self.answered.contains(&key(&entry.path, w))
                    || (self.answered_names.contains(&name) && same_name.get(&name) == Some(&1))
            })
            .map(|entry| entry.path.as_str())
            .filter(|p| !sent_keys.contains(&key(p, w)) && key(p, w) != transcript)
            .rev()
            .map(|p| Item::of(Path::new(p)))
            .collect();
        let downloads = self.downloads_now();
        let denied = downloads.is_none() && self.downloads_folder.as_ref().is_some_and(|f| f.exists());
        Files {
            sent: sent_now.iter().rev().map(|p| Item::of(Path::new(p))).collect(),
            made: made_now,
            downloads: downloads.unwrap_or_default(),
            downloads_denied: denied,
        }
    }

    fn downloads_now(&mut self) -> Option<Vec<Item>> {
        let folder = self.downloads_folder.clone()?;
        let changed = std::fs::metadata(&folder).and_then(|m| m.modified()).ok();
        if let (Some(changed), Some((at, items))) = (changed, &self.downloads_cache) {
            if *at == changed {
                return Some(items.clone());
            }
        }
        let items = downloads_in(&folder);
        // A refusal is not kept: access given later does not move the folder's date.
        if let (Some(changed), Some(items)) = (changed, &items) {
            self.downloads_cache = Some((changed, items.clone()));
        }
        items
    }

    // ---------------------------------------------------------------- the transcript

    fn read_new_lines(&mut self) {
        let Ok(mut file) = std::fs::File::open(&self.transcript) else { return };
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        if size < self.offset {
            // Written over: read again from the top.
            self.offset = 0;
            self.start = None;
            self.sent.clear();
            self.sent_seen.clear();
            self.made.clear();
            self.made_index.clear();
            self.answered.clear();
            self.answered_names.clear();
        }
        if size <= self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut data = Vec::new();
        if file.read_to_end(&mut data).is_err() {
            return;
        }
        // A line still being written waits for the next read.
        let Some(last) = data.iter().rposition(|&b| b == b'\n') else { return };
        let complete = &data[..=last];
        self.offset += complete.len() as u64;
        for line in complete.split(|&b| b == b'\n') {
            self.take(line);
        }
    }

    fn take(&mut self, line: &[u8]) {
        let marked = contains(line, br#""type":"user""#) || contains(line, br#""type":"assistant""#);
        if !marked && self.start.is_some() {
            return;
        }
        let Ok(json) = serde_json::from_slice::<Value>(line) else { return };
        if self.start.is_none() {
            if let Some(stamp) = json.get("timestamp").and_then(Value::as_str) {
                self.start = parse_time(stamp);
            }
        }
        if !marked || json.get("isSidechain") == Some(&Value::Bool(true)) || json.get("isMeta") == Some(&Value::Bool(true)) {
            return;
        }
        let Some(message) = json.get("message") else { return };
        match json.get("type").and_then(Value::as_str) {
            Some("user") => self.take_user(message, &json),
            Some("assistant") => self.take_assistant(message),
            _ => {}
        }
    }

    fn take_user(&mut self, message: &Value, json: &Value) {
        // The summary of a compaction, a message from another tab or a hook: not the person's.
        if json.get("isCompactSummary") == Some(&Value::Bool(true)) {
            return;
        }
        if let Some(kind) = json.get("origin").and_then(|o| o.get("kind")) {
            if kind.as_str() != Some("human") {
                return;
            }
        }
        let content = message.get("content");
        if let Some(text) = content.and_then(Value::as_str) {
            self.take_typed(text);
            return;
        }
        let Some(blocks) = content.and_then(Value::as_array) else { return };
        let mut ids: Vec<i64> = json
            .get("imagePasteIds")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();
        ids.reverse();
        for (index, block) in blocks.iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => self.take_typed(block.get("text").and_then(Value::as_str).unwrap_or("")),
                Some("image") => {
                    let id = ids.pop();
                    let uuid = json.get("uuid").and_then(Value::as_str).unwrap_or("imagem");
                    // Só letras, dígitos, "-" e "_": o uuid vem do transcrito e vira nome de arquivo.
                    let limpo: String =
                        uuid.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(8).collect();
                    let fallback = format!("{}-{index}", if limpo.is_empty() { "imagem" } else { &limpo });
                    if let Some(path) = self.write_image(block, id, &fallback) {
                        self.add_sent(path);
                    }
                }
                _ => {}
            }
        }
    }

    /// What the person typed or dropped, not what came in their name (a task's notice, a command's output: those
    /// open with a tag).
    fn take_typed(&mut self, text: &str) {
        if text.trim_start_matches([' ', '\t']).starts_with('<') {
            return;
        }
        for path in paths_in(text, &self.home, self.windows) {
            self.add_sent(path);
        }
    }

    fn take_assistant(&mut self, message: &Value) {
        let Some(blocks) = message.get("content").and_then(Value::as_array) else { return };
        let w = self.windows;
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                    for path in paths_in(text, &self.home, w) {
                        self.answered.insert(key(&path, w));
                        self.add_made(path, false, false);
                    }
                    for name in file_names(text, w) {
                        self.answered_names.insert(key(&name, w));
                    }
                }
                Some("tool_use") => {
                    let input = block.get("input").unwrap_or(&Value::Null);
                    let text = |field: &str| input.get(field).and_then(Value::as_str);
                    let path = |field: &str| text(field).and_then(|p| path_of(p, &self.home, w));
                    match block.get("name").and_then(Value::as_str) {
                        Some("Write") => {
                            if let Some(p) = path("file_path") {
                                self.add_made(p, true, false);
                            }
                        }
                        Some("NotebookEdit") => {
                            if let Some(p) = path("notebook_path") {
                                self.add_made(p, true, false);
                            }
                        }
                        Some("SendUserFile") => {
                            let files: Vec<String> = input
                                .get("files")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(Value::as_str).filter_map(|p| path_of(p, &self.home, w)).collect())
                                .unwrap_or_default();
                            for p in files {
                                self.add_made(p, true, true);
                            }
                        }
                        Some("Bash") | Some("PowerShell") => {
                            for p in paths_in(text("command").unwrap_or(""), &self.home, w) {
                                self.add_made(p, false, false);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn add_made(&mut self, path: String, own: bool, handed: bool) {
        let k = key(&path, self.windows);
        if let Some(&index) = self.made_index.get(&k) {
            let entry = &mut self.made[index];
            entry.own |= own;
            entry.handed |= handed;
            return;
        }
        self.made_index.insert(k, self.made.len());
        self.made.push(Made { path, own, handed });
    }

    fn add_sent(&mut self, path: String) {
        if self.sent_seen.insert(key(&path, self.windows)) {
            self.sent.push(path);
        }
    }

    fn write_image(&self, block: &Value, id: Option<i64>, fallback: &str) -> Option<String> {
        use base64::Engine;
        let source = block.get("source")?;
        if source.get("type").and_then(Value::as_str) != Some("base64") {
            return None;
        }
        let data = source.get("data").and_then(Value::as_str)?;
        let kind = source.get("media_type").and_then(Value::as_str).and_then(|t| t.rsplit('/').next()).unwrap_or("png");
        // Only a picture's own extension: the media type is the transcript's word, and becomes part of a file name.
        let ext = match kind {
            "jpeg" | "jpg" => "jpg",
            "png" | "gif" | "webp" | "bmp" | "tiff" | "heic" => kind,
            _ => "png",
        };
        let folder = self.image_folder.join(&self.session);
        // Each picture its own name: a message without paste ids may hold several.
        let name = format!("Imagem {}.{ext}", id.map(|i| i.to_string()).unwrap_or_else(|| fallback.to_string()));
        let path = folder.join(&name);
        if path.exists() {
            return Some(path.to_string_lossy().into_owned());
        }
        let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?;
        // A picture may hold anything on the person's screen: theirs alone to read (on Windows the folder under
        // LOCALAPPDATA already is).
        std::fs::create_dir_all(&folder).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700));
        }
        let partial = folder.join(format!(".{name}.parcial"));
        let written = (|| -> std::io::Result<()> {
            #[cfg(unix)]
            let mut file = {
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&partial)?
            };
            #[cfg(not(unix))]
            let mut file = std::fs::File::create(&partial)?;
            std::io::Write::write_all(&mut file, &bytes)?;
            drop(file);
            std::fs::rename(&partial, &path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&partial);
            return None;
        }
        Some(path.to_string_lossy().into_owned())
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A file that is there, asked without reaching a server on the network or a device (`NUL`, `COM1`).
fn is_file(path: &str) -> bool {
    !crate::links::is_network_path(path)
        && !(cfg!(windows) && crate::links::is_device_name(path))
        && std::fs::metadata(path).is_ok_and(|m| m.is_file())
}

/// What a folder holds that one would drag: its files and folders, hidden ones left out. None when it cannot be read.
pub fn downloads_in(folder: &Path) -> Option<Vec<Item>> {
    let entries = std::fs::read_dir(folder).ok()?;
    Some(
        entries
            .flatten()
            .filter(|entry| !hidden(entry))
            .map(|entry| Item::of(&entry.path()))
            .collect(),
    )
}

fn hidden(entry: &std::fs::DirEntry) -> bool {
    if entry.file_name().to_string_lossy().starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        if entry.metadata().is_ok_and(|m| m.file_attributes() & (HIDDEN | SYSTEM) != 0) {
            return true;
        }
    }
    false
}

/// Named often, never anybody's work: the system's, Claude Code's own records and a background task's output.
fn ignored(path: &str, home: &str, windows: bool) -> bool {
    let p = key(path, windows);
    let claude = key(&format!("{}{}.claude{}", home.trim_end_matches(['/', '\\']), sep(windows), sep(windows)), windows);
    let roots: &[&str] = if windows {
        &["c:\\windows\\", "c:\\program files\\", "c:\\program files (x86)\\", "c:\\programdata\\"]
    } else {
        &["/usr/", "/bin/", "/sbin/", "/opt/", "/System/", "/Library/", "/Applications/", "/dev/", "/etc/", "/private/etc/"]
    };
    roots.iter().any(|r| p.starts_with(r))
        || (!home.is_empty() && p.starts_with(&claude))
        || (p.ends_with(".output") && (p.contains("/tasks/") || p.contains("\\tasks\\")))
        || ["-wal", "-shm", "-journal"].iter().any(|s| p.ends_with(s))
}

fn sep(windows: bool) -> char {
    if windows {
        '\\'
    } else {
        '/'
    }
}

// ---------------------------------------------------------------- paths in text

/// Quotes, brackets and the sentence's punctuation around a path or a name.
const WRAPPING: &[char] = &['`', '\'', '"', '(', ')', '<', '>', '[', ']', '{', '}', '.', ',', ';', ':', '!', '?', '*'];

fn trim_wrapping(word: &str) -> &str {
    word.trim_matches(WRAPPING)
}

/// The absolute or `~/` paths a text holds: as words (a space escaped by a backslash kept, as a dropped file arrives
/// on a Mac), between quotes, and with the sentence's punctuation, a `file://` or a `:line` taken off. Each once,
/// as the file system would name it.
pub fn paths_in(text: &str, home: &str, windows: bool) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        // A backslash is a folder's end on Windows; it escapes only the blank after it.
        if c == '\\' && chars.peek().is_some_and(|n| n.is_whitespace()) {
            current.push(chars.next().unwrap_or(' '));
        } else if c.is_whitespace() {
            words.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    words.push(current);
    // A word that opens with a quote is read whole between the quotes, below.
    words.retain(|w| !w.starts_with(['"', '\'', '`']));
    for quote in ['"', '\'', '`'] {
        words.extend(text.split(quote).skip(1).step_by(2).map(str::to_string));
    }
    let mut found: Vec<String> = Vec::new();
    let mut keys = HashSet::new();
    for word in words {
        if let Some(path) = path_of(&word, home, windows) {
            if keys.insert(key(&path, windows)) {
                found.push(path);
            }
        }
    }
    found
}

/// The path a word stands for, or None when it is no absolute path.
fn path_of(word: &str, home: &str, windows: bool) -> Option<String> {
    let mut path = trim_wrapping(word).to_string();
    if let Some(rest) = path.strip_prefix("file://") {
        let local = percent_decoded(rest.strip_prefix("localhost").unwrap_or(rest));
        // `file:///C:/x` is `C:/x`.
        let bytes = local.as_bytes();
        path = if windows && bytes.len() > 2 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
            local[1..].to_string()
        } else {
            local
        };
    }
    // `:12` or `:12:5` after the path, as tools write where in a file.
    for _ in 0..2 {
        if let Some((head, tail)) = path.rsplit_once(':') {
            if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) && head.len() > 2 {
                path = head.to_string();
            }
        }
    }
    if path.chars().count() < 2 {
        return None;
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| if windows { path.strip_prefix("~\\") } else { None }) {
        if home.is_empty() {
            return None;
        }
        return Some(normalize(&format!("{}{}{rest}", home.trim_end_matches(['/', '\\']), sep(windows)), windows));
    }
    if windows {
        let b = path.as_bytes();
        let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
        let unc = path.starts_with("\\\\") || path.starts_with("//");
        if drive || unc {
            return Some(normalize(&path, true));
        }
        // Git Bash's `/c/Users/x` and WSL's `/mnt/c/Users/x`: the drive they stand for.
        return git_bash_drive(&path).map(|p| normalize(&p, true));
    }
    path.starts_with('/').then(|| normalize(&path, false))
}

/// `/c/Users/x` and `/mnt/c/Users/x` as `C:\Users\x`; None for any other rooted POSIX path, which names nothing here.
fn git_bash_drive(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/mnt/").or_else(|| path.strip_prefix('/'))?;
    let mut parts = rest.splitn(2, '/');
    let drive = parts.next()?;
    let tail = parts.next()?;
    if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() || tail.is_empty() {
        return None;
    }
    Some(format!("{}:\\{}", drive.to_ascii_uppercase(), tail.replace('/', "\\")))
}

/// `.` and `..` folded, one kind of separator, no doubled ones; a drive letter in capitals.
fn normalize(path: &str, windows: bool) -> String {
    let fold = |rest: &str, s: char| -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for part in rest.split(s) {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(part.to_string()),
            }
        }
        parts
    };
    if !windows {
        return format!("/{}", fold(path, '/').join("/"));
    }
    let text = path.replace('/', "\\");
    let b = text.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return format!("{}:\\{}", text[..1].to_ascii_uppercase(), fold(&text[2..], '\\').join("\\"));
    }
    if let Some(rest) = text.strip_prefix("\\\\") {
        let parts: Vec<&str> = rest.split('\\').filter(|p| !p.is_empty()).collect();
        let root = format!("\\\\{}", parts.iter().take(2).copied().collect::<Vec<_>>().join("\\"));
        let inner = fold(&parts.iter().skip(2).copied().collect::<Vec<_>>().join("\\"), '\\');
        return if inner.is_empty() { root } else { format!("{root}\\{}", inner.join("\\")) };
    }
    fold(&text, '\\').join("\\")
}

fn percent_decoded(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(high), Some(low)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((high * 16 + low) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The bare file names a text holds (`Resumo-2026.pdf`, `**grafico.png**`): words with an extension and no folder.
pub fn file_names(text: &str, windows: bool) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let name = trim_wrapping(word);
            if name.contains('/') || (windows && name.contains('\\')) {
                return None;
            }
            let (stem, ext) = name.rsplit_once('.')?;
            (!stem.is_empty() && !ext.is_empty() && ext.chars().all(char::is_alphanumeric)).then(|| name.to_string())
        })
        .collect()
}

// ---------------------------------------------------------------- time

/// An ISO 8601 moment as Claude Code writes it (`2026-11-17T12:30:00.123Z`, or with an offset).
fn parse_time(stamp: &str) -> Option<SystemTime> {
    let s = stamp.trim();
    let num = |range: std::ops::Range<usize>| s.get(range)?.parse::<i64>().ok();
    if s.len() < 19 || s.as_bytes()[4] != b'-' || s.as_bytes()[10] != b'T' {
        return None;
    }
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut rest = &s[19..];
    let mut nanos: i64 = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
        rest = &fraction[digits.len()..];
        let padded = format!("{:0<9}", &digits[..digits.len().min(9)]);
        nanos = padded.parse().ok()?;
    }
    let offset = match rest {
        "Z" | "z" | "" => 0,
        _ => {
            let sign = if rest.starts_with('-') { -1 } else { 1 };
            let hours = rest.get(1..3)?.parse::<i64>().ok()?;
            let minutes = rest.get(rest.len() - 2..)?.parse::<i64>().ok()?;
            sign * (hours * 3600 + minutes * 60)
        }
    };
    // Days from 1970-01-01 of a date in the Gregorian calendar (Howard Hinnant's `days_from_civil`).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset;
    let seconds = u64::try_from(seconds).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(seconds) + Duration::from_nanos(nanos as u64))
}

// ---------------------------------------------------------------- pictures kept for a while

/// Pictures a conversation pasted are not kept for good: a conversation's folder untouched for 30 days goes, once a
/// launch. One still read gets its pictures written again from the transcript.
const KEPT_FOR: Duration = Duration::from_secs(30 * 86_400);

static PRUNED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// Held through the whole sweep: a read that comes in meanwhile waits, and does not read pictures the sweep is
/// about to take away.
pub fn prune_once(folder: &Path, now: SystemTime) {
    let mut pruned = PRUNED.lock().unwrap_or_else(|e| e.into_inner());
    if !pruned.get_or_insert_with(HashSet::new).insert(folder.to_path_buf()) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > KEPT_FOR);
        if old {
            let path = entry.path();
            let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        }
    }
}

// ---------------------------------------------------------------- where things are

/// The person's home: `USERPROFILE` on Windows, else `HOME`.
pub fn home() -> String {
    std::env::var("USERPROFILE").ok().filter(|h| !h.is_empty()).or_else(|| std::env::var("HOME").ok()).unwrap_or_default()
}

/// Where pasted pictures are written: `%LOCALAPPDATA%\Keep\conversas`, else `~/.cache/keep/conversas`.
pub fn image_folder() -> PathBuf {
    match std::env::var("LOCALAPPDATA") {
        Ok(dir) if !dir.is_empty() => Path::new(&dir).join("Keep").join("conversas"),
        _ => Path::new(&home()).join(".cache").join("keep").join("conversas"),
    }
}

/// The Downloads folder: where Windows says it is (it can be moved), else `Downloads` in the home.
pub fn downloads_folder() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(known) = known_downloads() {
        return Some(known);
    }
    let home = home();
    (!home.is_empty()).then(|| Path::new(&home).join("Downloads"))
}

#[cfg(windows)]
fn known_downloads() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KNOWN_FOLDER_FLAG};
    // SAFETY: the system hands back a string it allocated, read once and freed with CoTaskMemFree.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Downloads, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.filter(|t| !t.is_empty()).map(PathBuf::from)
    }
}

// ---------------------------------------------------------------- the readers the windows share

/// One reader per transcript, the last few kept: a panel that goes back to a conversation reads only what it gained.
#[derive(Default)]
pub struct Readers {
    kept: Mutex<Vec<(String, Arc<Mutex<Reader>>)>>,
}

const KEPT_READERS: usize = 16;

/// A session id is a name, never a path: it names folders here.
pub fn valid_session(session: &str) -> bool {
    !session.is_empty() && session.len() <= 128 && session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Readers {
    pub fn reader(&self, transcript: &str, session: &str) -> Arc<Mutex<Reader>> {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let wanted = format!("{transcript}\u{0}{session}");
        let reader = match kept.iter().position(|(k, _)| *k == wanted) {
            Some(at) => kept.remove(at).1,
            None => Arc::new(Mutex::new(Reader::new(transcript, session, image_folder(), downloads_folder(), &home()))),
        };
        kept.push((wanted, Arc::clone(&reader)));
        while kept.len() > KEPT_READERS {
            kept.remove(0);
        }
        reader
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File, FileTimes};
    use std::io::Write;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keep-chat-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // The temporary folder may be reached through a link (`/var` is `/private/var` on a Mac).
        dir.canonicalize().unwrap()
    }

    fn s(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }

    /// An old file: created (and changed) long before the conversation began.
    fn old(path: &Path) {
        let then = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let file = File::options().write(true).open(path).unwrap();
        #[allow(unused_mut)]
        let mut times = FileTimes::new().set_modified(then);
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::FileTimesExt;
            times = times.set_created(then);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTimesExt;
            times = times.set_created(then);
        }
        file.set_times(times).unwrap();
    }

    fn file(root: &Path, name: &str, created_old: bool) -> String {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "x").unwrap();
        if created_old {
            old(&path);
        }
        s(&path)
    }

    fn line(json: Value) -> String {
        serde_json::to_string(&json).unwrap()
    }

    fn paths(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.path.clone()).collect()
    }

    fn reader(root: &Path, transcript: &Path, session: &str) -> Reader {
        Reader::with_flavor(&s(transcript), session, root.join("imagens"), None, &s(&root.join("casa")), false)
    }

    // The creation date is set only where the system lets a program set it.
    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn the_cases_of_the_macos_app() {
        let root = scratch("casos");
        let dropped = file(&root, "dir com espaço/contrato.pdf", true);
        let typed = file(&root, "notas.md", true);
        let written = file(&root, "relatorio.html", true);
        let step = file(&root, "rascunho.html", false);
        let scratchfile = file(&root, "extrai.sh", false);
        let handed = file(&root, "resumo.pdf", false);
        let by_bash = file(&root, "grafico.png", false);
        let only_read = file(&root, "entrada.csv", true);
        let in_summary = file(&root, "do-resumo.txt", true);
        let other_tab = file(&root, "de-outra-aba.txt", true);
        let png = "iVBORw==";
        // Created a minute ago: after the conversation began, which the transcript says.
        let begun = SystemTime::now() - Duration::from_secs(60);
        let begun_text = {
            let secs = begun.duration_since(UNIX_EPOCH).unwrap().as_secs();
            // Back to ISO 8601 through the parser's own arithmetic: tested below on its own.
            let days = secs / 86_400;
            let rem = secs % 86_400;
            let (y, m, d) = civil(days as i64);
            format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
        };
        let lines = [
            line(serde_json::json!({"type": "user", "timestamp": begun_text, "origin": {"kind": "human"},
                "message": {"role": "user", "content": format!("olha {} e `{}`", dropped.replace(' ', "\\ "), typed)}})),
            line(serde_json::json!({"type": "user", "timestamp": begun_text, "imagePasteIds": [3], "uuid": "u1",
                "message": {"role": "user", "content": [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": png}},
                                                         {"type": "text", "text": "[Image #3] veja"}]}})),
            line(serde_json::json!({"type": "user", "timestamp": begun_text, "isCompactSummary": true,
                "message": {"role": "user", "content": format!("Resumo: {in_summary}")}})),
            line(serde_json::json!({"type": "user", "timestamp": begun_text, "origin": {"kind": "task-notification"},
                "message": {"role": "user", "content": format!("de {other_tab}")}})),
            line(serde_json::json!({"type": "user", "timestamp": begun_text,
                "message": {"role": "user", "content": format!("<task-notification>{other_tab}</task-notification>")}})),
            line(serde_json::json!({"type": "assistant", "timestamp": begun_text, "message": {"role": "assistant", "content": [
                {"type": "tool_use", "name": "Write", "input": {"file_path": written, "content": "x"}},
                {"type": "tool_use", "name": "Write", "input": {"file_path": step, "content": "x"}},
                {"type": "tool_use", "name": "Write", "input": {"file_path": scratchfile, "content": "x"}},
                {"type": "tool_use", "name": "Bash", "input": {"command": format!("sh {scratchfile}; print {step} > {handed}; plot {only_read} > {by_bash} && cat /etc/hosts")}},
                {"type": "tool_use", "name": "SendUserFile", "input": {"files": [handed]}},
                {"type": "text", "text": format!("Gravei **{by_bash}** e o relatorio.html, a partir de entrada.csv: pronto.")},
            ]}})),
        ];
        let transcript = root.join("t.jsonl");
        fs::write(&transcript, lines.join("\n") + "\n").unwrap();
        let mut r = reader(&root, &transcript, "sessao");
        let files = r.read();
        let image = s(&root.join("imagens").join("sessao").join("Imagem 3.png"));
        assert_eq!(paths(&files.sent), vec![image.clone(), typed.clone(), dropped.clone()], "enviados: a imagem, o digitado e o solto");
        assert_eq!(paths(&files.made), vec![by_bash.clone(), handed.clone(), written.clone()], "gerados, sem os passos do caminho");
        assert_eq!(fs::read(&image).unwrap(), vec![0x89, 0x50, 0x4E, 0x47]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&image).unwrap().permissions().mode() & 0o777, 0o600);
        }

        // The transcript grows: only the new line is read, and it counts once whole.
        let later = file(&root, "depois.txt", false);
        let more = line(serde_json::json!({"type": "assistant", "timestamp": begun_text, "message": {"role": "assistant", "content": [
            {"type": "tool_use", "name": "Write", "input": {"file_path": later, "content": "x"}},
            {"type": "text", "text": "Também gravei depois.txt."}]}}));
        let mut handle = File::options().append(true).open(&transcript).unwrap();
        handle.write_all(more.as_bytes()).unwrap();
        assert_eq!(paths(&r.read().made), vec![by_bash.clone(), handed.clone(), written.clone()], "a linha ainda sendo escrita espera");
        handle.write_all(b"\n").unwrap();
        drop(handle);
        assert_eq!(paths(&r.read().made), vec![later.clone(), by_bash.clone(), handed.clone(), written.clone()]);

        // A file gone from disk leaves the list.
        fs::remove_file(&written).unwrap();
        assert_eq!(paths(&r.read().made), vec![later, by_bash, handed]);
        fs::remove_dir_all(&root).unwrap();
    }

    fn civil(days: i64) -> (i64, i64, i64) {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (yoe + era * 400 + i64::from(m <= 2), m, d)
    }

    #[test]
    fn a_trailing_escape_is_decoded() {
        // O último %XX da string também: `i + 2 < len` já alcança o fim.
        assert_eq!(percent_decoded("C:/Users/Jos%C3%A9"), "C:/Users/José");
        assert_eq!(percent_decoded("a%2"), "a%2", "escape cortado fica como está");
    }

    #[test]
    fn two_pictures_without_ids_are_two_files() {
        let root = scratch("duas");
        let png = "iVBORw==";
        let two = root.join("dois.jsonl");
        fs::write(
            &two,
            line(serde_json::json!({"type": "user", "timestamp": "2026-01-01T00:00:00Z", "uuid": "abcdef0123",
                "message": {"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": png}},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": png}}]}}))
                + "\n",
        )
        .unwrap();
        let names: Vec<String> = reader(&root, &two, "dois").read().sent.iter().map(|i| file_name(&i.path).to_string()).collect();
        assert_eq!(names, vec!["Imagem abcdef01-1.jpg", "Imagem abcdef01-0.jpg"]);
        // Um uuid com barras e pontos não tira a imagem da pasta da conversa.
        let fora = root.join("fora.jsonl");
        fs::write(
            &fora,
            line(serde_json::json!({"type": "user", "timestamp": "2026-01-01T00:00:00Z", "uuid": "/../../a",
                "message": {"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": png}}]}}))
                + "\n",
        )
        .unwrap();
        let sent = reader(&root, &fora, "fora").read().sent;
        assert_eq!(sent.iter().map(|i| file_name(&i.path).to_string()).collect::<Vec<_>>(), vec!["Imagem a-0.jpg"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_bare_name_two_files_share_names_neither() {
        let root = scratch("gemeos");
        let twins = [file(&root, "a/index.html", false), file(&root, "b/index.html", false)];
        let log = root.join("twins.jsonl");
        let mut content: Vec<Value> =
            twins.iter().map(|t| serde_json::json!({"type": "tool_use", "name": "Write", "input": {"file_path": t, "content": "x"}})).collect();
        content.push(serde_json::json!({"type": "text", "text": "Veja o index.html."}));
        fs::write(&log, line(serde_json::json!({"type": "assistant", "timestamp": "2020-01-01T00:00:00Z", "message": {"role": "assistant", "content": content}})) + "\n").unwrap();
        assert!(reader(&root, &log, "gemeos").read().made.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn a_namesake_only_read_does_not_hide_the_one_it_wrote() {
        let root = scratch("copia");
        let template = file(&root, "modelo/pagina.html", true);
        let page = file(&root, "pagina.html", false);
        let log = root.join("copiado.jsonl");
        fs::write(
            &log,
            // Begun after the template was made (2023) and before the page (now).
            line(serde_json::json!({"type": "assistant", "timestamp": "2025-01-01T00:00:00Z", "message": {"role": "assistant", "content": [
                {"type": "tool_use", "name": "Bash", "input": {"command": format!("cp {template} /tmp/nada")}},
                {"type": "tool_use", "name": "Write", "input": {"file_path": page, "content": "x"}},
                {"type": "text", "text": "Veja a pagina.html."}]}}))
                + "\n",
        )
        .unwrap();
        assert_eq!(paths(&reader(&root, &log, "copia").read().made), vec![page]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn what_a_file_made_before_the_conversation_and_only_named_is_not_its_own() {
        let root = scratch("antes");
        let named = file(&root, "antigo.pdf", false);
        let log = root.join("t.jsonl");
        // The conversation begins after the file was made (a day from now): named in an answer, it is not a result.
        let tomorrow = "2999-01-01T00:00:00Z";
        fs::write(
            &log,
            line(serde_json::json!({"type": "assistant", "timestamp": tomorrow, "message": {"role": "assistant", "content": [
                {"type": "text", "text": format!("Está em {named}.")}]}}))
                + "\n",
        )
        .unwrap();
        assert!(reader(&root, &log, "antes").read().made.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn uploads_of_the_session_are_sent() {
        let root = scratch("uploads");
        let up = file(&root, "casa/.claude/uploads/sess-1/foto.png", false);
        let log = root.join("t.jsonl");
        fs::write(&log, "").unwrap();
        assert_eq!(paths(&reader(&root, &log, "sess-1").read().sent), vec![up]);
        fs::remove_dir_all(&root).unwrap();
    }

    // A folder is opened for its times only on Unix (Windows wants a flag the standard library does not give).
    #[cfg(unix)]
    #[test]
    fn pictures_left_for_30_days_are_gone() {
        let root = scratch("poda");
        let cache = root.join("cache");
        for (name, age_days) in [("velha", 31u64), ("nova", 2)] {
            let dir = cache.join(name);
            fs::create_dir_all(&dir).unwrap();
            let when = SystemTime::now() - Duration::from_secs(age_days * 86_400);
            File::open(&dir).unwrap().set_modified(when).unwrap();
        }
        prune_once(&cache, SystemTime::now());
        let mut left: Vec<String> = fs::read_dir(&cache).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, vec!["nova"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_downloads_folder_without_hidden_files() {
        let root = scratch("baixados");
        fs::create_dir_all(root.join("pasta")).unwrap();
        fs::write(root.join("nota.pdf"), "").unwrap();
        fs::write(root.join(".DS_Store"), "").unwrap();
        let mut names: Vec<String> = downloads_in(&root).unwrap().iter().map(|i| file_name(&i.path).to_string()).collect();
        names.sort();
        assert_eq!(names, vec!["nota.pdf", "pasta"]);
        assert!(downloads_in(&root.join("nao-existe")).is_none(), "uma pasta que não se lê não é uma pasta vazia");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn paths_in_text_on_a_mac_or_linux() {
        assert_eq!(
            paths_in("veja file:///tmp/a.txt, ~/b.md:12 e \"/x/c d.pdf\".", "/casa", false),
            vec!["/tmp/a.txt", "/casa/b.md", "/x/c d.pdf"]
        );
        assert_eq!(paths_in("/a/b/../c.txt:3:4", "/casa", false), vec!["/a/c.txt"]);
    }

    #[test]
    fn paths_in_text_on_windows() {
        let home = "C:\\Users\\pessoa";
        assert_eq!(
            paths_in("Gravei C:\\Users\\pessoa\\Relatorio.pdf, e c:/temp/x.png:12 e ~/notas.md.", home, true),
            vec!["C:\\Users\\pessoa\\Relatorio.pdf", "C:\\temp\\x.png", "C:\\Users\\pessoa\\notas.md"]
        );
        // Git Bash's way, where Claude Code's Bash tool runs, and WSL's; any other rooted POSIX path is nothing here.
        assert_eq!(
            paths_in("cp /c/Users/pessoa/a.pdf /mnt/d/b.pdf /etc/hosts", home, true),
            vec!["C:\\Users\\pessoa\\a.pdf", "D:\\b.pdf"]
        );
        assert_eq!(paths_in("\"C:\\Meus Documentos\\c d.pdf\"", home, true), vec!["C:\\Meus Documentos\\c d.pdf"]);
        assert_eq!(paths_in("file:///C:/x%20y/z.txt", home, true), vec!["C:\\x y\\z.txt"]);
        assert_eq!(paths_in("\\\\servidor\\pasta\\a.txt", home, true), vec!["\\\\servidor\\pasta\\a.txt"]);
    }

    #[test]
    fn on_windows_case_does_not_tell_two_paths_apart() {
        assert_eq!(paths_in("C:\\A\\b.pdf c:\\a\\B.PDF", "", true), vec!["C:\\A\\b.pdf"]);
    }

    #[test]
    fn a_server_on_the_network_is_never_asked() {
        assert!(!is_file("\\\\servidor\\pasta\\a.txt"));
        assert!(!is_file("//servidor/pasta/a.txt"));
    }

    #[test]
    fn bare_names_in_text() {
        assert_eq!(file_names("Gravei **grafico.png** e o Resumo-2026.pdf, mais a/b.txt e v2", false), vec!["grafico.png", "Resumo-2026.pdf"]);
        assert_eq!(file_names("pasta\\x.pdf y.md", true), vec!["y.md"]);
    }

    #[test]
    fn a_name_and_its_type() {
        assert_eq!(split_name("Relatório.final.PDF"), ("Relatório.final".to_string(), "pdf".to_string()));
        assert_eq!(split_name(".gitignore"), (".gitignore".to_string(), String::new()));
        assert_eq!(split_name("LEIAME"), ("LEIAME".to_string(), String::new()));
    }

    #[test]
    fn moments_as_claude_code_writes_them() {
        let at = |s: &str| parse_time(s).unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis();
        assert_eq!(at("1970-01-01T00:00:00Z"), 0);
        assert_eq!(at("2026-11-17T12:30:00.123Z"), 1_794_918_600_123);
        assert_eq!(at("2026-11-17T09:30:00.123-03:00"), 1_794_918_600_123);
        assert_eq!(at("2024-02-29T00:00:00Z"), 1_709_164_800_000);
        assert!(parse_time("ontem").is_none());
    }

    #[test]
    fn a_session_is_a_name_not_a_path() {
        assert!(valid_session("3f2a-91_b"));
        assert!(!valid_session("../../x"));
        assert!(!valid_session("a\\b"));
        assert!(!valid_session(""));
    }
}
