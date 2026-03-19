/*
 * syphon_client_bridge.m
 *
 * Objective-C Metal bridge for scheng-input-syphon.
 * Wraps SyphonMetalClient in a C-compatible API callable from Rust FFI.
 *
 * Compiled via cc crate in build.rs with -fobjc-arc.
 * Links against Syphon.framework, Metal.framework, Foundation.framework.
 *
 * Frame pull strategy:
 *   SyphonMetalClient provides newFrameImage — a MTLTexture holding the latest
 *   published frame. We blit it to a shared-memory staging buffer and copy the
 *   RGBA bytes to the caller's buffer. On M1/M2 unified memory this is very cheap.
 */

@import Foundation;
@import Metal;

// Forward-declare SyphonMetalClient and SyphonServerDirectory
// so we don't need to import the full framework headers.
@interface SyphonServerDirectory : NSObject
+ (instancetype)sharedDirectory;
@property (readonly) NSArray* servers;
@end

@interface SyphonMetalClient : NSObject
- (instancetype)initWithServerDescription:(NSDictionary*)description
                                   device:(id<MTLDevice>)device
                                  options:(NSDictionary*)options
                         newFrameHandler:(void (^)(SyphonMetalClient*))handler;
- (id<MTLTexture>)newFrameImage;
- (BOOL)isValid;
- (void)stop;
@end

#include "syphon_client_bridge.h"
#include <string.h>

// ── Directory ─────────────────────────────────────────────────────────────

void* scheng_syphon_directory_create(void) {
    @autoreleasepool {
        SyphonServerDirectory* dir = [SyphonServerDirectory sharedDirectory];
        // sharedDirectory is a singleton — bridge-retain for Rust ownership tracking
        return (__bridge_retained void*)dir;
    }
}

uint32_t scheng_syphon_server_count(void* directory) {
    if (!directory) return 0;
    @autoreleasepool {
        SyphonServerDirectory* dir = (__bridge SyphonServerDirectory*)directory;
        return (uint32_t)[dir.servers count];
    }
}

// Static buffer for returning server name strings — valid until next call
static char s_server_name_buf[512];
static char s_server_app_buf[512];

const char* scheng_syphon_server_name(void* directory, uint32_t idx) {
    if (!directory) return NULL;
    @autoreleasepool {
        SyphonServerDirectory* dir = (__bridge SyphonServerDirectory*)directory;
        NSArray* servers = dir.servers;
        if (idx >= [servers count]) return NULL;
        NSDictionary* desc = servers[idx];
        NSString* name = desc[@"SyphonServerDescriptionNameKey"];
        if (!name) return NULL;
        strncpy(s_server_name_buf, [name UTF8String], sizeof(s_server_name_buf) - 1);
        s_server_name_buf[sizeof(s_server_name_buf) - 1] = '\0';
        return s_server_name_buf;
    }
}

const char* scheng_syphon_server_app(void* directory, uint32_t idx) {
    if (!directory) return NULL;
    @autoreleasepool {
        SyphonServerDirectory* dir = (__bridge SyphonServerDirectory*)directory;
        NSArray* servers = dir.servers;
        if (idx >= [servers count]) return NULL;
        NSDictionary* desc = servers[idx];
        NSString* app = desc[@"SyphonServerDescriptionAppNameKey"];
        if (!app) return NULL;
        strncpy(s_server_app_buf, [app UTF8String], sizeof(s_server_app_buf) - 1);
        s_server_app_buf[sizeof(s_server_app_buf) - 1] = '\0';
        return s_server_app_buf;
    }
}

// ── Client ────────────────────────────────────────────────────────────────

void* scheng_syphon_client_create(
    void*       directory,
    const char* server_name,
    void*       mtl_device
) {
    if (!directory || !server_name || !mtl_device) return NULL;
    @autoreleasepool {
        SyphonServerDirectory* dir    = (__bridge SyphonServerDirectory*)directory;
        id<MTLDevice>          device = (__bridge id<MTLDevice>)mtl_device;
        NSString*              target = [NSString stringWithUTF8String:server_name];

        // Find a matching server description
        NSDictionary* matched = nil;
        for (NSDictionary* desc in dir.servers) {
            NSString* name = desc[@"SyphonServerDescriptionNameKey"];
            if ([name isEqualToString:target]) {
                matched = desc;
                break;
            }
        }

        if (!matched) {
            NSLog(@"[scheng-input-syphon] Server '%@' not found. Available: %@",
                  target, [dir.servers valueForKey:@"SyphonServerDescriptionNameKey"]);
            return NULL;
        }

        SyphonMetalClient* client = [[SyphonMetalClient alloc]
            initWithServerDescription: matched
                               device: device
                              options: nil
                      newFrameHandler: nil]; // polling model — no callback needed

        if (!client) {
            NSLog(@"[scheng-input-syphon] Failed to create client for '%@'", target);
            return NULL;
        }

        NSLog(@"[scheng-input-syphon] Connected to Syphon server '%@'", target);
        return (__bridge_retained void*)client;
    }
}

int scheng_syphon_client_pull_rgba(
    void*          client,
    unsigned char* out_rgba,
    uint32_t*      out_width,
    uint32_t*      out_height,
    void*          mtl_device
) {
    if (!client || !out_rgba || !out_width || !out_height || !mtl_device) return 0;

    @autoreleasepool {
        SyphonMetalClient* c      = (__bridge SyphonMetalClient*)client;
        id<MTLDevice>      device = (__bridge id<MTLDevice>)mtl_device;

        if (![c isValid]) return 0;

        id<MTLTexture> tex = [c newFrameImage];
        if (!tex) return 0; // No new frame available

        uint32_t w = (uint32_t)[tex width];
        uint32_t h = (uint32_t)[tex height];
        *out_width  = w;
        *out_height = h;

        // Read pixels from the Metal texture into the caller's buffer.
        // On M1/M2 with MTLStorageModeShared this is a direct memory read — no copy.
        [tex getBytes: out_rgba
          bytesPerRow: w * 4
           fromRegion: MTLRegionMake2D(0, 0, w, h)
          mipmapLevel: 0];

        return 1;
    }
}

int scheng_syphon_client_is_connected(void* client) {
    if (!client) return 0;
    @autoreleasepool {
        SyphonMetalClient* c = (__bridge SyphonMetalClient*)client;
        return [c isValid] ? 1 : 0;
    }
}

void scheng_syphon_client_destroy(void* client) {
    if (!client) return;
    @autoreleasepool {
        // Bridge-release: transfers ownership back to ObjC ARC for deallocation
        SyphonMetalClient* c = (__bridge_transfer SyphonMetalClient*)client;
        [c stop];
        NSLog(@"[scheng-input-syphon] Client disconnected");
        (void)c; // ARC releases here
    }
}

void scheng_syphon_directory_destroy(void* directory) {
    if (!directory) return;
    // sharedDirectory is a singleton — just release our retain
    @autoreleasepool {
        SyphonServerDirectory* __unused dir =
            (__bridge_transfer SyphonServerDirectory*)directory;
        // ARC releases our retain — the singleton itself stays alive
    }
}
