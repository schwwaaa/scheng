/*
 * syphon_metal_bridge.h
 *
 * C-compatible header for the Syphon Metal bridge.
 * Called from Rust via FFI (see ffi.rs).
 *
 * Uses void* for all ObjC object pointers to avoid importing
 * framework headers into the Rust FFI layer.
 *
 * NOTE: This bridge uses SyphonMetalServer (Metal API).
 *       The existing native/syphon_bridge.m in scheng-runtime-glow
 *       uses SyphonServer (OpenGL API) — they are separate.
 */

#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Create a SyphonMetalServer.
 *
 * @param name       Server name visible to Syphon clients (e.g. "scheng")
 * @param mtl_device id<MTLDevice> as void* — obtain from wgpu Metal HAL
 * @return           SyphonMetalServer* as void*, or NULL on failure
 */
void* scheng_syphon_create(const char* name, void* mtl_device);

/**
 * Publish a frame from raw RGBA8 pixel data.
 *
 * Creates a temporary MTLTexture from the pixel data (using shared/unified
 * memory on Apple Silicon), then publishes via SyphonMetalServer.
 *
 * This is the primary path for wgpu — we readback pixels from the wgpu
 * RenderTarget and upload via this function. On M1/M2 unified memory,
 * the copy is very cheap.
 *
 * @param server_ptr  SyphonMetalServer* as void*
 * @param rgba        RGBA8 pixel data, tightly packed, top-left origin
 * @param width       Frame width in pixels
 * @param height      Frame height in pixels
 * @param mtl_device  id<MTLDevice> as void* — same device used at creation
 */
void scheng_syphon_publish_rgba(
    void*                server_ptr,
    const unsigned char* rgba,
    uint32_t             width,
    uint32_t             height,
    void*                mtl_device
);

/**
 * Publish a frame directly from a MTLTexture (zero-copy path).
 *
 * Use this when you have direct access to the Metal texture handle.
 * For Phase 3 (wgpu as_hal Metal interop).
 *
 * @param server_ptr   SyphonMetalServer* as void*
 * @param mtl_texture  id<MTLTexture> as void*
 */
void scheng_syphon_publish_texture(void* server_ptr, void* mtl_texture);

/**
 * Returns 1 if there are active Syphon clients, 0 otherwise.
 * Can be used to skip readback when no one is watching.
 */
int  scheng_syphon_has_clients(void* server_ptr);

/**
 * Stop the Syphon server and release all resources.
 * After this call, server_ptr is invalid — do not use it.
 */
void scheng_syphon_destroy(void* server_ptr);

#ifdef __cplusplus
}
#endif
