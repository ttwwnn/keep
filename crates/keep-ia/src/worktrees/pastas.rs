//! Caminhos como o disco os grava, e os lugares onde nunca se olha.
//!
//! Os caminhos andam como texto (é assim que vão para o JSON e que se
//! comparam com o que os transcritos citam); no Windows, com `\` e sem
//! diferença de caixa na comparação.

use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// O separador deste sistema.
pub const SEP: char = std::path::MAIN_SEPARATOR;

fn texto(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `os.path.normpath`: tira `.`, resolve `..` sem olhar o disco, junta
/// barras repetidas; no Windows, `/` vira `\`.
pub fn normaliza(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let mut saida = PathBuf::new();
    let mut partes = 0usize;
    for c in Path::new(p).components() {
        match c {
            Component::Prefix(_) | Component::RootDir => saida.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if partes > 0 {
                    saida.pop();
                    partes -= 1;
                } else if !saida.has_root() {
                    saida.push("..");
                }
            }
            Component::Normal(n) => {
                saida.push(n);
                partes += 1;
            }
        }
    }
    let s = texto(&saida);
    if s.is_empty() { ".".into() } else { s }
}

/// O caminho é absoluto aqui (no Windows: com unidade ou de rede).
pub fn e_absoluto(p: &str) -> bool {
    if cfg!(windows) {
        let b = p.as_bytes();
        (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
            || p.starts_with("\\\\")
            || p.starts_with("//")
    } else {
        p.starts_with('/')
    }
}

/// `os.path.abspath`.
pub fn absoluto(p: &str) -> String {
    if e_absoluto(p) {
        return normaliza(p);
    }
    let base = std::env::current_dir().map(|d| texto(&d)).unwrap_or_default();
    normaliza(&junta(&base, p))
}

pub fn junta(a: &str, b: &str) -> String {
    texto(&Path::new(a).join(b))
}

/// `os.path.dirname`.
pub fn pai(p: &str) -> String {
    let n = normaliza(p);
    Path::new(&n).parent().map(texto).unwrap_or(n)
}

/// `os.path.basename` (sem a barra do fim).
pub fn base(p: &str) -> String {
    let p = p.trim_end_matches(['/', SEP]);
    let i = p.rfind(['/', SEP]).map(|i| i + 1).unwrap_or(0);
    p[i..].to_string()
}

/// A forma de comparar: no Windows, sem caixa e com `\`.
fn chave(p: &str) -> String {
    if cfg!(windows) {
        p.replace('/', "\\").to_lowercase()
    } else {
        p.to_string()
    }
}

/// `p` é `pasta` ou está dentro dela.
pub fn dentro(p: &str, pasta: &str) -> bool {
    let (p, pasta) = (chave(p), chave(pasta));
    if p == pasta {
        return true;
    }
    let raiz = pasta.trim_end_matches(SEP);
    p.starts_with(&format!("{raiz}{SEP}"))
}

pub fn mesmo(a: &str, b: &str) -> bool {
    chave(a.trim_end_matches(['/', SEP])) == chave(b.trim_end_matches(['/', SEP]))
}

/// Caminho de outra máquina (transcrito trazido de outro computador): nunca
/// se olha no disco. No macOS, pelo prefixo `/home/` — `stat` lá passa pelo
/// autofs e trava.
pub fn outra_maquina(p: &str) -> bool {
    if p.is_empty() || !e_absoluto(p) {
        return true;
    }
    if cfg!(target_os = "macos") {
        return p == "/home" || p.starts_with("/home/");
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        return p.starts_with("/Users/");
    }
    false
}

/// A pasta de uma chamada conta (vazia conta: o Claude antigo não a gravava).
pub fn local(cwd: &str) -> bool {
    cwd.is_empty() || !outra_maquina(cwd)
}

/// O caminho como o disco o grava: links resolvidos e a caixa certa
/// (`~/Projetos` é `~/projetos` no APFS). Caminho que não existe: a parte
/// que existe resolvida, o resto como veio.
pub fn canon(p: &str) -> String {
    if outra_maquina(p) {
        return p.to_string();
    }
    #[cfg(target_os = "macos")]
    if let Some(c) = pelo_descritor(p) {
        return c;
    }
    match std::fs::canonicalize(p) {
        Ok(c) => sem_prefixo_literal(texto(&c)),
        Err(_) => frouxo(p),
    }
}

/// `F_GETPATH`: o caminho que o próprio arquivo aberto tem.
#[cfg(target_os = "macos")]
fn pelo_descritor(p: &str) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p).ok()?;
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let mut buf = vec![0u8; libc::PATH_MAX as usize + 1];
    let r = unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr()) };
    unsafe { libc::close(fd) };
    if r < 0 {
        return None;
    }
    let fim = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    (fim > 0).then(|| texto(Path::new(std::ffi::OsStr::from_bytes(&buf[..fim]))))
}

