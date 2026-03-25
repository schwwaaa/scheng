/*
 * spout_bridge.h
 *
 * C-compatible header for the Spout2 sender bridge.
 * Windows only — included by scheng-output-spout/src/ffi.rs.
 *
 * Place Spout2 SDK at vendor/Spout2/ (workspace root):
 *   git clone https://github.com/leadedge/Spout2 vendor/Spout2
 */

#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Create a named Spout2 sender. Returns opaque handle or NULL on failure. */
void* scheng_spout_create(const char* name);

/**
 * Send an RGBA8 frame.
 * rgba must point to w * h * 4 bytes of RGBA pixel data.
 */
void scheng_spout_send_rgba(
    void*          sender,
    const uint8_t* rgba,
    uint32_t       w,
    uint32_t       h
);

/** Returns the number of receivers currently connected to this sender. */
int scheng_spout_get_receiver_count(void* sender);

/** Release the sender. */
void scheng_spout_destroy(void* sender);

#ifdef __cplusplus
}
#endif
