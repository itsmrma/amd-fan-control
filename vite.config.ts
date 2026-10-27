import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  // Keep SVGs as local files: the desktop CSP intentionally disallows data URLs.
  build: { assetsInlineLimit: 0 },
  server: {
    watch: {
      // Rust output is at the workspace root; watching locked Windows build
      // executables can otherwise terminate Vite with EBUSY during tauri dev.
      ignored: ['**/target/**', '**/src-tauri/**', '**/crates/**'],
    },
  },
});
