#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <OpenGL/OpenGL.h>
#import <OpenGL/gl3.h>

#import "syphon_bridge.h"

// Syphon headers from the bundled framework
#import <Syphon/Syphon.h>

@interface schengSyphonServerBox : NSObject
@property (nonatomic, strong) SyphonOpenGLServer *server;
@property (nonatomic, assign) CGLContextObj cgl;
@end

@implementation schengSyphonServerBox
@end

void* syphon_server_create(const char* name_utf8) {
    @autoreleasepool {
        NSString *name = @"scheng";
        if (name_utf8 && name_utf8[0] != 0) {
            name = [NSString stringWithUTF8String:name_utf8];
            if (!name) name = @"scheng";
        }

        // Use the current CGL context. Caller must have made a GL context current.
        CGLContextObj cgl = CGLGetCurrentContext();
        if (!cgl) return NULL;

        schengSyphonServerBox *box = [schengSyphonServerBox new];
        box.cgl = cgl;

        // Create Syphon server for current context.
        box.server = [[SyphonOpenGLServer alloc] initWithName:name context:cgl options:nil];
        if (!box.server) return NULL;

        // Transfer ownership to Rust (opaque pointer).
        return (__bridge_retained void *)box;
    }
}

void syphon_server_destroy(void* server) {
    @autoreleasepool {
        if (!server) return;
        schengSyphonServerBox *box = (__bridge_transfer schengSyphonServerBox *)server;
        box.server = nil;
    }
}

void syphon_server_publish_texture(void* server, uint32_t tex, int32_t w, int32_t h, bool flipped) {
    @autoreleasepool {
        if (!server) return;
        schengSyphonServerBox *box = (__bridge schengSyphonServerBox *)server;
        if (!box.server) return;

        // Ensure we're on the same current context; Syphon expects the context used at init.
        // (Caller should manage context current-ness; we don't call CGLSetCurrentContext here.)
        GLenum target = GL_TEXTURE_2D;

        // Construct the texture region.
        NSRect region = NSMakeRect(0.0, 0.0, (CGFloat)w, (CGFloat)h);

        // The flipped flag can be passed via options.
        NSDictionary *options = flipped ? @{ SyphonServerOptionIsFrameFlipped : @YES } : nil;

        [box.server publishFrameTexture:tex
                          textureTarget:target
                            imageRegion:region
                      textureDimensions:NSMakeSize((CGFloat)w, (CGFloat)h)
                                flipped:flipped
                                options:options];
    }
}
