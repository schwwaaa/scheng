#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

void* scheng_syphon_create(const char* name);
void  scheng_syphon_publish_rgba(void* server_ptr, const unsigned char* rgba, uint32_t width, uint32_t height);
int   scheng_syphon_has_clients(void* server_ptr);
void  scheng_syphon_destroy(void* server_ptr);

#ifdef __cplusplus
}
#endif
