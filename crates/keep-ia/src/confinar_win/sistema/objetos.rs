//! Os objetos com nome que o MSYS2 e o Cygwin criam (memória compartilhada,
//! exclusões mútuas, eventos) e quem pode abri-los.
//!
//! O MSYS2 (o Git Bash, de que o Claude Code depende para o Bash) guarda o
//! estado do usuário numa memória compartilhada com nome, criada por quem
//! sobe primeiro com uma DACL só do usuário e do sistema. Um token restrito
//! passa por uma segunda checagem, só contra os SIDs de restrição, e o
//! usuário não está entre eles: o `bash` preso não abre o que o `bash` do
//! usuário criou (`CreateFileMapping … Win32 error 5`). Aqui se vê o que há,
//! de quem é, e se dá ao SID base do Keep o acesso a esses objetos.

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{NtOpenDirectoryObject, NtQueryDirectoryObject};
use windows_sys::Win32::Foundation::{GetLastError, HANDLE, UNICODE_STRING};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo,
    NO_MULTIPLE_TRUSTEE, SE_KERNEL_OBJECT, SetEntriesInAclW, SetSecurityInfo, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::{ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
use windows_sys::Win32::System::Memory::OpenFileMappingW;
use windows_sys::Win32::System::Threading::{OpenEventW, OpenMutexW, OpenSemaphoreW};
use windows_sys::core::PWSTR;

use super::{Alca, Local, Sid, falha, larga};

const READ_CONTROL: u32 = 0x0002_0000;
const WRITE_DAC: u32 = 0x0004_0000;
/// `GENERIC_ALL`: cada tipo de objeto traduz para o todo dele.
const TUDO: u32 = 0x1000_0000;

/// O que o NT guarda num diretório de objetos: o nome e o tipo de cada um.
#[repr(C)]
struct InfoDeDiretorio {
    nome: UNICODE_STRING,
    tipo: UNICODE_STRING,
}

fn texto(u: &UNICODE_STRING) -> String {
    if u.Buffer.is_null() || u.Length == 0 {
        return String::new();
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(u.Buffer, usize::from(u.Length) / 2) })
}

/// O que há num diretório de objetos do NT (`\BaseNamedObjects`,
/// `\Sessions\1\BaseNamedObjects`): o nome e o tipo de cada coisa.
pub fn lista(diretorio: &str) -> Result<Vec<(String, String)>, String> {
    let mut nome: Vec<u16> = diretorio.encode_utf16().collect();
    let bytes = (nome.len() * 2) as u16;
    let us = UNICODE_STRING { Length: bytes, MaximumLength: bytes, Buffer: nome.as_mut_ptr() };
    let oa = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: null_mut(),
        ObjectName: &us,
        Attributes: 0x40, // OBJ_CASE_INSENSITIVE
        SecurityDescriptor: null(),
        SecurityQualityOfService: null(),
    };
    let mut h: HANDLE = null_mut();
    // DIRECTORY_QUERY | DIRECTORY_TRAVERSE
    let st = unsafe { NtOpenDirectoryObject(&mut h, 0x1 | 0x2, &oa) };
    if st < 0 {
        return Err(format!("NtOpenDirectoryObject({diretorio}): 0x{:08X}", st as u32));
    }
    let dir = Alca(h);
    let mut saida = Vec::new();
    let mut contexto = 0u32;
    let mut recomeca = true;
    loop {
        // 128 KiB em palavras de 8 bytes: o alinhamento que a estrutura pede.
        let mut buf = vec![0u64; 16 * 1024];
        let mut devolvido = 0u32;
        let st = unsafe {
            NtQueryDirectoryObject(dir.0, buf.as_mut_ptr().cast(), (buf.len() * 8) as u32, false, recomeca, &mut contexto, &mut devolvido)
        };
        recomeca = false;
        let codigo = st as u32;
        if codigo == 0x8000_001A {
            break; // STATUS_NO_MORE_ENTRIES
        }
        if st < 0 {
            return Err(format!("NtQueryDirectoryObject({diretorio}): 0x{codigo:08X}"));
        }
        let mut p = buf.as_ptr() as *const InfoDeDiretorio;
        loop {
            let e = unsafe { &*p };
            if e.nome.Buffer.is_null() {
                break;
            }
            saida.push((texto(&e.nome), texto(&e.tipo)));
            p = unsafe { p.add(1) };
        }
        if codigo != 0x0000_0105 {
            break; // diferente de STATUS_MORE_ENTRIES: era tudo
        }
    }
    Ok(saida)
}

