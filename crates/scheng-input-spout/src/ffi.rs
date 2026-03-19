//! `ffi.rs` — Rust FFI declarations for the Spout receiver bridge.

#[cfg(target_os = "windows")]
extern "C" {
    pub fn scheng_spout_receiver_create() -> *mut std::ffi::c_void;
    pub fn scheng_spout_receiver_connect(
        receiver:    *mut std::ffi::c_void,
        sender_name: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
    pub fn scheng_spout_receiver_pull_rgba(
        receiver:   *mut std::ffi::c_void,
        out_rgba:   *mut u8,
        out_width:  *mut u32,
        out_height: *mut u32,
    ) -> std::ffi::c_int;
    pub fn scheng_spout_receiver_is_connected(
        receiver: *mut std::ffi::c_void,
    ) -> std::ffi::c_int;
    pub fn scheng_spout_receiver_list_senders(
        out_names:    *mut *mut std::ffi::c_char,
        max_count:    u32,
        name_buf_len: u32,
    ) -> u32;
    pub fn scheng_spout_receiver_destroy(receiver: *mut std::ffi::c_void);
}
