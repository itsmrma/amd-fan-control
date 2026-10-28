import { test, expect } from '@playwright/test';

// Exercise the real frontend with isolated IPC. These tests never access the GPU.
test.beforeEach(async ({ page }) => {
  await page.route('**/node_modules/.vite/deps/@tauri-apps_api_core.js*', route => route.fulfill({
    contentType: 'text/javascript',
    body: 'export const isTauri=()=>true; export const invoke=(name,args)=>window.mockInvoke(name,args);',
  }));
  await page.route('**/node_modules/.vite/deps/@tauri-apps_api_event.js*', route => route.fulfill({
    contentType: 'text/javascript',
    body: 'export const listen=async(name,fn)=>{window.emitStatus=()=>fn({payload:structuredClone(window.gpu)});return ()=>{};};',
  }));
  await page.addInitScript(() => {
    const adapter = { id: 'pci:41:0:0', name: 'AMD Radeon RX 5600 XT', od_version: 8 };
    window.calls = [];
    window.gpu = {
      config: {
        schema_version: 1, adapter_id: adapter.id,
        curve: [[40, 20], [50, 40], [65, 60], [75, 80], [85, 100]].map(([temperature, speed]) => ({ temperature, speed })),
        polling_ms: 2500, start_in_tray: false, custom_control_enabled: false, zero_rpm_below_percent: 25,
      },
      adapters: [adapter], selected: adapter,
      curve_info: {
        supported: true, zero_rpm_supported: true, zero_rpm_enabled: false,
        minimum: Array(5).fill({ temperature: 20, speed: 20 }),
        maximum: Array(5).fill({ temperature: 100, speed: 100 }),
      },
      sample: { edge_c: 38, hotspot_c: 40, rpm: 900, speed_percent: 25 },
      sampled_at_ms: Date.now(), manual: false, error: null, ready: true,
    };
    const saved = localStorage.getItem('test-config');
    if (saved) {
      window.gpu.config = JSON.parse(saved);
      // Model the confirmed worker snapshot after startup restoration.
      window.gpu.manual = window.gpu.config.custom_control_enabled;
    }
    window.mockInvoke = async (name, args) => {
      window.calls.push(name);
      const s = window.gpu;
      s.sampled_at_ms = Date.now();
      if (name === 'get_status') return structuredClone(s);
      if (name === 'autostart_enabled') return false;
      if (window.commandDelay) await new Promise(resolve => setTimeout(resolve, window.commandDelay));
      if (name === 'save_config') s.config = { ...structuredClone(args.config), custom_control_enabled: s.config.custom_control_enabled };
      if (name === 'apply_curve') {
        if (window.failApply) { s.manual = false; throw window.failApply; }
        s.manual = true;
        s.config.custom_control_enabled = true;
      }
      if (name === 'restore_automatic') {
        s.config.custom_control_enabled = false;
        localStorage.setItem('test-config', JSON.stringify(s.config));
        if (window.failRestore) throw 'Could not restore driver defaults. Keep the app open.';
        s.manual = false;
        s.error = null;
      }
      localStorage.setItem('test-config', JSON.stringify(s.config));
    };
    setInterval(() => { window.gpu.sampled_at_ms = Date.now(); window.emitStatus?.(); }, 1000);
  });
  await page.goto('/');
  await expect(page.getByRole('switch')).toBeEnabled();
});

for (const colorScheme of ['light', 'dark']) {
  test(`desktop fits the viewport in ${colorScheme} appearance`, async ({ page }) => {
    await page.emulateMedia({ colorScheme });
    for (const [width, height] of [[1920, 1080], [1920, 1000], [1536, 824], [1366, 768], [1280, 720], [1200, 790]]) {
      await page.setViewportSize({ width, height });
      await expect.poll(() => page.evaluate(() => ({
        x: document.documentElement.scrollWidth - innerWidth,
        y: document.documentElement.scrollHeight - innerHeight,
      })), { message: `viewport ${width} × ${height}` }).toEqual({ x: 0, y: 0 });
      await expect(page.getByRole('button', { name: 'Use driver defaults' })).toBeInViewport({ ratio: 1 });
      await expect(page.locator('footer')).toBeInViewport({ ratio: 1 });
    }
  });
}

