//! `preparar`, mover e `concluir`: o que acontece com cada worktree que o
//! diálogo mostrou e a pessoa aceitou.
//!
//! Preparar confere (worktree ligada, destravada, ninguém com a pasta atual
//! lá dentro) e guarda os commits: `refs/keep-lixeira/<id>` = HEAD, e
//! `refs/keep-lixeira/<id>-sujo` = `git stash create` quando há alteração
//! rastreada. Depois o app move a pasta para a Lixeira (o macOS pelo
//! `FileManager.trashItem`, que grava o "Colocar de volta"; o Windows por
//! `mover_para_lixeira`). Concluir tira o registro da worktree do repositório
//! (`git worktree remove` só daquela; nunca `prune`), para o ramo e o nome
//! ficarem livres — menos em repositório com submódulos, cujo registro guarda
//! os repositórios deles.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::ambiente::Ambiente;
use super::estado::{self, grava_json};
use super::git::git;
use super::listar::na_lixeira;
use super::pastas::{absoluto, base, canon, e_pasta, junta, mesmo, nascimento_ns, normaliza, outra_maquina, pai};
use super::prazo::Estourou;
use super::repos::{comum_de, nascimento_do_registro_em, repo_de};
use super::texto::{agora, iso};
use super::varredura;
use super::{DIR_PENDENTES, LOG_LIXEIRA, Conclusao, Preparo, ProcessoDentro};

/// O tempo de cada git daqui (o `preparar` e o `concluir` não têm pressa).
const PRAZO_GIT: f64 = 20.0;

