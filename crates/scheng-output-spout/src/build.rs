fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return; // no-op on non-Windows
    }

    // Requires Spout2 SDK source at vendor/Spout2/ in the workspace root.
    // Get it: git clone https://github.com/leadedge/Spout2 vendor/Spout2
    let spout_sdk = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    )
    .join("../../vendor/Spout2/SPOUT_SDK");

    cc::Build::new()
        .cpp(true)
        .file("native/spout_bridge.cpp")
        .include(&spout_sdk)
        // Spout2 itself pulls in OpenGL — link what it needs
        .compile("spout_bridge");

    println!("cargo:rustc-link-lib=opengl32");
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=gdi32");
    println!("cargo:rerun-if-changed=native/spout_bridge.cpp");
    println!("cargo:rerun-if-changed=native/spout_bridge.h");
}