/// `\\?\C:\x` → `C:\x`; `\\?\UNC\srv\x` → `\\srv\x`.
fn sem_prefixo_literal(s: String) -> String {
    if let Some(r) = s.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{r}");
    }
    match s.strip_prefix(r"\\?\") {
        Some(r) if r.as_bytes().get(1) == Some(&b':') => r.to_string(),
        _ => s,
    }
}

/// `os.path.realpath` que não exige o caminho inteiro.
fn frouxo(p: &str) -> String {
    let abs = absoluto(p);
    let mut existe = PathBuf::from(&abs);
    let mut resto: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(c) = std::fs::canonicalize(&existe) {
            let mut c = PathBuf::from(sem_prefixo_literal(texto(&c)));
            for r in resto.iter().rev() {
                c.push(r);
            }
            return normaliza(&texto(&c));
        }
        match (existe.file_name().map(|n| n.to_os_string()), existe.parent().map(Path::to_path_buf)) {
            (Some(n), Some(pai)) => {
                resto.push(n);
                existe = pai;
            }
            _ => return abs,
        }
    }
}

/// Pontos de montagem que não são disco local (NFS, SMB, autofs…): nunca se
/// olha dentro.
fn montagens_de_fora() -> &'static [String] {
    static M: OnceLock<Vec<String>> = OnceLock::new();
    M.get_or_init(sistema::montagens_de_fora)
}

pub fn fora_do_disco(p: &str) -> bool {
    outra_maquina(p)
        || montagens_de_fora().iter().any(|m| m != "/" && dentro(p, m))
        || sistema::de_rede(p)
}

