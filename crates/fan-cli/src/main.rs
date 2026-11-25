use anyhow::{bail, Context, Result};
use fan_core::{
    config::{Config, TemperatureSource},
    curve::{interpolate, native_points, StepDown},
    hardware::GpuBackend,
};
use std::time::{Duration, Instant};

fn run_test_curve<F>(backend: &mut dyn GpuBackend, config: &Config, mut sleep: F) -> Result<()>
where
    F: FnMut(Duration),
{
    let points = native_points(&config.curve)?;
    let info = backend.curve_info()?;
    if !info.writable {
        bail!("Fan control is read-only; write permissions are required to test a curve");
    }
    if config.zero_rpm_stop_temperature()?.is_some() && !info.zero_rpm_supported {
        bail!("This GPU/driver does not expose Zero RPM control");
    }
    let sample = backend.sample()?;
    if sample.too_hot() {
        bail!("Protective temperature threshold reached");
    }

    if info.native_curve_supported {
        let initial_zero_rpm = config.zero_rpm_requested(&sample, false)?;
        backend.apply_native_curve(&points, initial_zero_rpm)?;
        let mut zero_rpm = backend.curve_info()?.zero_rpm_enabled;
        let test = (|| -> Result<()> {
            for _ in 0..6 {
                sleep(Duration::from_millis(2500));
                let sample = backend.sample()?;
                println!("{}", serde_json::to_string(&sample)?);
                let desired = config.zero_rpm_requested(&sample, zero_rpm)?;
                if desired != zero_rpm {
                    backend.set_zero_rpm(desired)?;
                    zero_rpm = desired;
                }
                if sample.too_hot() {
                    bail!("Protective temperature threshold reached");
                }
            }
            Ok(())
        })();
        let reset = backend.restore_automatic();
        test?;
        reset?;
        return Ok(());
    }

    if !info.host_pwm_supported {
        bail!("Fan control is not supported by this GPU/driver");
    }
    if config.temperature_source == TemperatureSource::Firmware {
        bail!("Driver / firmware sensor is not supported by this GPU/driver");
    }

    let mut step_down = StepDown::default();
    let mut now = Instant::now();
    let mut last_write: Option<(i32, bool)> = None;
    let mut active_zero_rpm = false;

    let test = (|| -> Result<()> {
        let mut current_sample = sample;
        for tick in 0..=15 {
            if tick > 0 {
                sleep(Duration::from_secs(1));
                now += Duration::from_secs(1);
                current_sample = backend.sample()?;
                println!("{}", serde_json::to_string(&current_sample)?);
            }
            if current_sample.too_hot() {
                bail!("Protective temperature threshold reached");
            }
            let temp = config.temperature_source.temperature(&current_sample)?;
            let held = step_down.temperature(f64::from(temp), 3.0, Duration::from_secs(6), now);
            let speed = interpolate(&config.curve, held).ceil() as i32;
            let desired_zero_rpm = config.zero_rpm_requested(&current_sample, active_zero_rpm)?;
            if last_write != Some((speed, desired_zero_rpm)) {
                backend.apply_speed(speed, desired_zero_rpm)?;
                last_write = Some((speed, desired_zero_rpm));
                active_zero_rpm = desired_zero_rpm;
            }
        }
        Ok(())
    })();
    let reset = backend.restore_automatic();
    test?;
    reset?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut backend = amd_adl::open_backend()?;
    let adapters = backend.adapters()?;
    if args.is_empty() || args[0] == "list" {
        println!("{}", serde_json::to_string_pretty(&adapters)?);
        return Ok(());
    }
    let id = args
        .get(1)
        .context("Usage: fan-cli probe|test-curve|reset <exact GPU ID>")?;
    let adapter = adapters
        .iter()
        .find(|a| &a.id == id)
        .context("Unknown GPU ID; run fan-cli list")?;
    backend.select(adapter)?;
    match args[0].as_str() {
        "probe" => {
            println!("{}", serde_json::to_string_pretty(&backend.curve_info()?)?);
            for _ in 0..3 {
                std::thread::sleep(Duration::from_secs(2));
                println!("{}", serde_json::to_string(&backend.sample()?)?);
            }
        }
        "test-curve" => {
            let path = args
                .get(2)
                .context("Usage: test-curve <ID> <config.json> (15-second test)")?;
            let config = Config::load(std::path::Path::new(path))?;
            run_test_curve(backend.as_mut(), &config, std::thread::sleep)?;
            println!("Driver automatic fan defaults restored.");
        }
        "reset" => backend.restore_automatic()?,
        _ => bail!("Unknown command: list, probe, test-curve, reset"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fan_core::hardware::{Adapter, CurveInfo, GpuBackend, Sample};
    use std::cell::RefCell;

    struct MockHostPwmBackend {
        samples: Vec<Sample>,
        idx: usize,
        writes: Vec<(i32, bool)>,
        resets: usize,
    }

    impl GpuBackend for MockHostPwmBackend {
        fn adapters(&mut self) -> Result<Vec<Adapter>> {
            Ok(vec![Adapter {
                index: 0,
                id: "pci:03:00.0".into(),
                name: "Test GPU".into(),
                od_version: None,
            }])
        }
        fn select(&mut self, _: &Adapter) -> Result<()> {
            Ok(())
        }
        fn curve_info(&mut self) -> Result<CurveInfo> {
            Ok(CurveInfo {
                supported: true,
                native_curve_supported: false,
                host_pwm_supported: true,
                writable: true,
                zero_rpm_supported: true,
                zero_rpm_enabled: false,
                current: Default::default(),
                minimum: Default::default(),
                maximum: Default::default(),
            })
        }
        fn sample(&mut self) -> Result<Sample> {
            let s = self.samples[self.idx.min(self.samples.len() - 1)].clone();
            self.idx += 1;
            Ok(s)
        }
        fn apply_native_curve(&mut self, _: &[fan_core::curve::Point; 5], _: bool) -> Result<()> {
            bail!("not supported")
        }
        fn apply_speed(&mut self, percent: i32, zero_rpm: bool) -> Result<()> {
            self.writes.push((percent, zero_rpm));
            Ok(())
        }
        fn restore_automatic(&mut self) -> Result<()> {
            self.resets += 1;
            Ok(())
        }
    }

    #[test]
    fn host_pwm_test_curve_runs_15_ticks_aborts_on_too_hot_and_restores_automatic() {
        let cfg = Config::default();

        let mut ok_samples = vec![
            Sample {
                edge_c: Some(50),
                ..Sample::default()
            };
            16
        ];
        let mut backend = MockHostPwmBackend {
            samples: ok_samples.clone(),
            idx: 0,
            writes: Vec::new(),
            resets: 0,
        };
        let sleeps = RefCell::new(Vec::new());
        run_test_curve(&mut backend, &cfg, |d| sleeps.borrow_mut().push(d)).unwrap();
        assert_eq!(sleeps.borrow().len(), 15);
        assert_eq!(
            sleeps.borrow().iter().sum::<Duration>(),
            Duration::from_secs(15)
        );
        assert_eq!(backend.writes.len(), 1);
        assert_eq!(backend.resets, 1);

        // Overheating mid-test aborts and still restores automatic.
        ok_samples[3].edge_c = Some(90);
        let mut hot_backend = MockHostPwmBackend {
            samples: ok_samples,
            idx: 0,
            writes: Vec::new(),
            resets: 0,
        };
        let err = run_test_curve(&mut hot_backend, &cfg, |_| {}).unwrap_err();
        assert!(err.to_string().contains("Protective temperature"));
        assert_eq!(hot_backend.resets, 1);
    }
}
