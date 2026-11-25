//! A memória do computador, para o Keep não a estourar (pedido de 25/11/2026).
//!
//! Cada conversa do Claude viva pesa centenas de MB; abertas demais, levam o swap ao limite, os comandos a levar
//! segundos e o sistema a derrubar serviços. O Keep mede a memória e:
//!
//! - hiberna as conversas paradas quando ela aperta (`adormecer`, a rodada do app), a parada há mais tempo primeiro,
//!   uma por vez, medindo de novo entre elas;
//! - segura as abas novas (`keep nova`, `keep geral iniciar`): sem memória, hiberna as paradas e, se ainda faltar, o
//!   pedido vai para a fila (`fila`), que abre pela ordem quando houver;
//! - não passa de um teto de conversas vivas, pela RAM: (RAM − 3 GB) / 450 MB (11 num Mac de 8 GB).
//!
//! A leitura: no macOS, o nível de pressão do kernel (`kern.memorystatus_vm_pressure_level`: 1 normal, 2 alerta,
//! 4 crítica), a memória disponível (livre, inativa, purgeável e especulativa, de `host_statistics64`, a fonte do medidor
//! "Memória RAM" do app) e o swap (`vm.swapusage`); no Windows, `GlobalMemoryStatusEx` (a física disponível).
//! `KEEP_MEMORIA_TESTE` (só nos testes) troca a leitura por um arquivo.

use serde::Serialize;
use serde_json::{Value, json};

use keep_ia::daemon;

/// O que fica para o sistema e os outros programas, fora das conversas.
const RESERVA: u64 = 3 << 30;
/// O que pesa uma conversa viva do Claude, para o teto.
const POR_CONVERSA: u64 = 450 << 20;
/// Abaixo disto da RAM disponível, a memória está apertada.
const DISPONIVEL_MINIMA_PCT: u64 = 15;

/// Uma leitura da memória, em bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Leitura {
    pub total: u64,
    pub disponivel: u64,
    pub swap_usado: u64,
    pub swap_total: u64,
    /// O nível de pressão do kernel do macOS: 1 normal, 2 alerta, 4 crítica; 0 onde o sistema não diz (Windows).
    pub pressao: u32,
}

/// "0,3 GB".
pub fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64).replace('.', ",")
}

impl Leitura {
    /// Por que a memória está apertada, em palavras; `None` se não está. Pressão em alerta (ou crítica), menos de 15%
    /// da RAM disponível, ou o swap passando da metade.
    pub fn aperto(&self) -> Option<String> {
        if self.pressao >= 4 {
            return Some("pressão de memória crítica".into());
        }
        if self.pressao >= 2 {
            return Some("pressão de memória em alerta".into());
        }
        if self.total > 0 && self.disponivel * 100 < self.total * DISPONIVEL_MINIMA_PCT {
            return Some(format!("só {} disponíveis de {}", gb(self.disponivel), gb(self.total)));
        }
        if self.swap_total > 0 && self.swap_usado * 2 > self.swap_total {
            return Some(format!("swap em {} de {}", gb(self.swap_usado), gb(self.swap_total)));
        }
        None
    }

    /// Quantas conversas vivas cabem nesta RAM: (RAM − 3 GB) / 450 MB, no mínimo uma.
    pub fn teto(&self) -> usize {
        (self.total.saturating_sub(RESERVA) / POR_CONVERSA).max(1) as usize
    }
}

/// A memória de agora: a leitura de teste, a do sistema, ou `None` (sem leitura nada é segurado).
pub fn ler() -> Option<Leitura> {
    match std::env::var_os("KEEP_MEMORIA_TESTE") {
        Some(arq) if !arq.is_empty() => de_teste(std::path::Path::new(&arq)),
        _ => do_sistema(),
    }
}

