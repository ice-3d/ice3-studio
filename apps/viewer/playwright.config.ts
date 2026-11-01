import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  use: {
    baseURL: 'http://127.0.0.1:8120',
    ...(process.env.PLAYWRIGHT_CHANNEL === 'chrome'
      ? { channel: 'chrome' as const }
      : {}),
  },
  webServer: {
    command: 'node scripts/run-viewer-e2e-server.mjs',
    cwd: '../..',
    url: 'http://127.0.0.1:8120/health',
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
