// Regenerates the README screenshots from the real frontend with mocked IPC,
// the same isolation the Playwright suite uses. No GPU access.
//
//   npm run dev -- --port 1421        (in another terminal, or let CI start it)
//   node scripts/capture-screenshots.mjs
//
// Writes assets/screenshot.png (dark), assets/screenshot-light.png and
// assets/screenshot-backup.png (Settings close-up with the Backup section).
import { chromium } from '@playwright/test';

const BASE_URL = process.env.SCREENSHOT_URL ?? 'http://127.0.0.1:1421';

const browser = await chromium.launch();
try {
  for (const colorScheme of ['dark', 'light']) {
    const context = await browser.newContext({
      colorScheme,
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2,
    });
    const page = await context.newPage();
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
      window.gpu = {
        config: {
          schema_version: 1, adapter_id: adapter.id,
          curve: [[40, 20], [50, 40], [65, 60], [75, 80], [85, 100]].map(([temperature, speed]) => ({ temperature, speed })),
          polling_ms: 2500, temperature_source: 'gpu', start_in_tray: false,
          custom_control_enabled: false, zero_rpm_below_percent: 25,
        },
        adapters: [adapter], selected: adapter,
        curve_info: {
          supported: true, zero_rpm_supported: true, zero_rpm_enabled: false,
          minimum: Array(5).fill({ temperature: 20, speed: 20 }),
          maximum: Array(5).fill({ temperature: 100, speed: 100 }),
        },
        sample: { edge_c: 46, hotspot_c: 54, rpm: 1180, speed_percent: 34 },
        sampled_at_ms: Date.now(), manual: false, error: null, ready: true,
      };
      window.mockInvoke = async name => {
        window.gpu.sampled_at_ms = Date.now();
        if (name === 'get_status') return structuredClone(window.gpu);
        if (name === 'autostart_enabled') return false;
      };
      setInterval(() => { window.gpu.sampled_at_ms = Date.now(); window.emitStatus?.(); }, 1000);
    });
    await page.goto(BASE_URL);
    await page.getByRole('switch').waitFor({ state: 'attached' });
    await page.waitForFunction(() => document.getElementById('gpu-name')?.textContent?.includes('RX 5600 XT'));
    // Let layout and fonts settle before capturing.
    await page.waitForTimeout(600);
    if (colorScheme === 'dark') {
      await page.screenshot({ path: 'assets/screenshot.png' });
      await page.locator('.settings-panel').screenshot({ path: 'assets/screenshot-backup.png' });
    } else {
      await page.screenshot({ path: 'assets/screenshot-light.png' });
    }
    await context.close();
  }
} finally {
  await browser.close();
}
console.log('Screenshots written to assets/.');
