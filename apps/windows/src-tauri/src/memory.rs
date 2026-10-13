//! The computer's memory in use, for the meter under the sidebar's list, and
//! the monitor the meter opens.

use serde::Serialize;

#[derive(Serialize)]
pub struct Memory {
    pub used: u64,
    pub total: u64,
}

#[cfg(windows)]
pub fn now() -> Option<Memory> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: the structure is zeroed and sized as the call asks.
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) == 0 || status.ullTotalPhys == 0 {
            return None;
        }
        Some(Memory {
            used: status.ullTotalPhys - status.ullAvailPhys,
            total: status.ullTotalPhys,
        })
    }
}

/// Development on a Mac: the same two figures out of the system's own tools.
#[cfg(target_os = "macos")]
pub fn now() -> Option<Memory> {
    use std::process::Command;
    let run = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let total: u64 = run("sysctl", &["-n", "hw.memsize"])?.trim().parse().ok()?;
    let vm = run("vm_stat", &[])?;
    let page: u64 = vm
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| -> u64 {
        vm.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().trim_end_matches('.').parse().ok())
            .unwrap_or(0)
    };
    let free = (pages("Pages free") + pages("Pages inactive") + pages("Pages speculative")) * page;
    Some(Memory { used: total.saturating_sub(free), total })
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn now() -> Option<Memory> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kib = |key: &str| -> Option<u64> {
        info.lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let total = kib("MemTotal:")? * 1024;
    let available = kib("MemAvailable:")? * 1024;
    Some(Memory { used: total.saturating_sub(available), total })
}

/// The system's own monitor of what uses the memory.
pub fn open_monitor() {
    #[cfg(windows)]
    let _ = std::process::Command::new("taskmgr").spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").args(["-a", "Activity Monitor"]).spawn();
}
