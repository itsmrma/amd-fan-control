import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './style.css';

type Point = { temperature: number; speed: number };
type Config = { schema_version: number; adapter_id: string | null; curve: Point[]; polling_ms: number; start_in_tray: boolean };
type Adapter = { id: string; name: string; od_version: number };
type Sample = { edge_c: number | null; hotspot_c: number | null; rpm: number | null; speed_percent: number | null };
type Status = { config: Config; adapters: Adapter[]; selected: Adapter | null; curve_info: { supported: boolean; minimum: Point[]; maximum: Point[] } | null; sample: Sample | null; sampled_at_ms: number | null; manual: boolean; error: string | null; ready: boolean };
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const input = (id: string) => el<HTMLInputElement>(id);
let state: Status | null = null;
let draft: Config | null = null;
let dirty = false;
let pending = false;
let localError: string | null = null;
let dragging: number | null = null;
const chart = document.getElementById('chart') as unknown as SVGSVGElement;
const NS = 'http://www.w3.org/2000/svg';
const x = (t: number) => 48 + (t - 20) / 80 * 690;
const y = (s: number) => 210 - s / 100 * 180;

function svg(tag: string, attrs: Record<string, string | number>) {
  const node = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}

function draw() {
  if (!draft) return;
  chart.replaceChildren();
  for (const t of [20, 40, 60, 80, 100]) {
    chart.append(svg('line', { x1: x(t), x2: x(t), y1: 30, y2: 210, class: 'grid' }));
    const text = svg('text', { x: x(t), y: 233, 'text-anchor': 'middle' }); text.textContent = `${t} °C`; chart.append(text);
  }
  for (const v of [0, 25, 50, 75, 100]) {
    chart.append(svg('line', { x1: 48, x2: 738, y1: y(v), y2: y(v), class: 'grid' }));
    const text = svg('text', { x: 38, y: y(v) + 4, 'text-anchor': 'end' }); text.textContent = `${v}%`; chart.append(text);
  }
  const points = draft.curve;
  const line = [[20, points[0].speed], ...points.map(p => [p.temperature, p.speed]), [100, points[4].speed]];
  chart.append(svg('polyline', { points: line.map(([t, s]) => `${x(t)},${y(s)}`).join(' '), class: 'curve' }));
  points.forEach((p, i) => {
    chart.append(svg('circle', { cx: x(p.temperature), cy: y(p.speed), r: 6, class: 'point' }));
    const hit = svg('circle', { cx: x(p.temperature), cy: y(p.speed), r: 22, class: 'hit', 'data-point': i });
    chart.append(hit);
  });
}

function fields() {
  if (!draft) return;
  el('points').replaceChildren();
  draft.curve.forEach((p, i) => {
    const group = document.createElement('div');
    const label = document.createElement('label'); label.textContent = `Punto ${i + 1} · °C / %`;
    const box = document.createElement('div'); box.className = 'point-fields';
    for (const key of ['temperature', 'speed'] as const) {
      const field = document.createElement('input'); field.type = 'number'; field.value = String(p[key]);
      field.min = key === 'temperature' ? '25' : '25'; field.max = '100'; field.step = '1';
      field.setAttribute('aria-label', `Punto ${i + 1}, ${key === 'temperature' ? 'temperatura °C' : 'velocità %'}`);
      field.dataset.index = String(i); field.dataset.key = key; field.disabled = !!state?.manual;
      field.addEventListener('input', () => { p[key] = field.valueAsNumber; changed(); draw(); });
      box.append(field);
    }
    group.append(label, box); el('points').append(group);
  });
}

function changed() { dirty = true; localError = null; el('settings-feedback').textContent = 'Modifiche non salvate.'; controls(); }
function stale() { return !state?.sampled_at_ms || Date.now() - state.sampled_at_ms > Math.max(12000, state.config.polling_ms * 3); }
function controls() {
  for (const field of el('points').querySelectorAll('input')) field.disabled = pending || !!state?.manual;
  const writable = !!state?.ready && !!state.selected && !!state.curve_info?.supported;
  el<HTMLButtonElement>('save').disabled = pending || !draft || !!state?.manual || !dirty;
  el<HTMLButtonElement>('save-settings').disabled = pending || !draft || !!state?.manual || !dirty;
  el<HTMLButtonElement>('apply').disabled = pending || !writable || dirty || !!state?.manual || !state?.sample || stale();
  el<HTMLButtonElement>('automatic').disabled = pending || !writable;
  el<HTMLSelectElement>('adapter').disabled = pending || !!state?.manual;
  el<HTMLSelectElement>('polling').disabled = pending || !!state?.manual;
  input('start-tray').disabled = pending || !!state?.manual;
  el('mode').textContent = state?.manual ? 'Curva personalizzata attiva' : 'Monitoraggio';
  const error = localError ?? state?.error ?? (state?.sampled_at_ms && stale() ? 'Telemetria non aggiornata. Ripristina automatico e riavvia l’app.' : null);
  el('error').hidden = !error; el('error').textContent = error ?? '';
}

