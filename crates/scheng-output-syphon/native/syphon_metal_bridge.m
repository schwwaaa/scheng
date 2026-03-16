/*
 * syphon_metal_bridge.m
 *
 * Objective-C Metal bridge for scheng-output-syphon.
 * Wraps SyphonMetalServer in a C-compatible API callable from Rust FFI.
 *
 * Compiled via cc crate in build.rs with -fobjc-arc.
 * Links against Syphon.framework, Metal.framework, Foundation.framework.
 */

@import Foundation;
@import Metal;

// Forward-declare SyphonMetalServer so we don't need to import the full
// Syphon headers here (the framework is linked, not header-included).
// If your Syphon.framework has a different API, adjust accordingly.
@interface SyphonMetalServer : NSObject
- (instancetype)initWithName:(NSString*)name
                      device:(id<MTLDevice>)device
                     options:(NSDictionary*)options;
- (void)publishFrameTexture:(id<MTLTexture>)texture;
- (BOOL)hasClients;
- (void)stop;
@end

#include "syphon_metal_bridge.h"

// ── Create ────────────────────────────────────────────────────────────────

void* scheng_syphon_create(const char* name, void* mtl_device) {
    @autoreleasepool {
        NSString* serverName = name
            ? [NSString stringWithUTF8String:name]
            : @"scheng";

        id<MTLDevice> device = (__bridge id<MTLDevice>)mtl_device;
        if (!device) {
            NSLog(@"[scheng-syphon] ERROR: nil MTLDevice passed to scheng_syphon_create");
            return NULL;
        }

        SyphonMetalServer* server = [[SyphonMetalServer alloc]
            initWithName:serverName
                  device:device
                 options:nil];

        if (!server) {
            NSLog(@"[scheng-syphon] ERROR: SyphonMetalServer alloc/init returned nil");
            return NULL;
        }

        NSLog(@"[scheng-syphon] Syphon server '%@' created on %@",
              serverName, [device name]);

        // Bridge-retain: ownership passes to Rust (freed in scheng_syphon_destroy).
        return (__bridge_retained void*)server;
    }
}

// ── Publish RGBA pixels ───────────────────────────────────────────────────

void scheng_syphon_publish_rgba(
    void*                server_ptr,
    const unsigned char* rgba,
    uint32_t             width,
    uint32_t             height,
    void*                mtl_device
) {
    if (!server_ptr || !rgba || width == 0 || height == 0) return;

    @autoreleasepool {
        SyphonMetalServer* server = (__bridge SyphonMetalServer*)server_ptr;
        id<MTLDevice> device = (__bridge id<MTLDevice>)mtl_device;

        // Create a MTLTexture from the pixel data.
        // MTLStorageModeShared = unified memory (M1/M2) — zero extra copy.
        // Falls back to a managed blit on Intel Macs.
        MTLTextureDescriptor* desc = [MTLTextureDescriptor
            texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                         width:width
                                        height:height
                                     mipmapped:NO];
        desc.usage        = MTLTextureUsageShaderRead;
        desc.storageMode  = MTLStorageModeShared;

        id<MTLTexture> texture = [device newTextureWithDescriptor:desc];
        if (!texture) {
            NSLog(@"[scheng-syphon] ERROR: Failed to create MTLTexture for frame");
            return;
        }

        // Upload pixels. On unified memory (M1), this is a direct memory map.
        [texture replaceRegion:MTLRegionMake2D(0, 0, width, height)
                   mipmapLevel:0
                     withBytes:rgba
                   bytesPerRow:width * 4];

        // Publish to all Syphon clients.
        [server publishFrameTexture:texture];
    }
}

// ── Publish Metal texture (zero-copy path) ────────────────────────────────

void scheng_syphon_publish_texture(void* server_ptr, void* mtl_texture) {
    if (!server_ptr || !mtl_texture) return;
    @autoreleasepool {
        SyphonMetalServer* server = (__bridge SyphonMetalServer*)server_ptr;
        id<MTLTexture> texture = (__bridge id<MTLTexture>)mtl_texture;
        [server publishFrameTexture:texture];
    }
}

// ── Has clients ───────────────────────────────────────────────────────────

int scheng_syphon_has_clients(void* server_ptr) {
    if (!server_ptr) return 0;
    @autoreleasepool {
        SyphonMetalServer* server = (__bridge SyphonMetalServer*)server_ptr;
        return [server hasClients] ? 1 : 0;
    }
}

// ── Destroy ───────────────────────────────────────────────────────────────

void scheng_syphon_destroy(void* server_ptr) {
    if (!server_ptr) return;
    @autoreleasepool {
        // Bridge-transfer: take back ownership and ARC releases on scope exit.
        SyphonMetalServer* server = (__bridge_transfer SyphonMetalServer*)server_ptr;
        [server stop];
        NSLog(@"[scheng-syphon] Syphon server stopped");
        (void)server; // ARC releases here
    }
}
