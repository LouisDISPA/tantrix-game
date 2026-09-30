import os from 'node:os';
import path from 'node:path';
import { defineConfig } from '@playwright/test';

const port = 4175;
const baseURL = `http://127.0.0.1:${port}`;
const dioxus = path.join(process.env.CARGO_HOME ?? path.join(os.homedir(), '.cargo'), 'bin', 'dx');

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
    command: `"${dioxus}" run --package tantrix-app --web --fullstack true --features fullstack --port ${port} --open false --hot-reload false`,
    cwd: path.join(__dirname, '..'),
    env: { TANTRIX_ROOM_DB: ':memory:' },
    url: `${baseURL}/health`,
    timeout: 180_000,
    reuseExistingServer: false,
  },
});
