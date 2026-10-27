import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './style.css';

type Point = { temperature: number; speed: number };
type Config = { schema_version: number; adapter_id: string | null; curve: Point[]; polling_ms: number; temperature_source: 'gpu' | 'hotspot' | 'firmware'; start_in_tray: boolean; zero_rpm_below_percent: number | null };
type CurveInfo = { supported: boolean; zero_rpm_supported: boolean; zero_rpm_enabled: boolean; minimum: Point[]; maximum: Point[] };
type Adapter = { id: string; name: string; od_version: number };
type Sample = { edge_c: number | null; hotspot_c: number | null; rpm: number | null; speed_percent: number | null };
type Status = { config: Config; adapters: Adapter[]; selected: Adapter | null; curve_info: CurveInfo | null; sample: Sample | null; sampled_at_ms: number | null; manual: boolean; error: string | null; ready: boolean };
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const input = (id: string) => el<HTMLInputElement>(id);
const chart = document.getElementById('chart') as unknown as SVGSVGElement;
const NS = 'http://www.w3.org/2000/svg';
let state: Status | null = null;
let draft: Config | null = null;
let dirty = false;
let pending = false;
let localError: string | null = null;
let dragging: { index: number; pointerId: number; dx: number; dy: number } | null = null;
let chartWidth = 760;
let chartHeight = 300;
const left = 48, top = 24, right = 24, bottom = 34;
const plotWidth = () => chartWidth - left - right;
const plotHeight = () => chartHeight - top - bottom;
const x = (t: number) => left + (t - 20) / 80 * plotWidth();
const y = (s: number) => top + (100 - s) / 100 * plotHeight();
const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

