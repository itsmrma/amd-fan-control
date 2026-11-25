use anyhow::{bail, Context, Result};
use fan_core::{
    curve::Point,
    hardware::{Adapter, CurveInfo, GpuBackend, Sample},
};
use std::{
    collections::HashSet,
    fs,
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
};

pub struct SysfsBackend {
    root: PathBuf,
    selected: Option<SelectedAdapter>,
    _thread_bound: PhantomData<Rc<()>>,
}

#[derive(Debug, Clone)]
struct DiscoveredAdapter {
    adapter: Adapter,
    device_path: PathBuf,
}

#[derive(Debug, Clone)]
struct SelectedAdapter {
    hwmon_path: PathBuf,
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_i32(path: &Path) -> Option<i32> {
    read_trimmed(path)?.parse::<i32>().ok()
}

fn is_writable(path: &Path) -> bool {
    fs::OpenOptions::new().write(true).open(path).is_ok()
}

fn find_hwmon_dir(device_path: &Path) -> Result<PathBuf> {
    let hwmon_root = device_path.join("hwmon");
    let entries = fs::read_dir(&hwmon_root)
        .with_context(|| format!("Reading hwmon directory {}", hwmon_root.display()))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with("hwmon") && entry.path().is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    if let Some(amdgpu_dir) = dirs
        .iter()
        .find(|dir| read_trimmed(&dir.join("name")).as_deref() == Some("amdgpu"))
    {
        return Ok(amdgpu_dir.clone());
    }
    dirs.into_iter()
        .next()
        .context("No hwmon directory found for selected GPU")
}

fn is_drm_card_name(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("card")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

fn is_amdgpu_driver(device_path: &Path) -> bool {
    let driver_link = device_path.join("driver");
    if let Ok(target) = fs::read_link(&driver_link) {
        return target.file_name().and_then(|n| n.to_str()) == Some("amdgpu");
    }
    if let Ok(uevent) = fs::read_to_string(device_path.join("uevent")) {
        return uevent.lines().any(|line| line.trim() == "DRIVER=amdgpu");
    }
    false
}

fn normalize_pci_id(raw: &str) -> Option<String> {
    let slot = raw.trim().to_ascii_lowercase();
    if slot.is_empty() {
        return None;
    }
    let normalized = slot.strip_prefix("0000:").unwrap_or(&slot);
    Some(format!("pci:{normalized}"))
}

fn resolve_pci_id(device_path: &Path) -> Option<String> {
    if let Ok(uevent) = fs::read_to_string(device_path.join("uevent")) {
        for line in uevent.lines() {
            if let Some(slot) = line.strip_prefix("PCI_SLOT_NAME=") {
                if let Some(id) = normalize_pci_id(slot) {
                    return Some(id);
                }
            }
        }
    }
    let resolved = fs::canonicalize(device_path)
        .or_else(|_| fs::read_link(device_path))
        .ok()?;
    let name = resolved.file_name()?.to_str()?;
    normalize_pci_id(name)
}

fn resolve_adapter_name(device_path: &Path, pci_id: &str) -> String {
    if let Some(name) = read_trimmed(&device_path.join("product_name")) {
        return name;
    }
    if let Some(dev_id) = read_trimmed(&device_path.join("device")) {
        return format!("AMD Radeon GPU ({dev_id})");
    }
    format!("AMD Radeon GPU ({pci_id})")
}

impl SysfsBackend {
    pub fn open() -> Result<Self> {
        Ok(Self::with_root("/sys"))
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            selected: None,
            _thread_bound: PhantomData,
        }
    }

    fn hwmon_path(&self) -> Result<&Path> {
        self.selected
            .as_ref()
            .map(|s| s.hwmon_path.as_path())
            .context("Select a GPU before accessing hardware")
    }

    fn discover(&self) -> Result<Vec<DiscoveredAdapter>> {
        let drm_dir = self.root.join("class/drm");
        let entries = match fs::read_dir(&drm_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut cards = Vec::new();
        for entry in entries {
            let entry = entry?;
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            let Some(card_num) = is_drm_card_name(name) else {
                continue;
            };
            cards.push((card_num, entry.path()));
        }
        cards.sort_by_key(|(num, _)| *num);

        let mut seen_ids = HashSet::new();
        let mut discovered = Vec::new();
        for (_, card_path) in cards {
            let device_path = card_path.join("device");
            if !device_path.exists() {
                continue;
            }
            let Some(vendor) = read_trimmed(&device_path.join("vendor")) else {
                continue;
            };
            if !vendor.eq_ignore_ascii_case("0x1002") {
                continue;
            }
            if !is_amdgpu_driver(&device_path) {
                continue;
            }
            let Some(id) = resolve_pci_id(&device_path) else {
                continue;
            };
            if !seen_ids.insert(id.clone()) {
                continue;
            }
            let name = resolve_adapter_name(&device_path, &id);
            let index = discovered.len() as i32;
            discovered.push(DiscoveredAdapter {
                adapter: Adapter {
                    index,
                    id,
                    name,
                    od_version: None,
                },
                device_path,
            });
        }
        Ok(discovered)
    }
}

impl GpuBackend for SysfsBackend {
    fn adapters(&mut self) -> Result<Vec<Adapter>> {
        Ok(self.discover()?.into_iter().map(|d| d.adapter).collect())
    }

    fn select(&mut self, adapter: &Adapter) -> Result<()> {
        let found = self
            .discover()?
            .into_iter()
            .find(|d| d.adapter.id == adapter.id)
            .with_context(|| format!("GPU {} not found in sysfs", adapter.id))?;
        let hwmon_path = find_hwmon_dir(&found.device_path)?;
        self.selected = Some(SelectedAdapter { hwmon_path });
        Ok(())
    }

    fn curve_info(&mut self) -> Result<CurveInfo> {
        let hwmon = self.hwmon_path()?;
        let pwm1_enable = hwmon.join("pwm1_enable");
        let pwm1 = hwmon.join("pwm1");
        let host_pwm_supported = pwm1_enable.exists() && pwm1.exists();
        let writable = host_pwm_supported && is_writable(&pwm1_enable) && is_writable(&pwm1);
        let zero_rpm_supported =
            host_pwm_supported && read_i32(&hwmon.join("pwm1_min")).is_none_or(|min| min == 0);
        let zero_rpm_enabled =
            host_pwm_supported && read_i32(&pwm1_enable) == Some(1) && read_i32(&pwm1) == Some(0);

        Ok(CurveInfo {
            supported: host_pwm_supported,
            native_curve_supported: false,
            host_pwm_supported,
            writable,
            zero_rpm_supported,
            zero_rpm_enabled,
            current: [Point::default(); 5],
            minimum: [Point {
                temperature: 20,
                speed: 20,
            }; 5],
            maximum: [Point {
                temperature: 100,
                speed: 100,
            }; 5],
        })
    }

    fn sample(&mut self) -> Result<Sample> {
        let hwmon = self.hwmon_path()?;
        let mut sample = Sample::default();
        for idx in 1..=3 {
            let Some(millideg) = read_i32(&hwmon.join(format!("temp{idx}_input"))) else {
                continue;
            };
            let deg_c = millideg / 1000;
            let label = read_trimmed(&hwmon.join(format!("temp{idx}_label")))
                .map(|l| l.to_ascii_lowercase());
            match label.as_deref() {
                Some("edge") => sample.edge_c = Some(deg_c),
                Some("junction") | Some("hotspot") => sample.hotspot_c = Some(deg_c),
                Some(_) => {}
                None if idx == 1 => sample.edge_c = Some(deg_c),
                None if idx == 2 => sample.hotspot_c = Some(deg_c),
                None => {}
            }
        }
        sample.rpm = read_i32(&hwmon.join("fan1_input")).filter(|&r| r >= 0);
        sample.speed_percent = read_i32(&hwmon.join("pwm1"))
            .filter(|&p| (0..=255).contains(&p))
            .map(|p| (p * 100 + 127) / 255);
        sample.validate()?;
        Ok(sample)
    }

    fn apply_native_curve(&mut self, _points: &[Point; 5], _zero_rpm: bool) -> Result<()> {
        bail!("Native firmware fan curves are not supported on Linux sysfs")
    }

    fn restore_automatic(&mut self) -> Result<()> {
        bail!("Automatic fan restore is not implemented yet")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    struct FakeCard<'a> {
        card_name: &'a str,
        pci_slot: &'a str,
        vendor: &'a str,
        driver: &'a str,
        product_name: Option<&'a str>,
        device_id: Option<&'a str>,
    }

    fn create_fake_card(root: &Path, spec: FakeCard<'_>) -> PathBuf {
        let pci_dev = root.join("devices/pci0000:00").join(spec.pci_slot);
        fs::create_dir_all(&pci_dev).unwrap();
        fs::write(pci_dev.join("vendor"), format!("{}\n", spec.vendor)).unwrap();
        if let Some(dev_id) = spec.device_id {
            fs::write(pci_dev.join("device"), format!("{dev_id}\n")).unwrap();
        }
        if let Some(name) = spec.product_name {
            fs::write(pci_dev.join("product_name"), format!("{name}\n")).unwrap();
        }
        fs::write(
            pci_dev.join("uevent"),
            format!("DRIVER={}\nPCI_SLOT_NAME={}\n", spec.driver, spec.pci_slot),
        )
        .unwrap();

        let driver_dir = root.join("bus/pci/drivers").join(spec.driver);
        fs::create_dir_all(&driver_dir).unwrap();
        let driver_link = pci_dev.join("driver");
        if !driver_link.exists() {
            symlink(&driver_dir, &driver_link).unwrap();
        }

        let drm_dir = root.join("class/drm");
        fs::create_dir_all(&drm_dir).unwrap();
        let card_dir = drm_dir.join(spec.card_name);
        fs::create_dir_all(&card_dir).unwrap();
        symlink(&pci_dev, card_dir.join("device")).unwrap();
        pci_dev
    }

    #[test]
    fn empty_sysfs_returns_no_adapters_and_open_backend_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let mut backend = SysfsBackend::with_root(dir.path());
        assert!(backend.adapters().unwrap().is_empty());
        assert!(crate::open_backend().is_ok());
    }

    #[test]
    fn discovers_only_amdgpu_cards_ignoring_connectors_and_other_drivers() {
        let dir = tempfile::tempdir().unwrap();
        create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card0",
                pci_slot: "0000:03:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: Some("AMD Radeon RX 7800 XT"),
                device_id: Some("0x747e"),
            },
        );
        // Display connector and render node entries should be ignored
        fs::create_dir_all(dir.path().join("class/drm/card0-DP-1")).unwrap();
        fs::create_dir_all(dir.path().join("class/drm/renderD128")).unwrap();
        // Non-AMD GPU
        create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card1",
                pci_slot: "0000:04:00.0",
                vendor: "0x10de",
                driver: "nouveau",
                product_name: Some("NVIDIA GPU"),
                device_id: Some("0x2684"),
            },
        );
        // Legacy radeon driver GPU
        create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card2",
                pci_slot: "0000:05:00.0",
                vendor: "0x1002",
                driver: "radeon",
                product_name: Some("AMD Radeon HD 7970"),
                device_id: Some("0x6798"),
            },
        );

        let mut backend = SysfsBackend::with_root(dir.path());
        let adapters = backend.adapters().unwrap();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].index, 0);
        assert_eq!(adapters[0].id, "pci:03:00.0");
        assert_eq!(adapters[0].name, "AMD Radeon RX 7800 XT");
        assert_eq!(adapters[0].od_version, None);
    }

    #[test]
    fn deduplicates_by_pci_address_normalizes_ids_and_resolves_fallback_names() {
        let dir = tempfile::tempdir().unwrap();
        // GPU 1: domain 0000, uppercase hex, no product_name -> fallback to device ID
        create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card0",
                pci_slot: "0000:0C:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: None,
                device_id: Some("0x744c"),
            },
        );
        // Duplicate card entry pointing to the same PCI device
        create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card1",
                pci_slot: "0000:0c:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: None,
                device_id: Some("0x744c"),
            },
        );
        // GPU 2: non-zero PCI domain
        create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card2",
                pci_slot: "0001:03:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: Some("AMD Radeon RX 7900 XTX"),
                device_id: Some("0x744c"),
            },
        );

        let mut backend = SysfsBackend::with_root(dir.path());
        let adapters = backend.adapters().unwrap();
        assert_eq!(adapters.len(), 2);
        assert_eq!(adapters[0].index, 0);
        assert_eq!(adapters[0].id, "pci:0c:00.0");
        assert_eq!(adapters[0].name, "AMD Radeon GPU (0x744c)");
        assert_eq!(adapters[1].index, 1);
        assert_eq!(adapters[1].id, "pci:0001:03:00.0");
        assert_eq!(adapters[1].name, "AMD Radeon RX 7900 XTX");
    }

    #[test]
    fn reads_hwmon_telemetry_and_reports_missing_optional_sensors_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let pci_dev = create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card0",
                pci_slot: "0000:03:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: Some("AMD Radeon RX 7800 XT"),
                device_id: Some("0x747e"),
            },
        );
        let hwmon = pci_dev.join("hwmon/hwmon3");
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(hwmon.join("name"), "amdgpu\n").unwrap();
        fs::write(hwmon.join("temp1_label"), "edge\n").unwrap();
        fs::write(hwmon.join("temp1_input"), "45000\n").unwrap();
        fs::write(hwmon.join("temp2_label"), "junction\n").unwrap();
        fs::write(hwmon.join("temp2_input"), "62000\n").unwrap();
        fs::write(hwmon.join("fan1_input"), "1450\n").unwrap();
        fs::write(hwmon.join("pwm1"), "128\n").unwrap();

        let mut backend = SysfsBackend::with_root(dir.path());
        let adapters = backend.adapters().unwrap();
        backend.select(&adapters[0]).unwrap();

        let sample = backend.sample().unwrap();
        assert_eq!(sample.edge_c, Some(45));
        assert_eq!(sample.hotspot_c, Some(62));
        assert_eq!(sample.rpm, Some(1450));
        assert_eq!(sample.speed_percent, Some(50));

        // Remove optional sensors (hotspot, rpm, pwm)
        fs::remove_file(hwmon.join("temp2_input")).unwrap();
        fs::remove_file(hwmon.join("fan1_input")).unwrap();
        fs::remove_file(hwmon.join("pwm1")).unwrap();

        let partial = backend.sample().unwrap();
        assert_eq!(partial.edge_c, Some(45));
        assert_eq!(partial.hotspot_c, None);
        assert_eq!(partial.rpm, None);
        assert_eq!(partial.speed_percent, None);
    }

    #[test]
    fn rejects_implausible_or_missing_temperature_readings() {
        let dir = tempfile::tempdir().unwrap();
        let pci_dev = create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card0",
                pci_slot: "0000:03:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: Some("AMD Radeon RX 7800 XT"),
                device_id: None,
            },
        );
        let hwmon = pci_dev.join("hwmon/hwmon0");
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(hwmon.join("name"), "amdgpu\n").unwrap();

        let mut backend = SysfsBackend::with_root(dir.path());
        let adapters = backend.adapters().unwrap();
        backend.select(&adapters[0]).unwrap();

        // No temperature sensors at all
        assert!(backend.sample().is_err());

        // Below 1 °C
        fs::write(hwmon.join("temp1_input"), "0\n").unwrap();
        assert!(backend.sample().is_err());

        // Above 125 °C
        fs::write(hwmon.join("temp1_input"), "126000\n").unwrap();
        assert!(backend.sample().is_err());
    }

    #[test]
    fn probes_host_pwm_zero_rpm_and_writable_vs_readonly_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let pci_dev = create_fake_card(
            dir.path(),
            FakeCard {
                card_name: "card0",
                pci_slot: "0000:03:00.0",
                vendor: "0x1002",
                driver: "amdgpu",
                product_name: Some("AMD Radeon RX 7800 XT"),
                device_id: None,
            },
        );
        let hwmon = pci_dev.join("hwmon/hwmon0");
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(hwmon.join("name"), "amdgpu\n").unwrap();
        fs::write(hwmon.join("temp1_input"), "45000\n").unwrap();
        fs::write(hwmon.join("pwm1_enable"), "2\n").unwrap();
        fs::write(hwmon.join("pwm1"), "100\n").unwrap();
        fs::write(hwmon.join("pwm1_min"), "0\n").unwrap();

        let mut backend = SysfsBackend::with_root(dir.path());
        let adapters = backend.adapters().unwrap();
        backend.select(&adapters[0]).unwrap();

        // Both writable, pwm1_min == 0, automatic mode
        let info = backend.curve_info().unwrap();
        assert!(info.supported);
        assert!(!info.native_curve_supported);
        assert!(info.host_pwm_supported);
        assert!(info.writable);
        assert!(info.zero_rpm_supported);
        assert!(!info.zero_rpm_enabled);

        // Manual mode with pwm1 == 0 -> zero_rpm_enabled = true
        fs::write(hwmon.join("pwm1_enable"), "1\n").unwrap();
        fs::write(hwmon.join("pwm1"), "0\n").unwrap();
        assert!(backend.curve_info().unwrap().zero_rpm_enabled);

        // pwm1_min > 0 -> zero_rpm_supported = false
        fs::write(hwmon.join("pwm1_min"), "50\n").unwrap();
        assert!(!backend.curve_info().unwrap().zero_rpm_supported);

        // Read-only pwm1 -> writable = false
        fs::set_permissions(hwmon.join("pwm1"), fs::Permissions::from_mode(0o444)).unwrap();
        let ro_info = backend.curve_info().unwrap();
        assert!(ro_info.host_pwm_supported);
        assert!(!ro_info.writable);
        fs::set_permissions(hwmon.join("pwm1"), fs::Permissions::from_mode(0o644)).unwrap();

        // Read-only pwm1_enable -> writable = false
        fs::set_permissions(hwmon.join("pwm1_enable"), fs::Permissions::from_mode(0o444)).unwrap();
        assert!(!backend.curve_info().unwrap().writable);
        fs::set_permissions(hwmon.join("pwm1_enable"), fs::Permissions::from_mode(0o644)).unwrap();

        // Missing pwm1 -> host_pwm_supported = false, supported = false
        fs::remove_file(hwmon.join("pwm1")).unwrap();
        let no_pwm = backend.curve_info().unwrap();
        assert!(!no_pwm.supported);
        assert!(!no_pwm.host_pwm_supported);
        assert!(!no_pwm.writable);
        assert!(!no_pwm.zero_rpm_supported);
    }
}
