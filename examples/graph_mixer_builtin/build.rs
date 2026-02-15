use std::path::PathBuf;

fn main() {
    // Only do this when the example is built with `--features syphon`
    // (Cargo sets this env var automatically for enabled features.)
    if std::env::var_os("CARGO_FEATURE_SYPHON").is_none() {
        return;
    }

    // Resolve workspace-root/vendor from this crate dir: examples/graph_mixer_builtin -> ../../vendor
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vendor_dir = manifest_dir.join("../..").join("vendor");
    let vendor_dir = vendor_dir
        .canonicalize()
        .expect("could not canonicalize ../../vendor");

    // Ensure the final executable has an LC_RPATH that resolves @rpath/Syphon.framework
    //
    // 1) Absolute path for local dev runs (reliable)
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor_dir.display());

    // 2) Relative path from the executable (nice-to-have; works for target/{debug,release})
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
}