function update(s: Status) {
  state = s;
  if (!draft) {
    draft = structuredClone(s.config);
    const select = el<HTMLSelectElement>('adapter');
    select.replaceChildren(new Option('Seleziona GPU', ''));
    for (const a of s.adapters) select.append(new Option(`${a.name} · ${a.id}`, a.id));
    select.value = draft.adapter_id ?? s.selected?.id ?? '';
    el<HTMLSelectElement>('polling').value = String(draft.polling_ms);
    input('start-tray').checked = draft.start_in_tray;
    fields(); draw();
  } else if (el<HTMLSelectElement>('adapter').options.length === 1 && s.adapters.length) {
    for (const a of s.adapters) el<HTMLSelectElement>('adapter').append(new Option(`${a.name} · ${a.id}`, a.id));
    el<HTMLSelectElement>('adapter').value = draft.adapter_id ?? s.selected?.id ?? '';
  }
  el('gpu-name').textContent = s.selected ? `${s.selected.name} · Overdrive ${s.selected.od_version}` : s.ready ? 'Nessuna GPU selezionata' : 'Connessione alla GPU…';
  const sample = stale() ? null : s.sample;
  for (const [id, value, unit] of [['edge', sample?.edge_c, '°C'], ['hotspot', sample?.hotspot_c, '°C'], ['rpm', sample?.rpm, 'RPM'], ['percent', sample?.speed_percent, '%']] as const) {
    const small = document.createElement('small'); small.textContent = unit;
    el(id).replaceChildren(document.createTextNode(`${value ?? '—'} `), small);
  }
  if (!dirty && !pending) el('feedback').textContent = s.manual ? 'Curva verificata nel driver. Per modificare i punti, passa in automatico.' : s.curve_info?.supported ? 'Curva nativa disponibile. Salva le modifiche prima di applicarle.' : 'Controllo curva non disponibile con questo driver.';
  for (const field of el('points').querySelectorAll('input')) field.disabled = pending || s.manual;
  controls();
}

async function command(name: string, args?: Record<string, unknown>, success?: string) {
  pending = true; localError = null; controls();
  try {
    await invoke(name, args);
    if (success) el('feedback').textContent = success;
    update(await invoke<Status>('get_status'));
  } catch (e) { localError = String(e); }
  finally { pending = false; controls(); if (state) for (const field of el('points').querySelectorAll('input')) field.disabled = state.manual; }
}

async function saveDraft() {
  if (!draft) return;
  pending = true; controls();
  el('settings-feedback').textContent = 'Salvataggio…';
  try {
    await invoke('save_config', { config: draft });
    dirty = false; localError = null;
    update(await invoke<Status>('get_status'));
    el('feedback').textContent = 'Configurazione salvata. Puoi applicare la curva.';
    el('settings-feedback').textContent = 'Impostazioni salvate.';
  } catch (e) { localError = String(e); el('settings-feedback').textContent = `Salvataggio non riuscito: ${e}`; }
  finally { pending = false; controls(); }
}
el('save').addEventListener('click', saveDraft);
el('save-settings').addEventListener('click', saveDraft);
el('apply').addEventListener('click', () => command('apply_curve'));
el('automatic').addEventListener('click', () => command('restore_automatic'));
el('adapter').addEventListener('change', () => { if (draft) { draft.adapter_id = el<HTMLSelectElement>('adapter').value || null; changed(); } });
el('polling').addEventListener('change', () => { if (draft) { draft.polling_ms = Number(el<HTMLSelectElement>('polling').value); changed(); } });
input('start-tray').addEventListener('change', () => { if (draft) { draft.start_in_tray = input('start-tray').checked; changed(); } });
input('autostart').addEventListener('change', async () => {
  const checkbox = input('autostart'); const desired = checkbox.checked; checkbox.disabled = true;
  try { await invoke('set_autostart', { enabled: desired }); }
  catch (e) { checkbox.checked = !desired; localError = String(e); controls(); }
  finally { checkbox.disabled = false; }
});

chart.addEventListener('pointerdown', event => {
  if (pending || state?.manual || !draft) return;
  const target = event.target as SVGElement;
  const point = target.getAttribute('data-point');
  if (point !== null) { dragging = Number(point); chart.setPointerCapture(event.pointerId); }
});
chart.addEventListener('pointermove', event => {
  if (dragging === null || !draft) return;
  const p = new DOMPoint(event.clientX, event.clientY).matrixTransform(chart.getScreenCTM()!.inverse());
  const i = dragging; const curve = draft.curve;
  const min = state?.curve_info?.minimum[i]; const max = state?.curve_info?.maximum[i];
  curve[i].temperature = Math.max(curve[i-1]?.temperature + 1 || min?.temperature || 25,
    Math.min(curve[i+1]?.temperature - 1 || (i === 4 ? 85 : 100), Math.round(20 + (p.x - 48) / 690 * 80)));
  curve[i].temperature = Math.max(min?.temperature ?? 25, Math.min(max?.temperature ?? 100, curve[i].temperature));
  curve[i].speed = i === 4 ? 100 : Math.max(curve[i-1]?.speed ?? Math.max(25, min?.speed ?? 25),
    Math.min(curve[i+1]?.speed ?? 100, max?.speed ?? 100, Math.round((210 - p.y) / 180 * 100)));
  changed(); draw();
  for (const field of el('points').querySelectorAll<HTMLInputElement>('input')) {
    field.value = String(curve[Number(field.dataset.index)][field.dataset.key as keyof Point]);
  }
});
for (const name of ['pointerup', 'pointercancel', 'lostpointercapture']) chart.addEventListener(name, () => { dragging = null; });

async function start() {
  if (!isTauri()) {
    localError = 'Apri l’app desktop per collegarti alla GPU. La pagina nel browser non ha accesso all’hardware.';
    el('gpu-name').textContent = 'Connessione GPU disponibile nell’app desktop';
    controls();
    return;
  }
  try {
    await listen<Status>('gpu-status', event => update(event.payload));
    update(await invoke<Status>('get_status'));
    input('autostart').checked = await invoke<boolean>('autostart_enabled');
  } catch (e) { localError = `Connessione desktop non disponibile: ${e}`; controls(); }
}
void start();
setInterval(() => { if (state && stale()) update(state); }, 3000);