function svg(tag: string, attrs: Record<string, string | number>) {
  const node = document.createElementNS(NS, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
  return node;
}

function zeroStop(): number | null {
  if (!draft || draft.zero_rpm_below_percent == null) return null;
  const threshold = draft.zero_rpm_below_percent;
  if (!Number.isInteger(threshold) || threshold < 21 || threshold > 60 || draft.curve[0].speed >= threshold) return null;
  for (let i = 1; i < draft.curve.length; i++) {
    const a = draft.curve[i - 1], b = draft.curve[i];
    if (b.speed >= threshold) {
      const temperature = Math.floor(a.temperature + (threshold - a.speed) * (b.temperature - a.temperature) / (b.speed - a.speed));
      return Number.isFinite(temperature) && temperature <= 60 ? temperature : null;
    }
  }
  return null;
}

function draw() {
  chart.replaceChildren();
  chart.setAttribute('viewBox', `0 0 ${chartWidth} ${chartHeight}`);
  for (const t of [20, 40, 60, 80, 100]) {
    chart.append(svg('line', { x1: x(t), x2: x(t), y1: top, y2: y(0), class: 'grid' }));
    const text = svg('text', { x: x(t), y: y(0) + 24, 'text-anchor': 'middle' }); text.textContent = `${t} °C`; chart.append(text);
  }
  for (const speed of [0, 25, 50, 75, 100]) {
    chart.append(svg('line', { x1: left, x2: x(100), y1: y(speed), y2: y(speed), class: 'grid' }));
    const text = svg('text', { x: left - 10, y: y(speed) + 4, 'text-anchor': 'end' }); text.textContent = `${speed}%`; chart.append(text);
  }
  chart.append(svg('g', { id: 'zero-region' }), svg('polyline', { id: 'curve-line', class: 'curve' }));
  draft?.curve.forEach((_, index) => {
    chart.append(svg('circle', { id: `point-${index}`, r: 6, class: 'point' }));
    const hit = svg('circle', { id: `hit-${index}`, r: 24, class: 'hit', 'data-point': index, tabindex: 0, role: 'button' });
    hit.addEventListener('keydown', event => {
      if (!(event instanceof KeyboardEvent) || !draft || pending || state?.manual) return;
      const p = draft.curve[index];
      const step = event.shiftKey ? 5 : 1;
      const keys: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
      const delta = keys[event.key];
      if (!delta) return;
      event.preventDefault();
      movePoint(index, p.temperature + delta[0], p.speed + delta[1]);
    });
    chart.append(hit);
  });
  chart.append(svg('text', { id: 'point-caption', class: 'point-caption', 'text-anchor': 'middle' }));
  renderCurve();
}

function renderCurve() {
  if (!draft) return;
  const valid = draft.curve.every(p => Number.isFinite(p.temperature) && Number.isFinite(p.speed));
  const line = chart.querySelector('#curve-line')!;
  const points = [[20, draft.curve[0].speed], ...draft.curve.map(p => [p.temperature, p.speed]), [100, draft.curve[4].speed]];
  line.setAttribute('points', valid ? points.map(([t, s]) => `${x(t)},${y(s)}`).join(' ') : '');
  draft.curve.forEach((p, index) => {
    for (const prefix of ['point', 'hit']) {
      const node = chart.querySelector(`#${prefix}-${index}`)!;
      node.setAttribute('cx', String(x(Number.isFinite(p.temperature) ? p.temperature : 20)));
      node.setAttribute('cy', String(y(Number.isFinite(p.speed) ? p.speed : 20)));
    }
    const hit = chart.querySelector(`#hit-${index}`)!;
    hit.setAttribute('aria-label', `Point ${index + 1}: ${p.temperature} °C, ${p.speed}% fan speed. Left/right: temperature. Up/down: fan speed.`);
    hit.setAttribute('aria-disabled', String(pending || !!state?.manual));
    chart.querySelector(`#point-${index}`)!.classList.toggle('active-point', dragging?.index === index);
  });
  const region = chart.querySelector('#zero-region')!; region.replaceChildren();
  const stop = zeroStop();
  if (stop !== null && valid) {
    region.append(svg('rect', { x: left, y: top, width: Math.max(0, x(stop) - left), height: plotHeight(), class: 'zero-zone' }));
    region.append(svg('line', { x1: x(stop), x2: x(stop), y1: top, y2: y(0), class: 'zero-line' }));
  }
  el('zero-legend').hidden = stop === null || !valid;
  el('zero-legend-text').textContent = `Dashed line: fans may stop below ${stop} °C (shaded). Firmware decides; custom control only.`;
  const caption = chart.querySelector('#point-caption')!;
  caption.textContent = '';
  if (dragging) {
    const p = draft.curve[dragging.index];
    caption.setAttribute('x', String(clamp(x(p.temperature), left + 65, x(100) - 65)));
    caption.setAttribute('y', String(Math.max(16, y(p.speed) - 18)));
    caption.textContent = `${p.temperature} °C · ${p.speed}% fan`;
  }
}

function fields() {
  if (!draft) return;
  el('points').replaceChildren();
  draft.curve.forEach((p, index) => {
    const group = document.createElement('fieldset'); group.className = 'point-group';
    const legend = document.createElement('legend'); legend.textContent = `Point ${index + 1}`;
    const box = document.createElement('div'); box.className = 'point-fields';
    for (const key of ['temperature', 'speed'] as const) {
      const label = document.createElement('label'); label.textContent = key === 'temperature' ? 'Temp (°C)' : 'Fan (%)';
      const field = document.createElement('input'); field.type = 'number'; field.value = String(p[key]);
      const limits = state?.curve_info;
      field.min = String(key === 'speed' && index === 4 ? 100 : Math.max(key === 'temperature' ? 20 : 20, limits?.minimum[index][key] ?? 20));
      field.max = String(key === 'temperature' && index === 4 ? Math.min(85, limits?.maximum[index][key] ?? 85) : limits?.maximum[index][key] ?? 100);
      field.step = '1'; field.setAttribute('aria-label', `Point ${index + 1}, ${key === 'temperature' ? 'temperature in degrees Celsius' : 'fan speed in percent'}`);
      field.dataset.index = String(index); field.dataset.key = key;
      field.addEventListener('input', () => { p[key] = field.valueAsNumber; changed(); renderCurve(); });
      label.append(field); box.append(label);
    }
    group.append(legend, box); el('points').append(group);
  });
}

function zeroFeedback() {
  const available = !!state?.curve_info?.zero_rpm_supported;
  const active = !!state?.curve_info?.zero_rpm_enabled;
  const stop = zeroStop();
  el('zero-feedback').textContent = !state?.ready ? 'Waiting for GPU capabilities.'
    : !available ? 'Native Zero RPM is unavailable with this GPU/driver.'
    : draft?.zero_rpm_below_percent == null ? 'Disabled for custom control.'
    : stop === null ? 'Set a curve point below the threshold. The cutoff must be at or below 60 °C.'
    : `Fans may stop below ${stop} °C; re-arms below ${stop - 3} °C using ${draft?.temperature_source === 'gpu' ? 'GPU temperature' : draft?.temperature_source === 'hotspot' ? 'hotspot temperature' : 'the hotter sensor'}.${state?.manual ? active ? ' Stop requested.' : ' Stop not requested.' : ''}`;
}
function sensorFeedback() {
  const source = draft?.temperature_source ?? 'gpu';
  const label = source === 'gpu' ? 'GPU temperature' : source === 'hotspot' ? 'Hotspot temperature' : 'Firmware temperature';
  el('temperature-axis').textContent = `${label} (°C)`;
  chart.setAttribute('aria-label', `Fan curve: ${label.toLowerCase()} in degrees Celsius and fan speed in percent`);
  el('sensor-help').textContent = source === 'firmware'
    ? 'The driver chooses the curve sensor. Zero RPM uses the hotter reading.'
    : `Uses ${source === 'gpu' ? 'GPU' : 'hotspot'} temperature. Updates every second; both thermal safeguards stay active.`;
  el('temperature-source').title = el('sensor-help').textContent;
}
function changed() {
  dirty = true; localError = null;
  el('settings-feedback').textContent = 'Unsaved changes.';
  el('feedback').textContent = 'Unsaved changes. Save before turning custom control on.';
  controls();
}
function stale() { return !state?.sampled_at_ms || Date.now() - state.sampled_at_ms > Math.max(12000, state.config.polling_ms * 3); }
function controls() {
  const locked = pending || !!state?.manual || !state?.ready;
  for (const field of el('points').querySelectorAll('input')) field.disabled = locked;
  const writable = !!state?.ready && !!state.selected && !!state.curve_info?.supported;
  const source = draft?.temperature_source ?? 'gpu';
  const sensorAvailable = source === 'firmware' || (source === 'gpu' ? state?.sample?.edge_c : state?.sample?.hotspot_c) != null;
  el<HTMLButtonElement>('save').disabled = locked || !draft || !dirty;
  el<HTMLButtonElement>('save-settings').disabled = locked || !draft || !dirty;
  input('custom-control').checked = !!state?.manual;
  // Turning off remains available even with missing telemetry or changed capabilities.
  input('custom-control').disabled = pending || (state?.manual ? !state.ready || !state.selected : !writable || dirty || !state?.sample || stale() || !sensorAvailable);
  el<HTMLButtonElement>('automatic').disabled = pending || !state?.ready || !state.selected;
  el<HTMLSelectElement>('adapter').disabled = locked;
  el<HTMLSelectElement>('polling').disabled = locked;
  el<HTMLSelectElement>('temperature-source').disabled = locked;
  input('start-tray').disabled = locked;
  // An enabled option must remain removable even if the current driver loses support.
  input('zero-rpm').disabled = locked || (!state?.curve_info?.zero_rpm_supported && !input('zero-rpm').checked);
  input('zero-threshold').disabled = locked || !input('zero-rpm').checked;
  for (const hit of chart.querySelectorAll('.hit')) hit.setAttribute('aria-disabled', String(locked));
  el('mode').textContent = state?.manual ? 'Custom curve active' : state?.ready ? 'Custom control off' : 'Connecting';
  el('control-help').textContent = pending ? 'Updating fan control…' : state?.manual
    ? 'Your saved curve controls the fans. Turn off to restore driver defaults.'
    : state?.sample && !sensorAvailable ? 'Selected temperature sensor unavailable. Choose another curve temperature.'
    : dirty ? 'Save your changes before turning on. Turn off restores driver defaults.'
    : 'On: saved curve · Off: driver defaults.';
  const error = localError ?? state?.error ?? (state?.sampled_at_ms && stale() ? 'GPU readings are out of date. Use driver defaults, then restart the app.' : null);
  const [message, details] = (error ?? '').split('\n\nTechnical details: ');
  el('error').hidden = !error; el('error-message').textContent = message;
  el('error-details').hidden = !details; el('error-detail-text').textContent = details ?? '';
  zeroFeedback();
  sensorFeedback();
}

function update(s: Status) {
  state = s;
  if (!draft) {
    draft = structuredClone(s.config);
    draft.zero_rpm_below_percent ??= null;
    draft.temperature_source ??= 'gpu';
    el<HTMLSelectElement>('temperature-source').value = draft.temperature_source;
    el<HTMLSelectElement>('polling').value = String(draft.polling_ms);
    input('start-tray').checked = draft.start_in_tray;
    input('zero-rpm').checked = draft.zero_rpm_below_percent !== null;
    input('zero-threshold').value = String(draft.zero_rpm_below_percent ?? 25);
    fields(); draw();
  }
  const select = el<HTMLSelectElement>('adapter');
  const ids = s.adapters.map(a => a.id);
  if (select.options.length !== ids.length + 1 || ids.some((id, index) => select.options[index + 1]?.value !== id)) {
    select.replaceChildren(new Option('Select GPU', ''));
    for (const adapter of s.adapters) select.append(new Option(`${adapter.name} · ${adapter.id}`, adapter.id));
  }
  select.value = draft.adapter_id ?? s.selected?.id ?? '';
  el('gpu-name').textContent = s.selected ? `${s.selected.name} · Overdrive ${s.selected.od_version}` : s.ready ? 'No GPU selected' : 'Connecting to GPU…';
  const sample = stale() ? null : s.sample;
  for (const [id, value, unit] of [['edge', sample?.edge_c, '°C'], ['hotspot', sample?.hotspot_c, '°C'], ['rpm', sample?.rpm, 'RPM'], ['percent', sample?.speed_percent, '%']] as const) {
    const small = document.createElement('small'); small.textContent = unit;
    el(id).replaceChildren(document.createTextNode(`${value ?? '—'} `), small);
  }
  if (!dirty && !pending) el('feedback').textContent = s.manual ? 'Curve verified in the driver. Turn custom control off before editing.' : s.curve_info?.supported ? 'Custom control is off. Your saved curve is ready to use.' : 'Fan curve control is unavailable with this driver.';
  controls();
}

async function command(name: string, args?: Record<string, unknown>) {
  pending = true; localError = null; controls();
  try {
    await invoke(name, args);
    update(await invoke<Status>('get_status'));
  } catch (e) {
    localError = String(e);
    // The command may have partially succeeded or restored defaults after a failure.
    try { update(await invoke<Status>('get_status')); } catch { /* Keep the last confirmed state. */ }
  }
  finally { pending = false; if (state) update(state); else controls(); }
}
async function saveDraft() {
  if (!draft) return;
  pending = true; localError = null; controls();
  el('settings-feedback').textContent = 'Saving…';
  try {
    await invoke('save_config', { config: draft });
    const s = await invoke<Status>('get_status');
    dirty = false; draft = null; update(s);
    el('feedback').textContent = 'Curve and settings saved. Turn custom control on to use the curve.';
    el('settings-feedback').textContent = 'Settings saved.';
  } catch (e) { localError = String(e); el('settings-feedback').textContent = `Could not save: ${e}`; }
  finally { pending = false; controls(); }
}
el('save').addEventListener('click', saveDraft);
el('save-settings').addEventListener('click', saveDraft);
input('custom-control').addEventListener('change', () => {
  const enabled = input('custom-control').checked;
  void command(enabled ? 'apply_curve' : 'restore_automatic');
});
el('automatic').addEventListener('click', async () => {
  await command('restore_automatic');
  if (!localError && !state?.error) el('feedback').textContent = 'Driver default fan settings restored. Your saved custom curve is kept.';
});
el('adapter').addEventListener('change', () => { if (draft) { draft.adapter_id = el<HTMLSelectElement>('adapter').value || null; changed(); } });
el('polling').addEventListener('change', () => { if (draft) { draft.polling_ms = Number(el<HTMLSelectElement>('polling').value); changed(); } });
el('temperature-source').addEventListener('change', () => {
  if (!draft) return;
  draft.temperature_source = el<HTMLSelectElement>('temperature-source').value as Config['temperature_source'];
  changed();
});
input('start-tray').addEventListener('change', () => { if (draft) { draft.start_in_tray = input('start-tray').checked; changed(); } });
input('zero-rpm').addEventListener('change', () => {
  if (!draft) return;
  draft.zero_rpm_below_percent = input('zero-rpm').checked ? input('zero-threshold').valueAsNumber : null;
  changed(); renderCurve();
});
input('zero-threshold').addEventListener('input', () => {
  if (!draft) return;
  draft.zero_rpm_below_percent = input('zero-threshold').valueAsNumber;
  changed(); renderCurve();
});
input('autostart').addEventListener('change', async () => {
  const checkbox = input('autostart'); const desired = checkbox.checked; checkbox.disabled = true;
  try { await invoke('set_autostart', { enabled: desired }); }
  catch (e) { checkbox.checked = !desired; localError = String(e); controls(); }
  finally { checkbox.disabled = false; }
});

function movePoint(index: number, temperature: number, speed: number) {
  if (!draft) return;
  const curve = draft.curve, min = state?.curve_info?.minimum[index], max = state?.curve_info?.maximum[index];
  const loT = Math.max(20, min?.temperature ?? 20, index > 0 ? curve[index - 1].temperature + 1 : 20);
  const hiT = Math.min(index === 4 ? 85 : 100, max?.temperature ?? 100, index < 4 ? curve[index + 1].temperature - 1 : 100);
  const loS = Math.max(20, min?.speed ?? 20, index > 0 ? curve[index - 1].speed : 20);
  const hiS = Math.min(max?.speed ?? 100, index < 4 ? curve[index + 1].speed : 100);
  if (loT > hiT || loS > hiS || !Number.isFinite(loT + hiT + loS + hiS)) return;
  const t = clamp(Math.round(temperature), loT, hiT), s = index === 4 ? 100 : clamp(Math.round(speed), loS, hiS);
  if (curve[index].temperature === t && curve[index].speed === s) return;
  curve[index].temperature = t; curve[index].speed = s;
  for (const field of el('points').querySelectorAll<HTMLInputElement>('input')) {
    field.value = String(curve[Number(field.dataset.index)][field.dataset.key as keyof Point]);
  }
  changed(); renderCurve();
}
function chartPosition(event: PointerEvent) {
  const matrix = chart.getScreenCTM();
  return matrix ? new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse()) : null;
}
chart.addEventListener('pointerdown', event => {
  if (pending || !state?.ready || state.manual || !draft || !event.isPrimary || event.button !== 0) return;
  const point = (event.target as SVGElement).getAttribute('data-point');
  const position = chartPosition(event);
  if (point === null || !position) return;
  event.preventDefault(); window.getSelection()?.removeAllRanges();
  const index = Number(point), p = draft.curve[index];
  dragging = { index, pointerId: event.pointerId, dx: position.x - x(p.temperature), dy: position.y - y(p.speed) };
  chart.setPointerCapture(event.pointerId); document.body.classList.add('dragging'); renderCurve();
});
chart.addEventListener('pointermove', event => {
  if (!dragging || event.pointerId !== dragging.pointerId || !draft) return;
  if (pending || state?.manual) { endDrag(); return; }
  event.preventDefault();
  const position = chartPosition(event); if (!position) return;
  movePoint(dragging.index, 20 + (position.x - dragging.dx - left) / plotWidth() * 80,
    100 - (position.y - dragging.dy - top) / plotHeight() * 100);
});
function endDrag() {
  if (!dragging) return;
  const id = dragging.pointerId; dragging = null;
  if (chart.hasPointerCapture(id)) chart.releasePointerCapture(id);
  document.body.classList.remove('dragging'); window.getSelection()?.removeAllRanges(); renderCurve();
}
for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) chart.addEventListener(name, endDrag);
window.addEventListener('blur', endDrag);
new ResizeObserver(entries => {
  const { width, height } = entries[0].contentRect;
  if (width > 0 && height > 0) { endDrag(); chartWidth = width; chartHeight = height; draw(); }
}).observe(chart.parentElement!);

async function start() {
  if (!isTauri()) {
    localError = 'Open the desktop app to connect to your GPU. The browser preview has no hardware access.';
    el('gpu-name').textContent = 'GPU connection available in the desktop app'; controls(); return;
  }
  try {
    await listen<Status>('gpu-status', event => update(event.payload));
    update(await invoke<Status>('get_status'));
    input('autostart').checked = await invoke<boolean>('autostart_enabled');
    input('autostart').disabled = false;
  } catch (e) { localError = `Desktop connection unavailable: ${e}`; controls(); }
}
void start();
setInterval(() => { if (state && stale()) update(state); }, 3000);
