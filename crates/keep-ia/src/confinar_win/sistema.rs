//! A parte do Windows da IA presa: o token restrito, as permissões, o Job e o
//! processo. Cada função é uma chamada (ou duas) ao sistema, com o erro dito
//! em português; o que decide o quê está em `super` (`confinar_win.rs`).
//!
//! Escrito para o crate `windows-sys` 0.61 e conferido contra os cabeçalhos
//! dele; no Mac só se compila (`cargo check --target x86_64-pc-windows-msvc`),
//! o que roda é no CI do Windows.

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, GetLastError, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
    LUID, LocalFree, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW,
    GetSecurityInfo, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SE_KERNEL_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW,
    SetSecurityInfo, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, ACL_SIZE_INFORMATION, AclSizeInformation, AddAccessAllowedAce,
    CreateRestrictedToken, DACL_SECURITY_INFORMATION, DISABLE_MAX_PRIVILEGE, DuplicateTokenEx, EqualSid, GetAce,
    GetAclInformation, GetLengthSid, GetSecurityDescriptorControl, GetTokenInformation, INHERIT_ONLY_ACE,
    InitializeAcl, IsTokenRestricted, LUA_TOKEN, LUID_AND_ATTRIBUTES, LookupPrivilegeNameW, LookupPrivilegeValueW,
    NO_INHERITANCE, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, RevertToSelf, SE_DACL_PROTECTED,
    SID_AND_ATTRIBUTES, SUB_CONTAINERS_AND_OBJECTS_INHERIT, SecurityImpersonation,
    SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ALL_ACCESS, TOKEN_ASSIGN_PRIMARY, TOKEN_DEFAULT_DACL,
    TOKEN_DUPLICATE, TOKEN_GROUPS, TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL, TOKEN_OWNER, TOKEN_PRIVILEGES,
    TOKEN_QUERY, TOKEN_USER, TokenDefaultDacl, TokenGroups, TokenImpersonation, TokenIntegrityLevel, TokenOwner,
    TokenPrivileges, TokenUser, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, GetConsoleProcessList, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateMutexW, CreateProcessAsUserW,
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, PROCESS_INFORMATION, ReleaseMutex,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOW, SetThreadToken, TerminateProcess, WaitForSingleObject,
};
use windows_sys::core::PWSTR;

use super::Acesso;

pub mod objetos;

/// O atributo do rótulo de integridade de um token (`SE_GROUP_INTEGRITY`).
const GRUPO_INTEGRIDADE: u32 = 0x20;

/// O atributo de um grupo que o marca como o da sessão de logon.
const GRUPO_LOGON: u32 = 0xC000_0000;

/// A permissão de acesso total (`GENERIC_ALL`) da DACL padrão do token.
const TUDO: u32 = 0x1000_0000;

/// Pedir à alça o que o sistema deixar (`MAXIMUM_ALLOWED`).
const O_QUE_DEIXAR: u32 = 0x0200_0000;

fn larga(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// O que um código de erro do Windows costuma querer dizer neste caso.
fn descricao(codigo: u32) -> &'static str {
    match codigo {
        2 | 3 => "não encontrado",
        5 => "acesso negado",
        6 => "alça inválida",
        32 => "em uso por outro processo",
        50 | 1 => "operação sem suporte neste disco (precisa ser NTFS)",
        87 => "parâmetro inválido",
        193 => "não é um programa do Windows",
        1314 => "falta um privilégio",
        1337 => "SID inválido",
        1346 => "nível de representação insuficiente",
        _ => "",
    }
}

fn falha(oque: &str, codigo: u32) -> String {
    match descricao(codigo) {
        "" => format!("{oque} (erro {codigo} do Windows)"),
        d => format!("{oque}: {d} (erro {codigo})"),
    }
}

/// O erro da última chamada. Chamar logo depois dela, antes de qualquer outra.
fn falha_ultimo(oque: &str) -> String {
    let codigo = unsafe { GetLastError() };
    falha(oque, codigo)
}

// ------------------------------------------------------------------ alças e memória do sistema

/// Uma alça do sistema, fechada ao sair.
pub struct Alca(pub HANDLE);

impl Drop for Alca {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Memória que o sistema alocou e que se devolve com `LocalFree`.
struct Local(*mut c_void);

impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0 as HLOCAL) };
        }
    }
}

// ------------------------------------------------------------------ SIDs

/// Um SID, guardado em palavras de 4 bytes (o alinhamento que a estrutura pede).
pub struct Sid {
    palavras: Vec<u32>,
}

