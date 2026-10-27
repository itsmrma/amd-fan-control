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
        .context("Uso: fan-cli probe|test-curve|reset <ID GPU esatto>")?;
    let adapter = adapters
        .iter()
        .find(|a| &a.id == id)
        .context("ID GPU sconosciuto; esegui fan-cli list")?;
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
                .context("Uso: test-curve <ID> <config.json> (prova di 15 secondi)")?;
            let config = Config::load(std::path::Path::new(path))?;
            backend.apply_native_curve(&native_points(&config.curve)?)?;
            let test = (|| -> Result<()> {
                for _ in 0..6 {
                    std::thread::sleep(Duration::from_millis(2500));
                    let sample = backend.sample()?;
                    println!("{}", serde_json::to_string(&sample)?);
                    if sample.too_hot() {
                        bail!("Soglia termica di protezione raggiunta");
                    }
                }
                Ok(())
            })();
            let reset = backend.restore_automatic();
            test?;
            reset?;
            println!("Ripristinati i valori automatici del driver.");
        }
        "reset" => backend.restore_automatic()?,
        _ => bail!("Comando sconosciuto: list, probe, test-curve, reset"),
    }
    Ok(())
}
