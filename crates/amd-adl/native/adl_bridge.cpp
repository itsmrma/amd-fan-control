#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <new>
#include <string>
#include <vector>
#include "adl_sdk.h"
#include "adl_bridge.h"

// ADL exports use cdecl. Only its malloc callback uses stdcall (SDK definition).
using Create = int (*)(ADL_MAIN_MALLOC_CALLBACK, int, ADL_CONTEXT_HANDLE*);
using Destroy = int (*)(ADL_CONTEXT_HANDLE);
using Count = int (*)(ADL_CONTEXT_HANDLE, int*);
using Info = int (*)(ADL_CONTEXT_HANDLE, AdapterInfo*, int);
using Caps = int (*)(ADL_CONTEXT_HANDLE, int, int*, int*, int*);
using Init = int (*)(ADL_CONTEXT_HANDLE, int, ADLOD8InitSetting*);
using Current = int (*)(ADL_CONTEXT_HANDLE, int, ADLOD8CurrentSetting*);
using Set = int (*)(ADL_CONTEXT_HANDLE, int, ADLOD8SetSetting*, ADLOD8CurrentSetting*);
using InitX = int (*)(ADL_CONTEXT_HANDLE, int, int*, int*, ADLOD8SingleInitSetting**);
using CurrentX = int (*)(ADL_CONTEXT_HANDLE, int, int*, int**);
using LegacyLog = int (*)(ADL_CONTEXT_HANDLE, int, ADLPMLogDataOutput*);
using LogSupport = int (*)(ADL_CONTEXT_HANDLE, int, int*, int);
using DeviceCreate = int (*)(ADL_CONTEXT_HANDLE, int, ADL_D3DKMT_HANDLE*);
using DeviceDestroy = int (*)(ADL_CONTEXT_HANDLE, ADL_D3DKMT_HANDLE);
using LogStart = int (*)(ADL_CONTEXT_HANDLE, int, int, int, int*, ADL_D3DKMT_HANDLE*, void**, int);
using LogRead = int (*)(ADL_CONTEXT_HANDLE, int, int, int*, void**, ADLPMLogDataOutput*);
using LogStop = int (*)(ADL_CONTEXT_HANDLE, int, ADL_D3DKMT_HANDLE*);

static void* __stdcall allocate(int size) { return size > 0 ? std::malloc(size) : nullptr; }

struct Session {
    HMODULE dll = nullptr;
    ADL_CONTEXT_HANDLE context = nullptr;
    int selected = -1;
    std::string error;
    Destroy destroy = nullptr;
    Count count = nullptr;
    Info info = nullptr;
    Caps caps = nullptr;
    Init init = nullptr;
    InitX init_x = nullptr;
    Current current = nullptr;
    CurrentX current_x = nullptr;
    Set set = nullptr;
    LegacyLog legacy_log = nullptr;
    DeviceCreate device_create = nullptr;
    DeviceDestroy device_destroy = nullptr;
    LogSupport log_support = nullptr;
    LogStart log_start = nullptr;
    LogRead log_read = nullptr;
    LogStop log_stop = nullptr;
    ADL_D3DKMT_HANDLE device = 0;
    void* shared = nullptr;
    bool logging = false;
    HANDLE control_mutex = nullptr;
    bool locked = false;

