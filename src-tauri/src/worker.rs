use anyhow::{bail, Context, Result};
use fan_core::{
    config::{Config, TemperatureSource},
    curve::{host_pwm_target_speed, interpolate, native_points, StepDown},
    hardware::{speed_hold_points, Adapter, CurveInfo, GpuBackend, Sample},
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
    #[serde(skip)]
    applied_speed: Option<i32>,
    #[serde(skip)]
    step_down: StepDown,
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
            applied_speed: None,
            step_down: StepDown::default(),
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
    s.applied_speed = None;
    s.step_down = StepDown::default();
    s.curve_info = Some(backend.curve_info()?);
    Ok(())
}

fn restore_after_failure(
    backend: &mut dyn GpuBackend,
    s: &mut Snapshot,
    reason: &str,
    details: Option<&str>,
) -> String {
    match restore(backend, s) {
        Ok(()) => format!(
            "{reason} Driver default fan settings restored.{}",
            details.map_or(String::new(), |d| format!("\n\nTechnical details: {d}"))
        ),
        Err(e) => format!(
            "{reason} Could not confirm driver default fan settings. Custom control may still be active. Use driver defaults to try again; keep the app open.\n\nTechnical details: {}Restore: {e:#}",
            details.map_or(String::new(), |d| format!("{d}\n"))
        ),
    }
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
        if Some(info.current) != s.curve_info.as_ref().map(|i| i.current)
            || info.zero_rpm_enabled != active
        {
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

fn requested_speed(s: &Snapshot, sample: &Sample) -> Result<i32> {
    let temperature = s.config.temperature_source.temperature(sample)?;
    Ok(interpolate(&s.config.curve, f64::from(temperature)).ceil() as i32)
}

fn requested_host_pwm_speed(s: &mut Snapshot, sample: &Sample, now: Instant) -> Result<i32> {
    let temperature = s.config.temperature_source.temperature(sample)?;
    Ok(host_pwm_target_speed(
        &s.config.curve,
        &mut s.step_down,
        temperature,
        now,
    ))
}

fn update_sensor_control_at(
    backend: &mut dyn GpuBackend,
    s: &mut Snapshot,
    sample: &Sample,
    now: Instant,
) -> Result<()> {
    let expected = s
        .curve_info
        .as_ref()
        .context("Fan control state unavailable")?;
    let actual = backend.curve_info()?;
    if actual.zero_rpm_enabled != expected.zero_rpm_enabled
        || (actual.native_curve_supported && actual.current != expected.current)
    {
        bail!("Fan settings changed outside this app");
    }
    if s.config.zero_rpm_stop_temperature()?.is_some() && !actual.zero_rpm_supported {
        bail!("This GPU/driver does not expose Zero RPM control");
    }
    let zero_rpm = s
        .config
        .zero_rpm_requested(sample, actual.zero_rpm_enabled)?;
    if actual.native_curve_supported {
        let speed = requested_speed(s, sample)?;
        let points = speed_hold_points(&actual, speed)?;
        if actual.current != points || actual.zero_rpm_enabled != zero_rpm {
            backend.apply_speed(speed, zero_rpm)?;
            let info = backend.curve_info()?;
            if info.current != points || info.zero_rpm_enabled != zero_rpm {
                bail!("Driver did not retain the requested sensor-based fan settings");
            }
            s.curve_info = Some(info);
        }
    } else if actual.host_pwm_supported {
        let speed = requested_host_pwm_speed(s, sample, now)?;
        let target_speed = if zero_rpm { 0 } else { speed };
        if s.applied_speed != Some(target_speed) || actual.zero_rpm_enabled != zero_rpm {
            backend.apply_speed(speed, zero_rpm)?;
            let info = backend.curve_info()?;
            if info.zero_rpm_enabled != zero_rpm {
                bail!("Driver did not retain the requested sensor-based fan settings");
            }
            s.applied_speed = Some(target_speed);
            s.curve_info = Some(info);
        }
    } else {
        bail!("Sensor-based fan control is not supported by this GPU/driver");
    }
    Ok(())
}

#[cfg(test)]
fn update_sensor_control(
    backend: &mut dyn GpuBackend,
    s: &mut Snapshot,
    sample: &Sample,
) -> Result<()> {
    update_sensor_control_at(backend, s, sample, Instant::now())
}

const READONLY_PERMISSION_MESSAGE: &str = "Fan control is read-only; live monitoring still works, but hardware write permissions are required. Install the 60-amd-fan-control.rules udev rule for hwmon PWM access and reload udev rules (or install the native .deb/.rpm package).";
const PERMISSION_DENIED_REASON: &str = "Missing hardware permissions to write hwmon PWM fan controls. Live monitoring continues in read-only mode; install the 60-amd-fan-control.rules udev rule for hwmon PWM access and reload udev rules.";

fn is_permission_denied(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
    })
}

fn is_zero_rpm_error(details: &str) -> bool {
    details.contains("Zero RPM") || details.contains("pwm1_min > 0") || details.contains("pwm1=0")
}

fn check_sensor_control(backend: &mut dyn GpuBackend, s: &mut Snapshot, sample: &Sample) {
    if let Err(e) = update_sensor_control_at(backend, s, sample, Instant::now()) {
        let details = format!("{e:#}");
        let reason = if is_permission_denied(&e) {
            PERMISSION_DENIED_REASON
        } else if is_zero_rpm_error(&details) {
            "The GPU driver rejected Zero RPM fan stop or lost Zero RPM support. Turn off ‘Allow fans to stop’, save, then try your custom curve again."
        } else if s
            .curve_info
            .as_ref()
            .is_some_and(|info| info.native_curve_supported)
        {
            "Sensor-based fan control stopped. Check the selected sensor, or select Driver / firmware sensor to use the previous control mode."
        } else {
            "Sensor-based fan control stopped. Check the selected temperature sensor and hardware access."
        };
        s.error = Some(restore_after_failure(backend, s, reason, Some(&details)));
    }
}

fn check_zero_rpm(backend: &mut dyn GpuBackend, s: &mut Snapshot, sample: &Sample) {
    if let Err(e) = update_zero_rpm(backend, s, sample) {
        let details = format!("{e:#}");
        let reason = if details.contains("ADL -8") {
            "The GPU driver does not support this Zero RPM change. Turn off ‘Allow fans to stop’, save, then try your custom curve again."
        } else {
            "The app could not change the fan-stop setting. Turn off ‘Allow fans to stop’, save, then try your custom curve again."
        };
        s.error = Some(restore_after_failure(backend, s, reason, Some(&details)));
    }
}

