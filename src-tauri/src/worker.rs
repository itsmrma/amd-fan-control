use anyhow::{bail, Context, Result};
use fan_core::{
    config::Config,
    curve::native_points,
    hardware::{Adapter, CurveInfo, GpuBackend, Sample},
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub config: Config,
    pub adapters: Vec<Adapter>,
    pub selected: Option<Adapter>,
    pub curve_info: Option<CurveInfo>,
    pub sample: Option<Sample>,
    pub sampled_at_ms: Option<u64>,
    pub manual: bool,
    pub error: Option<String>,
    pub ready: bool,
}

pub enum Action {
    Save(Config),
    Apply,
    Automatic,
    Stop,
}
struct Request {
    action: Action,
    reply: SyncSender<Result<(), String>>,
}

pub struct Worker {
    sender: SyncSender<Request>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Worker {
    pub fn start(app: AppHandle, path: PathBuf) -> Self {
        let (config, error) = match Config::load(&path) {
            Ok(c) => (c, None),
            Err(e) => (
                Config::default(),
                Some(format!("{e:#}. Il file originale è preservato.")),
            ),
        };
        let snapshot = Arc::new(Mutex::new(Snapshot {
            config,
            adapters: vec![],
            selected: None,
            curve_info: None,
            sample: None,
            sampled_at_ms: None,
            manual: false,
            error,
            ready: false,
        }));
        let (sender, receiver) = mpsc::sync_channel(8);
        let shared = snapshot.clone();
        std::thread::Builder::new().name("gpu-controller".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&app, &path, &shared, receiver)
            }));
            if !matches!(result, Ok(Ok(()))) {
                let message = match result {
                    Ok(Err(e)) => format!("{e:#}"),
                    _ => "Worker interrotto. Il driver conserva l'ultima curva; riavvia l'app per ripristinare.".into(),
                };
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.error = Some(message); s.ready = true; s.sample = None;
                publish(&app, &s);
            }
        }).expect("Creazione worker GPU");
        Self { sender, snapshot }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn request(&self, action: Action) -> Result<(), String> {
        let (reply, receiver) = mpsc::sync_channel(1);
        let stopping = matches!(action, Action::Stop);
        if let Err(e) = self.sender.try_send(Request { action, reply }) {
            if stopping && !self.snapshot().manual {
                return Ok(());
            }
            return Err(format!("Worker non disponibile: {e}"));
        }
        receiver
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| {
                "Il driver non risponde. Controlla lo stato prima di riprovare.".to_string()
            })?
    }
}

fn publish(app: &AppHandle, s: &Snapshot) {
    let _ = app.emit("gpu-status", s);
    if let Some(tray) = app.tray_by_id("main") {
        let text = match &s.sample {
            Some(t) => format!(
                "AMD Fan Control · {} °C · {} RPM · {}",
                t.edge_c
                    .or(t.hotspot_c)
                    .map_or("—".into(), |v| v.to_string()),
                t.rpm.map_or("—".into(), |v| v.to_string()),
                if s.manual {
                    "Curva personalizzata"
                } else {
                    "Monitoraggio"
                }
            ),
            None => "AMD Fan Control · Telemetria non disponibile".into(),
        };
        let _ = tray.set_tooltip(Some(&text));
    }
}

fn select(backend: &mut dyn GpuBackend, s: &mut Snapshot) -> Result<()> {
    let selected = if let Some(id) = &s.config.adapter_id {
        s.adapters.iter().find(|a| &a.id == id).cloned()
    } else if s.adapters.len() == 1 {
        s.adapters.first().cloned()
    } else {
        None
    };
    let adapter = selected.context("Seleziona la GPU nelle impostazioni")?;
    backend.select(&adapter)?;
    s.selected = Some(adapter);
    s.curve_info = Some(backend.curve_info()?);
    Ok(())
}

fn restore(backend: &mut dyn GpuBackend, s: &mut Snapshot) -> Result<()> {
    backend.restore_automatic()?;
    s.manual = false;
    s.curve_info = Some(backend.curve_info()?);
    Ok(())
}

