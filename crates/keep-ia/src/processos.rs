//! O que se sabe de um processo desta máquina: pai, nome, quando nasceu,
//! pasta, linha de comando e ambiente — perguntado ao sistema, sem `ps`.
//!
//! A conta em que uma aba roda está no ambiente do processo da frente dela
//! (`KEEP_IA_ESCOLHA`, `CLAUDE_SECURESTORAGE_CONFIG_DIR`, `CODEX_HOME`), e a
//! conversa, na linha de comando (`--resume <id>`, `resume <id>`). Cada
//! sistema guarda isso num lugar:
//!
//! - macOS: `proc_pidinfo` (pai, nome, nascimento, pasta) e o
//!   `KERN_PROCARGS2` (linha de comando e ambiente, de processos do mesmo
//!   usuário);
//! - Linux: `/proc/<pid>`;
//! - Windows: a lista do Toolhelp e o bloco de parâmetros do processo,
//!   alcançado pelo PEB (o mesmo caminho do keepd para a pasta).
//!
//! Nada aqui escreve em processo nenhum.

use std::path::PathBuf;

/// Um processo, como a lista do sistema o mostra.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Processo {
    pub pid: u32,
    pub ppid: u32,
    /// O nome do executável, sem `.exe`.
    pub nome: String,
    /// Quando nasceu, em ms unix (0 = não se sabe).
    pub inicio_ms: u64,
}

/// Todos os processos que se consegue ver.
pub fn todos() -> Vec<Processo> {
    sistema::todos()
}

/// Um processo, se ele existe.
pub fn um(pid: u32) -> Option<Processo> {
    sistema::um(pid)
}

pub fn vivo(pid: u32) -> bool {
    pid != 0 && um(pid).is_some()
}

/// Quando o processo nasceu, em ms unix, perguntado só a ele (no Windows,
/// sem tirar a lista inteira). Com o pid, é o que identifica um processo: o
/// Windows dá logo a um processo novo o pid de um que acabou.
pub fn inicio_ms(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    let t = sistema::inicio_ms(pid);
    #[cfg(not(windows))]
    let t = um(pid).map(|p| p.inicio_ms).unwrap_or(0);
    (t > 0).then_some(t)
}

/// Onde o processo está trabalhando.
pub fn cwd(pid: u32) -> Option<PathBuf> {
    sistema::cwd(pid)
}

/// Como ele foi chamado: o programa e os argumentos.
pub fn argv(pid: u32) -> Option<Vec<String>> {
    sistema::argv(pid)
}

/// O ambiente com que ele nasceu (no Windows, o atual).
pub fn ambiente(pid: u32) -> Option<Vec<(String, String)>> {
    sistema::ambiente(pid)
}

/// Uma variável do ambiente dele.
pub fn variavel(pid: u32, nome: &str) -> Option<String> {
    ambiente(pid)?.into_iter().find(|(n, _)| igual_nome(n, nome)).map(|(_, v)| v)
}

fn igual_nome(a: &str, b: &str) -> bool {
    // No Windows o nome das variáveis não distingue caixa.
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

/// Os filhos, netos e assim por diante, de `pid` (sem ele), dos mais velhos
/// aos mais novos. Um filho tem de ter nascido depois do pai: o número de um
/// pai morto pode passar a outro processo, e os órfãos dele não são filhos
/// do novo.
pub fn descendentes(pid: u32) -> Vec<u32> {
    let lista = todos();
    let mut saida: Vec<&Processo> = Vec::new();
    let mut fila = vec![pid];
    while let Some(pai) = fila.pop() {
        let nasceu = lista.iter().find(|p| p.pid == pai).map(|p| p.inicio_ms).unwrap_or(0);
        for p in lista.iter().filter(|p| p.ppid == pai && p.pid != pai && p.pid != pid) {
            if p.inicio_ms != 0 && nasceu != 0 && p.inicio_ms + 1000 < nasceu {
                continue;
            }
            if !saida.iter().any(|q| q.pid == p.pid) {
                saida.push(p);
                fila.push(p.pid);
            }
        }
    }
    saida.sort_by_key(|p| (p.inicio_ms, p.pid));
    saida.into_iter().map(|p| p.pid).collect()
}

/// Separa um bloco de ambiente `NOME=valor`. Entradas sem `=` ou com nome
/// vazio (as `=C:=C:\…` do Windows) ficam de fora.
fn separa_ambiente<'a>(entradas: impl Iterator<Item = &'a str>) -> Vec<(String, String)> {
    entradas
        .filter_map(|e| {
            let (n, v) = e.split_once('=')?;
            (!n.is_empty()).then(|| (n.to_string(), v.to_string()))
        })
        .collect()
}

