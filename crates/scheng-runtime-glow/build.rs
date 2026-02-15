use std::path::PathBuf;

fn main() {
    // Only build the Syphon bridge on macOS when the `syphon` feature is enabled.
    let is_macos = std::env::var("CARGO_CFG_TARGET_OS").map(|v| v == "macos").unwrap_or(false);
    let syphon_enabled = std::env::var_os("CARGO_FEATURE_SYPHON").is_some();

    if !(is_macos && syphon_enabled) {
        return;
    }

    // Bridge source lives in this crate.
    println!("cargo:rerun-if-changed=native/syphon_bridge.m");
    println!("cargo:rerun-if-changed=native/syphon_bridge.h");

    // Syphon.framework lives at workspace root: vendor/Syphon.framework
    // Resolve workspace root from crate root: crates/scheng-runtime-glow -> workspace root is ../..
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = crate_dir.parent().and_then(|p| p.parent()).unwrap().to_path_buf();
    let syphon_fw = workspace_root.join("vendor").join("Syphon.framework");
    let syphon_fw_dir = syphon_fw.parent().unwrap();

    if !syphon_fw.exists() {
        panic!(
            "Syphon.framework not found at expected path: {}",
            syphon_fw.display()
        );
    }

    // Build the Objective-C bridge into a static lib.
    //
    // IMPORTANT: we must add the framework search path *at compile time* so
    // `#import <Syphon/Syphon.h>` resolves. (-F is the key flag here.)
    let fw_dir_str = syphon_fw_dir.to_string_lossy().to_string();
    let headers_dir = syphon_fw.join("Headers");

    cc::Build::new()
        .file("native/syphon_bridge.m")
        .flag("-fobjc-arc")
        .flag("-ObjC")
        .flag(format!("-F{fw_dir_str}"))
        // Some clang configurations also require an explicit "framework include" search path.
        .flag(format!("-iframework{fw_dir_str}"))
        .include("native")
        .include(headers_dir)
        .compile("syphon_bridge");

    // Ensure the static lib is linked (cc usually emits this, but we are explicit).
    println!("cargo:rustc-link-lib=static=syphon_bridge");

    // ---- Runtime loading: ensure dyld can find vendor/Syphon.framework ----
    //
    // The framework is linked as @rpath/Syphon.framework, so we must provide at least one LC_RPATH.
    // 1) Absolute vendor path (works for local dev runs)
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", syphon_fw_dir.display());
    // 2) Relative to the executable (target/{debug,release}/...), so it works across debug/release
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");

    // Link the framework and common macOS deps used by Syphon.
    println!("cargo:rustc-link-search=framework={}", syphon_fw_dir.display());
    println!("cargo:rustc-link-lib=framework=Syphon");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=OpenGL");
    println!("cargo:rustc-link-lib=framework=IOSurface");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../../vendor");


}