    template<class T> T symbol(const char* name) {
        return reinterpret_cast<T>(GetProcAddress(dll, name));
    }
    int fail(const char* operation, int result = -1) {
        error = std::string(operation) + " (ADL " + std::to_string(result) + ")";
        return -1;
    }
    void stop_log() {
        if (logging && log_stop) log_stop(context, selected, &device);
        if (device && device_destroy) device_destroy(context, device);
        device = 0; shared = nullptr; logging = false;
    }
    ~Session() {
        stop_log();
        if (locked) ReleaseMutex(control_mutex);
        if (control_mutex) CloseHandle(control_mutex);
        if (context && destroy) destroy(context);
        if (dll) FreeLibrary(dll);
    }
    int get_init(ADLOD8InitSetting& out) {
        out = {}; out.count = OD8_COUNT;
        int result;
        if (init_x) {
            int length = OD8_COUNT, capability = 0;
            ADLOD8SingleInitSetting* list = nullptr;
            result = init_x(context, selected, &capability, &length, &list);
            if (result == ADL_OK && list && length > 0 && length <= 4096) {
                out.count = std::min(length, static_cast<int>(OD8_COUNT));
                out.overdrive8Capabilities = capability;
                std::copy_n(list, out.count, out.od8SettingTable);
            } else if (result == ADL_OK) result = ADL_ERR;
            std::free(list);
        } else result = init ? init(context, selected, &out) : ADL_ERR_NOT_SUPPORTED;
        return result == ADL_OK ? 0 : fail("OD8 Init settings", result);
    }
    int get_current(ADLOD8CurrentSetting& out) {
        out = {}; out.count = OD8_COUNT;
        int result;
        if (current_x) {
            int length = OD8_COUNT; int* list = nullptr;
            result = current_x(context, selected, &length, &list);
            if (result == ADL_OK && list && length > 0 && length <= 4096) {
                out.count = std::min(length, static_cast<int>(OD8_COUNT));
                std::copy_n(list, out.count, out.Od8SettingTable);
            } else if (result == ADL_OK) result = ADL_ERR;
            std::free(list);
        } else result = current ? current(context, selected, &out) : ADL_ERR_NOT_SUPPORTED;
        return result == ADL_OK ? 0 : fail("OD8 Current settings", result);
    }
};