impl Sid {
    pub fn de_texto(texto: &str) -> Result<Sid, String> {
        let w = larga(texto);
        let mut psid: PSID = null_mut();
        if unsafe { ConvertStringSidToSidW(w.as_ptr(), &mut psid) } == 0 {
            return Err(falha_ultimo(&format!("o SID {texto} é inválido")));
        }
        let _guarda = Local(psid);
        Ok(Sid::copia(psid))
    }

    fn copia(origem: PSID) -> Sid {
        let n = unsafe { GetLengthSid(origem) } as usize;
        let mut palavras = vec![0u32; n.div_ceil(4)];
        unsafe { std::ptr::copy_nonoverlapping(origem as *const u8, palavras.as_mut_ptr() as *mut u8, n) };
        Sid { palavras }
    }

    pub fn ptr(&self) -> PSID {
        self.palavras.as_ptr() as PSID
    }

    pub fn texto(&self) -> Result<String, String> {
        let mut p: PWSTR = null_mut();
        if unsafe { ConvertSidToStringSidW(self.ptr(), &mut p) } == 0 {
            return Err(falha_ultimo("ConvertSidToStringSid"));
        }
        let _guarda = Local(p as *mut c_void);
        let mut n = 0usize;
        while unsafe { *p.add(n) } != 0 {
            n += 1;
        }
        Ok(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) }))
    }
}

// ------------------------------------------------------------------ o token

/// O token deste processo, com o que `CreateRestrictedToken` e a DACL padrão
/// pedem. Com tudo (o que o Chromium pede ao fazer o mesmo) e, se o sistema
/// não der tanto, com o que basta.
pub fn token_do_processo() -> Result<Alca, String> {
    let mut t: HANDLE = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ALL_ACCESS, &mut t) } == 0 {
        let basta = TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY | TOKEN_ADJUST_DEFAULT;
        if unsafe { OpenProcessToken(GetCurrentProcess(), basta, &mut t) } == 0 {
            return Err(falha_ultimo("não foi possível abrir o token do processo"));
        }
    }
    Ok(Alca(t))
}

/// Se o token é restrito: a prova de que o processo não leva a vantagem do
/// usuário inteiro.
pub fn e_restrito(token: &Alca) -> bool {
    unsafe { IsTokenRestricted(token.0) != 0 }
}

fn info_do_token(token: HANDLE, classe: TOKEN_INFORMATION_CLASS) -> Result<Vec<u64>, String> {
    let mut tam = 0u32;
    // A primeira chamada só diz quanto cabe.
    unsafe { GetTokenInformation(token, classe, null_mut(), 0, &mut tam) };
    if tam == 0 {
        return Err(falha_ultimo("GetTokenInformation"));
    }
    let mut buf = vec![0u64; (tam as usize).div_ceil(8)];
    if unsafe { GetTokenInformation(token, classe, buf.as_mut_ptr().cast(), tam, &mut tam) } == 0 {
        return Err(falha_ultimo("GetTokenInformation"));
    }
    Ok(buf)
}

pub fn sid_do_usuario(token: &Alca) -> Result<Sid, String> {
    let buf = info_do_token(token.0, TokenUser)?;
    let u = unsafe { &*(buf.as_ptr() as *const TOKEN_USER) };
    Ok(Sid::copia(u.User.Sid))
}

/// Os grupos de um `TOKEN_GROUPS` lido por `info_do_token`. A lista passa do
/// tamanho que a estrutura declara (é um array de um), por isso por ponteiro
/// e não por referência a ela.
fn grupos(buf: &[u64]) -> &[SID_AND_ATTRIBUTES] {
    let g = buf.as_ptr() as *const TOKEN_GROUPS;
    unsafe {
        let n = std::ptr::addr_of!((*g).GroupCount).read() as usize;
        std::slice::from_raw_parts(std::ptr::addr_of!((*g).Groups) as *const SID_AND_ATTRIBUTES, n)
    }
}

/// O SID da sessão de logon: sem ele no token restrito o processo não abre a
/// estação de janelas nem o desktop.
pub fn sid_do_logon(token: &Alca) -> Result<Sid, String> {
    let buf = info_do_token(token.0, TokenGroups)?;
    grupos(&buf)
        .iter()
        .find(|g| g.Attributes & GRUPO_LOGON == GRUPO_LOGON)
        .map(|g| Sid::copia(g.Sid))
        .ok_or_else(|| "o token não tem o SID da sessão de logon".to_string())
}

/// Os grupos administrativos do token (os `BUILTIN` além de `Usuários`):
/// ficam só para negar, como num token de administrador sem elevação.
pub fn grupos_administrativos(token: &Alca) -> Result<Vec<Sid>, String> {
    let buf = info_do_token(token.0, TokenGroups)?;
    let mut v = Vec::new();
    for g in grupos(&buf) {
        let sid = Sid::copia(g.Sid);
        let texto = sid.texto()?;
        if texto.starts_with("S-1-5-32-") && texto != super::SID_USUARIOS {
            v.push(sid);
        }
    }
    Ok(v)
}

