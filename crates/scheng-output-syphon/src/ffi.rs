#[cfg(target_os = "macos")]
extern "C" {
    pub fn scheng_syphon_create(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    pub fn scheng_syphon_publish_rgba(
        server_ptr: *mut std::ffi::c_void,
        rgba:       *const u8,
        width:      u32,
        height:     u32,
    );
    pub fn scheng_syphon_has_clients(server_ptr: *mut std::ffi::c_void) -> std::ffi::c_int;
    pub fn scheng_syphon_destroy(server_ptr: *mut std::ffi::c_void);
}
