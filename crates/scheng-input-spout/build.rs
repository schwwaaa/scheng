fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let spout_sdk = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    )
    .join("../../vendor/Spout2/SPOUT_SDK");

    cc::Build::new()
        .cpp(true)
        .file("native/spout_receiver_bridge.cpp")
        .include(&spout_sdk)
        .compile("spout_receiver_bridge");

    println!("cargo:rustc-link-lib=opengl32");
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=gdi32");
    println!("cargo:rerun-if-changed=native/spout_receiver_bridge.cpp");
    println!("cargo:rerun-if-changed=native/spout_receiver_bridge.h");
}