/// Uma DACL com `GENERIC_ALL` para cada SID.
fn monta_dacl(sids: &[&Sid]) -> Result<Vec<u32>, String> {
    let mut tam = std::mem::size_of::<ACL>();
    for s in sids {
        tam += std::mem::size_of::<ACCESS_ALLOWED_ACE>() - std::mem::size_of::<u32>()
            + unsafe { GetLengthSid(s.ptr()) } as usize;
    }
    let tam = (tam + 3) & !3;
    let mut buf = vec![0u32; tam / 4];
    let acl = buf.as_mut_ptr() as *mut ACL;
    if unsafe { InitializeAcl(acl, tam as u32, ACL_REVISION) } == 0 {
        return Err(falha_ultimo("InitializeAcl"));
    }
    for s in sids {
        if unsafe { AddAccessAllowedAce(acl, ACL_REVISION, TUDO, s.ptr()) } == 0 {
            return Err(falha_ultimo("AddAccessAllowedAce"));
        }
    }
    Ok(buf)
}

/// A chamada a `CreateRestrictedToken` e nada mais: o token, ou o código de
/// erro do Windows. É o que o diagnóstico repete com uma lista de cada vez
/// para achar o SID ou a opção que o sistema não aceita.
pub fn token_restrito_cru(
    origem: &Alca,
    flags: u32,
    desabilitar: &[&Sid],
    apagar: &[LUID_AND_ATTRIBUTES],
    restringir: &[&Sid],
) -> Result<Alca, u32> {
    let lista = |sids: &[&Sid]| -> Vec<SID_AND_ATTRIBUTES> {
        sids.iter().map(|s| SID_AND_ATTRIBUTES { Sid: s.ptr(), Attributes: 0 }).collect()
    };
    let a_restringir = lista(restringir);
    let a_desabilitar = lista(desabilitar);
    let ou_nulo = |v: &[SID_AND_ATTRIBUTES]| if v.is_empty() { null() } else { v.as_ptr() };
    let mut novo: HANDLE = null_mut();
    let ok = unsafe {
        CreateRestrictedToken(
            origem.0,
            flags,
            a_desabilitar.len() as u32,
            ou_nulo(&a_desabilitar),
            apagar.len() as u32,
            if apagar.is_empty() { null() } else { apagar.as_ptr() },
            a_restringir.len() as u32,
            ou_nulo(&a_restringir),
            &mut novo,
        )
    };
    if ok == 0 {
        return Err(unsafe { GetLastError() });
    }
    Ok(Alca(novo))
}

/// O que o código de erro do Windows quer dizer, para o diagnóstico.
pub fn erro_em_texto(codigo: u32) -> String {
    falha("erro", codigo)
}

/// Os grupos do token: o SID, em texto, e os atributos (o diagnóstico mostra
/// quais estão ligados, quais só negam e qual é o da sessão de logon).
pub fn grupos_do_token(token: &Alca) -> Result<Vec<(String, u32)>, String> {
    let buf = info_do_token(token.0, TokenGroups)?;
    grupos(&buf).iter().map(|g| Ok((Sid::copia(g.Sid).texto()?, g.Attributes))).collect()
}

/// Um token restrito do processo: sem privilégios, com `desabilitar` só para
/// negar, e acessos checados também contra `restringir`. A DACL padrão dá
/// tudo ao usuário, ao sistema e aos SIDs do Keep, para o que um processo
/// preso cria (pipe, memória compartilhada) abrir para os irmãos dele.
pub fn token_restrito(
    origem: &Alca,
    usuario: &Sid,
    sistema: &Sid,
    restringir: &[&Sid],
    desabilitar: &[&Sid],
    do_keep: &[&Sid],
) -> Result<Alca, String> {
    // Os privilégios são apagados, não só desligados (`DISABLE_MAX_PRIVILEGE`):
    // o token de um administrador tem muitos (backup, restauração, depuração),
    // e um processo religa o que só está desligado. Sem a lista, o desligar.
    let (flags, apagar) = match privilegios_a_apagar(origem) {
        Ok(p) => (0, p),
        Err(_) => (DISABLE_MAX_PRIVILEGE, Vec::new()),
    };
    let novo = token_restrito_cru(origem, flags, desabilitar, &apagar, restringir)
        .map_err(|codigo| falha("CreateRestrictedToken", codigo))?;
    let mut donos: Vec<&Sid> = vec![usuario, sistema];
    donos.extend(do_keep);
    let dacl = monta_dacl(&donos)?;
    let info = TOKEN_DEFAULT_DACL { DefaultDacl: dacl.as_ptr() as *mut ACL };
    let ok = unsafe {
        SetTokenInformation(
            novo.0,
            TokenDefaultDacl,
            (&info as *const TOKEN_DEFAULT_DACL).cast(),
            std::mem::size_of::<TOKEN_DEFAULT_DACL>() as u32,
        )
    };
    if ok == 0 {
        return Err(falha_ultimo("SetTokenInformation(DACL padrão)"));
    }
    // O dono do que o programa preso cria: o usuário. Num token elevado o dono
    // padrão é o grupo dos administradores, que aqui só nega, e o Git recusa um
    // repositório cujo dono não é quem o usa. Se o sistema não aceitar, fica o
    // dono padrão: o programa roda do mesmo jeito.
    let dono = TOKEN_OWNER { Owner: usuario.ptr() };
    unsafe {
        SetTokenInformation(
            novo.0,
            TokenOwner,
            (&dono as *const TOKEN_OWNER).cast(),
            std::mem::size_of::<TOKEN_OWNER>() as u32,
        );
    }
    Ok(novo)
}

