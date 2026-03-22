#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Initialize the Syphon server directory (call once at app start). */
void     scheng_syphon_directory_init(void);

void*    scheng_syphon_directory_create(void);
uint32_t scheng_syphon_server_count(void* directory);
const char* scheng_syphon_server_name(void* directory, uint32_t idx);
const char* scheng_syphon_server_app(void* directory, uint32_t idx);

void* scheng_syphon_client_create(void* directory, const char* server_name, void* mtl_device);
int   scheng_syphon_client_pull_rgba(void* client, unsigned char* out_rgba, uint32_t* out_width, uint32_t* out_height, void* mtl_device);
int   scheng_syphon_client_is_connected(void* client);
void  scheng_syphon_client_destroy(void* client);
void  scheng_syphon_directory_destroy(void* directory);

#ifdef __cplusplus
}
#endif
