#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::AdlBackend;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::SysfsBackend;

#[cfg(not(any(windows, target_os = "linux")))]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    anyhow::bail!("Hardware backend unavailable on this platform")
}

#[cfg(target_os = "linux")]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    Ok(Box::new(SysfsBackend::open()?))
}

#[cfg(windows)]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    Ok(Box::new(AdlBackend::open()?))
}
