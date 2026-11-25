import test from 'node:test';
import assert from 'node:assert/strict';
import { validateUpdaterManifest } from '../scripts/validate-updater-manifest.mjs';

function validManifest() {
  return {
    version: '0.1.0-alpha6',
    notes: 'Linux and Windows release',
    pub_date: '2026-11-25T18:00:00Z',
    platforms: {
      'windows-x86_64': {
        signature: 'dW50cnVzdGVkIGNvbW1lbnQ6IHdpbmRvd3MK',
        url: 'https://github.com/itsmrma/amd-fan-control/releases/download/v0.1.0-alpha6/AMD.Fan.Control_0.1.0-alpha6_x64-setup.exe',
      },
      'linux-x86_64': {
        signature: 'dW50cnVzdGVkIGNvbW1lbnQ6IGxpbnV4Cg==',
        url: 'https://github.com/itsmrma/amd-fan-control/releases/download/v0.1.0-alpha6/AMD.Fan.Control_0.1.0-alpha6_amd64.AppImage',
      },
    },
  };
}

test('accepts valid manifest with both windows-x86_64 and linux-x86_64 entries', () => {
  const manifest = validManifest();
  assert.doesNotThrow(() => validateUpdaterManifest(manifest, '0.1.0-alpha6'));
});

test('rejects manifest missing linux-x86_64 or windows-x86_64', () => {
  const missingLinux = validManifest();
  delete missingLinux.platforms['linux-x86_64'];
  assert.throws(() => validateUpdaterManifest(missingLinux), /linux-x86_64/);

  const missingWin = validManifest();
  delete missingWin.platforms['windows-x86_64'];
  assert.throws(() => validateUpdaterManifest(missingWin), /windows-x86_64/);
});

test('rejects empty signature or invalid platform asset URL', () => {
  const emptySig = validManifest();
  emptySig.platforms['linux-x86_64'].signature = '   ';
  assert.throws(() => validateUpdaterManifest(emptySig), /linux-x86_64.*signature/);

  const badUrl = validManifest();
  badUrl.platforms['linux-x86_64'].url = 'https://example.com/app.deb';
  assert.throws(() => validateUpdaterManifest(badUrl), /linux-x86_64.*url/);
});

test('rejects version mismatch when expectedVersion is provided', () => {
  assert.throws(() => validateUpdaterManifest(validManifest(), '0.1.0-alpha7'), /version/);
});
