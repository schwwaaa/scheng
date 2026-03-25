fn main() {
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        );
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
