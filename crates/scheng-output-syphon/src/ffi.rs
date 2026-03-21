//! FFI declarations for the Syphon Metal bridge.

#[cfg(target_os = "macos")]
extern "C" {
    pub fn scheng_syphon_create(
        name: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;

    /// Readback path (wgpu 22): upload RGBA pixels via staging texture.
    pub fn scheng_syphon_publish_rgba(
        server_ptr: *mut std::ffi::c_void,
        rgba:       *const u8,
        width:      u32,
        height:     u32,
    );

    /// Zero-copy path (wgpu 23+): publish raw MTLTexture pointer directly.
    pub fn scheng_syphon_publish_texture(
        server_ptr:  *mut std::ffi::c_void,
        mtl_texture: *mut std::ffi::c_void,
        width:       u32,
        height:      u32,
    );

    pub fn scheng_syphon_has_clients(
        server_ptr: *mut std::ffi::c_void,
    ) -> std::ffi::c_int;

    pub fn scheng_syphon_destroy(server_ptr: *mut std::ffi::c_void);
}
