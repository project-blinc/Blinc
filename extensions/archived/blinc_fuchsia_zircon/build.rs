//! Build script for blinc_fuchsia_zircon
//!
//! On Fuchsia, this links against libzircon.so from the sysroot.
//! On other platforms, no linking is needed (stubs are used).

fn main() {
    // Only link on Fuchsia
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "fuchsia" {
        // Link against libzircon.so
        // The sysroot is provided by the Fuchsia SDK toolchain
        println!("cargo:rustc-link-lib=zircon");

        // If FUCHSIA_SDK_PATH is set, add the sysroot lib path
        if let Ok(sdk_path) = std::env::var("FUCHSIA_SDK_PATH") {
            let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
            let arch_dir = match arch.as_str() {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                "riscv64" => "riscv64",
                _ => "x64",
            };
            let lib_path = format!("{}/arch/{}/sysroot/lib", sdk_path, arch_dir);
            println!("cargo:rustc-link-search=native={}", lib_path);
        }
    }

    // Tell Cargo to rerun if these change
    println!("cargo:rerun-if-env-changed=FUCHSIA_SDK_PATH");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
}