// ------------------------------------------------------------------ macOS

#[cfg(target_os = "macos")]
mod sistema {
    use super::{Processo, separa_ambiente};
    use std::ffi::CStr;
    use std::path::PathBuf;

    fn info(pid: u32) -> Option<libc::proc_bsdinfo> {
        let mut i: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let tamanho = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        let n = unsafe {
            libc::proc_pidinfo(pid as i32, libc::PROC_PIDTBSDINFO, 0, (&mut i as *mut libc::proc_bsdinfo).cast(), tamanho)
        };
        (n == tamanho).then_some(i)
    }

    fn texto(c: &[libc::c_char]) -> String {
        let bytes: Vec<u8> = c.iter().take_while(|&&b| b != 0).map(|&b| b as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn de_info(pid: u32, i: &libc::proc_bsdinfo) -> Processo {
        // pbi_name tem até 32 letras; pbi_comm, 16.
        let nome = texto(&i.pbi_name);
        let nome = if nome.is_empty() { texto(&i.pbi_comm) } else { nome };
        Processo {
            pid,
            ppid: i.pbi_ppid,
            nome,
            inicio_ms: i.pbi_start_tvsec * 1000 + i.pbi_start_tvusec / 1000,
        }
    }

    pub fn um(pid: u32) -> Option<Processo> {
        info(pid).map(|i| de_info(pid, &i))
    }

    pub fn todos() -> Vec<Processo> {
        let n = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if n <= 0 {
            return Vec::new();
        }
        let mut pids = vec![0i32; n as usize + 64];
        let bytes = (pids.len() * std::mem::size_of::<i32>()) as i32;
        let n = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        if n <= 0 {
            return Vec::new();
        }
        pids.truncate(n as usize);
        pids.into_iter().filter(|&p| p > 0).filter_map(|p| um(p as u32)).collect()
    }

    pub fn cwd(pid: u32) -> Option<PathBuf> {
        let mut v: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
        let tamanho = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
        let n = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                (&mut v as *mut libc::proc_vnodepathinfo).cast(),
                tamanho,
            )
        };
        if n != tamanho {
            return None;
        }
        let c = unsafe { CStr::from_ptr(v.pvi_cdir.vip_path.as_ptr().cast()) };
        let s = c.to_string_lossy().into_owned();
        (!s.is_empty()).then(|| PathBuf::from(s))
    }

    /// `KERN_PROCARGS2`: argc, o caminho do executável, os argumentos, o
    /// ambiente e, depois de uma entrada vazia, as do sistema (`executable_path=`…).
    fn procargs(pid: u32) -> Option<(Vec<String>, Vec<String>)> {
        let mut max: libc::c_int = 0;
        let mut tam = std::mem::size_of::<libc::c_int>();
        let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
        if unsafe { libc::sysctl(mib.as_mut_ptr(), 2, (&mut max as *mut libc::c_int).cast(), &mut tam, std::ptr::null_mut(), 0) } != 0
            || max <= 0
        {
            return None;
        }
        let mut buf = vec![0u8; max as usize];
        let mut tam = buf.len();
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
        if unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr().cast(), &mut tam, std::ptr::null_mut(), 0) } != 0 {
            return None;
        }
        buf.truncate(tam);
        if buf.len() < 4 {
            return None;
        }
        let argc = i32::from_ne_bytes(buf[..4].try_into().ok()?).max(0) as usize;
        let mut i = 4;
        // O caminho do executável, e os NULs que o alinham.
        while i < buf.len() && buf[i] != 0 {
            i += 1;
        }
        while i < buf.len() && buf[i] == 0 {
            i += 1;
        }
        let proximo = |i: &mut usize| -> Option<String> {
            if *i >= buf.len() {
                return None;
            }
            let inicio = *i;
            while *i < buf.len() && buf[*i] != 0 {
                *i += 1;
            }
            let s = String::from_utf8_lossy(&buf[inicio..*i]).into_owned();
            *i += 1;
            Some(s)
        };
        let mut argv = Vec::with_capacity(argc);
        for _ in 0..argc {
            argv.push(proximo(&mut i)?);
        }
        let mut env = Vec::new();
        while let Some(s) = proximo(&mut i) {
            if s.is_empty() {
                break;
            }
            env.push(s);
        }
        Some((argv, env))
    }

    pub fn argv(pid: u32) -> Option<Vec<String>> {
        procargs(pid).map(|(a, _)| a)
    }

    pub fn ambiente(pid: u32) -> Option<Vec<(String, String)>> {
        procargs(pid).map(|(_, e)| separa_ambiente(e.iter().map(String::as_str)))
    }
}

