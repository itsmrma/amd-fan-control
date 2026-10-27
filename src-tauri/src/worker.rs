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
                Some(format!("{e:#}. The original file has been preserved.")),
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
                    _ => "Worker stopped. The driver retains the last curve; restart the app to restore automatic control.".into(),
                };
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.error = Some(message); s.ready = true; s.sample = None;
                publish(&app, &s);
            }
        }).expect("Creating GPU worker");
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
            return Err(format!("Worker unavailable: {e}"));
        }
        receiver
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| {
                "The driver is not responding. Check the status before trying again.".to_string()
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
                    "Custom curve"
                } else {
                    "Monitoring"
                }
            ),
            None => "AMD Fan Control · Telemetry unavailable".into(),
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
    let adapter = selected.context("Select a GPU in Settings")?;
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

fn validate_zero_rpm(s: &Snapshot) -> Result<()> {
    if s.config.zero_rpm_stop_temperature()?.is_some()
        && !s
            .curve_info
            .as_ref()
            .is_some_and(|info| info.zero_rpm_supported)
    {
        bail!("This GPU/driver does not expose native Zero RPM control");
    }
    Ok(())
}

fn update_zero_rpm(backend: &mut dyn GpuBackend, s: &mut Snapshot, sample: &Sample) -> Result<()> {
    let active = s
        .curve_info
        .as_ref()
        .is_some_and(|info| info.zero_rpm_enabled);
    let desired = s.config.zero_rpm_requested(sample, active)?;
    if desired != active {
        let info = backend.curve_info()?;
        if info.current.as_slice() != s.config.curve.as_slice() || info.zero_rpm_enabled != active {
            bail!("Fan settings changed outside this app");
        }
        backend.set_zero_rpm(desired)?;
        let info = backend.curve_info()?;
        if info.zero_rpm_enabled != desired {
            bail!("Driver did not retain the requested Zero RPM state");
        }
        s.curve_info = Some(info);
    }
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
                bail!("Restore automatic control before editing settings");
            }
            // Select using a temporary snapshot; a failed selection does not replace the saved config.
            let mut next = s.clone();
            next.config = config;
            if let Err(e) = select(backend, &mut next).and_then(|_| {
                validate_zero_rpm(&next)?;
                next.config.save(path)
            }) {
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
                bail!("Wait for a valid temperature reading");
            }
            let points = native_points(&s.config.curve)?;
            validate_zero_rpm(s)?;
            let sample = backend.sample()?;
            let zero_rpm = s.config.zero_rpm_requested(&sample, false)?;
            s.manual = true;
            match backend
                .apply_native_curve(&points, zero_rpm)
                .and_then(|_| backend.curve_info())
            {
                Ok(info) => s.curve_info = Some(info),
                Err(e) => {
                    let reset = restore(backend, s);
                    bail!("{e:#}; automatic restore: {reset:?}");
                }
            }
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
                        "Resumed after sleep or a polling stall: automatic restore {reset:?}"
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
                                "Temperature threshold reached: automatic restore {reset:?}"
                            ));
                        }
                        if s.manual {
                            if let Err(e) = update_zero_rpm(backend.as_mut(), &mut s, &sample) {
                                let reset = restore(backend.as_mut(), &mut s);
                                s.error = Some(format!(
                                    "Zero RPM control failed: {e:#}; automatic restore: {reset:?}"
                                ));
                            }
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
                        s.error = Some(format!("Telemetry unavailable: {e:#}"));
                        if s.manual && failures >= 2 {
                            let reset = restore(backend.as_mut(), &mut s);
                            s.error = Some(format!("Telemetry lost. Automatic restore: {reset:?}"));
                        }
                    }
                }
                ticks += 1;
                // Detect an external tuning tool or driver reset; do not fight another controller.
                if s.manual && ticks.is_multiple_of(4) {
                    let info = backend.curve_info();
                    if !info.as_ref().is_ok_and(|i| {
                        i.current.as_slice() == s.config.curve.as_slice()
                            && i.zero_rpm_enabled
                                == s.curve_info
                                    .as_ref()
                                    .is_some_and(|info| info.zero_rpm_enabled)
                    }) {
                        let reset = restore(backend.as_mut(), &mut s);
                        s.error = Some(format!("Curve changed by the driver or another app. Automatic restore: {reset:?}"));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fan_core::curve::Point;

    struct FakeGpu {
        fail_apply: bool,
        fail_reset: bool,
        resets: usize,
    }
    impl GpuBackend for FakeGpu {
        fn adapters(&mut self) -> Result<Vec<Adapter>> {
            Ok(vec![])
        }
        fn select(&mut self, _: &Adapter) -> Result<()> {
            Ok(())
        }
        fn curve_info(&mut self) -> Result<CurveInfo> {
            Ok(CurveInfo {
                supported: true,
                zero_rpm_supported: true,
                zero_rpm_enabled: false,

                current: native_points(&Config::default().curve)?,
                minimum: [Point {
                    temperature: 25,
                    speed: 20,
                }; 5],
                maximum: [Point {
                    temperature: 100,
                    speed: 100,
                }; 5],
            })
        }
        fn sample(&mut self) -> Result<Sample> {
            Ok(Sample {
                edge_c: Some(50),
                ..Sample::default()
            })
        }
        fn apply_native_curve(&mut self, _: &[Point; 5], _: bool) -> Result<()> {
            if self.fail_apply {
                bail!("driver rejected write after partial apply");
            }
            Ok(())
        }
        fn restore_automatic(&mut self) -> Result<()> {
            self.resets += 1;
            if self.fail_reset {
                bail!("driver reset failed");
            }
            Ok(())
        }
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            config: Config::default(),
            adapters: vec![],
            selected: None,
            curve_info: None,
            sample: Some(Sample {
                edge_c: Some(50),
                ..Sample::default()
            }),
            sampled_at_ms: None,
            manual: false,
            error: None,
            ready: true,
        }
    }
    struct GateGpu {
        info: CurveInfo,
        writes: Vec<bool>,
        reject: bool,
    }
    impl GpuBackend for GateGpu {
        fn adapters(&mut self) -> Result<Vec<Adapter>> {
            Ok(vec![])
        }
        fn select(&mut self, _: &Adapter) -> Result<()> {
            Ok(())
        }
        fn curve_info(&mut self) -> Result<CurveInfo> {
            Ok(self.info.clone())
        }
        fn sample(&mut self) -> Result<Sample> {
            Ok(Sample {
                edge_c: Some(38),
                ..Sample::default()
            })
        }
        fn apply_native_curve(&mut self, points: &[Point; 5], zero_rpm: bool) -> Result<()> {
            self.info.current = *points;
            self.info.zero_rpm_enabled = zero_rpm;
            Ok(())
        }
        fn set_zero_rpm(&mut self, enabled: bool) -> Result<()> {
            self.writes.push(enabled);
            if !self.reject {
                self.info.zero_rpm_enabled = enabled;
            }
            Ok(())
        }
        fn restore_automatic(&mut self) -> Result<()> {
            self.info.zero_rpm_enabled = false;
            Ok(())
        }
    }
    fn gate_setup() -> (GateGpu, Snapshot) {
        let mut s = snapshot();
        s.config.curve[0].speed = 20;
        s.config.zero_rpm_below_percent = Some(25);
        let mut fake = FakeGpu {
            fail_apply: false,
            fail_reset: false,
            resets: 0,
        };
        let mut info = fake.curve_info().unwrap();
        info.current = native_points(&s.config.curve).unwrap();
        s.curve_info = Some(info.clone());
        (
            GateGpu {
                info,
                writes: vec![],
                reject: false,
            },
            s,
        )
    }
    #[test]
    fn zero_rpm_transitions_only_when_needed_and_keeps_the_native_curve() {
        let (mut gpu, mut s) = gate_setup();
        handle(
            Action::Apply,
            &mut gpu,
            std::path::Path::new("unused"),
            &mut s,
        )
        .unwrap();
        assert!(s.curve_info.as_ref().unwrap().zero_rpm_enabled);
        let warm = Sample {
            edge_c: Some(42),
            ..Sample::default()
        };
        update_zero_rpm(&mut gpu, &mut s, &warm).unwrap();
        update_zero_rpm(&mut gpu, &mut s, &warm).unwrap();
        let cool = Sample {
            edge_c: Some(38),
            ..Sample::default()
        };
        update_zero_rpm(&mut gpu, &mut s, &cool).unwrap();
        assert_eq!(gpu.writes, vec![false, true]);
        assert_eq!(gpu.info.current.as_slice(), s.config.curve.as_slice());
    }
    #[test]
    fn zero_rpm_rejects_unsupported_external_changes_and_failed_readback() {
        let (mut gpu, mut s) = gate_setup();
        let cool = Sample {
            edge_c: Some(38),
            ..Sample::default()
        };
        gpu.info.current[1].speed += 1;
        assert!(update_zero_rpm(&mut gpu, &mut s, &cool).is_err());
        assert!(gpu.writes.is_empty());
        gpu.info.current = native_points(&s.config.curve).unwrap();
        gpu.reject = true;
        assert!(update_zero_rpm(&mut gpu, &mut s, &cool).is_err());
        assert!(!s.curve_info.as_ref().unwrap().zero_rpm_enabled);
        s.curve_info.as_mut().unwrap().zero_rpm_supported = false;
        assert!(handle(
            Action::Apply,
            &mut gpu,
            std::path::Path::new("unused"),
            &mut s
        )
        .is_err());
        assert!(!s.manual);
    }
    #[test]
    fn failed_apply_attempts_reset_and_clears_manual_only_on_success() {
        let mut gpu = FakeGpu {
            fail_apply: true,
            fail_reset: false,
            resets: 0,
        };
        let mut s = snapshot();
        assert!(handle(
            Action::Apply,
            &mut gpu,
            std::path::Path::new("unused"),
            &mut s
        )
        .is_err());
        assert_eq!(gpu.resets, 1);
        assert!(!s.manual);
        gpu.fail_reset = true;
        assert!(handle(
            Action::Apply,
            &mut gpu,
            std::path::Path::new("unused"),
            &mut s
        )
        .is_err());
        assert!(s.manual); // A failed restore must never be reported as automatic.
    }
    #[test]
    fn failed_reset_prevents_normal_exit() {
        let mut gpu = FakeGpu {
            fail_apply: false,
            fail_reset: true,
            resets: 0,
        };
        let mut s = snapshot();
        s.manual = true;
        assert!(handle(
            Action::Stop,
            &mut gpu,
            std::path::Path::new("unused"),
            &mut s
        )
        .is_err());
        assert!(s.manual);
        gpu.fail_reset = false;
        assert!(handle(
            Action::Stop,
            &mut gpu,
            std::path::Path::new("unused"),
            &mut s
        )
        .unwrap());
        assert!(!s.manual);
    }
    #[test]
    fn cannot_replace_active_profile_or_persist_it_as_manual() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut gpu = FakeGpu {
            fail_apply: false,
            fail_reset: false,
            resets: 0,
        };
        let mut s = snapshot();
        s.config.save(&path).unwrap();
        s.manual = true;
        let mut modified = s.config.clone();
        modified.polling_ms = 10000;
        assert!(handle(Action::Save(modified), &mut gpu, &path, &mut s).is_err());
        assert_eq!(Config::load(&path).unwrap().polling_ms, 2500);
        assert!(s.manual);
    }
}
