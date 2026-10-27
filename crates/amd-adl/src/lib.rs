#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::AdlBackend;

#[cfg(not(windows))]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    anyhow::bail!("Hardware backend unavailable: Linux sysfs/hwmon is not implemented yet")
}

#[cfg(windows)]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    Ok(Box::new(AdlBackend::open()?))
}
