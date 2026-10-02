import path from 'node:path';
import { defineConfig } from '@playwright/test';

// Build the Worker and client with `just worker-build` first.
const port = 4174;
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL,
    browserName: 'chromium',
    actionTimeout: 15_000,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: `pnpm exec wrangler dev --port ${port} --ip 127.0.0.1`,
    cwd: path.join(__dirname, '..', 'cloudflare'),
    url: baseURL,
    timeout: 60_000,
    reuseExistingServer: false,
  },
});