test('small windows keep settings reachable without horizontal overflow', async ({ page }) => {
  for (const width of [1199, 920, 680, 390]) {
    await page.setViewportSize({ width, height: 650 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
    await page.getByRole('button', { name: 'Save all changes' }).scrollIntoViewIfNeeded();
    await expect(page.getByRole('button', { name: 'Save all changes' })).toBeInViewport();
  }
});

test('sensor choice updates the chart, persists, and locks while active', async ({ page }) => {
  const sensor = page.getByRole('combobox', { name: 'Curve temperature' });
  await expect(sensor).toHaveValue('gpu');
  await sensor.selectOption('hotspot');
  await expect(page.locator('#temperature-axis')).toHaveText('Hotspot temperature (°C)');
  await expect(page.locator('#zero-feedback')).toContainText('hotspot temperature');
  await expect(page.getByRole('switch')).toBeDisabled();
  await page.getByRole('button', { name: 'Save all changes' }).click();
  expect(await page.evaluate(() => window.gpu.config.temperature_source)).toBe('hotspot');
  await page.getByRole('switch').check();
  await expect(sensor).toBeDisabled();
  await page.getByRole('switch').uncheck();
  await expect(sensor).toBeEnabled();
  await expect(sensor).toHaveValue('hotspot');
});

for (const colorScheme of ['light', 'dark']) {
test(`curve fields stay uniform and readable while resizing in ${colorScheme}`, async ({ page }) => {
  await page.emulateMedia({ colorScheme });
  for (const width of [1920, 1366, 1120, 1024, 920, 900, 899, 760, 701, 700, 680, 520, 390]) {
    await page.setViewportSize({ width, height: 790 });
    const sizes = await page.locator('.points input').evaluateAll(fields => fields.map(field => {
      const { width, height } = field.getBoundingClientRect();
      return { width, height };
    }));
    expect(Math.min(...sizes.map(s => s.width)), `field width at ${width}px`).toBeGreaterThanOrEqual(60);
    expect(Math.max(...sizes.map(s => s.height)) - Math.min(...sizes.map(s => s.height)), `field heights at ${width}px`).toBeLessThan(1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth), `page overflow at ${width}px`).toBe(width);
    if ([1366, 920, 390].includes(width)) await page.screenshot({ path: test.info().outputPath(`layout-${width}.png`), fullPage: true });
  }
});
}

test('missing selected sensor blocks activation but keeps restoration available', async ({ page }) => {
  await page.getByRole('combobox', { name: 'Curve temperature' }).selectOption('hotspot');
  await page.getByRole('button', { name: 'Save all changes' }).click();
  await page.evaluate(() => { window.gpu.sample.hotspot_c = null; window.emitStatus(); });
  await expect(page.getByRole('switch')).toBeDisabled();
  await expect(page.locator('#control-help')).toContainText('Selected temperature sensor unavailable');
  await expect(page.getByRole('button', { name: 'Use driver defaults' })).toBeEnabled();
});

test('chart dragging and keyboard editing still work after reflow', async ({ page }) => {
  for (const width of [1366, 920, 390]) {
    await page.setViewportSize({ width, height: 790 });
    const field = page.getByRole('spinbutton', { name: 'Point 2, temperature in degrees Celsius', exact: true });
    await field.fill('50');
    await page.getByRole('spinbutton', { name: 'Point 2, fan speed in percent', exact: true }).fill('40');
    const point = page.locator('#hit-1');
    await page.locator('#chart').scrollIntoViewIfNeeded();
    const { x, y, width: w, height: h } = await point.boundingBox();
    await page.mouse.move(x + w / 2, y + h / 2);
    await page.mouse.down();
    await page.mouse.move(x + w / 2 + 16, y + h / 2 - 8, { steps: 5 });
    await page.mouse.up();
    expect(Number(await field.inputValue())).toBeGreaterThan(50);
    const previous = Number(await field.inputValue());
    await point.focus();
    await page.keyboard.press('ArrowLeft');
    await expect(field).toHaveValue(String(previous - 1));
    await expect(page.locator('body')).not.toHaveClass('dragging');
  }
});

test('save, switch on, switch off and retain the saved curve', async ({ page }) => {
  const control = page.getByRole('switch');
  await page.getByRole('spinbutton', { name: 'Point 2, fan speed in percent', exact: true }).fill('45');
  await expect(control).toBeDisabled();
  await page.getByRole('button', { name: 'Save curve & settings' }).click();
  await expect(control).toBeEnabled();
  expect(await page.evaluate(() => window.calls.includes('apply_curve'))).toBe(false);
  await control.check();
  await expect(control).toBeChecked();
  await expect(page.locator('#mode')).toHaveText('Custom curve active');
  await expect(page.getByRole('spinbutton', { name: 'Point 2, fan speed in percent', exact: true })).toBeDisabled();
  await control.uncheck();
  await expect(control).not.toBeChecked();
  await expect(control).toBeEnabled();
  await expect(page.getByRole('spinbutton', { name: 'Point 2, fan speed in percent', exact: true })).toHaveValue('45');
  expect(await page.evaluate(() => window.calls.filter(c => c !== 'get_status' && c !== 'autostart_enabled')))
    .toEqual(['save_config', 'apply_curve', 'restore_automatic']);
});

test('restart displays the confirmed saved custom-control choice', async ({ page }) => {
  const control = page.getByRole('switch');
  await control.check();
  await expect(control).toBeChecked();
  await expect(page.locator('#control-help')).toContainText('will resume when the app starts');
  await page.reload();
  await expect(control).toBeChecked();
  await expect(page.locator('#mode')).toHaveText('Custom curve active');
  expect(await page.evaluate(() => window.calls.includes('apply_curve'))).toBe(false);
  await control.uncheck();
  await page.reload();
  await expect(control).not.toBeChecked();
  await expect(page.locator('#mode')).toHaveText('Custom control off');
});

test('failed restore keeps custom control on and recovery remains available without telemetry', async ({ page }) => {
  const control = page.getByRole('switch');
  await control.check();
  await expect(control).toBeEnabled();
  await page.evaluate(() => {
    window.failRestore = true;
    window.gpu.sample = null;
    window.gpu.sampled_at_ms = null;
    window.gpu.curve_info.supported = false;
    window.emitStatus();
  });
  await expect(control).toBeEnabled();
  await control.click();
  await expect(control).toBeChecked();
  await expect(page.getByRole('alert')).toContainText('Could not restore driver defaults');
  await expect(page.getByRole('button', { name: 'Use driver defaults' })).toBeEnabled();
  await page.evaluate(() => { window.failRestore = false; });
  await page.getByRole('button', { name: 'Use driver defaults' }).click();
  await expect(control).not.toBeChecked();
  await expect(page.locator('#feedback')).toContainText('Driver default fan settings restored');
});

test('failed apply refreshes the confirmed mode and separates technical details', async ({ page }) => {
  const control = page.getByRole('switch');
  await page.evaluate(() => { window.failApply = 'The GPU driver does not support this Zero RPM change. Driver default fan settings restored.\n\nTechnical details: Zero RPM write (ADL -8)'; });
  await control.click();
  await expect(control).toBeEnabled();
  await expect(control).not.toBeChecked();
  await expect(page.locator('#error-message')).toContainText('Driver default fan settings restored');
  await expect(page.locator('#error-message')).not.toContainText('ADL -8');
  await expect(page.locator('#error-detail-text')).not.toBeVisible();
  await page.getByText('Technical details', { exact: true }).click();
  await expect(page.locator('#error-detail-text')).toHaveText('Zero RPM write (ADL -8)');
});

test('the legend explains the cutoff and buttons expose hover help', async ({ page }) => {
  await expect(page.locator('#zero-legend')).toContainText('fans may stop below 42 °C');
  await expect(page.locator('#zero-legend')).toContainText('Firmware decides');
  for (const button of await page.getByRole('button').all()) {
    // SVG point handles use role=button and expose their keyboard instructions in aria-label.
    if (await button.evaluate(e => e.tagName.toLowerCase() === 'button')) {
      expect((await button.getAttribute('title'))?.length).toBeGreaterThan(30);
    }
  }
  await page.getByRole('checkbox', { name: 'Allow fans to stop' }).uncheck();
  await expect(page.locator('#zero-legend')).toBeHidden();
});
