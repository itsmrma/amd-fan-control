#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::AdlBackend;

#[cfg(not(windows))]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    anyhow::bail!("Backend hardware non disponibile: sysfs/hwmon previsto per Linux")
}

#[cfg(windows)]
pub fn open_backend() -> anyhow::Result<Box<dyn fan_core::hardware::GpuBackend>> {
    Ok(Box::new(AdlBackend::open()?))
}