/// Os privilégios que o token tem, com os atributos de cada um (1 padrão ligado,
/// 2 ligado, 3... bits de `SE_PRIVILEGE_*`).
fn privilegios(token: &Alca) -> Result<Vec<LUID_AND_ATTRIBUTES>, String> {
    let buf = info_do_token(token.0, TokenPrivileges)?;
    Ok(unsafe {
        let p = buf.as_ptr() as *const TOKEN_PRIVILEGES;
        let n = std::ptr::addr_of!((*p).PrivilegeCount).read() as usize;
        std::slice::from_raw_parts(std::ptr::addr_of!((*p).Privileges) as *const LUID_AND_ATTRIBUTES, n).to_vec()
    })
}

/// Os privilégios que valem apagar do token restrito: todos menos o de
/// atravessar pastas sem permissão em cada uma (`SeChangeNotifyPrivilege`), de
/// que o Windows e os programas dependem.
pub fn privilegios_a_apagar(token: &Alca) -> Result<Vec<LUID_AND_ATTRIBUTES>, String> {
    let nome = larga("SeChangeNotifyPrivilege");
    let mut luid = LUID { LowPart: 0, HighPart: 0 };
    if unsafe { LookupPrivilegeValueW(null(), nome.as_ptr(), &mut luid) } == 0 {
        return Err(falha_ultimo("LookupPrivilegeValue"));
    }
    Ok(privilegios(token)?
        .into_iter()
        .filter(|p| !(p.Luid.LowPart == luid.LowPart && p.Luid.HighPart == luid.HighPart))
        .collect())
}

/// Os privilégios do token, pelo nome, e se estão ligados (para o diagnóstico).
pub fn privilegios_do_token(token: &Alca) -> Result<Vec<(String, bool)>, String> {
    let mut v = Vec::new();
    for p in privilegios(token)? {
        let mut tam = 128u32;
        let mut nome = vec![0u16; tam as usize];
        let texto = if unsafe { LookupPrivilegeNameW(null(), &p.Luid, nome.as_mut_ptr(), &mut tam) } != 0 {
            String::from_utf16_lossy(&nome[..tam as usize])
        } else {
            format!("{}:{}", p.Luid.HighPart, p.Luid.LowPart)
        };
        v.push((texto, p.Attributes & 0x2 != 0));
    }
    Ok(v)
}

/// O token de quem não está elevado: o de um administrador com o UAC ligado
/// (`LUA_TOKEN`: sem privilégios, com o grupo dos administradores só para
/// negar) e integridade média. É o token que o usuário tem de verdade; o do
/// CI, e o de quem abre o terminal "como administrador", é o inteiro.
pub fn token_comum(origem: &Alca) -> Result<Alca, String> {
    let mut novo: HANDLE = null_mut();
    if unsafe { CreateRestrictedToken(origem.0, LUA_TOKEN, 0, null(), 0, null(), 0, null(), &mut novo) } == 0 {
        return Err(falha_ultimo("CreateRestrictedToken(LUA_TOKEN)"));
    }
    let novo = Alca(novo);
    let medio = Sid::de_texto("S-1-16-8192")?;
    let rotulo = TOKEN_MANDATORY_LABEL { Label: SID_AND_ATTRIBUTES { Sid: medio.ptr(), Attributes: GRUPO_INTEGRIDADE } };
    // Se não aceitar, fica a integridade que o LUA_TOKEN deu.
    unsafe {
        SetTokenInformation(
            novo.0,
            TokenIntegrityLevel,
            (&rotulo as *const TOKEN_MANDATORY_LABEL).cast(),
            (std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32) + GetLengthSid(medio.ptr()),
        );
    }
    Ok(novo)
}

