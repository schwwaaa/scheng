//! build.rs for scheng-output-syphon
//!
//! Compiles the Objective-C Metal bridge and links Syphon.framework.
//! Only runs on macOS.
//!
//! Requirements:
//!   - Xcode Command Line Tools (for clang ObjC support)
//!   - Syphon.framework at `<workspace_root>/vendor/Syphon.framework`
//!     Download: https://github.com/Syphon/Syphon-Framework/releases

fn main() {
    #[cfg(target_os = "macos")]
    build_macos();
}

#[cfg(target_os = "macos")]
fn build_macos() {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // crates/scheng-output-syphon → crates → workspace root
    let workspace_root = manifest_dir
        .parent().expect("crates dir")
        .parent().expect("workspace root");

    let framework_path = workspace_root.join("vendor").join("Syphon.framework");
    if !framework_path.exists() {
        panic!(
            "\n\n\
            ╔══════════════════════════════════════════════════════════╗\n\
            ║  Syphon.framework not found                              ║\n\
            ╠══════════════════════════════════════════════════════════╣\n\
            ║  Expected: {:<42} ║\n\
            ║                                                          ║\n\
            ║  Download the latest release from:                       ║\n\
            ║  github.com/Syphon/Syphon-Framework/releases             ║\n\
            ║                                                          ║\n\
            ║  Then place at:                                          ║\n\
            ║    <workspace>/vendor/Syphon.framework                   ║\n\
            ╚══════════════════════════════════════════════════════════╝\n\n",
            framework_path.display().to_string()
        );
    }

    // Compile the ObjC Metal bridge
    cc::Build::new()
        .file("native/syphon_metal_bridge.m")
        .flag("-fobjc-arc")
        .flag("-fmodules")
        .flag(&format!("-F{}", workspace_root.join("vendor").display()))
        .compile("syphon_metal_bridge");

    // Link Syphon.framework from vendor/
    let vendor_dir = workspace_root.join("vendor");
    println!("cargo:rustc-link-search=framework={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=framework=Syphon");

    // System frameworks needed by the bridge
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=Metal");

    // RPATH entries so the built binary can find Syphon.framework at runtime
    // without needing DYLD_FRAMEWORK_PATH.
    //
    // Order matters — dyld tries each in order:
    // 1. Absolute path to workspace vendor/ (works for `cargo run` from workspace)
    // 2. Relative to executable (works for installed/distributed binaries)
    // 3. Standard macOS app bundle layout
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../../vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../Frameworks");

    // Re-run build.rs if the bridge source changes
    println!("cargo:rerun-if-changed=native/syphon_metal_bridge.m");
    println!("cargo:rerun-if-changed=native/syphon_metal_bridge.h");
}
