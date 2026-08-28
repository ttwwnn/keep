use std::path::PathBuf;

fn main() {
    // Vendored libghostty-vt, one directory per platform. Reproduce with
    // vendor/fetch.sh — the macOS slice comes from upstream's prebuilt
    // xcframework, the Linux ones are built from upstream's source tarball
    // with Zig (which is also what links the Rust for those targets).
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let slice = match os.as_str() {
        "linux" => format!("libghostty-vt-linux-{arch}"),
        _ => "libghostty-vt".to_string(),
    };
    let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(&slice)
        .canonicalize()
        .unwrap_or_else(|_| panic!("vendor/{slice} missing - run vendor/fetch.sh"));

    println!("cargo:rustc-link-search=native={}", vendor.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:include={}", vendor.join("include").display());
    println!("cargo:rerun-if-changed={}", vendor.join("libghostty-vt.a").display());
}