/// Roda `f` numa thread que se passa pelo token: os acessos a arquivo dela
/// são checados contra ele, como os do processo que vai nascer com ele.
pub fn como_o_token<R: Send>(token: &Alca, f: impl FnOnce() -> R + Send) -> Result<R, String> {
    let mut imp: HANDLE = null_mut();
    let ok = unsafe { DuplicateTokenEx(token.0, O_QUE_DEIXAR, null(), SecurityImpersonation, TokenImpersonation, &mut imp) };
    if ok == 0 {
        return Err(falha_ultimo("DuplicateTokenEx"));
    }
    let imp = Alca(imp);
    let alca = imp.0 as usize;
    std::thread::scope(|s| {
        s.spawn(move || {
            if unsafe { SetThreadToken(null(), alca as HANDLE) } == 0 {
                return Err(falha_ultimo("SetThreadToken"));
            }
            let r = f();
            unsafe { RevertToSelf() };
            Ok(r)
        })
        .join()
        .unwrap_or_else(|_| Err("a prova do token travou".to_string()))
    })
}

// ------------------------------------------------------------------ permissões de arquivo

fn nao_protegida(sd: PSECURITY_DESCRIPTOR) -> bool {
    let (mut controle, mut revisao) = (0u16, 0u32);
    if unsafe { GetSecurityDescriptorControl(sd, &mut controle, &mut revisao) } == 0 {
        return true;
    }
    controle & SE_DACL_PROTECTED == 0
}

/// A DACL de um caminho e o descritor que a guarda (a DACL aponta para dentro
/// dele; ambos valem até o `Local` cair).
fn dacl_de(caminho: &str) -> Result<(*mut ACL, PSECURITY_DESCRIPTOR, Local), String> {
    let w = larga(caminho);
    let (mut sd, mut dacl): (PSECURITY_DESCRIPTOR, *mut ACL) = (null_mut(), null_mut());
    let r = unsafe {
        GetNamedSecurityInfoW(w.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd)
    };
    if r != 0 {
        return Err(falha(&format!("não foi possível ler as permissões de {caminho}"), r));
    }
    Ok((dacl, sd, Local(sd)))
}

/// Dá `acesso` ao SID em `caminho`. Numa pasta com `heranca`, vale para o que
/// já está nela (o Windows percorre a árvore: a primeira vez numa pasta
/// grande demora) e para o que nascer. A DACL do caminho segue protegida ou
/// herdando, como estava.
pub fn conceder(caminho: &str, sid: &Sid, acesso: Acesso, heranca: bool) -> Result<(), String> {
    let (dacl, sd, _guarda) = dacl_de(caminho)?;
    // Sem DACL o objeto é de todos; pôr uma DACL só com este SID o tiraria de
    // todo o resto, o usuário inclusive.
    if dacl.is_null() {
        return Ok(());
    }
    let protegida = !nao_protegida(sd);
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: acesso.mascara(),
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: if heranca { SUB_CONTAINERS_AND_OBJECTS_INHERIT } else { NO_INHERITANCE },
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.ptr() as PWSTR,
        },
    };
    let mut nova: *mut ACL = null_mut();
    let r = unsafe { SetEntriesInAclW(1, &ea, dacl, &mut nova) };
    if r != 0 {
        return Err(falha("SetEntriesInAcl", r));
    }
    let _nova = Local(nova as *mut c_void);
    let w = larga(caminho);
    let estado = if protegida { PROTECTED_DACL_SECURITY_INFORMATION } else { UNPROTECTED_DACL_SECURITY_INFORMATION };
    let r = unsafe {
        SetNamedSecurityInfoW(w.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | estado, null_mut(), null_mut(), nova, null())
    };
    if r != 0 {
        return Err(falha(&format!("não foi possível dar a permissão em {caminho}"), r));
    }
    Ok(())
}