/// A leitura de um arquivo, nos testes: um objeto (`{"totalMb", "disponivelMb", "swapUsadoMb", "swapTotalMb",
/// "pressao"}`) vale sempre; uma lista dá uma leitura por vez, a primeira, e a última fica.
fn de_teste(arq: &std::path::Path) -> Option<Leitura> {
    let v: Value = serde_json::from_slice(&std::fs::read(arq).ok()?).ok()?;
    let atual = match &v {
        Value::Array(lista) => {
            let primeira = lista.first()?.clone();
            if lista.len() > 1 {
                let _ = std::fs::write(arq, Value::Array(lista[1..].to_vec()).to_string());
            }
            primeira
        }
        outro => outro.clone(),
    };
    let mb = |k: &str| atual.get(k).and_then(Value::as_u64).unwrap_or(0) << 20;
    Some(Leitura {
        total: mb("totalMb"),
        disponivel: mb("disponivelMb"),
        swap_usado: mb("swapUsadoMb"),
        swap_total: mb("swapTotalMb"),
        pressao: atual.get("pressao").and_then(Value::as_u64).unwrap_or(1) as u32,
    })
}

#[cfg(target_os = "macos")]
fn do_sistema() -> Option<Leitura> {
    use std::ffi::{CString, c_void};

    /// Um número do `sysctl` (de 4 ou de 8 bytes).
    fn numero(nome: &str) -> Option<u64> {
        let nome = CString::new(nome).ok()?;
        let mut v = [0u8; 8];
        let mut n = v.len();
        // SAFETY: o buffer tem o tamanho dito em `n`, que o kernel atualiza com o que escreveu.
        let r = unsafe { libc::sysctlbyname(nome.as_ptr(), v.as_mut_ptr() as *mut c_void, &mut n, std::ptr::null_mut(), 0) };
        match (r, n) {
            (0, 4) => Some(u32::from_ne_bytes([v[0], v[1], v[2], v[3]]) as u64),
            (0, 8) => Some(u64::from_ne_bytes(v)),
            _ => None,
        }
    }

    let total = numero("hw.memsize")?;
    let pagina = numero("vm.pagesize").unwrap_or(16_384);
    let pressao = numero("kern.memorystatus_vm_pressure_level").unwrap_or(1) as u32;
    let swap = {
        let nome = CString::new("vm.swapusage").ok()?;
        // SAFETY: `xsw_usage` é uma estrutura C simples; zerada é válida.
        let mut s: libc::xsw_usage = unsafe { std::mem::zeroed() };
        let mut n = std::mem::size_of::<libc::xsw_usage>();
        // SAFETY: o buffer é a estrutura, com o tamanho dela em `n`.
        let r = unsafe { libc::sysctlbyname(nome.as_ptr(), &mut s as *mut _ as *mut c_void, &mut n, std::ptr::null_mut(), 0) };
        (r == 0).then_some((s.xsu_used, s.xsu_total))
    };
    // SAFETY: `vm_statistics64` é uma estrutura C simples; zerada é válida.
    let mut e: libc::vm_statistics64 = unsafe { std::mem::zeroed() };
    // Contada até os campos lidos aqui (o HOST_VM_INFO64_REV1_COUNT do C), como o medidor do app: um tamanho maior
    // que o do kernel em uso é anotado por ele como erro de quem chama, a cada leitura.
    let mut conta = (std::mem::offset_of!(libc::vm_statistics64, swapped_count) / std::mem::size_of::<libc::integer_t>())
        as libc::mach_msg_type_number_t;
    // SAFETY: a estrutura tem pelo menos `conta` inteiros, e o kernel escreve no máximo isso.
    let r = unsafe {
        #[allow(deprecated)]
        let host = libc::mach_host_self();
        libc::host_statistics64(host, libc::HOST_VM_INFO64, &mut e as *mut _ as *mut libc::integer_t, &mut conta)
    };
    if r != libc::KERN_SUCCESS {
        return None;
    }
    let paginas = e.free_count as u64 + e.inactive_count as u64 + e.purgeable_count as u64 + e.speculative_count as u64;
    let (swap_usado, swap_total) = swap.unwrap_or((0, 0));
    Some(Leitura { total, disponivel: (paginas * pagina).min(total), swap_usado, swap_total, pressao })
}