extern "C" void* afc_open(char* error, int32_t size) {
    auto* s = new(std::nothrow) Session;
    if (!s) return nullptr;
    // Never search cwd, PATH or the application directory for a driver DLL.
    s->dll = LoadLibraryExW(L"atiadlxx.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!s->dll) s->fail("AMD driver DLL missing from System32", GetLastError());
    else {
        auto create = s->symbol<Create>("ADL2_Main_Control_Create");
        s->destroy = s->symbol<Destroy>("ADL2_Main_Control_Destroy");
        s->count = s->symbol<Count>("ADL2_Adapter_NumberOfAdapters_Get");
        s->info = s->symbol<Info>("ADL2_Adapter_AdapterInfo_Get");
        s->caps = s->symbol<Caps>("ADL2_Overdrive_Caps");
        s->init = s->symbol<Init>("ADL2_Overdrive8_Init_Setting_Get");
        s->init_x = s->symbol<InitX>("ADL2_Overdrive8_Init_SettingX2_Get");
        s->current = s->symbol<Current>("ADL2_Overdrive8_Current_Setting_Get");
        s->current_x = s->symbol<CurrentX>("ADL2_Overdrive8_Current_SettingX2_Get");
        s->set = s->symbol<Set>("ADL2_Overdrive8_Setting_Set");
        s->legacy_log = s->symbol<LegacyLog>("ADL2_New_QueryPMLogData_Get");
        s->device_create = s->symbol<DeviceCreate>("ADL2_Device_PMLog_Device_Create");
        s->device_destroy = s->symbol<DeviceDestroy>("ADL2_Device_PMLog_Device_Destroy");
        s->log_support = s->symbol<LogSupport>("ADL2_Overdrive8_PMLog_ShareMemory_Support");
        s->log_start = s->symbol<LogStart>("ADL2_Overdrive8_PMLog_ShareMemory_Start");
        s->log_read = s->symbol<LogRead>("ADL2_Overdrive8_PMLog_ShareMemory_Read");
        s->log_stop = s->symbol<LogStop>("ADL2_Overdrive8_PMLog_ShareMemory_Stop");
        if (!create || !s->destroy || !s->count || !s->info || !s->caps)
            s->fail("Required ADL2 exports missing");
        else {
            int r = create(allocate, 1, &s->context);
            if (r == ADL_OK && s->context) return s;
            s->fail("ADL2_Main_Control_Create", r);
        }
    }
    if (error && size > 0) std::snprintf(error, size, "%s", s->error.c_str());
    delete s;
    return nullptr;
}
extern "C" void afc_close(void* h) { delete static_cast<Session*>(h); }
extern "C" const char* afc_error(void* h) { return static_cast<Session*>(h)->error.c_str(); }
extern "C" int32_t afc_adapters(void* h, AfcAdapter* output, int32_t capacity) {
    auto& s = *static_cast<Session*>(h);
    int n = 0;
    int r = s.count(s.context, &n);
    if (r != ADL_OK || n <= 0 || n > 128) return s.fail("Adapter count", r);
    std::vector<AdapterInfo> adapters(n);
    for (auto& a : adapters) a.iSize = sizeof(AdapterInfo);
    r = s.info(s.context, adapters.data(), n * sizeof(AdapterInfo));
    if (r != ADL_OK) return s.fail("Adapter enumeration", r);
    int written = 0;
    std::vector<std::string> seen;
    for (const auto& a : adapters) {
        // ADL on Windows reports decimal 1002, some implementations report 0x1002.
        if (!a.iPresent || (a.iVendorID != 1002 && a.iVendorID != 0x1002)) continue;
        char id[256];
        std::snprintf(id, sizeof(id), "pci:%d:%d:%d", a.iBusNumber,
                      a.iDeviceNumber, a.iFunctionNumber);
        if (std::find(seen.begin(), seen.end(), id) != seen.end()) continue;
        seen.emplace_back(id);
        if (written >= capacity) return s.fail("Too many AMD adapters");
        auto& o = output[written++]; o = {}; o.index = a.iAdapterIndex;
        std::snprintf(o.id, sizeof(o.id), "%s", id);
        std::snprintf(o.name, sizeof(o.name), "%.255s", a.strAdapterName);
        int supported = 0, enabled = 0, version = 0;
        if (s.caps(s.context, a.iAdapterIndex, &supported, &enabled, &version) == ADL_OK && supported && enabled)
            o.od_version = version;
    }
    return written;
}
extern "C" int32_t afc_select(void* h, int32_t adapter) {
    auto& s = *static_cast<Session*>(h); s.stop_log(); s.selected = adapter;
    if (s.locked) return s.fail("Release fan control before selecting a GPU");
    if (s.control_mutex) CloseHandle(s.control_mutex);
    std::wstring mutex_name = L"Local\\AMD-Fan-Control-ADL-" + std::to_wstring(adapter);
    s.control_mutex = CreateMutexW(nullptr, FALSE, mutex_name.c_str());
    if (!s.control_mutex) return s.fail("Cannot create fan control mutex", GetLastError());
    int supported = 0;
    if (s.log_support && s.device_create && s.device_destroy && s.log_start && s.log_read && s.log_stop &&
        s.log_support(s.context, adapter, &supported, 0) == ADL_OK && supported &&
        s.device_create(s.context, adapter, &s.device) == ADL_OK) {
        int r = s.log_start(s.context, adapter, 1000, -1, nullptr, &s.device, &s.shared, 0);
        s.logging = r == ADL_OK && s.shared;
        if (!s.logging) s.stop_log();
    }
    if (!s.logging && !s.legacy_log) return s.fail("No supported temperature API");
    return 0;
}
extern "C" int32_t afc_curve_info(void* h, AfcCurveInfo* output) {
    auto& s = *static_cast<Session*>(h); *output = {};
    ADLOD8InitSetting init = {}; ADLOD8CurrentSetting current = {};
    if (s.get_init(init) || s.get_current(current)) return -1;
    if (!(init.overdrive8Capabilities & ADL_OD8_FAN_CURVE) || !s.set ||
        init.count <= OD8_FAN_CURVE_SPEED_5 || current.count <= OD8_FAN_CURVE_SPEED_5) return 0;
    output->supported = 1;
    for (int i = 0; i < 5; ++i) {
        int t = OD8_FAN_CURVE_TEMPERATURE_1 + i * 2, v = t + 1;
        output->current[i] = {current.Od8SettingTable[t], current.Od8SettingTable[v]};
        output->minimum[i] = {init.od8SettingTable[t].minValue, init.od8SettingTable[v].minValue};
        output->maximum[i] = {init.od8SettingTable[t].maxValue, init.od8SettingTable[v].maxValue};
    }
    return 0;
}
extern "C" int32_t afc_sample(void* h, AfcSample* output) {
    auto& s = *static_cast<Session*>(h);
    ADLPMLogDataOutput data = {}; data.size = sizeof(data);
    // An explicit list avoids interpreting driver-owned shared memory in Rust.
    int sensors[] = {ADL_PMLOG_TEMPERATURE_EDGE, ADL_PMLOG_TEMPERATURE_HOTSPOT,
                     ADL_PMLOG_FAN_RPM, ADL_PMLOG_FAN_PERCENTAGE};
    int r = s.logging ? s.log_read(s.context, s.selected, 4, sensors, &s.shared, &data)
                      : s.legacy_log(s.context, s.selected, &data);
    // Some driver versions export shared-memory logging but return empty support flags.
    if (s.logging && s.legacy_log && (r != ADL_OK ||
        (!data.sensors[sensors[0]].supported && !data.sensors[sensors[1]].supported))) {
        data = {}; data.size = sizeof(data);
        r = s.legacy_log(s.context, s.selected, &data);
    }
    if (r != ADL_OK) return s.fail("Temperature/PMLog read", r);
    auto value = [&](int id) { return data.sensors[id].supported ? data.sensors[id].value : -1; };
    *output = {value(sensors[0]), value(sensors[1]), value(sensors[2]), value(sensors[3])};
    return 0;
}
static int write_curve(Session& s, const AfcPoint* points, bool reset) {
    AfcCurveInfo info = {};
    if (afc_curve_info(&s, &info) || !info.supported) return s.fail("OD8 fan curve unavailable");
    ADLOD8SetSetting settings = {}; settings.count = OD8_COUNT;
    ADLOD8CurrentSetting current = {};
    if (s.get_current(current)) return -1;
    for (int i = 0; i < 5; ++i) {
        if (!reset && (points[i].temperature < info.minimum[i].temperature ||
            points[i].temperature > info.maximum[i].temperature ||
            points[i].speed < info.minimum[i].speed || points[i].speed > info.maximum[i].speed ||
            (i && (points[i].temperature <= points[i-1].temperature || points[i].speed < points[i-1].speed))))
            return s.fail("Fan curve outside driver limits or non-monotonic");
        int t = OD8_FAN_CURVE_TEMPERATURE_1 + 2 * i;
        settings.od8SettingTable[t] = {reset ? 0 : points[i].temperature, 1, reset ? 1 : 0};
        settings.od8SettingTable[t+1] = {reset ? 0 : points[i].speed, 1, reset ? 1 : 0};
    }
    ADLOD8InitSetting init = {};
    if (s.get_init(init)) return -1;
    if (init.overdrive8Capabilities & ADL_OD8_FAN_ZERO_RPM_CONTROL)
        settings.od8SettingTable[OD8_FAN_ZERORPM_CONTROL] = {0, 1, reset ? 1 : 0};
    // Only fan-related entries have requested=1; clocks/voltage/power are untouched.
    if (!s.locked) {
        DWORD wait = WaitForSingleObject(s.control_mutex, 0);
        if (wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED)
            return s.fail("Another AMD Fan Control instance owns this GPU");
        s.locked = true;
    }
    int r = s.set(s.context, s.selected, &settings, &current);
    if (r != ADL_OK) return s.fail("OD8 fan settings write", r);
    if (!reset) {
        if (s.get_current(current)) return -1;
        for (int i = 0; i < 5; ++i) {
            int t = OD8_FAN_CURVE_TEMPERATURE_1 + 2 * i;
            if (current.Od8SettingTable[t] != points[i].temperature || current.Od8SettingTable[t+1] != points[i].speed)
                return s.fail("Driver did not retain requested curve");
        }
    }
    if (reset && s.locked) { ReleaseMutex(s.control_mutex); s.locked = false; }
    return 0;
}
extern "C" int32_t afc_apply(void* h, const AfcPoint* points) {
    return write_curve(*static_cast<Session*>(h), points, false);
}
extern "C" int32_t afc_reset(void* h) {
    return write_curve(*static_cast<Session*>(h), nullptr, true);
}
