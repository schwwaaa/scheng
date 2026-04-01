fn main() {
    let vendor = "/Users/tgm/Documents/SPLASH/scheng/vendor";
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor);
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    println!("cargo:rerun-if-changed=build.rs");
}