// ------------------------------------------------------------------ Linux

#[cfg(all(unix, not(target_os = "macos")))]
mod sistema {
    use super::{Processo, separa_ambiente};
    use std::path::PathBuf;
    use std::sync::OnceLock;

    /// O boot, em ms unix, e quantos tiques o relógio dá por segundo.
    fn relogio() -> (u64, u64) {
        static R: OnceLock<(u64, u64)> = OnceLock::new();
        *R.get_or_init(|| {
            let boot = std::fs::read_to_string("/proc/stat")
                .ok()
                .and_then(|t| t.lines().find_map(|l| l.strip_prefix("btime ").and_then(|v| v.trim().parse::<u64>().ok())))
                .unwrap_or(0);
            let tiques = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
            (boot * 1000, if tiques > 0 { tiques as u64 } else { 100 })
        })
    }

    pub fn um(pid: u32) -> Option<Processo> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // O nome vem entre parênteses e pode ter espaços e parênteses.
        let abre = stat.find('(')?;
        let fecha = stat.rfind(')')?;
        let nome = stat[abre + 1..fecha].to_string();
        let resto: Vec<&str> = stat[fecha + 1..].split_whitespace().collect();
        let ppid = resto.get(1)?.parse().ok()?;
        let comeco: u64 = resto.get(19)?.parse().ok()?;
        let (boot, tiques) = relogio();
        let inicio_ms = if boot == 0 { 0 } else { boot + comeco * 1000 / tiques };
        Some(Processo { pid, ppid, nome, inicio_ms })
    }

    pub fn todos() -> Vec<Processo> {
        let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
        dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok()).filter_map(um).collect()
    }

    pub fn cwd(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    fn nulos(caminho: String) -> Option<Vec<String>> {
        let bytes = std::fs::read(caminho).ok()?;
        Some(
            bytes
                .split(|&b| b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect(),
        )
    }

    pub fn argv(pid: u32) -> Option<Vec<String>> {
        nulos(format!("/proc/{pid}/cmdline"))
    }

    pub fn ambiente(pid: u32) -> Option<Vec<(String, String)>> {
        let e = nulos(format!("/proc/{pid}/environ"))?;
        Some(separa_ambiente(e.iter().map(String::as_str)))
    }
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod sistema {
    use super::{Processo, separa_ambiente};
    use std::ffi::c_void;
    use std::path::PathBuf;

    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, IsWow64Process, OpenProcess, PROCESS_BASIC_INFORMATION, PROCESS_QUERY_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    struct Alca(HANDLE);

    impl Drop for Alca {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn abre(pid: u32, acesso: u32) -> Option<Alca> {
        let h = unsafe { OpenProcess(acesso, 0, pid) };
        (!h.is_null()).then_some(Alca(h))
    }

    /// FILETIME (100 ns desde 1601) em ms unix.
    pub(super) fn inicio_ms(pid: u32) -> u64 {
        let Some(p) = abre(pid, PROCESS_QUERY_LIMITED_INFORMATION) else { return 0 };
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut criado, mut saiu, mut k, mut u) = (zero, zero, zero, zero);
        if unsafe { GetProcessTimes(p.0, &mut criado, &mut saiu, &mut k, &mut u) } == 0 {
            return 0;
        }
        let t = (u64::from(criado.dwHighDateTime) << 32) | u64::from(criado.dwLowDateTime);
        t.saturating_sub(116_444_736_000_000_000) / 10_000
    }

    fn sem_exe(nome: &str) -> String {
        let n = nome.strip_suffix(".exe").or_else(|| nome.strip_suffix(".EXE")).unwrap_or(nome);
        n.to_string()
    }

    fn lista() -> Vec<(u32, u32, String)> {
        let foto = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if foto == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let foto = Alca(foto);
        let mut e: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut saida = Vec::new();
        let mut mais = unsafe { Process32FirstW(foto.0, &mut e) } != 0;
        while mais {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            saida.push((e.th32ProcessID, e.th32ParentProcessID, sem_exe(&String::from_utf16_lossy(&e.szExeFile[..n]))));
            mais = unsafe { Process32NextW(foto.0, &mut e) } != 0;
        }
        saida
    }

    pub fn todos() -> Vec<Processo> {
        lista()
            .into_iter()
            .filter(|(pid, _, _)| *pid != 0)
            .map(|(pid, ppid, nome)| Processo { pid, ppid, nome, inicio_ms: inicio_ms(pid) })
            .collect()
    }

    pub fn um(pid: u32) -> Option<Processo> {
        let (pid, ppid, nome) = lista().into_iter().find(|(p, _, _)| *p == pid)?;
        Some(Processo { pid, ppid, nome, inicio_ms: inicio_ms(pid) })
    }

    fn le(p: &Alca, endereco: usize, n: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; n];
        let mut lidos = 0usize;
        let ok = unsafe { ReadProcessMemory(p.0, endereco as *const c_void, buf.as_mut_ptr().cast(), n, &mut lidos) };
        (ok != 0 && lidos == n).then_some(buf)
    }

    fn le_usize(p: &Alca, endereco: usize) -> Option<usize> {
        Some(usize::from_le_bytes(le(p, endereco, 8)?.try_into().ok()?))
    }

    fn utf16(bytes: &[u8]) -> String {
        let w: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&w)
    }

    fn le_unicode(p: &Alca, endereco: usize) -> Option<String> {
        let cab = le(p, endereco, 16)?;
        let n = u16::from_le_bytes([cab[0], cab[1]]) as usize;
        let buf = usize::from_le_bytes(cab[8..16].try_into().ok()?);
        if n == 0 || buf == 0 || n > 64 * 1024 {
            return None;
        }
        Some(utf16(&le(p, buf, n)?))
    }

    // O bloco RTL_USER_PROCESS_PARAMETERS de um processo de 64 bits: o que
    // um depurador lê, estável desde o Vista.
    const PEB_PARAMETROS: usize = 0x20;
    const PARAM_PASTA: usize = 0x38;
    const PARAM_LINHA: usize = 0x70;
    const PARAM_AMBIENTE: usize = 0x80;
    const PARAM_TAMANHO_AMBIENTE: usize = 0x3F0;

    fn parametros(pid: u32) -> Option<(Alca, usize)> {
        if std::mem::size_of::<usize>() != 8 {
            return None;
        }
        let p = abre(pid, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)?;
        let mut wow = 0;
        if unsafe { IsWow64Process(p.0, &mut wow) } == 0 || wow != 0 {
            return None;
        }
        let mut info: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        let mut n = 0u32;
        let st = unsafe {
            NtQueryInformationProcess(
                p.0,
                ProcessBasicInformation,
                (&mut info as *mut PROCESS_BASIC_INFORMATION).cast(),
                std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                &mut n,
            )
        };
        if st < 0 || info.PebBaseAddress.is_null() {
            return None;
        }
        let params = le_usize(&p, info.PebBaseAddress as usize + PEB_PARAMETROS)?;
        (params != 0).then_some((p, params))
    }

    pub fn cwd(pid: u32) -> Option<PathBuf> {
        let (p, params) = parametros(pid)?;
        let d = le_unicode(&p, params + PARAM_PASTA)?;
        let d = match d.strip_suffix('\\') {
            Some(r) if !r.ends_with(':') => r.to_string(),
            _ => d,
        };
        (!d.is_empty()).then(|| PathBuf::from(d))
    }

    pub fn argv(pid: u32) -> Option<Vec<String>> {
        let (p, params) = parametros(pid)?;
        Some(super::divide_linha(&le_unicode(&p, params + PARAM_LINHA)?))
    }

    pub fn ambiente(pid: u32) -> Option<Vec<(String, String)>> {
        let (p, params) = parametros(pid)?;
        let bloco = le_usize(&p, params + PARAM_AMBIENTE)?;
        let tamanho = le_usize(&p, params + PARAM_TAMANHO_AMBIENTE)?;
        if bloco == 0 || tamanho == 0 || tamanho > 4 * 1024 * 1024 {
            return None;
        }
        let texto = utf16(&le(&p, bloco, tamanho & !1)?);
        Some(separa_ambiente(texto.split('\0')))
    }
}