fn corta(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

fn arquivo_pendente(amb: &Ambiente, caminho: &str) -> PathBuf {
    let h = Sha256::digest(normaliza(caminho).as_bytes());
    let nome: String = h.iter().take(8).map(|b| format!("{b:02x}")).collect();
    amb.estado.join(DIR_PENDENTES).join(format!("{nome}.json"))
}

/// Os processos com a pasta atual dentro do caminho (fora este).
pub fn processos_dentro(cam: &str) -> Vec<ProcessoDentro> {
    let eu = std::process::id();
    let mut v: Vec<ProcessoDentro> = varredura::crua()
        .procs
        .into_iter()
        .filter(|p| p.pid != eu && super::pastas::dentro(&p.cwd, cam))
        .map(|p| ProcessoDentro { pid: p.pid, nome: p.nome })
        .collect();
    v.sort_by_key(|p| p.pid);
    v
}

fn recusa(motivo: impl Into<String>) -> Preparo {
    Preparo { versao: crate::VERSAO, ok: false, motivo: Some(motivo.into()), processos: Some(Vec::new()), ..Preparo::default() }
}

fn ref_existe(amb: &Ambiente, cam: &str, r: &str) -> Result<bool, Estourou> {
    Ok(git(&amb.git, &["-C", cam, "show-ref", "--verify", "--quiet", r], PRAZO_GIT)?.ok())
}

/// Confere a worktree e guarda os commits dela antes de ela sair do lugar.
/// `nasceu`: o nascimento que o `listar` mostrou — outra worktree criada no
/// mesmo caminho enquanto a pergunta estava aberta tem outro, e o
/// consentimento não foi para ela.
pub fn preparar(amb: &Ambiente, caminho: &str, nasceu: Option<i64>) -> Preparo {
    match preparar_(amb, caminho, nasceu) {
        Ok(p) => p,
        Err(Estourou) => recusa("o git não respondeu a tempo"),
    }
}

fn preparar_(amb: &Ambiente, caminho: &str, nasceu: Option<i64>) -> Result<Preparo, Estourou> {
    let cam = absoluto(caminho);
    if outra_maquina(&cam) {
        return Ok(recusa("caminho de outra máquina"));
    }
    if !e_pasta(&cam) {
        return Ok(recusa("caminho inexistente"));
    }
    let cam = canon(&cam);
    if na_lixeira(amb, &cam) {
        return Ok(recusa("já está na Lixeira"));
    }
    let s = git(
        &amb.git,
        &["-C", &cam, "rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir", "--show-toplevel"],
        PRAZO_GIT,
    )?;
    let linhas: Vec<&str> = s.saida.lines().collect();
    if !s.ok() || linhas.len() < 3 {
        return Ok(recusa(format!("não é uma worktree git ({})", corta(&s.erro, 200))));
    }
    let (gd, cd, top) = (canon(linhas[0]), canon(linhas[1]), canon(linhas[2]));
    if mesmo(&gd, &cd) {
        return Ok(recusa("é a worktree principal do repositório: nunca vai para a Lixeira"));
    }
    if !mesmo(&top, &cam) {
        return Ok(recusa(format!("não é a raiz da worktree (a raiz é {top})")));
    }
    if let Some(n) = nasceu {
        let agora_ns = nascimento_do_registro_em(&gd).map(nascimento_ns);
        if agora_ns != Some(n) {
            return Ok(recusa("a worktree neste caminho não é a que foi listada (recriada depois da pergunta)"));
        }
    }
    match std::fs::read_to_string(junta(&gd, "locked")) {
        Ok(razao) => {
            let razao = razao.trim();
            return Ok(recusa(if razao.is_empty() { "travada".to_string() } else { format!("travada: {razao}") }));
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Ok(recusa("travada")),
        Err(_) => {}
    }
    let procs = processos_dentro(&cam);
    if !procs.is_empty() {
        let mut r = recusa(format!("{} processo(s) vivo(s) com a pasta atual dentro da worktree", procs.len()));
        r.processos = Some(procs);
        return Ok(r);
    }
    let wid = base(&gd);
    let repo = repo_de(&cd);
    let s = git(&amb.git, &["-C", &cam, "rev-parse", "--verify", "HEAD^{commit}"], PRAZO_GIT)?;
    if !s.ok() {
        return Ok(recusa(format!("HEAD sem commit ({})", corta(&s.erro, 200))));
    }
    let head = s.saida.trim().to_string();
    let s = git(&amb.git, &["-C", &cam, "symbolic-ref", "-q", "--short", "HEAD"], PRAZO_GIT)?;
    let ramo = if s.ok() { Some(s.saida.trim().to_string()).filter(|r| !r.is_empty()) } else { None };
    let base_ref = format!("refs/keep-lixeira/{wid}");
    let (mut r, mut k) = (base_ref.clone(), 0u32);
    while ref_existe(amb, &cam, &r)? || ref_existe(amb, &cam, &format!("{r}-sujo"))? {
        k += 1;
        let extra = if k == 1 { String::new() } else { format!("-{k}") };
        r = format!("{base_ref}-{}{extra}", agora() as i64);
    }
    let msg = format!("keep: {cam} -> lixeira");
    let s = git(&amb.git, &["-C", &cam, "update-ref", "--create-reflog", "-m", &msg, &r, &head, ""], PRAZO_GIT)?;
    if !s.ok() {
        return Ok(recusa(format!("não consegui gravar {r} ({})", corta(&s.erro, 200))));
    }
    let mut ref_sujo = None;
    let s = git(&amb.git, &["-C", &cam, "status", "--porcelain", "--untracked-files=no"], PRAZO_GIT)?;
    if s.ok() && !s.saida.trim().is_empty() {
        let st = git(&amb.git, &["-C", &cam, "stash", "create", &msg], PRAZO_GIT)?;
        let sha = st.saida.trim().to_string();
        if !st.ok() || sha.is_empty() {
            return Ok(recusa(format!("não consegui guardar as alterações (git stash create: {})", corta(&st.erro, 200))));
        }
        let sujo = format!("{r}-sujo");
        let s = git(&amb.git, &["-C", &cam, "update-ref", "--create-reflog", "-m", &msg, &sujo, &sha, ""], PRAZO_GIT)?;
        if !s.ok() {
            return Ok(recusa(format!("não consegui gravar {sujo} ({})", corta(&s.erro, 200))));
        }
        ref_sujo = Some(sujo);
    }
    let curto: String = head.chars().take(9).collect();
    let registro = json!({
        "caminho": cam, "repo": repo, "comum": cd, "id": wid, "ramo": ramo, "head": curto, "sha": head,
        "ref": r, "ref_sujo": ref_sujo, "preparado_em": iso(agora()),
    });
    let gravou = serde_json::to_vec_pretty(&registro)
        .map_err(std::io::Error::other)
        .and_then(|b| grava_json(&arquivo_pendente(amb, &cam), &b));
    if let Err(e) = gravou {
        return Ok(recusa(format!("não consegui anotar a worktree ({e})")));
    }
    Ok(Preparo {
        versao: crate::VERSAO,
        ok: true,
        motivo: None,
        processos: None,
        caminho: Some(cam),
        repo: Some(repo),
        id: Some(wid),
        r#ref: Some(r),
        ref_sujo: Some(ref_sujo),
        ramo: Some(ramo),
        head: Some(curto),
    })
}

/// O registro em `<comum>/worktrees/<id>` ainda aponta para `cam` (ou para
/// o mesmo nome numa pasta que é a mesma).
fn aponta_para(adm: &str, cam: &str) -> bool {
    let Ok(gd) = std::fs::read_to_string(junta(adm, "gitdir")) else { return false };
    let alvo = pai(&normaliza(gd.trim()));
    if mesmo(&alvo, cam) {
        return true;
    }
    let (pa, pc) = (pai(&alvo), pai(cam));
    base(&alvo) == base(cam) && e_pasta(&pa) && e_pasta(&pc) && mesmo(&canon(&pa), &canon(&pc))
}

fn falha(motivo: impl Into<String>) -> Conclusao {
    Conclusao { versao: crate::VERSAO, ok: false, motivo: Some(motivo.into()), ..Conclusao::default() }
}

/// Depois que a pasta saiu do lugar: tira o registro dela do repositório.
pub fn concluir(amb: &Ambiente, caminho: &str, destino: &str) -> Conclusao {
    match concluir_(amb, &normaliza(&absoluto(caminho)), &absoluto(destino)) {
        Ok(c) => c,
        Err(Estourou) => falha("o git não respondeu a tempo"),
    }
}

fn concluir_(amb: &Ambiente, cam: &str, destino: &str) -> Result<Conclusao, Estourou> {
    if outra_maquina(cam) {
        return Ok(falha("caminho de outra máquina"));
    }
    if std::fs::symlink_metadata(cam).is_ok() {
        return Ok(falha("o caminho ainda existe: a worktree não saiu de lá"));
    }
    let le = |p: PathBuf| std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).filter(Value::is_object);
    let texto = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let mut reg = le(arquivo_pendente(amb, cam)).filter(|r| texto(r, "caminho").is_some_and(|c| mesmo(&normaliza(&c), cam)));
    if reg.is_none() {
        let p = pai(cam);
        if e_pasta(&p) {
            reg = le(arquivo_pendente(amb, &junta(&canon(&p), &base(cam))));
        }
    }
    let reg = match reg {
        Some(r) => r,
        None => {
            // Sem o registro do preparar: o `.git` do destino ainda aponta o
            // registro da worktree.
            let Some(c) = (if e_pasta(destino) { comum_de(destino) } else { None }) else {
                return Ok(falha("não sei de que repositório é (sem o registro do preparar e sem .git no destino)"));
            };
            let gd = std::fs::read_to_string(junta(destino, ".git")).unwrap_or_default();
            let gd = gd.strip_prefix("gitdir:").unwrap_or("").trim().trim_end_matches(['/', '\\']).to_string();
            json!({"caminho": cam, "repo": repo_de(&c), "comum": c, "id": base(&gd),
                   "ramo": null, "head": null, "ref": null, "ref_sujo": null})
        }
    };
    let comum = texto(&reg, "comum").unwrap_or_default();
    let repo = texto(&reg, "repo").unwrap_or_default();
    let id = texto(&reg, "id").unwrap_or_default();
    let adm = junta(&junta(&comum, "worktrees"), &id);
    let submod = Path::new(destino).join(".gitmodules").exists() || Path::new(&repo).join(".gitmodules").exists();
    let (mut ok, mut nota, mut removido) = (true, None::<String>, false);
    if !e_pasta(&adm) {
        nota = Some("o registro da worktree já não existe".into());
    } else if !aponta_para(&adm, cam) {
        ok = false;
        nota = Some(format!("o registro {adm} não aponta para {cam}: não mexi"));
    } else if Path::new(&adm).join("locked").exists() {
        ok = false;
        nota = Some("a worktree foi travada depois do preparar: registro mantido".into());
    } else if submod {
        nota = Some(
            "repositório com submódulos: registro mantido (removê-lo apagaria os repositórios dos submódulos)".into(),
        );
    } else {
        let git_dir = format!("--git-dir={comum}");
        let mut args: Vec<&str> =
            if Path::new(&repo).join(".git").is_dir() { vec!["-C", &repo] } else { vec![&git_dir] };
        args.extend(["worktree", "remove", cam]);
        let s = git(&amb.git, &args, PRAZO_GIT)?;
        if s.ok() && !e_pasta(&adm) {
            removido = true;
        } else {
            ok = false;
            nota = Some(format!("git worktree remove falhou: {}", corta(&s.erro, 300)));
        }
    }
    let campo = |k: &str| reg.get(k).cloned().unwrap_or(Value::Null);
    let mut linha = json!({
        "ts": iso(agora()), "caminho": cam, "destino": destino, "repo": repo, "id": id,
        "ramo": campo("ramo"), "head": campo("head"), "ref": campo("ref"), "ref_sujo": campo("ref_sujo"),
        "registro_removido": removido,
    });
    if let Some(n) = &nota {
        linha["nota"] = json!(n);
    }
    estado::acrescenta(amb, LOG_LIXEIRA, &linha).ok();
    if ok {
        std::fs::remove_file(arquivo_pendente(amb, cam)).ok();
    }
    let opcional = |k: &str| Some(reg.get(k).and_then(Value::as_str).map(str::to_string));
    Ok(Conclusao {
        versao: crate::VERSAO,
        ok,
        caminho: Some(cam.to_string()),
        destino: Some(destino.to_string()),
        repo: Some(repo),
        id: Some(id),
        ramo: opcional("ramo"),
        head: opcional("head"),
        r#ref: opcional("ref"),
        ref_sujo: opcional("ref_sujo"),
        registro_removido: Some(removido),
        motivo: if ok { None } else { nota.clone() },
        nota,
    })
}

/// Move a pasta para a Lixeira do sistema e diz onde ela ficou (vazio se o
/// sistema não diz). No macOS, pelo `NSFileManager` (o "Colocar de volta" é
/// gravado depois, pelo processo que moveu: um processo que sai logo em
/// seguida pode perdê-lo — o app do macOS move por conta própria); no
/// Windows, pela Lixeira do Explorer, com aviso se ela não couber (o Windows
/// apagaria de vez); nos demais, pela Lixeira do freedesktop.
pub fn mover_para_lixeira(amb: &Ambiente, caminho: &Path) -> anyhow::Result<PathBuf> {
    if !caminho.exists() {
        anyhow::bail!("{} não existe", caminho.display());
    }
    if let Some(l) = &amb.ganchos.lixeira_falsa {
        estado::cria_pasta(l)?;
        let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "item".into());
        let mut destino = l.join(&nome);
        let mut n = 1;
        while destino.exists() {
            n += 1;
            destino = l.join(format!("{nome} {n}"));
        }
        std::fs::rename(caminho, &destino)?;
        return Ok(destino);
    }
    sistema::para_lixeira(caminho)
}