/// Abre um objeto com nome pelo tipo dele. `nome` já leva `Global\` quando é
/// da área global.
fn abre(tipo: &str, nome: &str, acesso: u32) -> Option<Alca> {
    let w = larga(nome);
    let h = unsafe {
        match tipo {
            "Section" => OpenFileMappingW(acesso, 0, w.as_ptr()),
            "Mutant" => OpenMutexW(acesso, 0, w.as_ptr()),
            "Event" => OpenEventW(acesso, 0, w.as_ptr()),
            "Semaphore" => OpenSemaphoreW(acesso, 0, w.as_ptr()),
            _ => null_mut(),
        }
    };
    (!h.is_null()).then_some(Alca(h))
}

/// O dono e a DACL de um objeto, no texto SDDL.
fn sddl(objeto: &Alca) -> Result<String, String> {
    let (mut sd, mut dono, mut dacl): (PSECURITY_DESCRIPTOR, *mut c_void, *mut ACL) = (null_mut(), null_mut(), null_mut());
    let r = unsafe {
        GetSecurityInfo(
            objeto.0,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut dono,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut sd,
        )
    };
    if r != 0 {
        return Err(falha("GetSecurityInfo", r));
    }
    let _guarda = Local(sd);
    let mut p: PWSTR = null_mut();
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, 1, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, &mut p, null_mut())
    };
    if ok == 0 {
        return Err(falha("ConvertSecurityDescriptorToStringSecurityDescriptor", unsafe { GetLastError() }));
    }
    let _texto = Local(p as *mut c_void);
    let mut n = 0usize;
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    Ok(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) }))
}

/// Acrescenta à DACL do objeto uma permissão total ao SID.
fn concede(objeto: &Alca, sid: &Sid) -> Result<(), String> {
    let (mut sd, mut dacl): (PSECURITY_DESCRIPTOR, *mut ACL) = (null_mut(), null_mut());
    let r = unsafe {
        GetSecurityInfo(objeto.0, SE_KERNEL_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd)
    };
    if r != 0 {
        return Err(falha("GetSecurityInfo", r));
    }
    let _guarda = Local(sd);
    let ea = EXPLICIT_ACCESS_W {
        grfAccessPermissions: TUDO,
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
    let r = unsafe { SetSecurityInfo(objeto.0, SE_KERNEL_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), nova, null()) };
    if r != 0 {
        return Err(falha("SetSecurityInfo", r));
    }
    Ok(())
}

/// Os objetos do MSYS2/Cygwin (e os que levam o SID do usuário no nome) nas
/// áreas de nomes global e das sessões: onde, o tipo, e de quem. Com `base`,
/// dá a ele o acesso a cada um. Devolve quantos achou.
pub fn mostra(usuario: &str, base: Option<&Sid>) -> usize {
    let mut diretorios: Vec<(String, &'static str)> = vec![("\\BaseNamedObjects".to_string(), "Global\\")];
    match lista("\\Sessions") {
        Ok(itens) => {
            for (nome, tipo) in itens {
                if tipo == "Directory" && !nome.is_empty() && nome.chars().all(|c| c.is_ascii_digit()) {
                    diretorios.push((format!("\\Sessions\\{nome}\\BaseNamedObjects"), ""));
                }
            }
        }
        Err(e) => println!("  não listou \\Sessions: {e}"),
    }
    let usuario = usuario.to_lowercase();
    let mut achados = 0usize;
    for (dir, prefixo) in diretorios {
        let itens = match lista(&dir) {
            Ok(i) => i,
            Err(e) => {
                println!("  {dir}: {e}");
                continue;
            }
        };
        println!("  {dir}: {} objeto(s)", itens.len());
        for (nome, tipo) in itens {
            let baixo = nome.to_lowercase();
            if !(baixo.contains("msys") || baixo.contains("cyg") || (!usuario.is_empty() && baixo.starts_with(&usuario))) {
                continue;
            }
            achados += 1;
            println!("    {nome}  [{tipo}]");
            let completo = format!("{prefixo}{nome}");
            match abre(&tipo, &completo, READ_CONTROL) {
                Some(h) => match sddl(&h) {
                    Ok(t) => println!("      {t}"),
                    Err(e) => println!("      não leu a DACL: {e}"),
                },
                None => println!("      não abriu para ler a DACL (erro {})", unsafe { GetLastError() }),
            }
            if let Some(sid) = base {
                match abre(&tipo, &completo, READ_CONTROL | WRITE_DAC) {
                    Some(h) => match concede(&h, sid) {
                        Ok(()) => println!("      acesso dado ao SID base do Keep"),
                        Err(e) => println!("      não deu o acesso: {e}"),
                    },
                    None => println!("      não abriu para mudar a DACL (erro {})", unsafe { GetLastError() }),
                }
            }
        }
    }
    achados
}