/// Uma linha de comando do Windows dividida como o runtime do C divide:
/// espaços separam, aspas agrupam, barra invertida só vale antes de aspas.
pub fn divide_linha(linha: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut atual = String::new();
    let mut aspas = false;
    let mut comecou = false;
    let mut barras = 0usize;
    for c in linha.chars() {
        match c {
            '\\' => {
                barras += 1;
                comecou = true;
            }
            '"' => {
                atual.extend(std::iter::repeat_n('\\', barras / 2));
                if barras % 2 == 1 {
                    atual.push('"');
                } else {
                    aspas = !aspas;
                }
                barras = 0;
                comecou = true;
            }
            ' ' | '\t' if !aspas => {
                atual.extend(std::iter::repeat_n('\\', barras));
                barras = 0;
                if comecou {
                    args.push(std::mem::take(&mut atual));
                    comecou = false;
                }
            }
            _ => {
                atual.extend(std::iter::repeat_n('\\', barras));
                barras = 0;
                atual.push(c);
                comecou = true;
            }
        }
    }
    atual.extend(std::iter::repeat_n('\\', barras));
    if comecou {
        args.push(atual);
    }
    args
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn este_processo_se_le() {
        let eu = std::process::id();
        let p = um(eu).expect("o próprio processo");
        assert!(!p.nome.is_empty());
        assert!(p.inicio_ms > 1_600_000_000_000, "{p:?}");
        assert!(vivo(eu));
        assert!(todos().iter().any(|q| q.pid == eu));
        let pasta = cwd(eu).expect("a própria pasta");
        assert_eq!(
            std::fs::canonicalize(&pasta).unwrap(),
            std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap()
        );
        assert!(!argv(eu).expect("a própria linha").is_empty());
    }

    /// O filho dos testes abaixo: o próprio executável de teste, que dorme
    /// quando chamado com a variável. Um programa do sistema não serve: o
    /// macOS esconde o ambiente dos binários dele (`/bin/sleep` dá vazio).
    #[test]
    #[ignore]
    fn dorminhoco() {
        if std::env::var_os("KEEP_IA_TESTE_PROCESSO").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }

    #[test]
    fn um_filho_mostra_o_ambiente_e_a_linha() {
        let mut filho = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "processos::testes::dorminhoco", "--ignored", "--test-threads=1", "-q"])
            .env("KEEP_IA_TESTE_PROCESSO", "valor com espaço")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = filho.id();
        // O exec do filho pode não ter acontecido ainda.
        let mut visto = None;
        for _ in 0..100 {
            visto = variavel(pid, "KEEP_IA_TESTE_PROCESSO");
            if visto.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let linha = argv(pid).unwrap_or_default();
        let desc = descendentes(std::process::id());
        let pasta = cwd(pid);
        filho.kill().ok();
        filho.wait().ok();
        assert_eq!(visto.as_deref(), Some("valor com espaço"));
        assert_eq!(linha.last().map(String::as_str), Some("-q"), "{linha:?}");
        assert!(desc.contains(&pid), "o filho é descendente do teste: {desc:?}");
        assert!(pasta.is_some());
    }

    #[test]
    fn linhas_do_windows_se_dividem_como_no_c() {
        assert_eq!(divide_linha(r#"cmd /c "C:\a b\claude.cmd" --x"#), ["cmd", "/c", r"C:\a b\claude.cmd", "--x"]);
        assert_eq!(divide_linha(r#"a\\"b c" d"#), [r"a\b c", "d"]);
        assert_eq!(divide_linha(r#"x "" y"#), ["x", "", "y"]);
    }
}

// ------------------------------------------------- o que a lixeira de worktrees também pergunta

/// `descendentes` sobre uma lista já tirada, para quem pergunta por várias
/// abas de uma vez (a mesma regra: um filho nasceu depois do pai).
pub fn descendentes_em(lista: &[Processo], pid: u32) -> Vec<u32> {
    let mut saida: Vec<&Processo> = Vec::new();
    let mut fila = vec![pid];
    while let Some(pai) = fila.pop() {
        let nasceu = lista.iter().find(|p| p.pid == pai).map(|p| p.inicio_ms).unwrap_or(0);
        for p in lista.iter().filter(|p| p.ppid == pai && p.pid != pai && p.pid != pid) {
            if p.inicio_ms != 0 && nasceu != 0 && p.inicio_ms + 1000 < nasceu {
                continue;
            }
            if !saida.iter().any(|q| q.pid == p.pid) {
                saida.push(p);
                fila.push(p.pid);
            }
        }
    }
    saida.sort_by_key(|p| (p.inicio_ms, p.pid));
    saida.into_iter().map(|p| p.pid).collect()
}

/// O pai de um processo, perguntado só a ele. No macOS, `todos` não traz os
/// processos de outro usuário (um `sudo` no meio de uma cadeia), e o pai
/// deles se lê assim mesmo.
pub fn pai(pid: u32) -> Option<u32> {
    extra::pai(pid)
}

/// Os arquivos que o processo tem abertos. No Windows, nenhum: lá não há
/// como saber sem ser administrador.
pub fn arquivos_abertos(pid: u32) -> Vec<PathBuf> {
    extra::arquivos_abertos(pid)
}

/// O terminal de um processo, onde há terminal (macOS e Linux).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terminal {
    /// O grupo de processos que está com o terminal: o comando rodando, ou o
    /// shell no prompt.
    pub frente: Option<u32>,
    pub colunas: u16,
    pub linhas: u16,
}

pub fn terminal(pid: u32) -> Option<Terminal> {
    extra::terminal(pid)
}

/// Que programa o processo é, como uma pessoa o chamaria: `claude`, `codex`,
/// `zsh`. Pela linha de comando, não pelo executável: o Claude Code nativo
/// roda de um arquivo com o nome da versão (`…/versions/2.1.289`), e quem
/// diz que ele é o `claude` é o `argv[0]`. Um lançador (`node`, `bun`,
/// `deno`, `cmd /c`) dá lugar ao que lançou.
pub fn programa(pid: u32) -> Option<String> {
    programa_de_argv(&argv(pid)?)
}

/// `programa` a partir de uma linha de comando já lida.
pub fn programa_de_argv(argv: &[String]) -> Option<String> {
    let primeiro = argv.first()?;
    let base = nome_de_arquivo(primeiro).trim_start_matches('-');
    let base = sem_exe(base);
    if base.is_empty() {
        return None;
    }
    let minusculo = base.to_ascii_lowercase();
    if matches!(minusculo.as_str(), "node" | "bun" | "deno") {
        if let Some(lancado) = script_lancado(&argv[1..]) {
            return Some(lancado);
        }
    }
    if minusculo == "cmd" {
        let pos = argv.iter().position(|a| a.eq_ignore_ascii_case("/c") || a.eq_ignore_ascii_case("/k"));
        if let Some(script) = pos.and_then(|i| argv.get(i + 1)) {
            let arquivo = nome_de_arquivo(script);
            let tronco = arquivo.rsplit_once('.').map(|(t, _)| t).unwrap_or(arquivo);
            if !tronco.is_empty() {
                return Some(tronco.to_ascii_lowercase());
            }
        }
    }
    Some(if cfg!(windows) { minusculo } else { base.to_string() })
}

/// O pacote ou o arquivo que um runtime de JavaScript recebeu.
fn script_lancado(args: &[String]) -> Option<String> {
    let script = args.iter().find(|a| {
        let a = a.to_ascii_lowercase();
        a.ends_with(".js") || a.ends_with(".mjs") || a.ends_with(".cjs")
    })?;
    let partes: Vec<&str> = script.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    if let Some(i) = partes.iter().rposition(|p| p.eq_ignore_ascii_case("node_modules")) {
        let pacote = match partes.get(i + 1) {
            Some(escopo) if escopo.starts_with('@') => partes.get(i + 2).copied(),
            outro => outro.copied(),
        }?;
        let pacote = pacote.to_ascii_lowercase();
        // `@anthropic-ai/claude-code` se instala como `claude`.
        return Some(if pacote == "claude-code" { "claude".into() } else { pacote });
    }
    let arquivo = partes.last()?;
    arquivo.rsplit_once('.').map(|(t, _)| t.to_ascii_lowercase())
}

fn nome_de_arquivo(caminho: &str) -> &str {
    caminho.rsplit(['/', '\\']).next().unwrap_or(caminho)
}

fn sem_exe(nome: &str) -> &str {
    match nome.len().checked_sub(4) {
        Some(i) if i > 0 && nome.is_char_boundary(i) && nome[i..].eq_ignore_ascii_case(".exe") => &nome[..i],
        _ => nome,
    }
}

#[cfg(target_os = "macos")]
mod extra {
    use super::Terminal;
    use std::ffi::{CStr, c_void};
    use std::path::PathBuf;

    // Posições em `struct kinfo_proc` (sys/sysctl.h, 64 bits), conferidas com
    // offsetof: a libc do Rust não traz a estrutura.
    const KINFO_TAMANHO: usize = 648;
    const KINFO_PID: usize = 40;
    const KINFO_PPID: usize = 560;
    const KINFO_TDEV: usize = 572;
    const KINFO_TPGID: usize = 576;

    fn i32_em(b: &[u8], i: usize) -> i32 {
        i32::from_ne_bytes(b[i..i + 4].try_into().unwrap())
    }

    /// A linha do processo na tabela do núcleo: responde também pelos de
    /// outro usuário, que a libproc recusa.
    fn kinfo(pid: u32) -> Option<Vec<u8>> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid as libc::c_int];
        let mut buf = vec![0u8; KINFO_TAMANHO];
        let mut tam: libc::size_t = buf.len();
        let r = unsafe {
            libc::sysctl(mib.as_mut_ptr(), 4, buf.as_mut_ptr().cast(), &mut tam, std::ptr::null_mut(), 0)
        };
        (r == 0 && tam == KINFO_TAMANHO && i32_em(&buf, KINFO_PID) == pid as i32).then_some(buf)
    }

    pub fn pai(pid: u32) -> Option<u32> {
        let p = i32_em(&kinfo(pid)?, KINFO_PPID);
        (p >= 0).then_some(p as u32)
    }

    pub fn arquivos_abertos(pid: u32) -> Vec<PathBuf> {
        use std::os::unix::ffi::OsStrExt;
        const PROC_PIDFDVNODEPATHINFO: libc::c_int = 2;
        // struct vnode_fdinfowithpath: proc_fileinfo (24) + vnode_info (152) + o caminho.
        const COM_CAMINHO: usize = 1200;
        const POSICAO_DO_CAMINHO: usize = 176;
        let pid = pid as libc::c_int;
        let n = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        if n <= 0 {
            return Vec::new();
        }
        let tamanho = std::mem::size_of::<libc::proc_fdinfo>();
        let mut fds = vec![libc::proc_fdinfo { proc_fd: 0, proc_fdtype: 0 }; n as usize / tamanho + 16];
        let bytes = (fds.len() * tamanho) as libc::c_int;
        let n = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, fds.as_mut_ptr().cast(), bytes) };
        if n <= 0 {
            return Vec::new();
        }
        fds.truncate(n as usize / tamanho);
        let mut saida = Vec::new();
        let mut buf = vec![0u8; COM_CAMINHO];
        for fd in fds {
            if fd.proc_fdtype != libc::PROX_FDTYPE_VNODE as u32 {
                continue;
            }
            let lido = unsafe {
                libc::proc_pidfdinfo(
                    pid,
                    fd.proc_fd,
                    PROC_PIDFDVNODEPATHINFO,
                    buf.as_mut_ptr() as *mut c_void,
                    COM_CAMINHO as libc::c_int,
                )
            };
            if lido < COM_CAMINHO as libc::c_int {
                continue;
            }
            let c = &buf[POSICAO_DO_CAMINHO..];
            let fim = c.iter().position(|&b| b == 0).unwrap_or(c.len());
            if fim > 0 {
                saida.push(PathBuf::from(std::ffi::OsStr::from_bytes(&c[..fim])));
            }
        }
        saida
    }

    pub fn terminal(pid: u32) -> Option<Terminal> {
        let k = kinfo(pid)?;
        let dev = i32_em(&k, KINFO_TDEV);
        if dev == -1 || dev == 0 {
            return None;
        }
        let tpgid = i32_em(&k, KINFO_TPGID);
        let nome = unsafe { libc::devname(dev as libc::dev_t, libc::S_IFCHR) };
        let (colunas, linhas) = if nome.is_null() {
            (0, 0)
        } else {
            let nome = unsafe { CStr::from_ptr(nome) }.to_string_lossy().into_owned();
            super::tamanho_do_tty(&format!("/dev/{nome}")).unwrap_or((0, 0))
        };
        Some(Terminal { frente: (tpgid > 0).then_some(tpgid as u32), colunas, linhas })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod extra {
    use super::Terminal;
    use std::path::PathBuf;

    /// Os campos do `/proc/<pid>/stat` depois do nome entre parênteses.
    fn campos(pid: u32) -> Option<Vec<String>> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fecha = stat.rfind(')')?;
        Some(stat[fecha + 1..].split_whitespace().map(str::to_string).collect())
    }

    pub fn pai(pid: u32) -> Option<u32> {
        campos(pid)?.get(1)?.parse().ok()
    }

    pub fn arquivos_abertos(pid: u32) -> Vec<PathBuf> {
        let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/fd")) else { return Vec::new() };
        dir.filter_map(|e| std::fs::read_link(e.ok()?.path()).ok()).filter(|p| p.is_absolute()).collect()
    }

    pub fn terminal(pid: u32) -> Option<Terminal> {
        let c = campos(pid)?;
        let tty: i64 = c.get(4)?.parse().ok()?;
        if tty <= 0 {
            return None;
        }
        let tpgid: i64 = c.get(5).and_then(|v| v.parse().ok()).unwrap_or(-1);
        let maior = (tty >> 8) & 0xfff;
        let menor = (tty & 0xff) | ((tty >> 12) & 0xfff00);
        // Os pseudoterminais: maiores 136 a 143, numerados em sequência.
        let (colunas, linhas) = if (136..=143).contains(&maior) {
            super::tamanho_do_tty(&format!("/dev/pts/{}", menor + (maior - 136) * 256)).unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        Some(Terminal { frente: (tpgid > 0).then_some(tpgid as u32), colunas, linhas })
    }
}

#[cfg(unix)]
fn tamanho_do_tty(tty: &str) -> Option<(u16, u16)> {
    let c = std::ffi::CString::new(tty).ok()?;
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK) };
    if fd < 0 {
        return None;
    }
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ as _, &mut ws) };
    unsafe { libc::close(fd) };
    (r == 0).then_some((ws.ws_col, ws.ws_row))
}

