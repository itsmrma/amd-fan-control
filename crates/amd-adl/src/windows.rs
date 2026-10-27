use anyhow::{bail, Result};
use fan_core::{
    curve::{native_points, Point},
    hardware::{Adapter, CurveInfo, GpuBackend, Sample},
};
use std::{
    ffi::{c_char, c_void, CStr},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};

#[repr(C)]
#[derive(Copy, Clone)]
struct RawAdapter {
    index: i32,
    id: [c_char; 256],
    name: [c_char; 256],
    od_version: i32,
}
#[repr(C)]
#[derive(Default)]
struct RawCurve {
    supported: i32,
    zero_rpm_supported: i32,
    zero_rpm_enabled: i32,
    current: [Point; 5],
    minimum: [Point; 5],
    maximum: [Point; 5],
}
#[repr(C)]
#[derive(Default)]
struct RawSample {
    edge: i32,
    hotspot: i32,
    rpm: i32,
    percent: i32,
}

unsafe extern "C" {
    fn afc_open(error: *mut c_char, size: i32) -> *mut c_void;
    fn afc_close(handle: *mut c_void);
    fn afc_adapters(handle: *mut c_void, output: *mut RawAdapter, capacity: i32) -> i32;
    fn afc_select(handle: *mut c_void, adapter: i32) -> i32;
    fn afc_curve_info(handle: *mut c_void, output: *mut RawCurve) -> i32;
    fn afc_sample(handle: *mut c_void, output: *mut RawSample) -> i32;
    fn afc_apply(handle: *mut c_void, points: *const Point, zero_rpm: i32) -> i32;
    fn afc_reset(handle: *mut c_void) -> i32;
    fn afc_zero_rpm(handle: *mut c_void, enabled: i32) -> i32;
    fn afc_error(handle: *mut c_void) -> *const c_char;
}

pub struct AdlBackend {
    handle: NonNull<c_void>,
    selected: bool,
    owns_curve: bool,
    _thread_bound: PhantomData<Rc<()>>,
}

fn string(bytes: &[c_char]) -> String {
    let bytes: Vec<u8> = bytes
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

impl AdlBackend {
    pub fn open() -> Result<Self> {
        let mut error = [0; 512];
        // SAFETY: buffer is writable for its declared size. The bridge returns an owned session.
        let handle = unsafe { afc_open(error.as_mut_ptr(), error.len() as i32) };
        let Some(handle) = NonNull::new(handle) else {
            bail!("{}", string(&error));
        };
        Ok(Self {
            handle,
            selected: false,
            owns_curve: false,
            _thread_bound: PhantomData,
        })
    }
    fn check(&self, code: i32) -> Result<()> {
        if code < 0 {
            // SAFETY: the live session owns a NUL-terminated error string until the next call.
            let error = unsafe { CStr::from_ptr(afc_error(self.handle.as_ptr())) };
            bail!("{}", error.to_string_lossy());
        }
        Ok(())
    }
    fn require_selected(&self) -> Result<()> {
        if !self.selected {
            bail!("Select a GPU before accessing hardware");
        }
        Ok(())
    }
}

impl GpuBackend for AdlBackend {
    fn adapters(&mut self) -> Result<Vec<Adapter>> {
        // SAFETY: all-zero integer/byte fields are valid; capacity matches allocation.
        let mut out: [RawAdapter; 128] = unsafe { std::mem::zeroed() };
        let n = unsafe { afc_adapters(self.handle.as_ptr(), out.as_mut_ptr(), 128) };
        self.check(n)?;
        if n as usize > out.len() {
            bail!("ADL adapter count out of range");
        }
        Ok(out[..n as usize]
            .iter()
            .map(|a| Adapter {
                index: a.index,
                id: string(&a.id),
                name: string(&a.name),
                od_version: a.od_version,
            })
            .collect())
    }
    fn select(&mut self, adapter: &Adapter) -> Result<()> {
        if self.owns_curve {
            self.restore_automatic()?;
        }
        if !self
            .adapters()?
            .iter()
            .any(|a| a.id == adapter.id && a.index == adapter.index)
        {
            bail!("GPU no longer available");
        }
        // SAFETY: live, thread-local session and a freshly enumerated adapter index.
        self.check(unsafe { afc_select(self.handle.as_ptr(), adapter.index) })?;
        self.selected = true;
        Ok(())
    }
    fn curve_info(&mut self) -> Result<CurveInfo> {
        self.require_selected()?;
        let mut raw = RawCurve::default();
        // SAFETY: bridge fills one initialized C-compatible output structure.
        self.check(unsafe { afc_curve_info(self.handle.as_ptr(), &mut raw) })?;
        Ok(CurveInfo {
            supported: raw.supported != 0,
            zero_rpm_supported: raw.zero_rpm_supported != 0,
            zero_rpm_enabled: raw.zero_rpm_enabled != 0,
            current: raw.current,
            minimum: raw.minimum,
            maximum: raw.maximum,
        })
    }
    fn sample(&mut self) -> Result<Sample> {
        self.require_selected()?;
        let mut raw = RawSample::default();
        // SAFETY: bridge fills one initialized C-compatible output structure.
        self.check(unsafe { afc_sample(self.handle.as_ptr(), &mut raw) })?;
        for t in [raw.edge, raw.hotspot] {
            if t != -1 && !(1..=125).contains(&t) {
                bail!("Implausible temperature reading: {t} °C");
            }
        }
        let valid = |v, min, max| {
            if (min..=max).contains(&v) {
                Some(v)
            } else {
                None
            }
        };
        let sample = Sample {
            edge_c: valid(raw.edge, 1, 125),
            hotspot_c: valid(raw.hotspot, 1, 125),
            rpm: valid(raw.rpm, 0, 15000),
            speed_percent: valid(raw.percent, 0, 100),
        };
        sample.validate()?;
        Ok(sample)
    }
    fn apply_native_curve(&mut self, points: &[Point; 5], zero_rpm: bool) -> Result<()> {
        self.require_selected()?;
        native_points(points)?;
        let sample = self.sample()?;
        if sample.too_hot() {
            bail!("GPU is too hot to start custom fan control");
        }
        self.owns_curve = true; // A partially successful write still needs cleanup.
                                // SAFETY: the bridge reads exactly five valid C-compatible points, synchronously.
        if let Err(e) = self
            .check(unsafe { afc_apply(self.handle.as_ptr(), points.as_ptr(), i32::from(zero_rpm)) })
        {
            let reset = self.restore_automatic();
            bail!("{e}; automatic restore: {reset:?}");
        }
        Ok(())
    }
    fn restore_automatic(&mut self) -> Result<()> {
        self.require_selected()?;
        // SAFETY: live session; bridge resets only fan entries using ADL reset flags.
        self.check(unsafe { afc_reset(self.handle.as_ptr()) })?;
        self.owns_curve = false;
        Ok(())
    }

    fn set_zero_rpm(&mut self, enabled: bool) -> Result<()> {
        self.require_selected()?;
        if !self.owns_curve {
            bail!("Apply a fan curve before changing Zero RPM");
        }
        // SAFETY: live thread-local session; the bridge changes only the fan-stop flag.
        self.check(unsafe { afc_zero_rpm(self.handle.as_ptr(), i32::from(enabled)) })
    }
}

impl Drop for AdlBackend {
    fn drop(&mut self) {
        if self.owns_curve {
            let _ = self.restore_automatic();
        }
        // SAFETY: unique owner closes exactly once, after the final ADL call.
        unsafe { afc_close(self.handle.as_ptr()) };
    }
}