/// Dá `acesso` ao SID só em `caminho`, sem herança e sem percorrer o que há
/// dentro dele. `conceder` troca a DACL pelo nome, e o Windows então refaz a
/// herança de tudo o que está embaixo: numa pasta de passagem, como a casa do
/// usuário, seria percorrer o perfil inteiro por uma permissão que não é
/// herdada. Pela alça, o sistema troca só a DACL deste objeto.
pub fn conceder_so_aqui(caminho: &str, sid: &Sid, acesso: Acesso) -> Result<(), String> {
    let w = larga(caminho);
    // READ_CONTROL | WRITE_DAC; backup semantics para abrir uma pasta.
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            0x0002_0000 | 0x0004_0000,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(falha_ultimo(&format!("não foi possível abrir {caminho} para dar a permissão")));
    }
    let alca = Alca(h);
    let (mut sd, mut dacl): (PSECURITY_DESCRIPTOR, *mut ACL) = (null_mut(), null_mut());
    let r = unsafe {
        GetSecurityInfo(alca.0, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd)
    };
    if r != 0 {
        return Err(falha(&format!("não foi possível ler as permissões de {caminho}"), r));
    }
    let _guarda = Local(sd);
    // Sem DACL o objeto é de todos.
    if dacl.is_null() {
        return Ok(());
    }
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: acesso.mascara(),
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.ptr() as PWSTR,
        },
    };
    let mut nova: *mut ACL = null_mut();
    let r = unsafe { SetEntriesInAclW(1, &ea, dacl, &mut nova) };
    if r != 0 {
        return Err(falha("SetEntriesInAcl", r));
    }
    let _nova = Local(nova as *mut c_void);
    let estado = if nao_protegida(sd) { UNPROTECTED_DACL_SECURITY_INFORMATION } else { PROTECTED_DACL_SECURITY_INFORMATION };
    let r = unsafe { SetSecurityInfo(alca.0, SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | estado, null_mut(), null_mut(), nova, null()) };
    if r != 0 {
        return Err(falha(&format!("não foi possível dar a permissão em {caminho}"), r));
    }
    Ok(())
}

/// Se o SID já tem, em `caminho`, ao menos `acesso` (e, numa pasta, para o
/// que vier a nascer nela): então não há o que percorrer de novo.
pub fn concedido(caminho: &str, sid: &Sid, acesso: Acesso, heranca: bool) -> Result<bool, String> {
    let (dacl, _sd, _guarda) = dacl_de(caminho)?;
    // Sem DACL o objeto é de todos.
    if dacl.is_null() {
        return Ok(true);
    }
    let mut info = ACL_SIZE_INFORMATION { AceCount: 0, AclBytesInUse: 0, AclBytesFree: 0 };
    let ok = unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return Err(falha_ultimo("GetAclInformation"));
    }
    let mascara = acesso.mascara();
    for i in 0..info.AceCount {
        let mut ace: *mut c_void = null_mut();
        if unsafe { GetAce(dacl, i, &mut ace) } == 0 {
            return Err(falha_ultimo("GetAce"));
        }
        let cabeca = unsafe { &*(ace as *const ACE_HEADER) };
        // Só as que permitem (tipo 0), e que valem para o próprio objeto.
        if cabeca.AceType != 0 || u32::from(cabeca.AceFlags) & INHERIT_ONLY_ACE != 0 {
            continue;
        }
        if heranca && u32::from(cabeca.AceFlags) & SUB_CONTAINERS_AND_OBJECTS_INHERIT != SUB_CONTAINERS_AND_OBJECTS_INHERIT
        {
            continue;
        }
        // O SID começa em `SidStart` e vai além do que a estrutura declara.
        let (mascara_da_ace, sid_da_ace) = unsafe {
            let a = ace as *const ACCESS_ALLOWED_ACE;
            (
                std::ptr::addr_of!((*a).Mask).read(),
                (ace as *mut u8).add(std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart)) as PSID,
            )
        };
        if mascara_da_ace & mascara == mascara && unsafe { EqualSid(sid_da_ace, sid.ptr()) } != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

// ------------------------------------------------------------------ uma trava entre processos

/// Uma exclusão mútua com nome: duas abas que sobem a IA ao mesmo tempo (a
/// restauração depois do reinício) não percorrem a mesma pasta juntas.
pub struct Trava(HANDLE);

pub fn pegar_trava(nome: &str, espera_ms: u32, ao_esperar: impl FnOnce()) -> Result<Trava, String> {
    let w = larga(nome);
    let h = unsafe { CreateMutexW(null(), 0, w.as_ptr()) };
    if h.is_null() {
        return Err(falha_ultimo("CreateMutex"));
    }
    let mut vez = unsafe { WaitForSingleObject(h, 0) };
    if vez == WAIT_TIMEOUT {
        ao_esperar();
        vez = unsafe { WaitForSingleObject(h, espera_ms) };
    }
    match vez {
        // Abandonada: quem a tinha morreu no meio; quem a pega segue.
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Trava(h)),
        WAIT_TIMEOUT => {
            unsafe { CloseHandle(h) };
            Err("outra aba está preparando as permissões e demorou demais".into())
        }
        _ => {
            let e = falha_ultimo("WaitForSingleObject");
            unsafe { CloseHandle(h) };
            Err(e)
        }
    }
}

