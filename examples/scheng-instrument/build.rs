//! build.rs for scheng-example-instrument
//!
//! Embeds rpath entries so the binary can find Syphon.framework
//! at runtime without needing DYLD_FRAMEWORK_PATH.

fn main() {
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        );
        // examples/scheng-instrument → examples → workspace root
        let workspace_root = manifest_dir
            .parent().expect("examples dir")
            .parent().expect("workspace root");

        let vendor_dir = workspace_root.join("vendor");

        // Absolute path — works for `cargo run` from within the workspace
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor_dir.display());

        // Relative paths — work for distributed/installed binaries
        // target/release/scheng-example-instrument → ../../vendor
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../../vendor");

        // Standard app bundle layout
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../Frameworks");

        println!("cargo:rerun-if-changed=build.rs");
    }
}
