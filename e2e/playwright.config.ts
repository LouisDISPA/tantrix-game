import { homedir } from 'node:os';
import path from 'node:path';
import { defineConfig } from '@playwright/test';

const port = 4173;
const baseURL = `http://127.0.0.1:${port}`;
const cargoHome = process.env.CARGO_HOME ?? path.join(homedir(), '.cargo');
const dx = path.join(cargoHome, 'bin', 'dx');

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
    command: `"${dx}" run --web --fullstack true --features fullstack --port ${port} --open false --interactive false --hot-reload false --watch false`,
    cwd: path.join(__dirname, '..', 'tantrix-app'),
    url: baseURL,
    timeout: 10 * 60_000,
    reuseExistingServer: false,
    gracefulShutdown: { signal: 'SIGINT', timeout: 5_000 },
  },
});