#[cfg(windows)]
fn do_sistema() -> Option<Leitura> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: a estrutura é zerada e leva o tamanho que a chamada pede.
    unsafe {
        let mut s: MEMORYSTATUSEX = std::mem::zeroed();
        s.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut s) == 0 || s.ullTotalPhys == 0 {
            return None;
        }
        Some(Leitura { total: s.ullTotalPhys, disponivel: s.ullAvailPhys, swap_usado: 0, swap_total: 0, pressao: 0 })
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn do_sistema() -> Option<Leitura> {
    None
}

/// "1 conversa viva", "9 conversas vivas".
fn conversas(n: usize) -> String {
    if n == 1 { "1 conversa viva".into() } else { format!("{n} conversas vivas") }
}

/// A memória e as conversas vivas nas abas do Keep, num instante.
#[derive(Clone, Debug)]
pub struct Situacao {
    pub leitura: Option<Leitura>,
    /// Abas com o Claude ou o Codex vivo.
    pub vivas: usize,
}

impl Situacao {
    pub fn de(lista: &daemon::Retrato) -> Situacao {
        Situacao { leitura: ler(), vivas: keep_ia::abas::das_abas(lista, &[]).len() }
    }

    /// Por que não cabem mais `mais` conversas vivas: a memória apertada, ou o teto da RAM; `None` se cabem (e
    /// sem leitura, que não segura nada). Com `mais` 0, se o que já está vivo cabe.
    pub fn falta(&self, mais: usize) -> Option<String> {
        let l = self.leitura.as_ref()?;
        if let Some(a) = l.aperto() {
            return Some(a);
        }
        let teto = l.teto();
        (self.vivas + mais > teto).then(|| format!("teto de {} para {} de RAM", conversas(teto), gb(l.total)))
    }

    /// "0,3 GB livres, 9 conversas vivas".
    pub fn resumo(&self) -> String {
        let livres = self.leitura.as_ref().map_or("memória sem leitura".into(), |l| format!("{} livres", gb(l.disponivel)));
        format!("{livres}, {}", conversas(self.vivas))
    }

    pub fn json(&self) -> Value {
        json!({
            "leitura": self.leitura,
            "vivas": self.vivas,
            "teto": self.leitura.as_ref().map(Leitura::teto),
            "aperto": self.leitura.as_ref().and_then(Leitura::aperto),
            "cabeMaisUma": self.falta(1).is_none(),
            "resumo": self.resumo(),
        })
    }
}

/// `keep restauro memoria`: a leitura, as conversas vivas, o teto e se cabe mais uma.
pub fn mostrar() -> Result<Value, String> {
    let lista = daemon::listar()?;
    let mut v = Situacao::de(&lista).json();
    v["versao"] = json!(crate::VERSAO);
    Ok(v)
}

#[cfg(test)]
mod testes {
    use super::*;

    const MB: u64 = 1 << 20;

    fn leitura(total_mb: u64, disponivel_mb: u64, swap: (u64, u64), pressao: u32) -> Leitura {
        Leitura { total: total_mb * MB, disponivel: disponivel_mb * MB, swap_usado: swap.0 * MB, swap_total: swap.1 * MB, pressao }
    }

    #[test]
    fn apertada_pela_pressao_pela_disponivel_ou_pelo_swap() {
        // folgada
        assert_eq!(leitura(8192, 3000, (1000, 5120), 1).aperto(), None);
        // o kernel em alerta ou crítico
        assert_eq!(leitura(8192, 3000, (0, 0), 2).aperto().as_deref(), Some("pressão de memória em alerta"));
        assert_eq!(leitura(8192, 3000, (0, 0), 4).aperto().as_deref(), Some("pressão de memória crítica"));
        // menos de 15% disponível (1228 MB de 8 GB): 1200 aperta, 1300 não
        assert_eq!(leitura(8192, 1200, (0, 0), 1).aperto().as_deref(), Some("só 1,2 GB disponíveis de 8,0 GB"));
        assert_eq!(leitura(8192, 1300, (0, 0), 1).aperto(), None);
        // o swap passando da metade
        assert_eq!(leitura(8192, 3000, (4200, 5120), 1).aperto().as_deref(), Some("swap em 4,1 GB de 5,0 GB"));
        assert_eq!(leitura(8192, 3000, (2560, 5120), 1).aperto(), None, "a metade exata ainda não");
        // sem swap (Windows, ou o Mac sem arquivo de swap) não conta
        assert_eq!(leitura(8192, 3000, (0, 0), 0).aperto(), None);
    }

