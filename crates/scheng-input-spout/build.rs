fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // Port the spout_bridge from scheng-output-spout/native/ (which itself
    // was ported from scheng-runtime-glow/native/spout_bridge/).
    // The receiver bridge follows the same pattern as the sender.
    cc::Build::new()
        .cpp(true)
        .file("native/spout_receiver_bridge.cpp")
        // Spout2 headers — place Spout2 source at vendor/Spout2/
        .include("vendor/Spout2/SPOUT_SDK")
        .compile("spout_receiver_bridge");

    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=gdi32");
    println!("cargo:rerun-if-changed=native/spout_receiver_bridge.cpp");
    println!("cargo:rerun-if-changed=native/spout_receiver_bridge.h");
}
