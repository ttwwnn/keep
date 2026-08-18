use std::path::PathBuf;

fn main() {
    // Vendored libghostty-vt. Reproduce with: vendor/fetch.sh
    let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/libghostty-vt")
        .canonicalize()
        .expect("vendor/libghostty-vt missing - run vendor/fetch.sh");

    println!("cargo:rustc-link-search=native={}", vendor.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:include={}", vendor.join("include").display());
    println!("cargo:rerun-if-changed={}", vendor.join("libghostty-vt.a").display());
}
