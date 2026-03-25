/*
 * spout_receiver_bridge.h
 *
 * C-compatible header for the Spout2 receiver bridge. Windows only.
 * Mirrors the sender bridge in scheng-output-spout.
 */

#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Create a Spout receiver. Returns handle or NULL on failure. */
void* scheng_spout_receiver_create(void);

/**
 * Connect to a Spout sender by name.
 * Returns 1 on success, 0 if the sender is not available.
 */
int scheng_spout_receiver_connect(void* receiver, const char* sender_name);

/**
 * Pull the latest frame into a caller-provided RGBA8 buffer.
 * out_rgba must be at least out_width * out_height * 4 bytes.
 * Returns 1 if a new frame was received, 0 otherwise.
 */
int scheng_spout_receiver_pull_rgba(
    void*          receiver,
    unsigned char* out_rgba,
    uint32_t*      out_width,
    uint32_t*      out_height
);

/** Returns 1 if connected to a live sender. */
int scheng_spout_receiver_is_connected(void* receiver);

/** List available Spout senders. Fills names array (caller allocates). */
uint32_t scheng_spout_receiver_list_senders(
    char**   out_names,
    uint32_t max_count,
    uint32_t name_buf_len
);

/** Release the receiver. */
void scheng_spout_receiver_destroy(void* receiver);

#ifdef __cplusplus
}
#endif
