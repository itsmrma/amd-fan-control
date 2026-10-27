use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub temperature: i32,
    pub speed: i32,
}

pub fn validate(points: &[Point]) -> Result<()> {
    if !(2..=12).contains(&points.len()) {
        bail!("Servono da 2 a 12 punti");
    }
    for (i, p) in points.iter().enumerate() {
        if !(20..=100).contains(&p.temperature) || !(25..=100).contains(&p.speed) {
            bail!("Temperatura 20–100 °C e velocità 25–100% richieste");
        }
        if i > 0 && (p.temperature <= points[i - 1].temperature || p.speed < points[i - 1].speed) {
            bail!("Temperature crescenti e velocità non decrescenti richieste");
        }
    }
    let last = points.last().unwrap();
    if last.speed != 100 || last.temperature > 85 {
        bail!("L'ultimo punto deve raggiungere il 100% entro 85 °C");
    }
    Ok(())
}

/// Linear interpolation, clamped to endpoints. Call validate before using a curve.
pub fn interpolate(points: &[Point], temperature: f64) -> f64 {
    if temperature <= points[0].temperature as f64 {
        return points[0].speed as f64;
    }
    for pair in points.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        if temperature <= b.temperature as f64 {
            let f = (temperature - a.temperature as f64) / (b.temperature - a.temperature) as f64;
            return a.speed as f64 + f * (b.speed - a.speed) as f64;
        }
    }
    points.last().unwrap().speed as f64
}

/// OD8 has exactly five native points. No silent resampling of a user's curve.
pub fn native_points(points: &[Point]) -> Result<[Point; 5]> {
    validate(points)?;
    points
        .try_into()
        .map_err(|_| anyhow::anyhow!("Overdrive8 richiede esattamente 5 punti"))
}

/// Host-PWM controller for future sysfs/legacy backends. Never used for native OD8.
/// Rise immediately; lower only after the entire delay below the temperature band.
#[derive(Default)]
pub struct StepDown {
    held_temperature: Option<f64>,
    cooling_since: Option<Instant>,
}

impl StepDown {
    pub fn temperature(&mut self, value: f64, band: f64, delay: Duration, now: Instant) -> f64 {
        let held = self.held_temperature.get_or_insert(value);
        if value >= *held {
            *held = value;
            self.cooling_since = None;
        } else if value <= *held - band {
            let since = self.cooling_since.get_or_insert(now);
            if now.duration_since(*since) >= delay {
                *held = value;
                self.cooling_since = None;
            }
        } else {
            self.cooling_since = None;
        }
        *held
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn points() -> [Point; 5] {
        [(40, 30), (50, 40), (65, 60), (75, 80), (85, 100)]
            .map(|(temperature, speed)| Point { temperature, speed })
    }
    #[test]
    fn interpolates_and_clamps() {
        let p = points();
        assert_eq!(interpolate(&p, 45.0), 35.0);
        assert_eq!(interpolate(&p, 0.0), 30.0);
        assert_eq!(interpolate(&p, 120.0), 100.0);
    }
    #[test]
    fn rejects_unsafe_or_ambiguous_curves() {
        let mut p = points();
        p[1].temperature = 40;
        assert!(validate(&p).is_err());
        p = points();
        p[4].speed = 90;
        assert!(validate(&p).is_err());
        p = points();
        p[2].speed = 26;
        assert!(validate(&p).is_err());
        assert!(native_points(&p[..4]).is_err());
    }
    #[test]
    fn stepdown_requires_continuous_cooling_but_rises_immediately() {
        let mut h = StepDown::default();
        let now = Instant::now();
        let delay = Duration::from_secs(6);
        assert_eq!(h.temperature(70.0, 3.0, delay, now), 70.0);
        assert_eq!(h.temperature(60.0, 3.0, delay, now), 70.0);
        assert_eq!(h.temperature(68.0, 3.0, delay, now + delay), 70.0);
        assert_eq!(h.temperature(60.0, 3.0, delay, now + delay), 70.0);
        assert_eq!(h.temperature(61.0, 3.0, delay, now + delay * 2), 61.0);
        assert_eq!(h.temperature(80.0, 3.0, delay, now + delay * 2), 80.0);
    }
}
