@import Foundation;
@import Metal;

// Use the actual header from the framework
#import <Syphon/SyphonMetalServer.h>

#include "syphon_metal_bridge.h"

typedef struct {
    SyphonMetalServer* __strong server;
    id<MTLDevice>      __strong device;
    id<MTLCommandQueue> __strong queue;
    id<MTLTexture>     __strong staging;
    uint32_t           stage_w;
    uint32_t           stage_h;
} SyphonCtx;

void* scheng_syphon_create(const char* name) {
    @autoreleasepool {
        @try {
            NSString* serverName = name ? [NSString stringWithUTF8String:name] : @"scheng";
            id<MTLDevice> device = MTLCreateSystemDefaultDevice();
            if (!device) return NULL;

            SyphonMetalServer* server = [[SyphonMetalServer alloc]
                initWithName:serverName device:device options:nil];
            if (!server) return NULL;

            id<MTLCommandQueue> queue = [device newCommandQueue];
            if (!queue) return NULL;

            SyphonCtx* ctx = (SyphonCtx*)calloc(1, sizeof(SyphonCtx));
            ctx->server = server;
            ctx->device = device;
            ctx->queue  = queue;
            NSLog(@"[scheng-syphon] Server '%@' on %@", serverName, device.name);
            return (void*)ctx;
        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] create exception: %@", e);
            return NULL;
        }
    }
}

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
                    texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                                 width:width height:height mipmapped:NO];
                desc.storageMode = MTLStorageModeShared;
                desc.usage = MTLTextureUsageShaderRead;
                ctx->staging = [ctx->device newTextureWithDescriptor:desc];
                ctx->stage_w = width;
                ctx->stage_h = height;
            }

            [ctx->staging replaceRegion:MTLRegionMake2D(0, 0, width, height)
                            mipmapLevel:0
                              withBytes:rgba
                            bytesPerRow:width * 4];

            // publishFrameTexture:onCommandBuffer:imageRegion:flipped: is the correct API
            id<MTLCommandBuffer> cmd = [ctx->queue commandBuffer];
            NSRect region = NSMakeRect(0, 0, width, height);
            [ctx->server publishFrameTexture:ctx->staging
                             onCommandBuffer:cmd
                                 imageRegion:region
                                     flipped:NO];
            [cmd commit];

        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] publish exception: %@", e);
        }
    }
}

int scheng_syphon_has_clients(void* server_ptr) {
    if (!server_ptr) return 0;
    @try {
        return ((SyphonCtx*)server_ptr)->server.hasClients ? 1 : 0;
    } @catch(...) { return 0; }
}

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
        } @catch (NSException* e) {
            NSLog(@"[scheng-syphon] destroy exception: %@", e);
        }
    }
}