#[cfg(target_os = "macos")]
mod sistema {
    pub fn montagens_de_fora() -> Vec<String> {
        let n = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
        if n <= 0 {
            return Vec::new();
        }
        let mut v: Vec<libc::statfs> = Vec::with_capacity(n as usize + 8);
        let bytes = (v.capacity() * std::mem::size_of::<libc::statfs>()) as libc::c_int;
        let n = unsafe { libc::getfsstat(v.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
        if n <= 0 {
            return Vec::new();
        }
        unsafe { v.set_len(n as usize) };
        let s = |c: &[libc::c_char]| {
            let b: Vec<u8> = c.iter().take_while(|&&x| x != 0).map(|&x| x as u8).collect();
            String::from_utf8_lossy(&b).into_owned()
        };
        v.iter()
            .filter(|f| !matches!(s(&f.f_fstypename).as_str(), "apfs" | "hfs" | "devfs"))
            .map(|f| s(&f.f_mntonname))
            .collect()
    }

    pub fn de_rede(_p: &str) -> bool {
        false
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod sistema {
    const DE_REDE: &[&str] = &[
        "nfs", "nfs4", "cifs", "smb3", "smbfs", "sshfs", "fuse.sshfs", "autofs", "9p", "ceph", "glusterfs",
        "fuse.glusterfs", "afs", "davfs", "fuse.rclone", "lustre", "fuse.gvfsd-fuse",
    ];

    /// `\040` e companhia, como o `/proc/self/mounts` escreve.
    fn desescapa(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
                out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
                i += 4;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    pub fn montagens_de_fora() -> Vec<String> {
        let Ok(t) = std::fs::read_to_string("/proc/self/mounts") else { return Vec::new() };
        t.lines()
            .filter_map(|l| {
                let mut c = l.split_whitespace();
                let (_, ponto, tipo) = (c.next()?, c.next()?, c.next()?);
                DE_REDE.contains(&tipo).then(|| desescapa(ponto))
            })
            .collect()
    }

    pub fn de_rede(_p: &str) -> bool {
        false
    }
}

#[cfg(windows)]
mod sistema {
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub fn montagens_de_fora() -> Vec<String> {
        Vec::new()
    }

    /// Caminho de rede (`\\servidor\…`) ou numa unidade mapeada da rede.
    pub fn de_rede(p: &str) -> bool {
        const DRIVE_REMOTE: u32 = 4;
        if p.starts_with("\\\\") || p.starts_with("//") {
            return true;
        }
        let b = p.as_bytes();
        if b.len() < 2 || b[1] != b':' || !b[0].is_ascii_alphabetic() {
            return false;
        }
        static UNIDADES: Mutex<Option<HashMap<u8, bool>>> = Mutex::new(None);
        let letra = b[0].to_ascii_uppercase();
        let mut g = UNIDADES.lock().unwrap_or_else(|e| e.into_inner());
        let mapa = g.get_or_insert_with(HashMap::new);
        *mapa.entry(letra).or_insert_with(|| {
            let raiz: Vec<u16> = format!("{}:\\", letra as char).encode_utf16().chain([0]).collect();
            unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(raiz.as_ptr()) == DRIVE_REMOTE }
        })
    }
}

fn segundos(t: std::time::SystemTime) -> Option<f64> {
    let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    // Como o Python monta o `st_birthtime`: segundos + nanossegundos × 1e-9.
    Some(d.as_secs() as f64 + f64::from(d.subsec_nanos()) * 1e-9)
}

/// Quando o arquivo nasceu (no Linux sem `statx`, a última mudança de
/// metadados, como o kit fazia fora do macOS).
pub fn nascimento(m: &Metadata) -> f64 {
    if let Some(t) = m.created().ok().and_then(segundos) {
        return t;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.ctime() as f64 + m.ctime_nsec() as f64 * 1e-9
    }
    #[cfg(not(unix))]
    {
        mtime(m)
    }
}

/// O nascimento em nanossegundos, com a precisão do microssegundo: é o
/// número que o `listar` mostra e que o `preparar` confere.
pub fn nascimento_ns(t: f64) -> i64 {
    (t * 1e6).round_ties_even() as i64 * 1000
}

pub fn mtime(m: &Metadata) -> f64 {
    m.modified().ok().and_then(segundos).unwrap_or(0.0)
}

/// O que identifica o arquivo além do caminho (o inode; no Windows, o
/// nascimento): outro arquivo no mesmo caminho se lê do zero.
pub fn identidade(m: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        m.ino()
    }
    #[cfg(not(unix))]
    {
        m.created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}

pub fn e_pasta(p: &str) -> bool {
    Path::new(p).is_dir()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normalizar() {
        if cfg!(windows) {
            assert_eq!(normaliza("C:/a/./b/../c"), r"C:\a\c");
            assert_eq!(base(r"C:\a\b\"), "b");
            assert!(dentro(r"c:\A\b", r"C:\a"));
        } else {
            assert_eq!(normaliza("/a/./b/../c//d/"), "/a/c/d");
            assert_eq!(normaliza("/.."), "/");
            assert_eq!(pai("/a/b/"), "/a");
            assert_eq!(base("/a/b/"), "b");
            assert!(dentro("/a/b", "/a"));
            assert!(!dentro("/a", "/a/"), "como o kit: a pasta vem sem barra no fim");
            assert!(!dentro("/ab", "/a"));
            assert!(dentro("/x", "/"));
        }
    }

    #[test]
    fn outra_maquina_pelo_prefixo() {
        assert!(outra_maquina(""));
        assert!(outra_maquina("relativo/x"));
        if cfg!(target_os = "macos") {
            assert!(outra_maquina("/home/ttwwnn/Projetos"));
            assert!(!outra_maquina("/homem"));
            assert!(!outra_maquina("/Users/x"));
        }
        assert!(local(""));
    }

    #[test]
    fn canon_de_caminho_que_nao_existe() {
        let d = tempfile::tempdir().unwrap();
        let real = canon(&texto(d.path()));
        let falta = junta(&junta(&texto(d.path()), "nao"), "existe");
        assert_eq!(canon(&falta), junta(&junta(&real, "nao"), "existe"));
        assert!(fora_do_disco("") && !fora_do_disco(&real));
    }

    #[test]
    fn nascimento_em_microssegundos() {
        assert_eq!(nascimento_ns(1790482232.2116084), 1790482232211608000);
        // Como o Python: round(1.0000005 * 1e6) = 1000001.
        assert_eq!(nascimento_ns(1.0000005), 1000001000);
    }
}