fn handle(
    action: Action,
    backend: &mut dyn GpuBackend,
    path: &std::path::Path,
    s: &mut Snapshot,
) -> Result<bool> {
    match action {
        Action::Save(config) => {
            config.validate()?;
            if s.manual {
                bail!("Passa in automatico prima di modificare le impostazioni");
            }
            // Select using a temporary snapshot; a failed selection does not replace the saved config.
            let mut next = s.clone();
            next.config = config;
            if let Err(e) = select(backend, &mut next).and_then(|_| next.config.save(path)) {
                s.selected = None;
                s.sample = None;
                s.sampled_at_ms = None;
                s.curve_info = None;
                return Err(e);
            }
            next.sample = None;
            next.sampled_at_ms = None;
            next.error = None;
            *s = next;
        }
        Action::Apply => {
            if !s.ready || s.sample.is_none() {
                bail!("Attendi una lettura termica valida");
            }
            let points = native_points(&s.config.curve)?;
            s.manual = true;
            if let Err(e) = backend.apply_native_curve(&points) {
                let reset = restore(backend, s);
                bail!("{e:#}; ripristino automatico: {reset:?}");
            }
            s.curve_info = Some(backend.curve_info()?);
        }
        Action::Automatic => restore(backend, s)?,
        Action::Stop => {
            if s.manual {
                restore(backend, s)?;
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn run(
    app: &AppHandle,
    path: &std::path::Path,
    shared: &Arc<Mutex<Snapshot>>,
    receiver: Receiver<Request>,
) -> Result<()> {
    let mut backend = amd_adl::open_backend()?;
    let mut s = shared.lock().unwrap().clone();
    s.adapters = backend.adapters()?;
    if let Err(e) = select(backend.as_mut(), &mut s) {
        s.error = Some(format!("{e:#}"));
    }
    s.ready = true;
    let mut last_tick = Instant::now();
    let mut deadline = Instant::now();
    let mut failures = 0;
    let mut ticks = 0u32;
    loop {
        *shared.lock().unwrap_or_else(|e| e.into_inner()) = s.clone();
        publish(app, &s);
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(request) => {
                let result = handle(request.action, backend.as_mut(), path, &mut s);
                let stop = matches!(result, Ok(true));
                let reply = result.map(|_| ()).map_err(|e| format!("{e:#}"));
                if let Err(e) = &reply {
                    s.error = Some(e.clone());
                } else {
                    s.error = None;
                }
                *shared.lock().unwrap_or_else(|e| e.into_inner()) = s.clone();
                publish(app, &s);
                let _ = request.reply.send(reply);
                if stop {
                    return Ok(());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                if s.manual && now.duration_since(last_tick) > Duration::from_secs(15) {
                    let reset = restore(backend.as_mut(), &mut s);
                    s.error = Some(format!(
                        "Ripresa dopo sospensione: ripristino automatico {reset:?}"
                    ));
                }
                last_tick = now;
                deadline = now + Duration::from_millis(s.config.polling_ms);
                if s.selected.is_none() {
                    continue;
                }
                match backend.sample() {
                    Ok(sample) => {
                        failures = 0;
                        if s.manual && sample.too_hot() {
                            let reset = restore(backend.as_mut(), &mut s);
                            s.error = Some(format!(
                                "Soglia termica raggiunta: ripristino automatico {reset:?}"
                            ));
                        }
                        s.sample = Some(sample);
                        s.sampled_at_ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .ok()
                            .map(|t| t.as_millis() as u64);
                    }
                    Err(e) => {
                        failures += 1;
                        s.sample = None;
                        s.sampled_at_ms = None;
                        s.error = Some(format!("Telemetria non disponibile: {e:#}"));
                        if s.manual && failures >= 2 {
                            let reset = restore(backend.as_mut(), &mut s);
                            s.error = Some(format!(
                                "Telemetria persa. Ripristino automatico: {reset:?}"
                            ));
                        }
                    }
                }
                ticks += 1;
                // Detect an external tuning tool or driver reset; do not fight another controller.
                if s.manual && ticks.is_multiple_of(4) {
                    let info = backend.curve_info();
                    if !info
                        .as_ref()
                        .is_ok_and(|i| i.current.as_slice() == s.config.curve.as_slice())
                    {
                        let reset = restore(backend.as_mut(), &mut s);
                        s.error = Some(format!("Curva modificata dal driver o da un altro programma. Ripristino: {reset:?}"));
                    }
                }
            }
        }
    }
}