#[cfg(windows)]
mod extra {
    use super::Terminal;
    use std::path::PathBuf;

    pub fn pai(pid: u32) -> Option<u32> {
        super::um(pid).map(|p| p.ppid)
    }

    pub fn arquivos_abertos(_pid: u32) -> Vec<PathBuf> {
        Vec::new()
    }

    pub fn terminal(_pid: u32) -> Option<Terminal> {
        None
    }
}

#[cfg(test)]
mod testes_extra {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn o_programa_sai_da_linha_de_comando() {
        assert_eq!(programa_de_argv(&v(&["/Users/x/.local/bin/claude", "--resume", "a"])).as_deref(), Some("claude"));
        assert_eq!(programa_de_argv(&v(&["-zsh"])).as_deref(), Some("zsh"));
        assert_eq!(
            programa_de_argv(&v(&["node", "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js"])).as_deref(),
            Some("claude")
        );
        assert_eq!(
            programa_de_argv(&v(&["node", r"C:\x\node_modules\@openai\codex\bin\codex.js", "resume"])).as_deref(),
            Some("codex")
        );
        assert_eq!(programa_de_argv(&v(&["node", "server.js"])).as_deref(), Some("server"));
        assert_eq!(programa_de_argv(&v(&["node"])).as_deref(), Some("node"));
        assert_eq!(
            programa_de_argv(&v(&[r"C:\Windows\system32\cmd.exe", "/d", "/s", "/c", r"C:\npm\claude.cmd"])).as_deref(),
            Some("claude")
        );
        assert_eq!(programa_de_argv(&v(&[r"C:\Program Files\Claude\claude.exe"])).as_deref(), Some("claude"));
        assert_eq!(programa_de_argv(&v(&[""])), None);
        assert_eq!(programa_de_argv(&[]), None);
    }

