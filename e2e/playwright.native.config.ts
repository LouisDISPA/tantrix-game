import os from 'node:os';
import path from 'node:path';
import { defineConfig } from '@playwright/test';

const port = 4175;
const root = path.join(__dirname, '..');
const baseURL = process.env.BASE_URL ?? `http://127.0.0.1:${port}`;
const dioxus = path.join(process.env.CARGO_HOME ?? path.join(os.homedir(), '.cargo'), 'bin', 'dx');

export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  globalTimeout: 15 * 60_000,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 4 : 8,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL,
    browserName: 'chromium',
    actionTimeout: 15_000,
    screenshot: 'only-on-failure',
    trace: process.env.CI ? 'on-first-retry' : 'off',
  },
  webServer: process.env.BASE_URL ? undefined : {
    command: `"${dioxus}" run --release --package tantrix-app --web --fullstack true --features fullstack --port ${port} --open false`,
    cwd: root,
    env: { TANTRIX_ROOM_DB: ':memory:' },
    url: `${baseURL}/health`,
    timeout: 10 * 60_000,
    reuseExistingServer: !process.env.CI,
  },
});
