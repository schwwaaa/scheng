/*
 * spout_bridge.cpp
 *
 * Spout2 sender bridge for scheng-output-spout. Windows only.
 *
 * Requires Spout2 SDK source at vendor/Spout2/SPOUT_SDK/ (workspace root):
 *   git clone https://github.com/leadedge/Spout2 vendor/Spout2
 *
 * Spout2 uses OpenGL internally for GPU-to-GPU texture sharing.
 * This bridge creates a hidden OpenGL context so callers don't need one.
 */

#include "spout_bridge.h"

#ifdef _WIN32

#include "SpoutSender.h"  // from Spout2/SPOUT_SDK/

#include <cstdio>
#include <cstring>
#include <windows.h>
#include <GL/gl.h>

// ── Internal context ──────────────────────────────────────────────────────

struct SpoutCtx {
    SpoutSender* sender;
    char         name[256];
    GLuint       tex_id;
    uint32_t     tex_w;
    uint32_t     tex_h;
    HGLRC        gl_ctx;
    HDC          hdc;
    HWND         hwnd;
};

static bool create_gl_context(SpoutCtx* ctx) {
    // Create a hidden message-only window to own the DC
    ctx->hwnd = CreateWindowA(
        "STATIC", "scheng_spout_gl",
        WS_POPUP, 0, 0, 1, 1,
        HWND_MESSAGE, NULL, NULL, NULL
    );
    if (!ctx->hwnd) return false;

    ctx->hdc = GetDC(ctx->hwnd);
    if (!ctx->hdc) return false;

    PIXELFORMATDESCRIPTOR pfd = {};
    pfd.nSize      = sizeof(pfd);
    pfd.nVersion   = 1;
    pfd.dwFlags    = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER;
    pfd.iPixelType = PFD_TYPE_RGBA;
    pfd.cColorBits = 32;

    int fmt = ChoosePixelFormat(ctx->hdc, &pfd);
    if (!fmt || !SetPixelFormat(ctx->hdc, fmt, &pfd)) return false;

    ctx->gl_ctx = wglCreateContext(ctx->hdc);
    if (!ctx->gl_ctx) return false;

    wglMakeCurrent(ctx->hdc, ctx->gl_ctx);
    return true;
}

static void destroy_gl_context(SpoutCtx* ctx) {
    if (ctx->gl_ctx) {
        wglMakeCurrent(NULL, NULL);
        wglDeleteContext(ctx->gl_ctx);
        ctx->gl_ctx = NULL;
    }
    if (ctx->hdc && ctx->hwnd) {
        ReleaseDC(ctx->hwnd, ctx->hdc);
        ctx->hdc = NULL;
    }
    if (ctx->hwnd) {
        DestroyWindow(ctx->hwnd);
        ctx->hwnd = NULL;
    }
}

static void ensure_texture(SpoutCtx* ctx, uint32_t w, uint32_t h) {
    if (ctx->tex_id && ctx->tex_w == w && ctx->tex_h == h) return;

    if (ctx->tex_id) glDeleteTextures(1, &ctx->tex_id);

    glGenTextures(1, &ctx->tex_id);
    glBindTexture(GL_TEXTURE_2D, ctx->tex_id);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    glBindTexture(GL_TEXTURE_2D, 0);

    ctx->tex_w = w;
    ctx->tex_h = h;
}

// ── Public C API ──────────────────────────────────────────────────────────

extern "C" {

void* scheng_spout_create(const char* name) {
    SpoutCtx* ctx = new SpoutCtx{};
    strncpy_s(ctx->name, sizeof(ctx->name), name, _TRUNCATE);

    if (!create_gl_context(ctx)) {
        fprintf(stderr, "[scheng-output-spout] OpenGL context creation failed\n");
        delete ctx;
        return nullptr;
    }

    ctx->sender = new SpoutSender();
    if (!ctx->sender->CreateSender(ctx->name, 1, 1)) {
        fprintf(stderr, "[scheng-output-spout] SpoutSender::CreateSender failed for '%s'\n", name);
        delete ctx->sender;
        destroy_gl_context(ctx);
        delete ctx;
        return nullptr;
    }

    fprintf(stderr, "[scheng-output-spout] Sender '%s' created\n", name);
    return (void*)ctx;
}

void scheng_spout_send_rgba(
    void*          sender_ptr,
    const uint8_t* rgba,
    uint32_t       w,
    uint32_t       h
) {
    if (!sender_ptr || !rgba || w == 0 || h == 0) return;
    SpoutCtx* ctx = (SpoutCtx*)sender_ptr;

    wglMakeCurrent(ctx->hdc, ctx->gl_ctx);
    ensure_texture(ctx, w, h);

    // Upload CPU RGBA pixels into the GL texture
    glBindTexture(GL_TEXTURE_2D, ctx->tex_id);
    glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, w, h, GL_RGBA, GL_UNSIGNED_BYTE, rgba);
    glBindTexture(GL_TEXTURE_2D, 0);

    // Share via Spout
    ctx->sender->SendTexture(ctx->tex_id, GL_TEXTURE_2D, w, h);
}

int scheng_spout_get_receiver_count(void* sender_ptr) {
    if (!sender_ptr) return 0;
    SpoutCtx* ctx = (SpoutCtx*)sender_ptr;
    // SpoutSender doesn't expose receiver count directly in all versions;
    // return 1 if sender is active as a reasonable proxy.
    return ctx->tex_id ? 1 : 0;
}

void scheng_spout_destroy(void* sender_ptr) {
    if (!sender_ptr) return;
    SpoutCtx* ctx = (SpoutCtx*)sender_ptr;

    wglMakeCurrent(ctx->hdc, ctx->gl_ctx);

    if (ctx->tex_id) {
        glDeleteTextures(1, &ctx->tex_id);
        ctx->tex_id = 0;
    }

    if (ctx->sender) {
        ctx->sender->ReleaseSender();
        delete ctx->sender;
        ctx->sender = nullptr;
    }

    destroy_gl_context(ctx);
    delete ctx;
    fprintf(stderr, "[scheng-output-spout] Sender destroyed\n");
}

} // extern "C"

#else // !_WIN32

// Non-Windows stubs so the file compiles on macOS/Linux (not linked)
extern "C" {
void* scheng_spout_create(const char*) { return nullptr; }
void  scheng_spout_send_rgba(void*, const unsigned char*, unsigned int, unsigned int) {}
int   scheng_spout_get_receiver_count(void*) { return 0; }
void  scheng_spout_destroy(void*) {}
}

#endif // _WIN32
