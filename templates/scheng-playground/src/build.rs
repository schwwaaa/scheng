fn main() {
    // Always emit for macOS — harmless on other platforms
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let vendor = std::path::Path::new(&manifest_dir)
        .parent().unwrap()
        .join("scheng").join("vendor");

    // Absolute path to vendor dir (dev builds)
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor.display());
    // Bundle-relative paths (distribution)
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");

    println!("cargo:rerun-if-changed=build.rs");
}