    #[test]
    fn descendentes_de_uma_lista_exigem_pai_mais_velho() {
        let p = |pid, ppid, inicio_ms| Processo { pid, ppid, nome: String::new(), inicio_ms };
        // 30 é neto de 10; 40 diz que o pai é 10 mas nasceu bem antes dele
        // (pid reaproveitado); 50 não diz quando nasceu.
        let lista = vec![p(10, 1, 10_000), p(20, 10, 20_000), p(30, 20, 30_000), p(40, 10, 500), p(50, 10, 0)];
        assert_eq!(descendentes_em(&lista, 10), vec![50, 20, 30]);
        assert!(descendentes_em(&lista, 30).is_empty());
    }

    #[test]
    fn o_pai_e_os_arquivos_do_proprio_processo() {
        let eu = std::process::id();
        #[cfg(unix)]
        assert_eq!(pai(eu), Some(std::os::unix::process::parent_id()));
        #[cfg(windows)]
        assert_eq!(pai(eu), um(eu).map(|p| p.ppid));
        let pasta = tempfile::tempdir().unwrap();
        let arq = pasta.path().join("aberto.lock");
        let _f = std::fs::File::create(&arq).unwrap();
        let abertos = arquivos_abertos(eu);
        if cfg!(unix) {
            let alvo = std::fs::canonicalize(&arq).unwrap();
            assert!(
                abertos.iter().any(|p| std::fs::canonicalize(p).ok().as_deref() == Some(alvo.as_path())),
                "{abertos:?}"
            );
        } else {
            assert!(abertos.is_empty());
        }
        assert_eq!(programa(eu), argv(eu).and_then(|a| programa_de_argv(&a)));
        assert!(descendentes_em(&todos(), eu).iter().all(|&p| p != eu));
    }
}