fn remember_custom_control(path: &std::path::Path, s: &mut Snapshot, enabled: bool) -> Result<()> {
    let mut config = s.config.clone();
    config.custom_control_enabled = enabled;
    if enabled {
        // Persist the exact selected GPU, including when it was selected implicitly.
        config.adapter_id = Some(s.selected.as_ref().context("No GPU selected")?.id.clone());
    }
    if config.custom_control_enabled != s.config.custom_control_enabled
        || config.adapter_id != s.config.adapter_id
    {
        config.save(path)?;
        s.config = config;
    }
    Ok(())
}

fn resume_custom_control(
    backend: &mut dyn GpuBackend,
    path: &std::path::Path,
    s: &mut Snapshot,
    pending: &mut bool,
) {
    if !*pending || !s.ready || s.sample.is_none() || s.selected.is_none() {
        return;
    }
    // One attempt per launch, after telemetry arrives. Safety stops and driver
    // rejections must not cause repeated automatic writes in this session.
    *pending = false;
    if !s.config.custom_control_enabled || s.manual {
        return;
    }
    if s.config.adapter_id.as_deref() != s.selected.as_ref().map(|a| a.id.as_str()) {
        s.error = Some("Could not resume the saved curve: the saved GPU is not selected.".into());
        return;
    }
    match handle(Action::Apply, backend, path, s) {
        Ok(_) => s.error = None,
        Err(e) => {
            s.error = Some(format!(
                "Could not resume the saved custom fan curve. {e:#}"
            ))
        }
    }
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
                bail!("Turn custom control off before editing settings");
            }
            // Select using a temporary snapshot; a failed selection does not replace the saved config.
            let mut next = s.clone();
            next.config = config;
            // The switch owns this preference; an older frontend draft must
            // not overwrite a more recent explicit on/off choice.
            next.config.custom_control_enabled = s.config.custom_control_enabled;
            if next.config.custom_control_enabled && next.config.adapter_id.is_none() {
                next.config.adapter_id = s.config.adapter_id.clone();
            }
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
            if !s.ready || s.sample.is_none() || s.selected.is_none() {
                bail!("Wait for a valid temperature reading");
            }
            let points = native_points(&s.config.curve)?;
            validate_zero_rpm(s)?;
            let sample = backend.sample()?;
            if sample.too_hot() {
                bail!("GPU is too hot to start custom fan control");
            }
            let current_info = backend.curve_info()?;
            if !current_info.writable {
                bail!("{READONLY_PERMISSION_MESSAGE}");
            }
            let (speed, expected_points) =
                if s.config.temperature_source == TemperatureSource::Firmware {
                    if !current_info.native_curve_supported {
                        bail!("Driver / firmware sensor is not supported by this GPU/driver");
                    }
                    (None, Some(points))
                } else {
                    if !current_info.host_pwm_supported {
                        bail!("Sensor-based fan control is not supported by this GPU/driver");
                    }
                    s.step_down = StepDown::default();
                    let (speed, expected) = if current_info.native_curve_supported {
                        let speed = requested_speed(s, &sample)?;
                        (speed, Some(speed_hold_points(&current_info, speed)?))
                    } else {
                        let speed = requested_host_pwm_speed(s, &sample, Instant::now())?;
                        (speed, None)
                    };
                    (Some(speed), expected)
                };
            let zero_rpm = s.config.zero_rpm_requested(&sample, false)?;
            s.manual = true;
            let result = if let Some(speed) = speed {
                backend.apply_speed(speed, zero_rpm)
            } else {
                backend.apply_native_curve(&points, zero_rpm)
            };
            match result.and_then(|_| backend.curve_info()).and_then(|info| {
                if expected_points.is_some_and(|expected| info.current != expected)
                    || info.zero_rpm_enabled != zero_rpm
                {
                    bail!("Driver did not retain the requested fan settings");
                }
                Ok(info)
            }) {
                Ok(info) => {
                    s.applied_speed = speed.map(|v| if zero_rpm { 0 } else { v });
                    s.curve_info = Some(info);
                }
                Err(e) => {
                    let details = format!("{e:#}");
                    let reason = if is_permission_denied(&e) {
                        PERMISSION_DENIED_REASON
                    } else if zero_rpm && is_zero_rpm_error(&details) {
                        "The GPU driver rejected Zero RPM fan stop. Turn off ‘Allow fans to stop’, save, then try your custom curve again."
                    } else {
                        "The custom fan curve could not be applied."
                    };
                    bail!(
                        "{}",
                        restore_after_failure(backend, s, reason, Some(&details))
                    );
                }
            }
            if let Err(e) = remember_custom_control(path, s, true) {
                bail!(
                    "{}",
                    restore_after_failure(
                        backend,
                        s,
                        "The custom-control choice could not be saved.",
                        Some(&format!("{e:#}")),
                    )
                );
            }
        }
        Action::Automatic => {
            // Still restore hardware if saving fails. Conversely, persist the
            // explicit off choice even if the driver refuses restoration.
            let saved = remember_custom_control(path, s, false);
            restore(backend, s).context("Could not confirm driver default fan settings. Use driver defaults to try again; keep the app open")?;
            saved.context("Driver defaults restored, but the off choice could not be saved. Custom control may resume on the next launch")?;
        }
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
    let mut resume_pending = s.config.custom_control_enabled;
    let mut last_tick = Instant::now();
    let mut deadline = Instant::now();
    let mut failures = 0;
    let mut ticks = 0u32;
    loop {
        *shared.lock().unwrap_or_else(|e| e.into_inner()) = s.clone();
        publish(app, &s);
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(request) => {
                // An explicit user action takes precedence over startup restoration.
                resume_pending = false;
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
                deadline = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                if s.manual && now.duration_since(last_tick) > Duration::from_secs(15) {
                    s.error = Some(restore_after_failure(
                        backend.as_mut(),
                        &mut s,
                        "Custom control stopped after sleep or a pause in GPU readings.",
                        None,
                    ));
                }
                last_tick = now;
                // Selected-sensor control has a one-second control clock even
                // when a slower monitoring refresh interval is configured.
                let interval =
                    if s.manual && s.config.temperature_source != TemperatureSource::Firmware {
                        s.config.polling_ms.min(1000)
                    } else {
                        s.config.polling_ms
                    };
                deadline = now + Duration::from_millis(interval);
                if s.selected.is_none() {
                    continue;
                }
                match backend.sample() {
                    Ok(sample) => {
                        failures = 0;
                        if s.manual && sample.too_hot() {
                            s.error = Some(restore_after_failure(
                                backend.as_mut(),
                                &mut s,
                                "The GPU reached the app's temperature safety limit.",
                                None,
                            ));
                        }
                        if s.manual && s.config.temperature_source != TemperatureSource::Firmware {
                            check_sensor_control(backend.as_mut(), &mut s, &sample);
                        } else if s.manual {
                            check_zero_rpm(backend.as_mut(), &mut s, &sample);
                        }
                        s.sample = Some(sample);
                        s.sampled_at_ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .ok()
                            .map(|t| t.as_millis() as u64);
                        resume_custom_control(backend.as_mut(), path, &mut s, &mut resume_pending);
                    }
                    Err(e) => {
                        failures += 1;
                        s.sample = None;
                        s.sampled_at_ms = None;
                        s.error = Some(format!("Telemetry unavailable: {e:#}"));
                        if s.manual && failures >= 2 {
                            s.error = Some(restore_after_failure(
                                backend.as_mut(),
                                &mut s,
                                "GPU readings were lost.",
                                Some(&format!("{e:#}")),
                            ));
                        }
                    }
                }
                ticks += 1;
                // Detect an external tuning tool or driver reset; do not fight another controller.
                if s.manual
                    && s.config.temperature_source == TemperatureSource::Firmware
                    && ticks.is_multiple_of(4)
                {
                    let info = backend.curve_info();
                    if !info.as_ref().is_ok_and(|i| {
                        Some(i.current) == s.curve_info.as_ref().map(|info| info.current)
                            && i.zero_rpm_enabled
                                == s.curve_info
                                    .as_ref()
                                    .is_some_and(|info| info.zero_rpm_enabled)
                    }) {
                        s.error = Some(restore_after_failure(
                            backend.as_mut(),
                            &mut s,
                            "Fan settings changed in the driver or another app.",
                            None,
                        ));
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
                native_curve_supported: true,
                host_pwm_supported: true,
                writable: true,
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
        let adapter = Adapter {
            index: 0,
            id: "pci:41:0:0".into(),
            name: "Test GPU".into(),
            od_version: Some(8),
        };
        Snapshot {
            config: Config {
                temperature_source: TemperatureSource::Firmware,
                ..Config::default()
            },
            adapters: vec![adapter.clone()],
            selected: Some(adapter),
            curve_info: None,
            sample: Some(Sample {
                edge_c: Some(50),
                ..Sample::default()
            }),
            sampled_at_ms: None,
            manual: false,
            error: None,
            ready: true,
            applied_speed: None,
            step_down: StepDown::default(),
        }
    }
    fn test_request(action: Action, gpu: &mut dyn GpuBackend, s: &mut Snapshot) -> Result<bool> {
        let dir = tempfile::tempdir()?;
        handle(action, gpu, &dir.path().join("config.json"), s)
    }
    struct GateGpu {
        info: CurveInfo,
        writes: Vec<bool>,
        speed_writes: Vec<i32>,
        sample: Sample,
        reject: bool,
        fail_write: bool,
        fail_reset: bool,
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
            Ok(self.sample.clone())
        }
        fn apply_native_curve(&mut self, points: &[Point; 5], zero_rpm: bool) -> Result<()> {
            self.info.current = *points;
            self.info.zero_rpm_enabled = zero_rpm;
            Ok(())
        }
        fn apply_speed(&mut self, percent: i32, zero_rpm: bool) -> Result<()> {
            self.speed_writes.push(percent);
            if self.fail_write {
                bail!("Sensor speed write (ADL -8)");
            }
            if !self.reject {
                self.info.current = speed_hold_points(&self.info, percent)?;
                self.info.zero_rpm_enabled = zero_rpm;
            }
            Ok(())
        }
        fn set_zero_rpm(&mut self, enabled: bool) -> Result<()> {
            self.writes.push(enabled);
            if self.fail_write {
                bail!("Zero RPM write (ADL -8)");
            }
            if !self.reject {
                self.info.zero_rpm_enabled = enabled;
            }
            Ok(())
        }
        fn restore_automatic(&mut self) -> Result<()> {
            if self.fail_reset {
                bail!("driver reset failed");
            }
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
                speed_writes: vec![],
                sample: Sample {
                    edge_c: Some(38),
                    hotspot_c: Some(38),
                    ..Sample::default()
                },
                reject: false,
                fail_write: false,
                fail_reset: false,
            },
            s,
        )
    }
    #[test]
    fn enabling_custom_control_persists_the_choice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let (mut gpu, mut s) = gate_setup();
        handle(Action::Apply, &mut gpu, &path, &mut s).unwrap();
        assert!(
            path.exists(),
            "The custom-control choice must be saved immediately"
        );
        let saved = serde_json::to_value(Config::load(&path).unwrap()).unwrap();
        assert_eq!(saved["custom_control_enabled"], true);
    }

    #[test]
    fn restart_resumes_the_saved_curve_after_telemetry_and_exit_preserves_the_choice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let (mut gpu, mut s) = gate_setup();
        s.config.temperature_source = TemperatureSource::Gpu;
        handle(Action::Apply, &mut gpu, &path, &mut s).unwrap();
        assert_eq!(s.config.adapter_id.as_deref(), Some("pci:41:0:0"));
        handle(Action::Stop, &mut gpu, &path, &mut s).unwrap();
        assert!(!s.manual);
        assert!(Config::load(&path).unwrap().custom_control_enabled);

        let (mut gpu, mut restarted) = gate_setup();
        restarted.config = Config::load(&path).unwrap();
        restarted.ready = false;
        restarted.sample = None;
        let mut pending = restarted.config.custom_control_enabled;
        resume_custom_control(&mut gpu, &path, &mut restarted, &mut pending);
        assert!(pending);
        assert!(!restarted.manual);
        assert!(gpu.speed_writes.is_empty());
        restarted.ready = true;
        resume_custom_control(&mut gpu, &path, &mut restarted, &mut pending);
        assert!(pending);
        assert!(gpu.speed_writes.is_empty());
        restarted.sample = Some(gpu.sample.clone());
        restarted.error = Some("Telemetry unavailable during startup".into());
        resume_custom_control(&mut gpu, &path, &mut restarted, &mut pending);
        assert!(!pending);
        assert!(restarted.manual);
        assert!(restarted.error.is_none());
        assert_eq!(gpu.speed_writes, [20]);
        resume_custom_control(&mut gpu, &path, &mut restarted, &mut pending);
        assert_eq!(gpu.speed_writes.len(), 1);

        handle(Action::Automatic, &mut gpu, &path, &mut restarted).unwrap();
        assert!(!restarted.manual);
        assert!(!Config::load(&path).unwrap().custom_control_enabled);
        let (mut gpu, mut off) = gate_setup();
        off.config = Config::load(&path).unwrap();
        let mut pending = off.config.custom_control_enabled;
        resume_custom_control(&mut gpu, &path, &mut off, &mut pending);
        assert!(!off.manual);
        assert!(gpu.speed_writes.is_empty());
    }

    #[test]
    fn startup_rejects_unsafe_readings_and_driver_failures_without_retrying() {
        for failure in ["hot", "missing", "invalid", "rejected", "wrong_gpu"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.json");
            let (mut gpu, mut s) = gate_setup();
            s.config.temperature_source = TemperatureSource::Gpu;
            s.config.custom_control_enabled = true;
            s.config.adapter_id = Some("pci:41:0:0".into());
            s.config.save(&path).unwrap();
            match failure {
                "hot" => gpu.sample.edge_c = Some(90),
                "missing" => gpu.sample.edge_c = None,
                "invalid" => gpu.sample.edge_c = Some(0),
                "rejected" => gpu.reject = true,
                "wrong_gpu" => s.selected.as_mut().unwrap().id = "pci:42:0:0".into(),
                _ => unreachable!(),
            }
            let mut pending = true;
            resume_custom_control(&mut gpu, &path, &mut s, &mut pending);
            assert!(!pending, "{failure}");
            assert!(!s.manual, "{failure}");
            assert!(s.error.as_ref().unwrap().starts_with("Could not resume"));
            assert_eq!(gpu.speed_writes.len(), usize::from(failure == "rejected"));
            gpu.reject = false;
            gpu.sample.edge_c = Some(38);
            s.selected.as_mut().unwrap().id = "pci:41:0:0".into();
            let writes = gpu.speed_writes.len();
            resume_custom_control(&mut gpu, &path, &mut s, &mut pending);
            assert!(!s.manual);
            assert_eq!(gpu.speed_writes.len(), writes);
        }
    }

    #[test]
    fn failed_choice_save_restores_hardware_and_does_not_enable_restart() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("config.json");
        std::fs::create_dir(&blocked).unwrap();
        let (mut gpu, mut s) = gate_setup();
        s.config.temperature_source = TemperatureSource::Gpu;
        let error = handle(Action::Apply, &mut gpu, &blocked, &mut s).unwrap_err();
        assert!(error.to_string().contains("choice could not be saved"));
        assert!(!s.manual);
        assert!(!s.config.custom_control_enabled);
        assert!(!gpu.info.zero_rpm_enabled);
    }

    #[test]
    fn explicit_off_is_saved_even_when_driver_restore_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let (mut gpu, mut s) = gate_setup();
        handle(Action::Apply, &mut gpu, &path, &mut s).unwrap();
        gpu.fail_reset = true;
        assert!(handle(Action::Automatic, &mut gpu, &path, &mut s).is_err());
        assert!(s.manual);
        assert!(!s.config.custom_control_enabled);
        assert!(!Config::load(&path).unwrap().custom_control_enabled);
    }

    #[test]
    fn explicit_off_still_restores_hardware_when_saving_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let blocked = dir.path().join("blocked.json");
        std::fs::create_dir(&blocked).unwrap();
        let (mut gpu, mut s) = gate_setup();
        handle(Action::Apply, &mut gpu, &path, &mut s).unwrap();
        let error = handle(Action::Automatic, &mut gpu, &blocked, &mut s).unwrap_err();
        assert!(error.to_string().contains("off choice could not be saved"));
        assert!(!s.manual);
        assert!(!gpu.info.zero_rpm_enabled);
        assert!(Config::load(&path).unwrap().custom_control_enabled);
    }

    #[test]
    fn stale_settings_draft_cannot_overwrite_the_switch_preference() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let (mut gpu, mut s) = gate_setup();
        handle(Action::Apply, &mut gpu, &path, &mut s).unwrap();
        restore(&mut gpu, &mut s).unwrap();
        let mut draft = s.config.clone();
        draft.custom_control_enabled = false;
        draft.adapter_id = None;
        draft.polling_ms = 3000;
        handle(Action::Save(draft), &mut gpu, &path, &mut s).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert!(loaded.custom_control_enabled);
        assert_eq!(loaded.adapter_id.as_deref(), Some("pci:41:0:0"));
        assert_eq!(loaded.polling_ms, 3000);
    }

    #[test]
    fn sensor_selection_controls_speed_and_unchanged_speed_skips_writes() {
        for (source, expected) in [
            (TemperatureSource::Gpu, 40),
            (TemperatureSource::Hotspot, 70),
        ] {
            let (mut gpu, mut s) = gate_setup();
            s.config.temperature_source = source;
            s.config.zero_rpm_below_percent = None;
            gpu.sample.edge_c = Some(50);
            gpu.sample.hotspot_c = Some(70);
            test_request(Action::Apply, &mut gpu, &mut s).unwrap();
            assert!(s.manual);
            assert_eq!(gpu.speed_writes, [expected]);
            assert_eq!(gpu.info.current[0].speed, expected);
            assert_eq!(gpu.info.current[4].speed, 100);
            let sample = gpu.sample.clone();
            update_sensor_control(&mut gpu, &mut s, &sample).unwrap();
            assert_eq!(gpu.speed_writes.len(), 1);
            let warmer = Sample {
                edge_c: Some(65),
                hotspot_c: Some(75),
                ..Sample::default()
            };
            update_sensor_control(&mut gpu, &mut s, &warmer).unwrap();
            assert_eq!(
                gpu.speed_writes[1],
                if source == TemperatureSource::Gpu {
                    60
                } else {
                    80
                }
            );
            let missing = Sample {
                edge_c: None,
                hotspot_c: None,
                ..Sample::default()
            };
            assert!(update_sensor_control(&mut gpu, &mut s, &missing).is_err());
            gpu.info.current[0].speed += 1;
            assert!(update_sensor_control(&mut gpu, &mut s, &warmer).is_err());
            assert_eq!(gpu.speed_writes.len(), 2);
        }
    }
    #[test]
    fn sensor_control_failure_restores_defaults_or_keeps_recovery_available() {
        for fail_reset in [false, true] {
            for missing_sensor in [false, true] {
                let (mut gpu, mut s) = gate_setup();
                s.config.temperature_source = TemperatureSource::Gpu;
                test_request(Action::Apply, &mut gpu, &mut s).unwrap();
                gpu.fail_reset = fail_reset;
                gpu.fail_write = !missing_sensor;
                let sample = Sample {
                    edge_c: if missing_sensor { None } else { Some(60) },
                    hotspot_c: Some(70),
                    ..Sample::default()
                };
                check_sensor_control(&mut gpu, &mut s, &sample);
                assert_eq!(s.manual, fail_reset);
                assert!(s.error.as_ref().unwrap().contains(if fail_reset {
                    "Could not confirm"
                } else {
                    "Driver default fan settings restored"
                }));
            }
        }
        let (mut gpu, mut s) = gate_setup();
        s.config.temperature_source = TemperatureSource::Gpu;
        gpu.reject = true;
        assert!(test_request(Action::Apply, &mut gpu, &mut s).is_err());
        assert!(!s.manual);
    }
    #[test]
    fn missing_selected_sensor_prevents_apply_without_falling_back() {
        let (mut gpu, mut s) = gate_setup();
        s.config.temperature_source = TemperatureSource::Hotspot;
        gpu.sample.hotspot_c = None;
        let error = test_request(Action::Apply, &mut gpu, &mut s).unwrap_err();
        assert!(error
            .to_string()
            .contains("Hotspot temperature sensor unavailable"));
        assert!(gpu.speed_writes.is_empty());
        assert!(!s.manual);
    }
    #[test]
    fn zero_rpm_failure_explains_rejection_and_reports_restore_outcome() {
        for fail_reset in [false, true] {
            let (mut gpu, mut s) = gate_setup();
            s.manual = true;
            gpu.fail_write = true;
            gpu.fail_reset = fail_reset;
            let cool = Sample {
                edge_c: Some(38),
                ..Sample::default()
            };
            check_zero_rpm(&mut gpu, &mut s, &cool);
            let error = s.error.as_ref().unwrap();
            assert!(error.starts_with("The GPU driver does not support this Zero RPM change."));
            assert!(error.contains("Technical details: Zero RPM write (ADL -8)"));
            assert!(!error.contains("Ok(())"));
            assert!(!error.contains("Err("));
            assert_eq!(s.manual, fail_reset);
            if fail_reset {
                assert!(error.contains("Custom control may still be active"));
                assert!(!error.contains("Driver default fan settings restored"));
                assert!(error.contains("Restore: driver reset failed"));
            } else {
                assert!(error.contains("Driver default fan settings restored"));
                assert!(error.contains("Allow fans to stop"));
            }
        }
    }
    #[test]
    fn switching_off_restores_defaults_without_discarding_the_saved_curve() {
        let (mut gpu, mut s) = gate_setup();
        let saved = s.config.curve.clone();
        test_request(Action::Apply, &mut gpu, &mut s).unwrap();
        assert!(s.manual);
        test_request(Action::Automatic, &mut gpu, &mut s).unwrap();
        assert!(!s.manual);
        assert_eq!(s.config.curve, saved);
    }
    #[test]
    fn zero_rpm_transitions_only_when_needed_and_keeps_the_native_curve() {
        let (mut gpu, mut s) = gate_setup();
        test_request(Action::Apply, &mut gpu, &mut s).unwrap();
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
        assert!(test_request(Action::Apply, &mut gpu, &mut s).is_err());
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
        let error = test_request(Action::Apply, &mut gpu, &mut s)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("Driver default fan settings restored"),
            "{error}"
        );
        assert!(!error.contains("Ok(())"), "{error}");
        assert_eq!(gpu.resets, 1);
        assert!(!s.manual);
        gpu.fail_reset = true;
        assert!(test_request(Action::Apply, &mut gpu, &mut s).is_err());
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
        assert!(test_request(Action::Stop, &mut gpu, &mut s).is_err());
        assert!(s.manual);
        gpu.fail_reset = false;
        assert!(test_request(Action::Stop, &mut gpu, &mut s).unwrap());
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

    struct HostPwmGpu {
        info: CurveInfo,
        sample: Sample,
        speed_writes: Vec<(i32, bool)>,
        resets: usize,
    }
    impl HostPwmGpu {
        fn new(writable: bool) -> Self {
            Self {
                info: CurveInfo {
                    supported: true,
                    native_curve_supported: false,
                    host_pwm_supported: true,
                    writable,
                    zero_rpm_supported: true,
                    zero_rpm_enabled: false,
                    current: [Point::default(); 5],
                    minimum: [Point {
                        temperature: 20,
                        speed: 20,
                    }; 5],
                    maximum: [Point {
                        temperature: 100,
                        speed: 100,
                    }; 5],
                },
                sample: Sample {
                    edge_c: Some(50),
                    hotspot_c: Some(70),
                    ..Sample::default()
                },
                speed_writes: vec![],
                resets: 0,
            }
        }
    }
    impl GpuBackend for HostPwmGpu {
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
            Ok(self.sample.clone())
        }
        fn apply_native_curve(&mut self, _: &[Point; 5], _: bool) -> Result<()> {
            bail!("native firmware curve is not supported on host-PWM backend");
        }
        fn apply_speed(&mut self, percent: i32, zero_rpm: bool) -> Result<()> {
            self.speed_writes.push((percent, zero_rpm));
            self.info.zero_rpm_enabled = zero_rpm;
            Ok(())
        }
        fn restore_automatic(&mut self) -> Result<()> {
            self.resets += 1;
            self.info.zero_rpm_enabled = false;
            Ok(())
        }
    }

    fn host_pwm_snapshot(gpu: &HostPwmGpu, source: TemperatureSource) -> Snapshot {
        let mut s = snapshot();
        s.selected.as_mut().unwrap().od_version = None;
        s.config.temperature_source = source;
        s.config.zero_rpm_below_percent = None;
        s.curve_info = Some(gpu.info.clone());
        s.sample = Some(gpu.sample.clone());
        s
    }

    #[test]
    fn host_pwm_only_backend_controls_speed_without_od8_points() {
        for (source, expected_speed) in [
            (TemperatureSource::Gpu, 40),
            (TemperatureSource::Hotspot, 70),
        ] {
            let mut gpu = HostPwmGpu::new(true);
            let mut s = host_pwm_snapshot(&gpu, source);
            test_request(Action::Apply, &mut gpu, &mut s).unwrap();
            assert!(s.manual);
            assert_eq!(gpu.speed_writes, vec![(expected_speed, false)]);

            // Unchanged sample skips redundant writes.
            let sample = gpu.sample.clone();
            update_sensor_control(&mut gpu, &mut s, &sample).unwrap();
            assert_eq!(gpu.speed_writes.len(), 1);

            // Warmer sample updates speed without calling speed_hold_points.
            let warmer = Sample {
                edge_c: Some(65),
                hotspot_c: Some(75),
                ..Sample::default()
            };
            update_sensor_control(&mut gpu, &mut s, &warmer).unwrap();
            let next_speed = if source == TemperatureSource::Gpu {
                60
            } else {
                80
            };
            assert_eq!(
                gpu.speed_writes,
                vec![(expected_speed, false), (next_speed, false)]
            );

            // External zero_rpm change is detected.
            gpu.info.zero_rpm_enabled = true;
            assert!(update_sensor_control(&mut gpu, &mut s, &warmer).is_err());

            test_request(Action::Automatic, &mut gpu, &mut s).unwrap();
            assert!(!s.manual);
            assert_eq!(gpu.resets, 1);
        }
    }

    #[test]
    fn host_pwm_apply_with_zero_rpm_skips_redundant_write_on_next_tick() {
        let mut gpu = HostPwmGpu::new(true);
        gpu.sample = Sample {
            edge_c: Some(38),
            hotspot_c: Some(42),
            ..Sample::default()
        };
        let mut s = host_pwm_snapshot(&gpu, TemperatureSource::Gpu);
        s.config.curve[0].speed = 20;
        s.config.zero_rpm_below_percent = Some(25);

        test_request(Action::Apply, &mut gpu, &mut s).unwrap();
        assert!(s.manual);
        assert!(s.curve_info.as_ref().unwrap().zero_rpm_enabled);
        assert_eq!(gpu.speed_writes.len(), 1);

        let sample = gpu.sample.clone();
        update_sensor_control(&mut gpu, &mut s, &sample).unwrap();
        assert_eq!(gpu.speed_writes.len(), 1);
    }

    #[test]
    fn host_pwm_only_backend_rejects_firmware_sensor_without_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut gpu = HostPwmGpu::new(true);
        let mut s = host_pwm_snapshot(&gpu, TemperatureSource::Firmware);

        let err = handle(Action::Apply, &mut gpu, &path, &mut s).unwrap_err();
        assert!(err.to_string().contains("Driver / firmware sensor"));
        assert!(!s.manual);
        assert!(gpu.speed_writes.is_empty());

        // Startup resume must also reject Firmware mode without substituting a sensor.
        s.config.custom_control_enabled = true;
        s.config.adapter_id = Some(s.selected.as_ref().unwrap().id.clone());
        s.config.save(&path).unwrap();
        let mut pending = true;
        resume_custom_control(&mut gpu, &path, &mut s, &mut pending);
        assert!(!pending);
        assert!(!s.manual);
        assert!(s.error.as_ref().unwrap().starts_with("Could not resume"));
        assert!(s
            .error
            .as_ref()
            .unwrap()
            .contains("Driver / firmware sensor"));
        assert!(gpu.speed_writes.is_empty());
    }

    #[test]
    fn read_only_backend_rejects_apply_and_resume_while_keeping_telemetry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut gpu = HostPwmGpu::new(false);
        let mut s = host_pwm_snapshot(&gpu, TemperatureSource::Gpu);

        let err = handle(Action::Apply, &mut gpu, &path, &mut s).unwrap_err();
        assert!(err.to_string().contains("read-only"));
        assert!(!s.manual);
        assert!(s.sample.is_some());
        assert!(gpu.speed_writes.is_empty());

        s.config.custom_control_enabled = true;
        s.config.adapter_id = Some(s.selected.as_ref().unwrap().id.clone());
        s.config.save(&path).unwrap();
        let mut pending = true;
        resume_custom_control(&mut gpu, &path, &mut s, &mut pending);
        assert!(!pending);
        assert!(!s.manual);
        assert!(s.error.as_ref().unwrap().contains("read-only"));
        assert!(gpu.speed_writes.is_empty());
    }

    #[test]
    fn host_pwm_uses_stepdown_hysteresis_rising_immediately_and_cooling_after_delay() {
        let mut gpu = HostPwmGpu::new(true);
        gpu.sample.edge_c = Some(70);
        let mut s = host_pwm_snapshot(&gpu, TemperatureSource::Gpu);

        test_request(Action::Apply, &mut gpu, &mut s).unwrap();
        let speed_70 = requested_speed(&s, &gpu.sample).unwrap();
        assert_eq!(gpu.speed_writes, vec![(speed_70, false)]);

        let t0 = Instant::now();
        let cooler = Sample {
            edge_c: Some(55),
            ..gpu.sample.clone()
        };
        let speed_55 = requested_speed(&s, &cooler).unwrap();
        assert!(speed_55 < speed_70);

        // Within the 6s hold delay, cooling does not step down yet.
        update_sensor_control_at(&mut gpu, &mut s, &cooler, t0).unwrap();
        update_sensor_control_at(&mut gpu, &mut s, &cooler, t0 + Duration::from_secs(5)).unwrap();
        assert_eq!(gpu.speed_writes, vec![(speed_70, false)]);

        // After 6s continuously below the 3 °C band, speed steps down.
        update_sensor_control_at(&mut gpu, &mut s, &cooler, t0 + Duration::from_secs(6)).unwrap();
        assert_eq!(gpu.speed_writes, vec![(speed_70, false), (speed_55, false)]);

        // Temperature rise increases fan speed immediately.
        let warmer = Sample {
            edge_c: Some(75),
            ..gpu.sample.clone()
        };
        let speed_75 = requested_speed(&s, &warmer).unwrap();
        update_sensor_control_at(&mut gpu, &mut s, &warmer, t0 + Duration::from_secs(7)).unwrap();
        assert_eq!(
            gpu.speed_writes,
            vec![(speed_70, false), (speed_55, false), (speed_75, false)]
        );
    }

    #[cfg(target_os = "linux")]
    #[cfg(target_os = "linux")]
    #[test]
    fn sysfs_worker_applies_curve_restores_on_tampering_and_resumes_exact_pci_id() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pci_dev = root.join("devices/pci0000:00/0000:03:00.0");
        std::fs::create_dir_all(&pci_dev).unwrap();
        std::fs::write(pci_dev.join("vendor"), "0x1002\n").unwrap();
        std::fs::write(
            pci_dev.join("uevent"),
            "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:03:00.0\n",
        )
        .unwrap();
        let driver_dir = root.join("bus/pci/drivers/amdgpu");
        std::fs::create_dir_all(&driver_dir).unwrap();
        symlink(&driver_dir, pci_dev.join("driver")).unwrap();
        let card_dir = root.join("class/drm/card0");
        std::fs::create_dir_all(&card_dir).unwrap();
        symlink(&pci_dev, card_dir.join("device")).unwrap();

        let hwmon = pci_dev.join("hwmon/hwmon0");
        std::fs::create_dir_all(&hwmon).unwrap();
        std::fs::write(hwmon.join("name"), "amdgpu\n").unwrap();
        std::fs::write(hwmon.join("temp1_input"), "50000\n").unwrap();
        std::fs::write(hwmon.join("pwm1_enable"), "2\n").unwrap();
        std::fs::write(hwmon.join("pwm1"), "80\n").unwrap();

        let mut backend = amd_adl::SysfsBackend::with_root(root);
        let adapters = backend.adapters().unwrap();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].id, "pci:03:00.0");

        let config_path = root.join("config.json");
        let mut s = Snapshot {
            config: Config::default(),
            adapters: adapters.clone(),
            selected: None,
            curve_info: None,
            sample: None,
            sampled_at_ms: None,
            manual: false,
            error: None,
            ready: true,
            applied_speed: None,
            step_down: StepDown::default(),
        };
        select(&mut backend, &mut s).unwrap();
        s.sample = Some(backend.sample().unwrap());

        handle(Action::Apply, &mut backend, &config_path, &mut s).unwrap();
        assert!(s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "1"
        );
        let expected_speed = requested_speed(&s, s.sample.as_ref().unwrap()).unwrap();
        let expected_pwm = ((expected_speed * 255 + 50) / 100).to_string();
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1")).unwrap().trim(),
            expected_pwm
        );

        // External tampering triggers safety restoration.
        std::fs::write(hwmon.join("pwm1"), "250\n").unwrap();
        let sample = backend.sample().unwrap();
        check_sensor_control(&mut backend, &mut s, &sample);
        assert!(!s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "2"
        );
        assert!(s.error.is_some());

        // Startup resume: mismatched PCI ID fails once without retrying.
        s.config.custom_control_enabled = true;
        s.config.adapter_id = Some("pci:99:00.0".into());
        let mut pending = true;
        resume_custom_control(&mut backend, &config_path, &mut s, &mut pending);
        assert!(!pending);
        assert!(!s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "2"
        );

        // Startup resume: exact saved PCI ID activates manual control and Stop restores automatic.
        s.config.adapter_id = Some("pci:03:00.0".into());
        let mut pending = true;
        resume_custom_control(&mut backend, &config_path, &mut s, &mut pending);
        assert!(!pending);
        assert!(s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "1"
        );
        assert!(handle(Action::Stop, &mut backend, &config_path, &mut s).unwrap());
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "2"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sysfs_worker_zero_rpm_uses_selected_sensor_hysteresis_and_thermal_safety() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pci_dev = root.join("devices/pci0000:00/0000:03:00.0");
        std::fs::create_dir_all(&pci_dev).unwrap();
        std::fs::write(pci_dev.join("vendor"), "0x1002\n").unwrap();
        std::fs::write(
            pci_dev.join("uevent"),
            "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:03:00.0\n",
        )
        .unwrap();
        let card = root.join("class/drm/card0");
        std::fs::create_dir_all(&card).unwrap();
        std::os::unix::fs::symlink(&pci_dev, card.join("device")).unwrap();
        let hwmon = pci_dev.join("hwmon/hwmon0");
        std::fs::create_dir_all(&hwmon).unwrap();
        std::fs::write(hwmon.join("name"), "amdgpu\n").unwrap();
        std::fs::write(hwmon.join("temp1_input"), "38000\n").unwrap();
        std::fs::write(hwmon.join("temp1_label"), "edge\n").unwrap();
        std::fs::write(hwmon.join("temp2_input"), "60000\n").unwrap();
        std::fs::write(hwmon.join("temp2_label"), "junction\n").unwrap();
        std::fs::write(hwmon.join("pwm1_enable"), "2\n").unwrap();
        std::fs::write(hwmon.join("pwm1"), "80\n").unwrap();
        std::fs::write(hwmon.join("pwm1_min"), "0\n").unwrap();

        let mut backend = amd_adl::SysfsBackend::with_root(root);
        let adapters = backend.adapters().unwrap();
        let config_path = root.join("config.json");
        let mut config = Config {
            temperature_source: TemperatureSource::Gpu,
            zero_rpm_below_percent: Some(25),
            ..Config::default()
        };
        config.curve[0].speed = 20;
        let mut s = Snapshot {
            config,
            adapters,
            selected: None,
            curve_info: None,
            sample: None,
            sampled_at_ms: None,
            manual: false,
            error: None,
            ready: true,
            applied_speed: None,
            step_down: StepDown::default(),
        };
        select(&mut backend, &mut s).unwrap();
        s.sample = Some(backend.sample().unwrap());

        // 38 °C edge (< 42 - 3 = 39 °C) requests fan stop even though hotspot is 60 °C.
        handle(Action::Apply, &mut backend, &config_path, &mut s).unwrap();
        assert!(s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1")).unwrap().trim(),
            "0"
        );
        assert!(s.curve_info.as_ref().unwrap().zero_rpm_enabled);

        // 40 °C edge (between 39 and 42 °C) retains fan stop despite 0% measured fan speed.
        std::fs::write(hwmon.join("temp1_input"), "40000\n").unwrap();
        let sample = backend.sample().unwrap();
        assert_eq!(sample.speed_percent, Some(0));
        check_sensor_control(&mut backend, &mut s, &sample);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1")).unwrap().trim(),
            "0"
        );
        assert!(s.curve_info.as_ref().unwrap().zero_rpm_enabled);

        // 42 °C edge (>= cutoff) withdraws fan stop immediately.
        std::fs::write(hwmon.join("temp1_input"), "42000\n").unwrap();
        let sample = backend.sample().unwrap();
        check_sensor_control(&mut backend, &mut s, &sample);
        assert!(!s.curve_info.as_ref().unwrap().zero_rpm_enabled);
        assert_ne!(
            std::fs::read_to_string(hwmon.join("pwm1")).unwrap().trim(),
            "0"
        );

        // Cooling to 40 °C (>= 39 °C) does not re-arm fan stop yet.
        std::fs::write(hwmon.join("temp1_input"), "40000\n").unwrap();
        let sample = backend.sample().unwrap();
        check_sensor_control(&mut backend, &mut s, &sample);
        assert!(!s.curve_info.as_ref().unwrap().zero_rpm_enabled);

        // Cooling to 38 °C (< 39 °C) re-arms fan stop.
        std::fs::write(hwmon.join("temp1_input"), "38000\n").unwrap();
        let sample = backend.sample().unwrap();
        check_sensor_control(&mut backend, &mut s, &sample);
        assert!(s.curve_info.as_ref().unwrap().zero_rpm_enabled);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1")).unwrap().trim(),
            "0"
        );

        // Hotspot reaching thermal safety limit (105 °C) disables Zero RPM and restores automatic.
        std::fs::write(hwmon.join("temp2_input"), "105000\n").unwrap();
        let sample = backend.sample().unwrap();
        assert!(sample.too_hot());
        assert!(!s.config.zero_rpm_requested(&sample, true).unwrap());
        s.error = Some(restore_after_failure(
            &mut backend,
            &mut s,
            "The GPU reached the app's temperature safety limit.",
            None,
        ));
        assert!(!s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "2"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sysfs_worker_recovers_and_reports_clear_error_when_zero_rpm_rejected_or_support_lost() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pci_dev = root.join("devices/pci0000:00/0000:03:00.0");
        std::fs::create_dir_all(&pci_dev).unwrap();
        std::fs::write(pci_dev.join("vendor"), "0x1002\n").unwrap();
        std::fs::write(
            pci_dev.join("uevent"),
            "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:03:00.0\n",
        )
        .unwrap();
        let card = root.join("class/drm/card0");
        std::fs::create_dir_all(&card).unwrap();
        std::os::unix::fs::symlink(&pci_dev, card.join("device")).unwrap();
        let hwmon = pci_dev.join("hwmon/hwmon0");
        std::fs::create_dir_all(&hwmon).unwrap();
        std::fs::write(hwmon.join("name"), "amdgpu\n").unwrap();
        std::fs::write(hwmon.join("temp1_input"), "38000\n").unwrap();
        std::fs::write(hwmon.join("pwm1_enable"), "2\n").unwrap();
        std::fs::write(hwmon.join("pwm1"), "80\n").unwrap();
        std::fs::write(hwmon.join("pwm1_min"), "0\n").unwrap();

        let mut backend = amd_adl::SysfsBackend::with_root(root);
        let adapters = backend.adapters().unwrap();
        let config_path = root.join("config.json");
        let mut config = Config {
            temperature_source: TemperatureSource::Gpu,
            zero_rpm_below_percent: Some(25),
            ..Config::default()
        };
        config.curve[0].speed = 20;
        let mut s = Snapshot {
            config,
            adapters,
            selected: None,
            curve_info: None,
            sample: None,
            sampled_at_ms: None,
            manual: false,
            error: None,
            ready: true,
            applied_speed: None,
            step_down: StepDown::default(),
        };
        select(&mut backend, &mut s).unwrap();
        s.sample = Some(backend.sample().unwrap());

        // Apply succeeds with Zero RPM active.
        handle(Action::Apply, &mut backend, &config_path, &mut s).unwrap();
        assert!(s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1")).unwrap().trim(),
            "0"
        );

        // Driver loses Zero RPM support (pwm1_min > 0) while custom control is active.
        std::fs::write(hwmon.join("pwm1_min"), "50\n").unwrap();
        let sample = backend.sample().unwrap();
        check_sensor_control(&mut backend, &mut s, &sample);
        assert!(!s.manual);
        assert_eq!(
            std::fs::read_to_string(hwmon.join("pwm1_enable"))
                .unwrap()
                .trim(),
            "2"
        );
        assert!(!s.curve_info.as_ref().unwrap().zero_rpm_supported);
        let err = s.error.as_deref().unwrap();
        assert!(
            err.contains("Allow fans to stop"),
            "unexpected error: {err}"
        );
        assert!(
            !err.contains("Driver / firmware sensor"),
            "host-PWM error should not suggest Firmware sensor: {err}"
        );
        assert!(
            err.contains("\n\nTechnical details: "),
            "expected technical details: {err}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sysfs_worker_reports_actionable_udev_message_on_readonly_and_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pci_dev = root.join("devices/pci0000:00/0000:03:00.0");
        std::fs::create_dir_all(&pci_dev).unwrap();
        std::fs::write(pci_dev.join("vendor"), "0x1002\n").unwrap();
        std::fs::write(
            pci_dev.join("uevent"),
            "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:03:00.0\n",
        )
        .unwrap();
        let card = root.join("class/drm/card0");
        std::fs::create_dir_all(&card).unwrap();
        std::os::unix::fs::symlink(&pci_dev, card.join("device")).unwrap();
        let hwmon = pci_dev.join("hwmon/hwmon0");
        std::fs::create_dir_all(&hwmon).unwrap();
        std::fs::write(hwmon.join("name"), "amdgpu\n").unwrap();
        std::fs::write(hwmon.join("temp1_input"), "50000\n").unwrap();
        std::fs::write(hwmon.join("pwm1_enable"), "2\n").unwrap();
        std::fs::write(hwmon.join("pwm1"), "80\n").unwrap();

        let mut backend = amd_adl::SysfsBackend::with_root(root);
        let adapters = backend.adapters().unwrap();
        let config_path = root.join("config.json");
        let mut s = Snapshot {
            config: Config::default(),
            adapters,
            selected: None,
            curve_info: None,
            sample: None,
            sampled_at_ms: None,
            manual: false,
            error: None,
            ready: true,
            applied_speed: None,
            step_down: StepDown::default(),
        };

        // Case 1: Adapter opened read-only (pwm1 not writable).
        std::fs::set_permissions(hwmon.join("pwm1"), std::fs::Permissions::from_mode(0o444))
            .unwrap();
        select(&mut backend, &mut s).unwrap();
        s.sample = Some(backend.sample().unwrap());
        let ro_err = handle(Action::Apply, &mut backend, &config_path, &mut s).unwrap_err();
        let ro_msg = ro_err.to_string();
        assert!(ro_msg.contains("read-only"), "expected read-only: {ro_msg}");
        assert!(ro_msg.contains("udev"), "expected udev guidance: {ro_msg}");
        assert!(ro_msg.contains("hwmon"), "expected hwmon mention: {ro_msg}");

        // Case 2: Permissions lost before write -> EACCES/PermissionDenied tucked under Technical details.
        std::fs::set_permissions(hwmon.join("pwm1"), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        select(&mut backend, &mut s).unwrap();
        handle(Action::Apply, &mut backend, &config_path, &mut s).unwrap();
        assert!(s.manual);

        std::fs::set_permissions(hwmon.join("pwm1"), std::fs::Permissions::from_mode(0o444))
            .unwrap();
        std::fs::set_permissions(
            hwmon.join("pwm1_enable"),
            std::fs::Permissions::from_mode(0o444),
        )
        .unwrap();
        std::fs::write(hwmon.join("temp1_input"), "65000\n").unwrap();
        let sample = backend.sample().unwrap();
        check_sensor_control(&mut backend, &mut s, &sample);

        std::fs::set_permissions(hwmon.join("pwm1"), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        std::fs::set_permissions(
            hwmon.join("pwm1_enable"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();

        let err = s.error.as_deref().unwrap();
        let (summary, details) = err
            .split_once("\n\nTechnical details: ")
            .expect("expected Technical details section");
        assert!(
            summary.contains("read-only") && summary.contains("udev") && summary.contains("hwmon"),
            "expected actionable summary: {summary}"
        );
        assert!(
            !summary.contains("os error"),
            "raw OS error must not leak into summary: {summary}"
        );
        assert!(
            details.contains("Permission denied") || details.contains("os error"),
            "expected raw OS error in details: {details}"
        );
    }
}
