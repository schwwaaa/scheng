//! `ffi.rs` — Rust FFI declarations for the Syphon client bridge.

#[cfg(all(target_os = "macos", feature = "syphon-framework"))]
extern "C" {
    pub fn scheng_syphon_directory_init();
    pub fn scheng_syphon_directory_create() -> *mut std::ffi::c_void;
    pub fn scheng_syphon_server_count(directory: *mut std::ffi::c_void) -> u32;
    pub fn scheng_syphon_server_name(
        directory: *mut std::ffi::c_void,
        idx:       u32,
    ) -> *const std::ffi::c_char;
    pub fn scheng_syphon_server_app(
        directory: *mut std::ffi::c_void,
        idx:       u32,
    ) -> *const std::ffi::c_char;
    pub fn scheng_syphon_client_create(
        directory:   *mut std::ffi::c_void,
        server_name: *const std::ffi::c_char,
        mtl_device:  *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    pub fn scheng_syphon_client_pull_rgba(
        client:     *mut std::ffi::c_void,
        out_rgba:   *mut u8,
        out_width:  *mut u32,
        out_height: *mut u32,
        mtl_device: *mut std::ffi::c_void,
    ) -> std::ffi::c_int;
    pub fn scheng_syphon_client_is_connected(client: *mut std::ffi::c_void) -> std::ffi::c_int;
    pub fn scheng_syphon_client_destroy(client: *mut std::ffi::c_void);
    pub fn scheng_syphon_directory_destroy(directory: *mut std::ffi::c_void);
}