#[cfg(target_os = "macos")]
mod sistema {
    use std::ffi::{CStr, CString, c_void};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    type Id = *mut c_void;
    type Sel = *mut c_void;

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(nome: *const libc::c_char) -> Id;
        fn sel_registerName(nome: *const libc::c_char) -> Sel;
        fn objc_msgSend();
        fn objc_autoreleasePoolPush() -> *mut c_void;
        fn objc_autoreleasePoolPop(p: *mut c_void);
    }

    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    fn classe(n: &str) -> Id {
        let c = CString::new(n).unwrap();
        unsafe { objc_getClass(c.as_ptr()) }
    }

    fn seletor(n: &str) -> Sel {
        let c = CString::new(n).unwrap();
        unsafe { sel_registerName(c.as_ptr()) }
    }

    unsafe fn manda0(o: Id, s: &str) -> Id {
        let f: unsafe extern "C" fn(Id, Sel) -> Id = unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        unsafe { f(o, seletor(s)) }
    }

    unsafe fn manda1(o: Id, s: &str, a: *const c_void) -> Id {
        let f: unsafe extern "C" fn(Id, Sel, *const c_void) -> Id =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        unsafe { f(o, seletor(s), a) }
    }

    unsafe fn texto(ns: Id) -> String {
        if ns.is_null() {
            return String::new();
        }
        let c = unsafe { manda0(ns, "UTF8String") } as *const libc::c_char;
        if c.is_null() { String::new() } else { unsafe { CStr::from_ptr(c) }.to_string_lossy().into_owned() }
    }

    pub fn para_lixeira(caminho: &Path) -> anyhow::Result<PathBuf> {
        let c = CString::new(caminho.as_os_str().as_bytes())?;
        unsafe {
            let pool = objc_autoreleasePoolPush();
            let r = (|| {
                let (gerente, nsstring, nsurl) = (classe("NSFileManager"), classe("NSString"), classe("NSURL"));
                if gerente.is_null() || nsstring.is_null() || nsurl.is_null() {
                    anyhow::bail!("Foundation indisponível");
                }
                let fm = manda0(gerente, "defaultManager");
                let s = manda1(nsstring, "stringWithUTF8String:", c.as_ptr().cast());
                let url = manda1(nsurl, "fileURLWithPath:", s as *const c_void);
                let (mut novo, mut erro): (Id, Id) = (std::ptr::null_mut(), std::ptr::null_mut());
                let f: unsafe extern "C" fn(Id, Sel, Id, *mut Id, *mut Id) -> i8 =
                    std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
                let ok = f(fm, seletor("trashItemAtURL:resultingItemURL:error:"), url, &mut novo, &mut erro);
                if ok == 0 {
                    let porque = if erro.is_null() { String::new() } else { texto(manda0(erro, "localizedDescription")) };
                    anyhow::bail!("não deu para mover para a Lixeira: {porque}");
                }
                Ok(if novo.is_null() { PathBuf::new() } else { PathBuf::from(texto(manda0(novo, "path"))) })
            })();
            objc_autoreleasePoolPop(pool);
            r
        }
    }
}

