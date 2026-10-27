use crate::curve::Point;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adapter {
    pub index: i32,
    pub id: String,
    pub name: String,
    pub od_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurveInfo {
    pub supported: bool,
    pub zero_rpm_supported: bool,
    pub zero_rpm_enabled: bool,
    pub current: [Point; 5],
    pub minimum: [Point; 5],
    pub maximum: [Point; 5],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sample {
    pub edge_c: Option<i32>,
    pub hotspot_c: Option<i32>,
    pub rpm: Option<i32>,
    /// PMLog reports RPM/max RPM, not a measured electrical PWM duty cycle.
    pub speed_percent: Option<i32>,
}

impl Sample {
    pub fn validate(&self) -> Result<()> {
        if self.edge_c.is_none() && self.hotspot_c.is_none() {
            bail!("No temperature sensor available");
        }
        for t in [self.edge_c, self.hotspot_c].into_iter().flatten() {
            if !(1..=125).contains(&t) {
                bail!("Implausible temperature: {t} °C");
            }
        }
        Ok(())
    }

    pub fn too_hot(&self) -> bool {
        self.edge_c.is_some_and(|t| t >= 90) || self.hotspot_c.is_some_and(|t| t >= 105)
    }
}

/// A backend is constructed and used on one worker thread. No Send/Sync FFI handle.
/// An eventual sysfs implementation exposes the same contract.
pub trait GpuBackend {
    fn adapters(&mut self) -> Result<Vec<Adapter>>;
    fn select(&mut self, adapter: &Adapter) -> Result<()>;
    fn curve_info(&mut self) -> Result<CurveInfo>;
    fn sample(&mut self) -> Result<Sample>;
    fn apply_native_curve(&mut self, points: &[Point; 5], zero_rpm: bool) -> Result<()>;
    fn restore_automatic(&mut self) -> Result<()>;

    fn set_zero_rpm(&mut self, _enabled: bool) -> Result<()> {
        bail!("Native Zero RPM control is not supported by this backend");
    }

    fn set_pwm(&mut self, _percent: u8) -> Result<()> {
        bail!("Direct PWM is not supported by this backend");
    }
}
