//! Rust FFI for the Spout2 C++ bridge (Windows only).
//!
//! The bridge implementation lives in native/spout_bridge/ — port from
//! scheng-runtime-glow/native/spout_bridge/.
//!
//! Expected C bridge API (to implement in the C++ bridge):
//!
//! ```c
//! void* scheng_spout_create(const char* name);
//! void  scheng_spout_send_rgba(void* sender, const uint8_t* rgba, uint32_t w, uint32_t h);
//! void  scheng_spout_destroy(void* sender);
//! int   scheng_spout_get_receiver_count(void* sender);
//! ```

#[cfg(target_os = "windows")]
#[link(name = "spout_bridge", kind = "static")]
extern "C" {
    pub fn scheng_spout_create(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;

    pub fn scheng_spout_send_rgba(
        sender: *mut std::ffi::c_void,
        rgba:   *const u8,
        width:  u32,
        height: u32,
    );

    pub fn scheng_spout_destroy(sender: *mut std::ffi::c_void);

    pub fn scheng_spout_get_receiver_count(sender: *mut std::ffi::c_void) -> std::ffi::c_int;
}