impl Drop for Trava {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

// ------------------------------------------------------------------ o processo

/// O que o filho recebe como entrada, saída e erro padrão: cópias que ele
/// herda, feitas aqui para não mexer nas alças deste processo.
pub struct Padrao {
    entrada: HANDLE,
    saida: HANDLE,
    erro: HANDLE,
}

fn duplica_herdavel(h: HANDLE) -> HANDLE {
    if h.is_null() || h == INVALID_HANDLE_VALUE {
        return null_mut();
    }
    let mut novo: HANDLE = null_mut();
    let eu = unsafe { GetCurrentProcess() };
    if unsafe { DuplicateHandle(eu, h, eu, &mut novo, 0, 1, DUPLICATE_SAME_ACCESS) } == 0 {
        return null_mut();
    }
    novo
}

impl Padrao {
    /// As deste processo: o console da aba, em geral.
    pub fn do_processo() -> Padrao {
        let pega = |id| duplica_herdavel(unsafe { GetStdHandle(id) });
        Padrao { entrada: pega(STD_INPUT_HANDLE), saida: pega(STD_OUTPUT_HANDLE), erro: pega(STD_ERROR_HANDLE) }
    }

    /// Arquivos dados (nos testes: a saída num arquivo para ler depois).
    pub fn de_arquivos(entrada: Option<&std::fs::File>, saida: Option<&std::fs::File>, erro: Option<&std::fs::File>) -> Padrao {
        let de = |f: Option<&std::fs::File>| f.map(|f| duplica_herdavel(f.as_raw_handle() as HANDLE)).unwrap_or(null_mut());
        Padrao { entrada: de(entrada), saida: de(saida), erro: de(erro) }
    }

    fn alguma(&self) -> bool {
        !(self.entrada.is_null() && self.saida.is_null() && self.erro.is_null())
    }
}

impl Drop for Padrao {
    fn drop(&mut self) {
        for h in [self.entrada, self.saida, self.erro] {
            if !h.is_null() {
                unsafe { CloseHandle(h) };
            }
        }
    }
}

/// Se este processo tem um console (o da aba). Sem ele, o filho que é um
/// programa de console abriria uma janela própria.
pub fn tem_console() -> bool {
    let mut pid = 0u32;
    unsafe { GetConsoleProcessList(&mut pid, 1) != 0 }
}

/// Se o processo está ligado ao console deste: `None` se não dá para saber.
/// É como se confere que o console da aba chegou ao programa preso: um que
/// subiu sem ele não aparece na lista e ficaria mudo.
pub fn no_console(pid: u32) -> Option<bool> {
    let mut lista = vec![0u32; 256];
    let n = unsafe { GetConsoleProcessList(lista.as_mut_ptr(), lista.len() as u32) } as usize;
    if n == 0 || n > lista.len() {
        return None;
    }
    Some(lista[..n].contains(&pid))
}

/// Termina um processo que não serve mais (o que subiu sem o console).
pub fn matar(filho: &Filho) {
    unsafe { TerminateProcess(filho.processo.0, 1) };
}

/// O processo e a primeira thread dele, ainda suspensa.
pub struct Filho {
    pub processo: Alca,
    pub thread: Alca,
    pub pid: u32,
}

/// O programa, suspenso, com o token, o ambiente e as alças dadas. O console
/// vem por herança: sem `CREATE_NEW_CONSOLE` nem `DETACHED_PROCESS`, ele usa o
/// deste processo.
pub fn criar_processo(
    token: &Alca,
    programa: &str,
    linha: &str,
    pasta: &str,
    ambiente: &[u16],
    padrao: &Padrao,
    sem_janela: bool,
) -> Result<Filho, String> {
    let app = larga(programa);
    let mut cmd = larga(linha);
    let cwd = larga(pasta);
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let herda = padrao.alguma();
    if herda {
        si.dwFlags = STARTF_USESTDHANDLES;
        si.hStdInput = padrao.entrada;
        si.hStdOutput = padrao.saida;
        si.hStdError = padrao.erro;
    }
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | if sem_janela { CREATE_NO_WINDOW } else { 0 };
    let ok = unsafe {
        CreateProcessAsUserW(
            token.0,
            app.as_ptr(),
            cmd.as_mut_ptr(),
            null(),
            null(),
            i32::from(herda),
            flags,
            ambiente.as_ptr().cast(),
            cwd.as_ptr(),
            &si,
            &mut pi,
        )
    };
    if ok == 0 {
        return Err(falha_ultimo(&format!(
            "não foi possível iniciar {programa} com o token restrito (linha de comando: {linha}; pasta: {pasta})"
        )));
    }
    Ok(Filho { processo: Alca(pi.hProcess), thread: Alca(pi.hThread), pid: pi.dwProcessId })
}

/// Um Job que mata tudo o que está nele quando este processo (o único que o
/// segura) acaba, e que não deixa o que está nele sair.
pub fn job_que_mata() -> Result<Alca, String> {
    let h = unsafe { CreateJobObjectW(null(), null()) };
    if h.is_null() {
        return Err(falha_ultimo("CreateJobObject"));
    }
    let job = Alca(h);
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        return Err(falha_ultimo("SetInformationJobObject"));
    }
    Ok(job)
}

