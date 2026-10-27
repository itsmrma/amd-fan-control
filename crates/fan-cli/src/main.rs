use anyhow::{bail, Context, Result};
use fan_core::{config::Config, curve::native_points};
use std::time::Duration;

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
            let sample = backend.sample()?;
            let initial_zero_rpm = config.zero_rpm_requested(&sample, false)?;
            backend.apply_native_curve(&native_points(&config.curve)?, initial_zero_rpm)?;
            let mut zero_rpm = backend.curve_info()?.zero_rpm_enabled;
            let test = (|| -> Result<()> {
                for _ in 0..6 {
                    std::thread::sleep(Duration::from_millis(2500));
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
            println!("Driver automatic fan defaults restored.");
        }
        "reset" => backend.restore_automatic()?,
        _ => bail!("Unknown command: list, probe, test-curve, reset"),
    }
    Ok(())
}
