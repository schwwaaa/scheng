/*
 * syphon_client_bridge.h
 *
 * C-compatible header for the Syphon Metal client (receiver) bridge.
 * Mirrors the server bridge in scheng-output-syphon but for receiving.
 *
 * SyphonMetalClient connects to a named Syphon server and pulls the
 * latest published texture each frame.
 */

#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Start discovering Syphon servers on the local machine.
 * Returns a SyphonServerDirectory* as void* — used to enumerate servers.
 * Keep alive for the lifetime of the receiver.
 */
void* scheng_syphon_directory_create(void);

/**
 * Return the number of currently available Syphon servers.
 */
uint32_t scheng_syphon_server_count(void* directory);

/**
 * Return the name of server at index `idx` as a UTF-8 C string.
 * The string is valid until the next call to this function.
 * Returns NULL if idx is out of range.
 */
const char* scheng_syphon_server_name(void* directory, uint32_t idx);

/**
 * Return the app name (publisher) of server at index `idx`.
 * Returns NULL if idx is out of range.
 */
const char* scheng_syphon_server_app(void* directory, uint32_t idx);

/**
 * Connect to a Syphon server by name.
 *
 * @param directory   SyphonServerDirectory* from scheng_syphon_directory_create
 * @param server_name Name of the server to connect to (matched against available servers)
 * @param mtl_device  id<MTLDevice> as void* — must be the same device used by wgpu
 * @return            SyphonMetalClient* as void*, or NULL on failure
 */
void* scheng_syphon_client_create(
    void*       directory,
    const char* server_name,
    void*       mtl_device
);

/**
 * Pull the latest frame from the connected Syphon server into a caller-provided
 * RGBA8 pixel buffer.
 *
 * @param client      SyphonMetalClient* from scheng_syphon_client_create
 * @param out_rgba    Caller-allocated buffer: must be width * height * 4 bytes
 * @param out_width   Filled with the actual frame width
 * @param out_height  Filled with the actual frame height
 * @param mtl_device  id<MTLDevice> as void*
 * @return            1 if a new frame was copied, 0 if no new frame available
 */
int scheng_syphon_client_pull_rgba(
    void*          client,
    unsigned char* out_rgba,
    uint32_t*      out_width,
    uint32_t*      out_height,
    void*          mtl_device
);

/**
 * Returns 1 if the client is connected to a live server, 0 otherwise.
 */
int scheng_syphon_client_is_connected(void* client);

/**
 * Disconnect and release the client. After this call, client is invalid.
 */
void scheng_syphon_client_destroy(void* client);

/**
 * Release the server directory.
 */
void scheng_syphon_directory_destroy(void* directory);

#ifdef __cplusplus
}
#endif