pub fn no_job(job: &Alca, filho: &Filho) -> Result<(), String> {
    if unsafe { AssignProcessToJobObject(job.0, filho.processo.0) } == 0 {
        return Err(falha_ultimo("AssignProcessToJobObject"));
    }
    Ok(())
}

/// Deixa o processo andar.
pub fn retomar(filho: &Filho) -> Result<(), String> {
    if unsafe { ResumeThread(filho.thread.0) } == u32::MAX {
        let e = falha_ultimo("ResumeThread");
        unsafe { TerminateProcess(filho.processo.0, 1) };
        return Err(e);
    }
    Ok(())
}

/// Espera o processo acabar por até `ms`: o código de saída dele, ou `None`
/// se ainda anda.
pub fn esperar(processo: &Alca, ms: u32) -> Result<Option<u32>, String> {
    match unsafe { WaitForSingleObject(processo.0, ms) } {
        WAIT_OBJECT_0 => {
            let mut codigo = 0u32;
            if unsafe { GetExitCodeProcess(processo.0, &mut codigo) } == 0 {
                return Err(falha_ultimo("GetExitCodeProcess"));
            }
            Ok(Some(codigo))
        }
        WAIT_TIMEOUT => Ok(None),
        _ => Err(falha_ultimo("WaitForSingleObject")),
    }
}

/// Para enquanto o programa preso roda este processo não morrer com o Ctrl+C
/// que a aba manda a todos os que dividem o console: quem o trata é o
/// programa. O "ignorar" do sistema (`SetConsoleCtrlHandler(NULL, TRUE)`) o
/// passaria adiante aos filhos, por isso é uma rotina própria.
pub fn nao_morrer_com_ctrl_c() {
    unsafe extern "system" fn rotina(tipo: u32) -> i32 {
        i32::from(tipo == CTRL_C_EVENT || tipo == CTRL_BREAK_EVENT)
    }
    unsafe { SetConsoleCtrlHandler(Some(rotina), 1) };
}

/// O que o processo preso pode fazer com o próprio token: consultar, copiar
/// e passar-se por ele (`TOKEN_DUPLICATE | TOKEN_IMPERSONATE | TOKEN_QUERY |
/// TOKEN_QUERY_SOURCE` e `READ_CONTROL`). Quase todo programa pergunta ao
/// token quem ele é (o `git`, ao conferir de quem é a pasta; o .NET; o Node).
const O_QUE_O_TOKEN_DEIXA: u32 = 0x0002_001E;

/// Deixa o processo preso abrir o próprio token. O token de um processo só
/// nomeia, na DACL, quem o criou; com a segunda checagem do token restrito,
/// nenhum SID de restrição casaria e o `OpenProcessToken` do próprio processo
/// seria negado. Quem dá a permissão é este processo, que pode (o usuário é o
/// dono) e antes de o filho andar. O que não der é só dito: um token que já
/// se abre, ou que nunca se abrirá, não impede o resto.
pub fn abrir_o_token_ao_filho(filho: &Filho, sid: &Sid) -> Result<(), String> {
    let mut t: HANDLE = null_mut();
    // READ_CONTROL e WRITE_DAC: o que `GetSecurityInfo` e `SetSecurityInfo` pedem.
    if unsafe { OpenProcessToken(filho.processo.0, 0x0002_0000 | 0x0004_0000, &mut t) } == 0 {
        return Err(falha_ultimo("não foi possível abrir o token do filho"));
    }
    let token = Alca(t);
    let (mut sd, mut dacl): (PSECURITY_DESCRIPTOR, *mut ACL) = (null_mut(), null_mut());
    let r = unsafe {
        GetSecurityInfo(token.0, SE_KERNEL_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd)
    };
    if r != 0 {
        return Err(falha("não foi possível ler as permissões do token do filho", r));
    }
    let _guarda = Local(sd);
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: O_QUE_O_TOKEN_DEIXA,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.ptr() as PWSTR,
        },
    };
    let mut nova: *mut ACL = null_mut();
    let r = unsafe { SetEntriesInAclW(1, &ea, dacl, &mut nova) };
    if r != 0 {
        return Err(falha("SetEntriesInAcl (token do filho)", r));
    }
    let _nova = Local(nova as *mut c_void);
    let r = unsafe { SetSecurityInfo(token.0, SE_KERNEL_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), nova, null()) };
    if r != 0 {
        return Err(falha("não foi possível dar a permissão no token do filho", r));
    }
    Ok(())
}

/// Esperar sem prazo.
pub const INFINITO: u32 = INFINITE;
