/*
 * spout_receiver_bridge.cpp
 *
 * Spout2 receiver bridge for scheng-input-spout. Windows only.
 *
 * Requires Spout2 SDK source at vendor/Spout2/SPOUT_SDK/ (workspace root):
 *   git clone https://github.com/leadedge/Spout2 vendor/Spout2
 */

#include "spout_receiver_bridge.h"

#ifdef _WIN32

#include "SpoutReceiver.h"

#include <cstdio>
#include <cstring>
#include <windows.h>
#include <GL/gl.h>

// ── Internal context ──────────────────────────────────────────────────────

struct SpoutRecvCtx {
    SpoutReceiver* receiver;
    GLuint         tex_id;
    uint32_t       tex_w;
    uint32_t       tex_h;
    HGLRC          gl_ctx;
    HDC            hdc;
    HWND           hwnd;
};

static bool create_gl_context_recv(SpoutRecvCtx* ctx) {
    ctx->hwnd = CreateWindowA(
        "STATIC", "scheng_spout_recv_gl",
        WS_POPUP, 0, 0, 1, 1,
        HWND_MESSAGE, NULL, NULL, NULL
    );
    if (!ctx->hwnd) return false;

    ctx->hdc = GetDC(ctx->hwnd);
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

static void destroy_gl_context_recv(SpoutRecvCtx* ctx) {
    if (ctx->gl_ctx) { wglMakeCurrent(NULL, NULL); wglDeleteContext(ctx->gl_ctx); ctx->gl_ctx = NULL; }
    if (ctx->hdc && ctx->hwnd) { ReleaseDC(ctx->hwnd, ctx->hdc); ctx->hdc = NULL; }
    if (ctx->hwnd) { DestroyWindow(ctx->hwnd); ctx->hwnd = NULL; }
}

// ── Public C API ──────────────────────────────────────────────────────────

extern "C" {

void* scheng_spout_receiver_create(void) {
    SpoutRecvCtx* ctx = new SpoutRecvCtx{};
    if (!create_gl_context_recv(ctx)) {
        fprintf(stderr, "[scheng-input-spout] GL context creation failed\n");
        delete ctx;
        return nullptr;
    }

    // Allocate a GL texture for receiving
    glGenTextures(1, &ctx->tex_id);
    ctx->receiver = new SpoutReceiver();
    return (void*)ctx;
}

int scheng_spout_receiver_connect(void* receiver_ptr, const char* sender_name) {
    if (!receiver_ptr) return 0;
    SpoutRecvCtx* ctx = (SpoutRecvCtx*)receiver_ptr;
    wglMakeCurrent(ctx->hdc, ctx->gl_ctx);
    return ctx->receiver->SetActiveSender(sender_name) ? 1 : 0;
}

int scheng_spout_receiver_pull_rgba(
    void*          receiver_ptr,
    unsigned char* out_rgba,
    uint32_t*      out_width,
    uint32_t*      out_height
) {
    if (!receiver_ptr || !out_rgba) return 0;
    SpoutRecvCtx* ctx = (SpoutRecvCtx*)receiver_ptr;
    wglMakeCurrent(ctx->hdc, ctx->gl_ctx);

    unsigned int w = 0, h = 0;
    char name[256] = {};
    // ReceiveTexture fills our GL texture from the shared Spout texture
    if (!ctx->receiver->ReceiveTexture(name, sizeof(name), ctx->tex_id, GL_TEXTURE_2D)) {
        return 0;
    }

    ctx->receiver->GetSenderSize(name, w, h);
    if (w == 0 || h == 0) return 0;

    // Readback GL texture → CPU RGBA buffer
    glBindTexture(GL_TEXTURE_2D, ctx->tex_id);
    glGetTexImage(GL_TEXTURE_2D, 0, GL_RGBA, GL_UNSIGNED_BYTE, out_rgba);
    glBindTexture(GL_TEXTURE_2D, 0);

    *out_width  = w;
    *out_height = h;
    return 1;
}

int scheng_spout_receiver_is_connected(void* receiver_ptr) {
    if (!receiver_ptr) return 0;
    SpoutRecvCtx* ctx = (SpoutRecvCtx*)receiver_ptr;
    return ctx->receiver->IsConnected() ? 1 : 0;
}

uint32_t scheng_spout_receiver_list_senders(
    char**   out_names,
    uint32_t max_count,
    uint32_t name_buf_len
) {
    SpoutReceiver tmp;
    int count = tmp.GetSenderCount();
    if (count < 0) count = 0;
    uint32_t n = (uint32_t)count < max_count ? (uint32_t)count : max_count;
    for (uint32_t i = 0; i < n; i++) {
        char name[256] = {};
        tmp.GetSender(i, name, sizeof(name));
        strncpy_s(out_names[i], name_buf_len, name, _TRUNCATE);
    }
    return n;
}

void scheng_spout_receiver_destroy(void* receiver_ptr) {
    if (!receiver_ptr) return;
    SpoutRecvCtx* ctx = (SpoutRecvCtx*)receiver_ptr;
    wglMakeCurrent(ctx->hdc, ctx->gl_ctx);
    if (ctx->tex_id) { glDeleteTextures(1, &ctx->tex_id); ctx->tex_id = 0; }
    if (ctx->receiver) { delete ctx->receiver; ctx->receiver = nullptr; }
    destroy_gl_context_recv(ctx);
    delete ctx;
}

} // extern "C"

#else // !_WIN32

extern "C" {
void*    scheng_spout_receiver_create(void)                                                            { return nullptr; }
int      scheng_spout_receiver_connect(void*, const char*)                                             { return 0; }
int      scheng_spout_receiver_pull_rgba(void*, unsigned char*, unsigned int*, unsigned int*)          { return 0; }
int      scheng_spout_receiver_is_connected(void*)                                                     { return 0; }
unsigned scheng_spout_receiver_list_senders(char**, unsigned, unsigned)                                { return 0; }
void     scheng_spout_receiver_destroy(void*)                                                          {}
}

#endif // _WIN32
