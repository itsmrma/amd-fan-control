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

    /// Hold a host-computed speed through OD8, retaining a native high-temperature
    /// ramp to 100%. The backend verifies driver readback after every write.
    fn apply_speed(&mut self, _percent: i32, _zero_rpm: bool) -> Result<()> {
        bail!("Sensor-based fan control is not supported by this backend");
    }

    fn set_zero_rpm(&mut self, _enabled: bool) -> Result<()> {
        bail!("Native Zero RPM control is not supported by this backend");
    }

    fn set_pwm(&mut self, _percent: u8) -> Result<()> {
        bail!("Direct PWM is not supported by this backend");
    }
}

/// Keep the requested speed independent of the firmware's internal sensor over
/// its normal range. Put a final 1 °C ramp to 100% at the driver's upper limit
/// (at most 100 °C), so a retained command still has native thermal escalation.
pub fn speed_hold_points(info: &CurveInfo, percent: i32) -> Result<[Point; 5]> {
    if !info.supported || !(20..=100).contains(&percent) {
        bail!("Sensor-based fan control requires a supported curve and 20–100% speed");
    }
    let mut points = [Point::default(); 5];
    let mut ceiling = info.maximum[4].temperature.min(100);
    for i in (0..5).rev() {
        let temperature = info.maximum[i].temperature.min(ceiling);
        let speed = if i == 4 { 100 } else { percent };
        if temperature < info.minimum[i].temperature
            || temperature < 20
            || speed < info.minimum[i].speed
            || speed > info.maximum[i].speed
        {
            bail!("Driver curve limits do not support sensor-based fan control at {percent}%");
        }
        points[i] = Point { temperature, speed };
        ceiling = temperature - 1;
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_speed_is_sensor_independent_with_a_native_emergency_ramp() {
        let mut info = CurveInfo {
            supported: true,
            zero_rpm_supported: true,
            zero_rpm_enabled: false,
            current: [Point::default(); 5],
            minimum: [Point {
                temperature: 25,
                speed: 20,
            }; 5],
            maximum: [Point {
                temperature: 100,
                speed: 100,
            }; 5],
        };
        for speed in [20, 40, 70, 100] {
            let points = speed_hold_points(&info, speed).unwrap();
            for temperature in 25..=99 {
                assert_eq!(
                    crate::curve::interpolate(&points, temperature as f64),
                    speed as f64
                );
            }
            assert_eq!(
                points[4],
                Point {
                    temperature: 100,
                    speed: 100
                }
            );
            assert!(points
                .windows(2)
                .all(|p| p[0].temperature < p[1].temperature && p[0].speed <= p[1].speed));
        }
        info.maximum[4].temperature = 95;
        assert_eq!(speed_hold_points(&info, 40).unwrap()[4].temperature, 95);
        info.minimum[3].temperature = 95;
        assert!(speed_hold_points(&info, 40).is_err());
        info.minimum[3].temperature = 25;
        info.minimum[2].speed = 50;
        assert!(speed_hold_points(&info, 40).is_err());
        assert!(speed_hold_points(&info, 0).is_err());
        assert!(speed_hold_points(&info, 101).is_err());
        info.supported = false;
        assert!(speed_hold_points(&info, 100).is_err());
    }
}
