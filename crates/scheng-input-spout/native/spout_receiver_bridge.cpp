/*
 * spout_receiver_bridge.cpp
 *
 * Spout2 receiver bridge for scheng-input-spout. Windows only.
 *
 * Requires Spout2 SDK source at vendor/Spout2/SPOUT_SDK/.
 * Get it from: https://github.com/leadedge/Spout2
 *
 * Build pattern mirrors scheng-output-spout's sender bridge.
 */

#include "spout_receiver_bridge.h"

// TODO: include Spout2 SDK headers once vendor/Spout2 is placed
// #include "SpoutReceiver.h"

#include <cstring>
#include <cstdio>

// ── Stub implementation ───────────────────────────────────────────────────
// Replace the stub bodies below with real SpoutReceiver calls once
// vendor/Spout2/SPOUT_SDK/ is present.
//
// Pattern:
//   void* scheng_spout_receiver_create() {
//       SpoutReceiver* r = new SpoutReceiver();
//       return (void*)r;
//   }
//
//   int scheng_spout_receiver_connect(void* receiver, const char* sender_name) {
//       SpoutReceiver* r = (SpoutReceiver*)receiver;
//       return r->SetActiveSender(sender_name) ? 1 : 0;
//   }
//
//   int scheng_spout_receiver_pull_rgba(
//       void* receiver, unsigned char* out_rgba,
//       unsigned int* out_width, unsigned int* out_height)
//   {
//       SpoutReceiver* r = (SpoutReceiver*)receiver;
//       unsigned int w = 0, h = 0;
//       if (!r->ReceiveImage(out_rgba, GL_RGBA, false)) return 0;
//       r->GetSenderSize(r->GetActiveSender(), w, h);  // adjust to actual API
//       *out_width  = w;
//       *out_height = h;
//       return 1;
//   }

extern "C" {

void* scheng_spout_receiver_create(void) {
    // TODO: return new SpoutReceiver();
    fprintf(stderr, "[scheng-input-spout] Spout2 SDK not yet wired — stub\n");
    return nullptr;
}

int scheng_spout_receiver_connect(void* receiver, const char* sender_name) {
    (void)receiver; (void)sender_name;
    return 0;
}

int scheng_spout_receiver_pull_rgba(
    void*          receiver,
    unsigned char* out_rgba,
    uint32_t*      out_width,
    uint32_t*      out_height
) {
    (void)receiver; (void)out_rgba; (void)out_width; (void)out_height;
    return 0;
}

int scheng_spout_receiver_is_connected(void* receiver) {
    (void)receiver;
    return 0;
}

uint32_t scheng_spout_receiver_list_senders(
    char**   out_names,
    uint32_t max_count,
    uint32_t name_buf_len
) {
    (void)out_names; (void)max_count; (void)name_buf_len;
    return 0;
}

void scheng_spout_receiver_destroy(void* receiver) {
    (void)receiver;
    // TODO: delete (SpoutReceiver*)receiver;
}

} // extern "C"
