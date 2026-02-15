#pragma once
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque server pointer.
void* syphon_server_create(const char* name_utf8);
void  syphon_server_destroy(void* server);

// Publish an existing OpenGL texture name (GLuint).
// flipped: if true, the client should treat the texture as vertically flipped.
void  syphon_server_publish_texture(void* server, uint32_t tex, int32_t w, int32_t h, bool flipped);

#ifdef __cplusplus
}
#endif
