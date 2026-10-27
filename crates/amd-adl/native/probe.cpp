// Read-only standalone probe; useful before setting up a Rust/MSVC toolchain.
#include "adl_bridge.h"
#include <cstdio>
#include <thread>
#include <chrono>
int main() {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    char error[512] = {};
    void* session = afc_open(error, sizeof(error));
    if (!session) { std::fprintf(stderr, "%s\n", error); return 1; }
    AfcAdapter adapters[128] = {};
    int count = afc_adapters(session, adapters, 128);
    for (int i = 0; i < count; ++i) {
        std::printf("GPU: %s | %s | OD %d\n", adapters[i].name, adapters[i].id, adapters[i].od_version);
        if (afc_select(session, adapters[i].index)) continue;
        AfcCurveInfo curve = {};
        if (!afc_curve_info(session, &curve)) {
            std::printf("Native curve: %d\n", curve.supported);
            for (int p = 0; p < 5; ++p)
                std::printf("Point %d: current=%dC/%d%% min=%dC/%d%% max=%dC/%d%%\n", p,
                    curve.current[p].temperature, curve.current[p].speed,
                    curve.minimum[p].temperature, curve.minimum[p].speed,
                    curve.maximum[p].temperature, curve.maximum[p].speed);
        } else std::fprintf(stderr, "%s\n", afc_error(session));
        for (int j = 0; j < 3; ++j) {
            std::this_thread::sleep_for(std::chrono::seconds(2));
            AfcSample sample = {};
            if (afc_sample(session, &sample)) std::fprintf(stderr, "%s\n", afc_error(session));
            else std::printf("Edge=%dC Hotspot=%dC RPM=%d Speed=%d%%\n", sample.edge,
                            sample.hotspot, sample.rpm, sample.percent);
        }
    }
    if (count < 0) std::fprintf(stderr, "%s\n", afc_error(session));
    afc_close(session);
    return count > 0 ? 0 : 1;
}
