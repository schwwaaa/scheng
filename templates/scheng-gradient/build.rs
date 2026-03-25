/// build.rs — embeds rpath so Syphon.framework is found at runtime
/// without needing DYLD_FRAMEWORK_PATH (macOS only).
fn main() {
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        );
        // scheng-gradient lives next to scheng/ workspace:
        //   projects/scheng-gradient/  →  projects/scheng/vendor/
        let vendor = manifest_dir
            .parent().unwrap()
            .join("scheng")
            .join("vendor");

        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rerun-if-changed=build.rs");
    }
}