#[cfg(windows)]
mod sistema {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW,
        SHFileOperationW,
    };

    pub fn para_lixeira(caminho: &Path) -> anyhow::Result<PathBuf> {
        let caminho = std::path::absolute(caminho)?;
        // O nome que a Lixeira grava é o longo: um caminho em 8.3
        // (`C:\Users\RUNNER~1\…`, como o TEMP costuma vir) é procurado pelos dois.
        let longo = std::fs::canonicalize(&caminho).ok().map(|p| {
            let t = p.to_string_lossy().into_owned();
            PathBuf::from(t.strip_prefix(r"\\?\").unwrap_or(&t))
        });
        let mut de: Vec<u16> = caminho.as_os_str().encode_wide().collect();
        de.extend([0, 0]);
        let desde = SystemTime::now();
        let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        op.wFunc = FO_DELETE as _;
        op.pFrom = de.as_ptr();
        // Sem perguntas, menos uma: se não couber na Lixeira, o Windows avisa
        // em vez de apagar de vez.
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as _;
        let (r, abortou) = unsafe {
            let com = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let r = SHFileOperationW(&mut op);
            if com >= 0 {
                CoUninitialize();
            }
            (r, op.fAnyOperationsAborted != 0)
        };
        if r != 0 {
            anyhow::bail!("o Windows não moveu {} para a Lixeira (código {r:#x})", caminho.display());
        }
        if abortou || caminho.exists() {
            anyhow::bail!("{} continua no lugar", caminho.display());
        }
        let nomes: Vec<&Path> = std::iter::once(caminho.as_path()).chain(longo.as_deref()).collect();
        Ok(onde_ficou(&nomes, desde).unwrap_or_default())
    }

    /// `<unidade>\$Recycle.Bin\<SID>\$I<x>` guarda o caminho de antes; o item
    /// é o `$R<x>` ao lado. O mais recente com um dos nomes dados.
    fn onde_ficou(nomes: &[&Path], desde: SystemTime) -> Option<PathBuf> {
        let dobra = |p: &str| p.replace('/', "\\").trim_end_matches('\\').to_lowercase();
        let procurados: Vec<String> = nomes.iter().map(|p| dobra(&p.to_string_lossy())).collect();
        let raiz: PathBuf =
            nomes.first()?.components().take_while(|c| !matches!(c, std::path::Component::Normal(_))).collect();
        let cedo = desde.checked_sub(Duration::from_secs(5)).unwrap_or(desde);
        let mut melhor: Option<(SystemTime, PathBuf)> = None;
        for dono in std::fs::read_dir(raiz.join("$Recycle.Bin")).ok()?.flatten() {
            let Ok(itens) = std::fs::read_dir(dono.path()) else { continue };
            for e in itens.flatten() {
                let nome = e.file_name().to_string_lossy().into_owned();
                let Some(resto) = nome.strip_prefix("$I") else { continue };
                let Some(quando) = e.metadata().ok().and_then(|m| m.modified().ok()) else { continue };
                if quando < cedo {
                    continue;
                }
                let Some(de) = std::fs::read(e.path()).ok().and_then(|b| caminho_gravado(&b)) else { continue };
                if !procurados.contains(&dobra(&de)) {
                    continue;
                }
                let item = e.path().with_file_name(format!("$R{resto}"));
                if item.exists() && melhor.as_ref().is_none_or(|(t, _)| quando > *t) {
                    melhor = Some((quando, item));
                }
            }
        }
        melhor.map(|(_, p)| p)
    }

    /// O caminho que um `$I` grava: a versão 2 (Windows 10 em diante) tem o
    /// tamanho antes; a 1, 260 letras fixas.
    fn caminho_gravado(b: &[u8]) -> Option<String> {
        if b.len() < 28 {
            return None;
        }
        let versao = i64::from_le_bytes(b[0..8].try_into().ok()?);
        let (ini, n) = match versao {
            2 => (28, u32::from_le_bytes(b[24..28].try_into().ok()?) as usize),
            1 => (24, 260),
            _ => return None,
        };
        let w: Vec<u16> = b
            .get(ini..)?
            .chunks_exact(2)
            .take(n)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        (!w.is_empty()).then(|| String::from_utf16_lossy(&w))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod sistema {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    /// A Lixeira do freedesktop: `files/<nome>` e `info/<nome>.trashinfo`.
    pub fn para_lixeira(caminho: &Path) -> anyhow::Result<PathBuf> {
        let caminho = std::path::absolute(caminho)?;
        let dados = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::caminhos::casa().join(".local").join("share"));
        let (arquivos, info) = (dados.join("Trash").join("files"), dados.join("Trash").join("info"));
        super::estado::cria_pasta(&arquivos)?;
        super::estado::cria_pasta(&info)?;
        let nome = caminho.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "item".into());
        for n in 1..10_000 {
            let este = if n == 1 { nome.clone() } else { format!("{nome}.{n}") };
            let ficha = info.join(format!("{este}.trashinfo"));
            let destino = arquivos.join(&este);
            if destino.exists() {
                continue;
            }
            let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&ficha) else { continue };
            let url: String = caminho
                .as_os_str()
                .as_bytes()
                .iter()
                .map(|&b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
                    _ => format!("%{b:02X}"),
                })
                .collect();
            let hora = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
            f.write_all(format!("[Trash Info]\nPath={url}\nDeletionDate={hora}\n").as_bytes())?;
            drop(f);
            if let Err(e) = std::fs::rename(&caminho, &destino) {
                std::fs::remove_file(&ficha).ok();
                anyhow::bail!("não deu para mover para a Lixeira: {e}");
            }
            return Ok(destino);
        }
        anyhow::bail!("a Lixeira não tem nome livre para {nome}")
    }
}
