//! `ffi.rs` — Rust FFI declarations for the ObjC Metal bridge.
//!
//! These match the C functions in native/syphon_metal_bridge.m exactly.
//! Linked via build.rs (cc crate + Syphon.framework).

#[link(name = "syphon_metal_bridge", kind = "static")]
extern "C" {
    /// Create a SyphonMetalServer.
    /// `name` — C string, UTF-8. `mtl_device` — raw id<MTLDevice>.
    /// Returns SyphonMetalServer* as *mut c_void, or null on failure.
    pub fn scheng_syphon_create(
        name:       *const std::ffi::c_char,
        mtl_device: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    /// Publish a frame from raw RGBA8 pixels.
    pub fn scheng_syphon_publish_rgba(
        server_ptr: *mut std::ffi::c_void,
        rgba:       *const u8,
        width:      u32,
        height:     u32,
        mtl_device: *mut std::ffi::c_void,
    );

    /// Publish a frame directly from a MTLTexture (zero-copy).
    pub fn scheng_syphon_publish_texture(
        server_ptr:  *mut std::ffi::c_void,
        mtl_texture: *mut std::ffi::c_void,
    );

    /// Returns 1 if there are active Syphon clients, 0 otherwise.
    pub fn scheng_syphon_has_clients(server_ptr: *mut std::ffi::c_void) -> std::ffi::c_int;

    /// Stop and release the Syphon server.
    pub fn scheng_syphon_destroy(server_ptr: *mut std::ffi::c_void);
}
