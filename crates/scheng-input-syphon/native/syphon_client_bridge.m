/*
 * syphon_client_bridge.m
 *
 * Objective-C Metal bridge for scheng-input-syphon.
 *
 * KEY DESIGN: SyphonServerDirectory must be kept alive from app start
 * to receive NSDistributedNotificationCenter updates. We hold a static
 * reference initialized once and never destroyed — this is how OBS and
 * other Syphon clients work.
 */

@import Foundation;
@import Metal;

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

// ── Persistent directory ───────────────────────────────────────────────────
// Held alive from init until process exit. Never destroyed.
static SyphonServerDirectory* s_directory = nil;

void scheng_syphon_directory_init(void) {
    @autoreleasepool {
        if (!s_directory) {
            s_directory = [SyphonServerDirectory sharedDirectory];
            NSLog(@"[scheng-input-syphon] Directory initialized (servers will populate as run loop runs)");
        }
    }
}

void* scheng_syphon_directory_create(void) {
    scheng_syphon_directory_init();
    // Return a non-owning pointer — caller must NOT release this
    return (__bridge void*)s_directory;
}

uint32_t scheng_syphon_server_count(void* directory) {
    @autoreleasepool {
        // Always use the persistent directory
        SyphonServerDirectory* dir = s_directory ? s_directory :
            (__bridge SyphonServerDirectory*)directory;
        return dir ? (uint32_t)[dir.servers count] : 0;
    }
}

static char s_server_name_buf[512];
static char s_server_app_buf[512];

const char* scheng_syphon_server_name(void* directory, uint32_t idx) {
    @autoreleasepool {
        SyphonServerDirectory* dir = s_directory ? s_directory :
            (__bridge SyphonServerDirectory*)directory;
        if (!dir) return NULL;
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
    @autoreleasepool {
        SyphonServerDirectory* dir = s_directory ? s_directory :
            (__bridge SyphonServerDirectory*)directory;
        if (!dir) return NULL;
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
    if (!server_name || !mtl_device) return NULL;
    scheng_syphon_directory_init();
    @autoreleasepool {
        SyphonServerDirectory* dir = s_directory;
        id<MTLDevice> device = (__bridge id<MTLDevice>)mtl_device;
        NSString* target = [NSString stringWithUTF8String:server_name];

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
                      newFrameHandler: nil];

        if (!client) {
            NSLog(@"[scheng-input-syphon] Failed to create client for '%@'", target);
            return NULL;
        }

        NSLog(@"[scheng-input-syphon] Connected to '%@'", target);
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
    if (!client || !out_rgba || !out_width || !out_height) return 0;
    @autoreleasepool {
        SyphonMetalClient* c = (__bridge SyphonMetalClient*)client;
        if (![c isValid]) return 0;
        id<MTLTexture> tex = [c newFrameImage];
        if (!tex) return 0;
        uint32_t w = (uint32_t)[tex width];
        uint32_t h = (uint32_t)[tex height];
        *out_width  = w;
        *out_height = h;
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
        SyphonMetalClient* c = (__bridge_transfer SyphonMetalClient*)client;
        [c stop];
        (void)c;
    }
}

void scheng_syphon_directory_destroy(void* directory) {
    // No-op — we keep the directory alive for the process lifetime
    (void)directory;
}