    #[test]
    fn o_teto_de_conversas_vem_da_ram() {
        assert_eq!(leitura(8192, 0, (0, 0), 1).teto(), 11, "(8 − 3) GB / 450 MB");
        assert_eq!(leitura(16_384, 0, (0, 0), 1).teto(), 29);
        assert_eq!(leitura(4096, 0, (0, 0), 1).teto(), 2);
        assert_eq!(leitura(2048, 0, (0, 0), 1).teto(), 1, "no mínimo uma");
    }

    #[test]
    fn cabe_mais_uma_pela_memoria_e_pelo_teto() {
        let folgada = Some(leitura(8192, 4000, (0, 0), 1));
        let s = |vivas: usize, leitura: Option<Leitura>| Situacao { leitura, vivas };
        assert_eq!(s(10, folgada.clone()).falta(1), None);
        assert_eq!(s(11, folgada.clone()).falta(1).as_deref(), Some("teto de 11 conversas vivas para 8,0 GB de RAM"));
        assert_eq!(s(11, folgada.clone()).falta(0), None, "as 11 cabem; a 12ª, não");
        assert_eq!(s(12, folgada).falta(0).as_deref(), Some("teto de 11 conversas vivas para 8,0 GB de RAM"));
        assert_eq!(s(1, Some(leitura(8192, 4000, (0, 0), 2))).falta(1).as_deref(), Some("pressão de memória em alerta"));
        assert_eq!(s(40, None).falta(1), None, "sem leitura, nada é segurado");
        assert_eq!(s(1, Some(leitura(2048, 1500, (0, 0), 1))).falta(1).as_deref(), Some("teto de 1 conversa viva para 2,0 GB de RAM"));
        assert_eq!(s(9, Some(leitura(8192, 60, (4200, 5120), 2))).resumo(), "0,1 GB livres, 9 conversas vivas");
    }

    #[test]
    fn a_leitura_de_teste_vem_do_arquivo_uma_por_vez() {
        let d = tempfile::tempdir().unwrap();
        let arq = d.path().join("memoria.json");
        std::fs::write(
            &arq,
            r#"[{"totalMb":8192,"disponivelMb":100,"pressao":2},{"totalMb":8192,"disponivelMb":4000,"swapUsadoMb":10,"swapTotalMb":20}]"#,
        )
        .unwrap();
        assert_eq!(de_teste(&arq), Some(leitura(8192, 100, (0, 0), 2)));
        assert_eq!(de_teste(&arq), Some(leitura(8192, 4000, (10, 20), 1)));
        assert_eq!(de_teste(&arq), Some(leitura(8192, 4000, (10, 20), 1)), "a última fica");
        std::fs::write(&arq, r#"{"totalMb":4096,"disponivelMb":1}"#).unwrap();
        assert_eq!(de_teste(&arq), Some(leitura(4096, 1, (0, 0), 1)));
        assert_eq!(de_teste(&d.path().join("nada.json")), None);
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn a_leitura_do_sistema_diz_a_ram_inteira() {
        let l = do_sistema().expect("a leitura do sistema");
        assert!(l.total >= 1 << 30, "{l:?}");
        assert!(l.disponivel <= l.total, "{l:?}");
        assert!(l.swap_usado <= l.swap_total || l.swap_total == 0, "{l:?}");
    }
}
