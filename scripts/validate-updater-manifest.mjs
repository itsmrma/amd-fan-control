const PLATFORM_SUFFIXES = {
  'windows-x86_64': '-setup.exe',
  'linux-x86_64': '.AppImage',
};

/**
 * Validate a Tauri updater `latest.json` manifest for both Windows and Linux AppImage targets.
 */
export function validateUpdaterManifest(manifest, expectedVersion) {
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) {
    throw new Error('Updater manifest must be a JSON object');
  }
  if (typeof manifest.version !== 'string' || !manifest.version.trim()) {
    throw new Error('Updater manifest version must be a non-empty string');
  }
  if (expectedVersion && manifest.version !== expectedVersion) {
    throw new Error(
      `Updater manifest version ${manifest.version} does not match expected ${expectedVersion}`,
    );
  }
  if (typeof manifest.notes !== 'string') {
    throw new Error('Updater manifest notes must be a string');
  }
  if (typeof manifest.pub_date !== 'string' || Number.isNaN(Date.parse(manifest.pub_date))) {
    throw new Error('Updater manifest pub_date must be a valid ISO-8601 timestamp');
  }
  if (!manifest.platforms || typeof manifest.platforms !== 'object' || Array.isArray(manifest.platforms)) {
    throw new Error('Updater manifest platforms must be an object');
  }

  for (const [platform, suffix] of Object.entries(PLATFORM_SUFFIXES)) {
    const entry = manifest.platforms[platform];
    if (!entry || typeof entry !== 'object') {
      throw new Error(`Updater manifest is missing platform entry: ${platform}`);
    }
    if (typeof entry.signature !== 'string' || !entry.signature.trim()) {
      throw new Error(`Updater manifest ${platform} signature must be a non-empty string`);
    }
    if (
      typeof entry.url !== 'string' ||
      !entry.url.startsWith('https://') ||
      !entry.url.endsWith(suffix)
    ) {
      throw new Error(`Updater manifest ${platform} url must start with https:// and end with ${suffix}`);
    }
  }

  return manifest;
}
