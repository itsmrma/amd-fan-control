use crate::curve::{self, Point};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub adapter_id: Option<String>,
    pub curve: Vec<Point>,
    pub polling_ms: u64,
    pub start_in_tray: bool,
    /// Missing in older profiles: preserve their previous fan behavior.
    pub zero_rpm_below_percent: Option<i32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            adapter_id: None,
            curve: [(40, 30), (50, 40), (65, 60), (75, 80), (85, 100)]
                .map(|(temperature, speed)| Point { temperature, speed })
                .to_vec(),
            polling_ms: 2500,
            start_in_tray: false,
            zero_rpm_below_percent: None,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("Unsupported configuration version");
        }
        if !(1000..=10000).contains(&self.polling_ms) {
            bail!("Refresh interval must be 1000–10000 ms");
        }
        curve::native_points(&self.curve)?;
        self.zero_rpm_stop_temperature()?;
        Ok(())
    }

    pub fn zero_rpm_stop_temperature(&self) -> Result<Option<i32>> {
        self.zero_rpm_below_percent
            .map(|threshold| curve::zero_rpm_stop_temperature(&self.curve, threshold))
            .transpose()
    }

    /// Re-enable only after cooling 3 °C below the cutoff; resume immediately
    /// at the cutoff. Use the hottest available sensor and never the measured
    /// fan percentage (which falls to zero as a consequence of fan stop).
    pub fn zero_rpm_requested(
        &self,
        sample: &crate::hardware::Sample,
        active: bool,
    ) -> Result<bool> {
        sample.validate()?;
        let Some(stop) = self.zero_rpm_stop_temperature()? else {
            return Ok(false);
        };
        let temperature = [sample.edge_c, sample.hotspot_c]
            .into_iter()
            .flatten()
            .max()
            .context("No temperature available for Zero RPM")?;
        Ok(!sample.too_hot() && temperature < stop - if active { 0 } else { 3 })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).context("Reading configuration"),
        };
        let config: Self = serde_json::from_slice(&data).context("Invalid configuration JSON")?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .context("Configuration directory unavailable")?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.as_file().sync_all()?;
        // Same-volume replace; a failed save preserves the previous config.
        file.persist(path)
            .map_err(|e| e.error)
            .context("Saving configuration atomically")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::Sample;
    #[test]
    fn zero_rpm_uses_hotter_sensor_and_rearms_only_after_cooling() {
        let mut c = Config {
            zero_rpm_below_percent: Some(25),
            ..Config::default()
        };
        c.curve[0].speed = 20;
        let sample = |edge, hotspot| Sample {
            edge_c: edge,
            hotspot_c: hotspot,
            ..Sample::default()
        };
        assert_eq!(c.zero_rpm_stop_temperature().unwrap(), Some(42));
        assert!(c
            .zero_rpm_requested(&sample(Some(38), Some(38)), false)
            .unwrap());
        assert!(!c
            .zero_rpm_requested(&sample(Some(39), Some(39)), false)
            .unwrap());
        assert!(c
            .zero_rpm_requested(&sample(Some(41), Some(41)), true)
            .unwrap());
        assert!(!c
            .zero_rpm_requested(&sample(Some(42), Some(42)), true)
            .unwrap());
        assert!(!c
            .zero_rpm_requested(&sample(Some(35), Some(50)), true)
            .unwrap());
        assert!(c
            .zero_rpm_requested(&sample(None, Some(38)), false)
            .unwrap());
        assert!(c.zero_rpm_requested(&sample(None, None), true).is_err());
        assert!(!c
            .zero_rpm_requested(&sample(Some(90), Some(105)), true)
            .unwrap());
        c.zero_rpm_below_percent = None;
        assert!(!c.zero_rpm_requested(&sample(Some(30), None), true).unwrap());
    }

    #[test]
    fn old_profiles_preserve_disabled_zero_rpm_and_new_settings_roundtrip() {
        let mut json = serde_json::to_value(Config::default()).unwrap();
        json.as_object_mut()
            .unwrap()
            .remove("zero_rpm_below_percent");
        let c: Config = serde_json::from_value(json).unwrap();
        assert!(c.zero_rpm_below_percent.is_none());
        c.validate().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut enabled = c;
        enabled.curve[0].speed = 20;
        enabled.zero_rpm_below_percent = Some(25);
        enabled.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.zero_rpm_below_percent, Some(25));
        assert_eq!(loaded.zero_rpm_stop_temperature().unwrap(), Some(42));
    }
    #[test]
    fn config_roundtrip_replace_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut c = Config::load(&path).unwrap();
        c.save(&path).unwrap();
        c.polling_ms = 3000;
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().polling_ms, 3000);
        std::fs::write(&path, "broken").unwrap();
        assert!(Config::load(&path).is_err());
    }
}
