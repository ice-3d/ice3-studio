import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  use: {
    baseURL: 'http://127.0.0.1:8121',
  },
  webServer: [
    {
      command: 'node scripts/run-viewer-e2e-server.mjs',
      cwd: '../..',
      url: 'http://127.0.0.1:8120/health',
      reuseExistingServer: false,
      timeout: 120_000,
    },
    {
      command: 'npm run dev -- --host 127.0.0.1 --port 8121',
      url: 'http://127.0.0.1:8121',
      reuseExistingServer: false,
      env: {
        STUDIO_SESSION_TOKEN: 'studio-e2e',
      },
      timeout: 120_000,
    },
  ],
});
