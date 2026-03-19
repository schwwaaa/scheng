fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    // Only compile the ObjC bridge when the framework is explicitly requested.
    // Without this feature the crate compiles as a pure Rust stub.
    // Enable with: cargo build -p scheng-input-syphon --features syphon-framework
    if std::env::var("CARGO_FEATURE_SYPHON_FRAMEWORK").is_err() {
        return;
    }

    cc::Build::new()
        .file("native/syphon_client_bridge.m")
        .flag("-fobjc-arc")
        .flag("-fmodules")
        .compile("syphon_client_bridge");

    println!("cargo:rustc-link-search=framework=vendor");
    println!("cargo:rustc-link-lib=framework=Syphon");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=Foundation");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // crate is at crates/scheng-input-syphon/ — workspace root is ../../
    println!("cargo:rustc-link-arg=-Wl,-rpath,{manifest_dir}/../../vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{manifest_dir}/vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/Library/Frameworks");

    println!("cargo:rerun-if-changed=native/syphon_client_bridge.m");
    println!("cargo:rerun-if-changed=native/syphon_client_bridge.h");
}
