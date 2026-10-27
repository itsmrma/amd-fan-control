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
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("Versione configurazione non supportata");
        }
        if !(1000..=10000).contains(&self.polling_ms) {
            bail!("Polling consentito: 1000–10000 ms");
        }
        curve::native_points(&self.curve)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).context("Lettura configurazione"),
        };
        let config: Self =
            serde_json::from_slice(&data).context("JSON configurazione non valido")?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path.parent().context("Directory configurazione assente")?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.as_file().sync_all()?;
        // Same-volume replace; a failed save preserves the previous config.
        file.persist(path)
            .map_err(|e| e.error)
            .context("Salvataggio atomico configurazione")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
