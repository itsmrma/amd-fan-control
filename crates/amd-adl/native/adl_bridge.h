#pragma once
#include <stdint.h>

// Fixed C ABI; no ADL structures or C++ ownership cross into Rust.
struct AfcAdapter {
    int32_t index;
    char id[256];
    char name[256];
    int32_t od_version;
};
struct AfcPoint { int32_t temperature; int32_t speed; };
struct AfcCurveInfo {
    int32_t supported;
    AfcPoint current[5];
    AfcPoint minimum[5];
    AfcPoint maximum[5];
};
struct AfcSample {
    int32_t edge;
    int32_t hotspot;
    int32_t rpm;
    int32_t percent;
};
extern "C" {
void* afc_open(char* error, int32_t size);
void afc_close(void* handle);
int32_t afc_adapters(void* handle, AfcAdapter* output, int32_t capacity);
int32_t afc_select(void* handle, int32_t adapter);
int32_t afc_curve_info(void* handle, AfcCurveInfo* output);
int32_t afc_sample(void* handle, AfcSample* output);
int32_t afc_apply(void* handle, const AfcPoint* points);
int32_t afc_reset(void* handle);
const char* afc_error(void* handle);
}

