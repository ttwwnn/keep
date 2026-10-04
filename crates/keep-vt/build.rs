use std::path::PathBuf;

fn main() {
    // Vendored libghostty-vt, one directory per platform. Reproduce with
    // vendor/fetch.sh — the macOS slice comes from upstream's prebuilt
    // xcframework, the Linux ones are built from upstream's source tarball
    // with Zig (which is also what links the Rust for those targets), and
    // the Windows one is built with Zig on Windows, against the MSVC
    // toolchain the Rust links with (vendor/fetch-windows.ps1).
    //
    // `KEEP_GHOSTTY_VT_DIR` names the directory outright, for a build that
    // keeps the library somewhere else.
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    println!("cargo:rerun-if-env-changed=KEEP_GHOSTTY_VT_DIR");
    let vendor = match std::env::var("KEEP_GHOSTTY_VT_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => {
            let slice = match os.as_str() {
                "linux" => format!("libghostty-vt-linux-{arch}"),
                "windows" => format!("libghostty-vt-windows-{arch}"),
                _ => "libghostty-vt".to_string(),
            };
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor").join(slice)
        }
    };
    let vendor = vendor
        .canonicalize()
        .unwrap_or_else(|_| panic!("{} missing - run vendor/fetch.sh", vendor.display()));

    // Zig names the static library for the platform: `libghostty-vt.a`, or
    // `ghostty-vt-static.lib` beside the import library of the DLL.
    let (lib, file) = if os == "windows" {
        ("ghostty-vt-static", "ghostty-vt-static.lib")
    } else {
        ("ghostty-vt", "libghostty-vt.a")
    };
    println!("cargo:rustc-link-search=native={}", vendor.display());
    println!("cargo:rustc-link-lib=static={lib}");
    if os == "windows" {
        // The Zig standard library calls into these directly.
        println!("cargo:rustc-link-lib=ntdll");
        println!("cargo:rustc-link-lib=kernel32");
    }
    println!("cargo:include={}", vendor.join("include").display());
    println!("cargo:rerun-if-changed={}", vendor.join(file).display());
}
