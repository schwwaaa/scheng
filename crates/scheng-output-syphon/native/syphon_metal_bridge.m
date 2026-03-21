/*
 * syphon_metal_bridge.m
 *
 * Two publish paths:
 *
 * scheng_syphon_publish_rgba   — wgpu 22: CPU readback → staging texture → Syphon
 * scheng_syphon_publish_texture — wgpu 23+: raw MTLTexture pointer → Syphon directly
 *
 * The zero-copy path eliminates the 1-2 frame delay.
 */

@import Foundation;
@import Metal;
#import <Syphon/SyphonMetalServer.h>
#include "syphon_metal_bridge.h"

typedef struct {
    SyphonMetalServer*  __strong server;
    id<MTLDevice>       __strong device;
    id<MTLCommandQueue> __strong queue;
    id<MTLTexture>      __strong staging; // used by rgba path only
    uint32_t            stage_w;
    uint32_t            stage_h;
} SyphonCtx;

// ── Create ────────────────────────────────────────────────────────────────

void* scheng_syphon_create(const char* name) {
    @autoreleasepool {
        @try {
            NSString* serverName = name
                ? [NSString stringWithUTF8String:name]
                : @"scheng";

            id<MTLDevice> device = MTLCreateSystemDefaultDevice();
            if (!device) { NSLog(@"[scheng-syphon] No MTLDevice"); return NULL; }

            SyphonMetalServer* server = [[SyphonMetalServer alloc]
                initWithName:serverName device:device options:nil];
            if (!server) { NSLog(@"[scheng-syphon] Server init failed"); return NULL; }

            id<MTLCommandQueue> queue = [device newCommandQueue];
            if (!queue) { NSLog(@"[scheng-syphon] CommandQueue init failed"); return NULL; }

            SyphonCtx* ctx = (SyphonCtx*)calloc(1, sizeof(SyphonCtx));
            ctx->server  = server;
            ctx->device  = device;
            ctx->queue   = queue;
            ctx->staging = nil;
            ctx->stage_w = 0;
            ctx->stage_h = 0;

            NSLog(@"[scheng-syphon] Server '%@' ready on %@", serverName, device.name);
            return (void*)ctx;

        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] create exception: %@", e);
            return NULL;
        }
    }
}

// ── Publish: RGBA pixels (wgpu 22 readback path) ──────────────────────────

void scheng_syphon_publish_rgba(
    void* server_ptr, const unsigned char* rgba,
    uint32_t width, uint32_t height)
{
    if (!server_ptr || !rgba || width == 0 || height == 0) return;
    @autoreleasepool {
        @try {
            SyphonCtx* ctx = (SyphonCtx*)server_ptr;

            // Reallocate staging texture only when dimensions change
            if (!ctx->staging || ctx->stage_w != width || ctx->stage_h != height) {
                MTLTextureDescriptor* desc = [MTLTextureDescriptor
                    texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm_sRGB
                                                 width:width
                                                height:height
                                             mipmapped:NO];
                desc.storageMode = MTLStorageModeShared;
                desc.usage       = MTLTextureUsageShaderRead;
                ctx->staging = [ctx->device newTextureWithDescriptor:desc];
                ctx->stage_w = width;
                ctx->stage_h = height;
            }

            // Upload pixels
            [ctx->staging replaceRegion:MTLRegionMake2D(0, 0, width, height)
                            mipmapLevel:0
                              withBytes:rgba
                            bytesPerRow:width * 4];

            // Publish
            id<MTLCommandBuffer> cmd = [ctx->queue commandBuffer];
            [ctx->server publishFrameTexture:ctx->staging
                             onCommandBuffer:cmd
                                 imageRegion:NSMakeRect(0, 0, width, height)
                                     flipped:NO];
            [cmd commit];

        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] publish_rgba exception: %@", e);
        }
    }
}

// ── Publish: raw MTLTexture (wgpu 23+ zero-copy path) ─────────────────────

void scheng_syphon_publish_texture(
    void* server_ptr, void* mtl_texture,
    uint32_t width, uint32_t height)
{
    if (!server_ptr || !mtl_texture || width == 0 || height == 0) return;
    @autoreleasepool {
        @try {
            SyphonCtx*     ctx = (SyphonCtx*)server_ptr;
            // __bridge: wgpu still owns the texture, we only borrow it
            id<MTLTexture> tex = (__bridge id<MTLTexture>)mtl_texture;

            id<MTLCommandBuffer> cmd = [ctx->queue commandBuffer];
            [ctx->server publishFrameTexture:tex
                             onCommandBuffer:cmd
                                 imageRegion:NSMakeRect(0, 0, width, height)
                                     flipped:NO];
            [cmd commit];

        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] publish_texture exception: %@", e);
        }
    }
}

// ── Has clients ───────────────────────────────────────────────────────────

int scheng_syphon_has_clients(void* server_ptr) {
    if (!server_ptr) return 0;
    @try {
        return ((SyphonCtx*)server_ptr)->server.hasClients ? 1 : 0;
    } @catch (...) { return 0; }
}

// ── Destroy ───────────────────────────────────────────────────────────────

void scheng_syphon_destroy(void* server_ptr) {
    if (!server_ptr) return;
    @autoreleasepool {
        @try {
            SyphonCtx* ctx = (SyphonCtx*)server_ptr;
            [ctx->server stop];
            ctx->server  = nil;
            ctx->queue   = nil;
            ctx->device  = nil;
            ctx->staging = nil;
            free(ctx);
            NSLog(@"[scheng-syphon] Server stopped");
        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] destroy exception: %@", e);
        }
    }
}
